//! Fragmented MP4 (fMP4) のマルチプレックス機能を提供するモジュール
//!
//! このモジュールは、複数のメディアトラック（音声・映像）からのサンプルを
//! 初期化セグメントとメディアセグメントに分けて生成する機能を提供する。
//!
//! # fMP4 の構造
//!
//! fMP4 は以下の 2 種類のセグメントで構成される:
//!
//! - **初期化セグメント**: `ftyp` + `moov` (サンプルテーブルの代わりに `mvex/trex` を含む)
//! - **メディアセグメント**: `moof` + `mdat` のペア（繰り返し）
//!
//! # `Mp4FileMuxer` との主な違い
//!
//! [`crate::mux::Mp4FileMuxer`] は、既にどこかへ書き込まれたサンプルデータを
//! `data_offset` と `data_size` で参照する構造になっている。
//! 一方で [`Fmp4SegmentMuxer`] は、セグメント本体の `mdat` をその場で構築するため、
//! サンプルデータそのものを受け取る。
//!
//! fMP4 の sample entry 自体は `stsd` にしか格納できないが、
//! [`Fmp4SegmentMuxer`] は `create_media_segment()` に渡されたサンプルから
//! トラック情報と sample entry を学習し、その時点までに観測した内容を反映した
//! init segment を [`init_segment_bytes()`](Fmp4SegmentMuxer::init_segment_bytes) で返す。
//!
//! そのため、`Mp4FileMuxer` と同様にサンプルごとに `track_kind` / `timescale` /
//! `sample_entry` を受け取る設計になっている。
//! 現時点では `Mp4FileMuxer` と同様に、同時に扱えるトラックは
//! Audio 1 本と Video 1 本までに制限している。
//! 将来、同種複数トラックに対応する場合は file muxer と合わせて拡張する想定である。
//!
//! # Examples
//!
//! ```no_run
//! use std::num::NonZeroU32;
//!
//! use shiguredo_mp4::TrackKind;
//! use shiguredo_mp4::mux::{Fmp4SegmentMuxer, SegmentSample};
//!
//! # fn main() -> Result<(), Box<dyn 'static + std::error::Error>> {
//! let sample_entry = todo!("使用するコーデックに合わせたサンプルエントリーを構築する");
//! let mut muxer = Fmp4SegmentMuxer::new()?;
//!
//! // メディアセグメントを生成する
//! let sample_data = vec![0u8; 1024];
//! let samples = vec![SegmentSample {
//!     track_kind: TrackKind::Video,
//!     timescale: NonZeroU32::new(90000).expect("non-zero"),
//!     sample_entry: Some(sample_entry),
//!     duration: 3000,
//!     keyframe: true,
//!     composition_time_offset: None,
//!     data: &sample_data,
//! }];
//! let segment_bytes = muxer.create_media_segment(&samples)?;
//!
//! // その時点までに観測した内容を反映した init segment を取得する
//! let init_bytes = muxer.init_segment_bytes()?;
//! # Ok(())
//! # }
//! ```
use alloc::{vec, vec::Vec};
use core::{num::NonZeroU32, time::Duration};

use crate::{
    BoxHeader, BoxSize, Either, Encode, Error, FixedPointNumber, Mp4FileTime, SampleFlags,
    TrackKind, Utf8String,
    boxes::{
        Brand, DinfBox, FtypBox, HdlrBox, MdatBox, MdhdBox, MdiaBox, MehdBox, MfhdBox, MfraBox,
        MfroBox, MinfBox, MoofBox, MoovBox, MvexBox, MvhdBox, SampleEntry, SidxBox, SidxReference,
        SmhdBox, StblBox, StcoBox, StscBox, StsdBox, StszBox, SttsBox, TfdtBox, TfhdBox, TfraBox,
        TfraEntry, TkhdBox, TrafBox, TrakBox, TrexBox, TrunBox, TrunSample, VmhdBox,
    },
    mux_mp4_file::MuxError,
};

/// [`Fmp4SegmentMuxer`] 用のオプション
#[derive(Debug, Clone)]
pub struct SegmentMuxerOptions {
    /// ファイル作成時刻（構築される fMP4 内のメタデータとして使われる）
    ///
    /// デフォルト値は UNIX エポック（1970年1月1日 00:00:00 UTC）
    pub creation_timestamp: Duration,
}

impl Default for SegmentMuxerOptions {
    fn default() -> Self {
        Self {
            creation_timestamp: Duration::ZERO,
        }
    }
}

/// fMP4 メディアセグメントに追加するサンプル
#[derive(Debug, Clone)]
pub struct SegmentSample<'a> {
    /// サンプルが属するトラックの種類
    ///
    /// 現時点では Audio 1 本、Video 1 本までを扱う。
    /// 同種複数トラックへの対応は将来 `Mp4FileMuxer` と合わせて拡張する想定である。
    pub track_kind: TrackKind,

    /// サンプルのタイムスケール
    ///
    /// 同じトラック種別のサンプルは、すべて同じタイムスケールを使う必要がある。
    pub timescale: NonZeroU32,

    /// サンプルの詳細情報（コーデック種別など）
    ///
    /// 最初のサンプルでは必須。以後は変更がない限り `None` を指定できる。
    /// `Fmp4SegmentMuxer` は、ここで観測した sample entry を track ごとに蓄積し、
    /// `init_segment_bytes()` ではその時点までに観測した `stsd` を生成する。
    ///
    /// なお、ひとつのメディアセグメント内で同一トラックの sample entry を
    /// 切り替えることは、現在の `1 track = 1 traf` 設計ではサポートしていない。
    pub sample_entry: Option<SampleEntry>,

    /// サンプルの尺（トラックのタイムスケール単位）
    pub duration: u32,

    /// キーフレームかどうか
    pub keyframe: bool,

    /// コンポジション時間オフセット（B フレーム向け）
    ///
    /// PTS と DTS の差分をタイムスケール単位で指定する。
    /// B フレームを含まない場合は `None` を指定する。
    pub composition_time_offset: Option<i32>,

    /// サンプルのデータ
    ///
    /// [`crate::mux::Sample`] の `data_offset` / `data_size` と異なり、
    /// fMP4 segment mux は `mdat` をその場で構築するため payload 自体を受け取る。
    pub data: &'a [u8],
}

#[derive(Debug, Clone)]
struct TrackEntry {
    track_kind: TrackKind,
    timescale: NonZeroU32,
    sample_entries: Vec<SampleEntry>,
    track_id: u32,
    /// 累積デコード時間（タイムスケール単位）
    decode_time: u64,
    current_sample_entry_index: Option<usize>,
}

/// tfra ボックス用の 1 セグメント分のエントリ
#[derive(Debug, Clone)]
struct TfraSegmentEntry {
    /// このセグメントの先頭サンプルのデコード時間
    time: u64,
    /// media segment 列の先頭を 0 としたときの moof ボックスの相対オフセット
    moof_relative_offset: u64,
    /// moof 内でのこのトラックの traf の 1 ベースインデックス
    traf_number: u32,
}

#[derive(Debug)]
struct ResolvedSegmentTrack<'a> {
    track_index: usize,
    samples: Vec<ResolvedSegmentSample<'a>>,
    total_duration: u64,
    sample_description_index: Option<u32>,
}

#[derive(Debug)]
struct ResolvedSegmentSample<'a> {
    duration: u32,
    keyframe: bool,
    composition_time_offset: Option<i32>,
    data: &'a [u8],
}

/// fMP4 ファイルを生成するマルチプレックス処理を行うための構造体
///
/// この構造体は、複数のメディアトラック（音声・映像）からのサンプルを
///  fMP4 形式の初期化セグメントとメディアセグメントに変換する。
///
/// [`crate::mux::Mp4FileMuxer`] と同様に、サンプルごとに `track_kind` / `timescale` /
/// `sample_entry` を受け取り、そこからトラック情報を蓄積する。
/// init segment は、[`init_segment_bytes()`](Self::init_segment_bytes) を呼んだ時点までに
/// 観測した内容を反映して構築される。
///
/// 基本的な使用フロー：
/// 1. [`new()`](Self::new) または [`with_options()`](Self::with_options) でインスタンスを作成
/// 2. [`create_media_segment()`](Self::create_media_segment) を繰り返し呼び出してトラック情報と sample entry を蓄積しつつメディアセグメントを生成
/// 3. [`init_segment_bytes()`](Self::init_segment_bytes) で、その時点までに観測した内容を反映した初期化セグメントを取得
/// 4. 必要に応じて [`mfra_bytes()`](Self::mfra_bytes) でランダムアクセスインデックスを取得
#[derive(Debug, Clone)]
pub struct Fmp4SegmentMuxer {
    tracks: Vec<TrackEntry>,
    options: SegmentMuxerOptions,
    sequence_number: u32,
    /// create_media_segment で書き出した media segment のバイト数累計
    media_bytes_written: u64,
    /// トラックごとの tfra エントリ（tracks と同じインデックス）
    tfra_entries: Vec<Vec<TfraSegmentEntry>>,
}

impl Fmp4SegmentMuxer {
    /// [`Fmp4SegmentMuxer`] インスタンスを生成する
    pub fn new() -> Result<Self, MuxError> {
        Self::with_options(SegmentMuxerOptions::default())
    }

    /// オプションを指定して [`Fmp4SegmentMuxer`] インスタンスを生成する
    pub fn with_options(options: SegmentMuxerOptions) -> Result<Self, MuxError> {
        Ok(Self {
            tracks: Vec::new(),
            options,
            sequence_number: 0,
            media_bytes_written: 0,
            tfra_entries: Vec::new(),
        })
    }

    /// 初期化セグメント（`ftyp` + `moov`）のバイト列を返す
    ///
    /// 返される `moov` には、このメソッドを呼んだ時点までに
    /// [`create_media_segment()`](Self::create_media_segment) ないし
    /// [`create_media_segment_with_sidx()`](Self::create_media_segment_with_sidx) で
    /// 観測したトラック情報と sample entry が反映される。
    ///
    /// まだどのトラックも観測されていない状態では `EmptyTracks` を返す。
    /// また、後から新しい sample entry を観測した場合は、
    /// このメソッドを再度呼ぶことで更新後の `stsd` を含む init segment を取得できる。
    pub fn init_segment_bytes(&self) -> Result<Vec<u8>, MuxError> {
        if self.tracks.is_empty() {
            return Err(MuxError::EmptyTracks);
        }
        let ftyp = self.build_ftyp();
        let moov = self.build_init_moov()?;

        let mut bytes = ftyp.encode_to_vec()?;
        bytes.extend_from_slice(&moov.encode_to_vec()?);
        Ok(bytes)
    }

    /// メディアセグメント（`moof` + `mdat`）のバイト列を生成する
    ///
    /// `samples` に含まれるサンプルは `track_kind` でグループ化される。
    /// 同一セグメント内の同一トラックのサンプルは、渡された順序で `mdat` に格納される。
    ///
    /// [`crate::mux::Mp4FileMuxer::append_sample()`] と異なり、
    /// ここではファイル上の offset ではなくサンプル payload 自体を受け取って
    /// セグメントを構築する。
    ///
    /// このメソッドは、メディアセグメントを生成するだけでなく、
    /// `init_segment_bytes()` の構築に必要なトラック情報と sample entry も内部に蓄積する。
    pub fn create_media_segment(&mut self, samples: &[SegmentSample]) -> Result<Vec<u8>, MuxError> {
        self.build_media_segment_bytes(samples)
    }

    /// `sidx` ボックスを先頭に付加したメディアセグメントを生成する
    ///
    /// `sidx` はセグメントインデックスボックスであり、
    /// MPEG-DASH などのアダプティブストリーミングで利用される。
    ///
    /// `sidx` の `reference_id` は最初のサンプルのトラック種別に対応する track_id を使用する。
    ///
    /// このメソッドも [`create_media_segment()`](Self::create_media_segment) と同様に、
    /// 観測したトラック情報と sample entry を内部に蓄積する。
    pub fn create_media_segment_with_sidx(
        &mut self,
        samples: &[SegmentSample],
    ) -> Result<Vec<u8>, MuxError> {
        if samples.is_empty() {
            return Err(MuxError::EmptySamples);
        }
        let first_track_kind = samples[0].track_kind;

        let subsegment_duration: u64 = samples
            .iter()
            .filter(|s| s.track_kind == first_track_kind)
            .map(|s| s.duration as u64)
            .sum();
        let first_sample_is_keyframe = samples
            .iter()
            .find(|s| s.track_kind == first_track_kind)
            .map(|s| s.keyframe)
            .unwrap_or(false);
        let earliest_presentation_time = self
            .tracks
            .iter()
            .find(|track| track.track_kind == first_track_kind)
            .map_or(0, |track| track.decode_time);
        let media_segment = self.build_media_segment_bytes(samples)?;
        let media_segment_size = media_segment.len();
        let reference_track = self
            .tracks
            .iter()
            .find(|track| track.track_kind == first_track_kind)
            .expect("bug: first sample track must exist after media segment creation");

        let referenced_size = u32::try_from(media_segment_size).map_err(|_| {
            MuxError::EncodeError(Error::invalid_data(
                "referenced_size overflow: media segment size exceeds u32 max",
            ))
        })?;
        let subsegment_duration_u32 = u32::try_from(subsegment_duration).map_err(|_| {
            MuxError::EncodeError(Error::invalid_data(
                "subsegment_duration overflow: duration exceeds u32 max",
            ))
        })?;

        let sidx_box = SidxBox {
            reference_id: reference_track.track_id,
            timescale: reference_track.timescale.get(),
            earliest_presentation_time,
            first_offset: 0,
            references: vec![SidxReference {
                reference_type: false,
                referenced_size,
                subsegment_duration: subsegment_duration_u32,
                starts_with_sap: first_sample_is_keyframe,
                sap_type: if first_sample_is_keyframe { 1 } else { 0 },
                sap_delta_time: 0,
            }],
        };

        let sidx_bytes = sidx_box.encode_to_vec()?;
        let mut result = sidx_bytes;
        result.extend_from_slice(&media_segment);
        Ok(result)
    }

    fn build_media_segment_bytes(
        &mut self,
        samples: &[SegmentSample],
    ) -> Result<Vec<u8>, MuxError> {
        if samples.is_empty() {
            return Err(MuxError::EmptySamples);
        }
        let moof_relative_offset = self.media_bytes_written;
        let sequence_number = self.sequence_number.checked_add(1).ok_or_else(|| {
            MuxError::EncodeError(Error::invalid_data("sequence number overflow"))
        })?;
        let mut next_tracks = self.tracks.clone();
        let resolved_tracks = resolve_segment_tracks(&mut next_tracks, samples)?;

        // mdat ペイロードを構築する（トラック順にサンプルデータを連結）
        let mut mdat_payload: Vec<u8> = Vec::new();
        for resolved_track in &resolved_tracks {
            for sample in &resolved_track.samples {
                mdat_payload.extend_from_slice(sample.data);
            }
        }

        // mdat ヘッダーを先に確定する
        // ペイロードサイズに応じて U32 (8 バイトヘッダー) か U64 (16 バイトヘッダー) を選択する
        let mdat_payload_size = mdat_payload.len();
        let mdat_box_size_value = BoxHeader::MIN_SIZE as u64 + mdat_payload_size as u64;
        let (mdat_box_size, mdat_header_size) = if mdat_box_size_value <= u32::MAX as u64 {
            (
                BoxSize::U32(mdat_box_size_value as u32),
                BoxHeader::MIN_SIZE,
            )
        } else {
            // 拡張サイズが必要: ヘッダーが 16 バイトになるため合計値を再計算する
            let extended_box_size = 16u64 + mdat_payload_size as u64;
            (BoxSize::U64(extended_box_size), 16)
        };
        let mdat_header = BoxHeader::new(MdatBox::TYPE, mdat_box_size);
        let mdat_header_bytes = mdat_header.encode_to_vec()?;

        // moof のサイズを確定させるために、仮の data_offset=0 で一度エンコードする。
        // data_offset は i32 固定長フィールドのため、値が変わっても moof のサイズは変わらない。
        let placeholder_offsets = vec![0i32; next_tracks.len()];
        let moof_size = self
            .build_moof(
                &next_tracks,
                &resolved_tracks,
                sequence_number,
                &placeholder_offsets,
            )?
            .encode_to_vec()?
            .len();

        // 各トラックのサンプルデータの data_offset (moof 先頭からの相対値) を計算する
        let mut track_data_offsets = vec![0i32; next_tracks.len()];
        let mut accumulated_data_size = moof_size + mdat_header_size;

        for resolved_track in &resolved_tracks {
            track_data_offsets[resolved_track.track_index] = i32::try_from(accumulated_data_size)
                .map_err(|_| {
                MuxError::EncodeError(Error::invalid_data(
                    "data_offset overflow: moof + mdat header exceeds i32 max",
                ))
            })?;
            let track_data_size: usize = resolved_track
                .samples
                .iter()
                .map(|sample| sample.data.len())
                .sum();
            accumulated_data_size += track_data_size;
        }

        // 正しい data_offset で moof を構築する
        let moof = self.build_moof(
            &next_tracks,
            &resolved_tracks,
            sequence_number,
            &track_data_offsets,
        )?;
        let moof_bytes = moof.encode_to_vec()?;

        // moof + mdat を結合する
        let mut segment = moof_bytes;
        segment.extend_from_slice(&mdat_header_bytes);
        segment.extend_from_slice(&mdat_payload);

        // tfra エントリを記録してから decode_time を更新する
        let mut next_tfra_entries = self.tfra_entries.clone();
        while next_tfra_entries.len() < next_tracks.len() {
            next_tfra_entries.push(Vec::new());
        }
        for (traf_pos, resolved_track) in resolved_tracks.iter().enumerate() {
            let ti = resolved_track.track_index;
            let entry = TfraSegmentEntry {
                time: self.tracks.get(ti).map_or(0, |track| track.decode_time),
                moof_relative_offset,
                traf_number: u32::try_from(traf_pos + 1).expect("traf count exceeds u32::MAX"),
            };
            next_tfra_entries[ti].push(entry);
        }

        for resolved_track in &resolved_tracks {
            let track = &mut next_tracks[resolved_track.track_index];
            track.decode_time = track
                .decode_time
                .checked_add(resolved_track.total_duration)
                .ok_or(MuxError::Overflow)?;
        }

        self.media_bytes_written = self
            .media_bytes_written
            .checked_add(segment.len() as u64)
            .ok_or(MuxError::Overflow)?;
        self.sequence_number = sequence_number;
        self.tracks = next_tracks;
        self.tfra_entries = next_tfra_entries;
        Ok(segment)
    }

    /// ランダムアクセスインデックス（`mfra`）のバイト列を生成する
    ///
    /// `mfra` ボックスはファイルの末尾に付加することで、
    /// ランダムアクセスを高速化するために利用される。
    ///
    /// [`init_segment_bytes()`](Self::init_segment_bytes) と
    /// [`create_media_segment()`](Self::create_media_segment) を呼び出した後でないと
    /// 正しいオフセット情報が得られないことに注意。
    pub fn mfra_bytes(&self) -> Result<Vec<u8>, MuxError> {
        let init_segment_size =
            u64::try_from(self.init_segment_bytes()?.len()).expect("init segment size exceeds u64");
        let mut tfra_boxes = Vec::new();

        for (ti, entries) in self.tfra_entries.iter().enumerate() {
            if entries.is_empty() {
                continue;
            }
            let track = &self.tracks[ti];

            // time / moof_offset が u32 に収まるか否かで version を決める
            let needs_v1 = entries.iter().any(|e| {
                let moof_offset = init_segment_size + e.moof_relative_offset;
                e.time > u32::MAX as u64 || moof_offset > u32::MAX as u64
            });
            let version = if needs_v1 { 1 } else { 0 };

            let tfra_entries: Vec<TfraEntry> = entries
                .iter()
                .map(|e| TfraEntry {
                    time: e.time,
                    moof_offset: init_segment_size + e.moof_relative_offset,
                    traf_number: e.traf_number,
                    trun_number: 1,
                    sample_number: 1,
                })
                .collect();

            // traf_number の最大値に応じてフィールドサイズを決定する
            // ISO 14496-12: 0=1byte, 1=2bytes, 2=3bytes, 3=4bytes
            let max_traf_num = entries.iter().map(|e| e.traf_number).max().unwrap_or(0);
            let length_size_of_traf_num: u8 = if max_traf_num <= 0xFF {
                0
            } else if max_traf_num <= 0xFFFF {
                1
            } else if max_traf_num <= 0xFF_FFFF {
                2
            } else {
                3
            };

            tfra_boxes.push(TfraBox {
                version,
                track_id: track.track_id,
                length_size_of_traf_num,
                // trun_number / sample_number は常に 1 なので 1 バイトで十分
                length_size_of_trun_num: 0,
                length_size_of_sample_num: 0,
                entries: tfra_entries,
            });
        }

        // mfro.size は mfra 全体のサイズ。まず 0 でエンコードしてサイズを確定させる
        let mut mfra_box = MfraBox {
            tfra_boxes,
            mfro_box: MfroBox { size: 0 },
        };
        let placeholder = mfra_box.encode_to_vec()?;
        let mfra_size = u32::try_from(placeholder.len()).map_err(|_| {
            MuxError::EncodeError(Error::invalid_data(
                "mfra box size overflow: size exceeds u32 max",
            ))
        })?;
        mfra_box.mfro_box.size = mfra_size;

        Ok(mfra_box.encode_to_vec()?)
    }

    fn build_ftyp(&self) -> FtypBox {
        let mut has_avc1 = false;
        let mut has_hev1 = false;
        let mut has_hvc1 = false;
        let mut has_av01 = false;

        for track in &self.tracks {
            for sample_entry in &track.sample_entries {
                match sample_entry {
                    SampleEntry::Avc1(_) => has_avc1 = true,
                    SampleEntry::Hev1(_) => has_hev1 = true,
                    SampleEntry::Hvc1(_) => has_hvc1 = true,
                    SampleEntry::Av01(_) => has_av01 = true,
                    _ => {}
                }
            }
        }

        let mut compatible_brands = vec![Brand::ISOM, Brand::ISO5, Brand::ISO6, Brand::MP41];
        if has_avc1 {
            compatible_brands.push(Brand::AVC1);
        }
        if has_hev1 {
            compatible_brands.push(Brand::HEV1);
        }
        if has_hvc1 {
            compatible_brands.push(Brand::HVC1);
        }
        if has_av01 {
            compatible_brands.push(Brand::AV01);
        }

        FtypBox {
            major_brand: Brand::ISO5,
            minor_version: 0,
            compatible_brands,
        }
    }

    fn build_init_moov(&self) -> Result<MoovBox, MuxError> {
        if self.tracks.is_empty() {
            return Err(MuxError::EmptyTracks);
        }
        let creation_time = Mp4FileTime::from_unix_time(self.options.creation_timestamp);

        let trak_boxes: Result<Vec<_>, MuxError> = self
            .tracks
            .iter()
            .map(|t| self.build_init_trak(t, creation_time))
            .collect();
        let trak_boxes = trak_boxes?;

        let trex_boxes: Vec<_> = self
            .tracks
            .iter()
            .map(|t| TrexBox {
                track_id: t.track_id,
                default_sample_description_index: 1,
                default_sample_duration: 0,
                default_sample_size: 0,
                default_sample_flags: SampleFlags::new(0),
            })
            .collect();

        let mvex_box = MvexBox {
            mehd_box: Some(MehdBox {
                fragment_duration: 0,
            }),
            trex_boxes,
            unknown_boxes: Vec::new(),
        };

        let mvhd_box = MvhdBox {
            creation_time,
            modification_time: creation_time,
            timescale: NonZeroU32::new(1000).expect("1000 is non-zero"),
            duration: 0,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: u32::try_from(self.tracks.len() + 1)
                .expect("track count exceeds u32::MAX"),
        };

        Ok(MoovBox {
            mvhd_box,
            trak_boxes,
            mvex_box: Some(mvex_box),
            unknown_boxes: Vec::new(),
        })
    }

    fn build_init_trak(
        &self,
        entry: &TrackEntry,
        creation_time: Mp4FileTime,
    ) -> Result<TrakBox, MuxError> {
        let sample_entry = entry
            .sample_entries
            .first()
            .ok_or(MuxError::MissingSampleEntry {
                track_kind: entry.track_kind,
            })?;
        let visual = match sample_entry {
            SampleEntry::Avc1(b) => Some(&b.visual),
            SampleEntry::Hev1(b) => Some(&b.visual),
            SampleEntry::Hvc1(b) => Some(&b.visual),
            SampleEntry::Vp08(b) => Some(&b.visual),
            SampleEntry::Vp09(b) => Some(&b.visual),
            SampleEntry::Av01(b) => Some(&b.visual),
            _ => None,
        };
        let (volume, width, height) = match visual {
            Some(v) => {
                let w = i16::try_from(v.width).map_err(|_| {
                    MuxError::EncodeError(crate::Error::invalid_data(
                        "video width exceeds i16::MAX",
                    ))
                })?;
                let h = i16::try_from(v.height).map_err(|_| {
                    MuxError::EncodeError(crate::Error::invalid_data(
                        "video height exceeds i16::MAX",
                    ))
                })?;
                (
                    TkhdBox::DEFAULT_VIDEO_VOLUME,
                    FixedPointNumber::new(w, 0),
                    FixedPointNumber::new(h, 0),
                )
            }
            None => (
                TkhdBox::DEFAULT_AUDIO_VOLUME,
                FixedPointNumber::default(),
                FixedPointNumber::default(),
            ),
        };

        let tkhd_box = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time,
            modification_time: creation_time,
            track_id: entry.track_id,
            duration: 0,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width,
            height,
        };

        let handler_type = match entry.track_kind {
            TrackKind::Video => HdlrBox::HANDLER_TYPE_VIDE,
            TrackKind::Audio => HdlrBox::HANDLER_TYPE_SOUN,
        };

        let hdlr_box = HdlrBox {
            handler_type,
            name: Utf8String::EMPTY.into_null_terminated_bytes(),
        };

        let smhd_or_vmhd = match entry.track_kind {
            TrackKind::Audio => Some(Either::A(SmhdBox::default())),
            TrackKind::Video => Some(Either::B(VmhdBox::default())),
        };

        // fMP4 の初期化セグメントでは stbl は stsd のみ持てばよく、
        // 他のサンプルテーブルは空にする
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: entry.sample_entries.clone(),
            },
            stts_box: SttsBox::from_sample_deltas(core::iter::empty()),
            ctts_box: None,
            cslg_box: None,
            stsc_box: StscBox { entries: vec![] },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![],
            }),
            stss_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let mdhd_box = MdhdBox {
            creation_time,
            modification_time: creation_time,
            timescale: entry.timescale,
            duration: 0,
            language: MdhdBox::LANGUAGE_UNDEFINED,
        };

        let minf_box = MinfBox {
            smhd_or_vmhd_box: smhd_or_vmhd,
            dinf_box: DinfBox::LOCAL_FILE,
            stbl_box,
            unknown_boxes: Vec::new(),
        };

        let mdia_box = MdiaBox {
            mdhd_box,
            hdlr_box,
            minf_box,
            unknown_boxes: Vec::new(),
        };

        Ok(TrakBox {
            tkhd_box,
            edts_box: None,
            mdia_box,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_moof(
        &self,
        tracks: &[TrackEntry],
        resolved_tracks: &[ResolvedSegmentTrack<'_>],
        sequence_number: u32,
        data_offsets: &[i32],
    ) -> Result<MoofBox, MuxError> {
        let mfhd_box = MfhdBox { sequence_number };

        let mut traf_boxes = Vec::new();
        for resolved_track in resolved_tracks {
            let track = &tracks[resolved_track.track_index];
            let has_any_cto = resolved_track
                .samples
                .iter()
                .any(|sample| sample.composition_time_offset.is_some());

            let trun_samples: Vec<TrunSample> = resolved_track
                .samples
                .iter()
                .map(|sample| -> Result<TrunSample, MuxError> {
                    Ok(TrunSample {
                        duration: Some(sample.duration),
                        size: Some(u32::try_from(sample.data.len()).map_err(|_| {
                            MuxError::EncodeError(Error::invalid_data(
                                "sample data size exceeds u32::MAX",
                            ))
                        })?),
                        flags: Some(build_sample_flags(sample.keyframe)),
                        composition_time_offset: if has_any_cto {
                            Some(sample.composition_time_offset.unwrap_or(0))
                        } else {
                            None
                        },
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let trun_box = TrunBox {
                data_offset: Some(data_offsets[resolved_track.track_index]),
                first_sample_flags: None,
                samples: trun_samples,
            };

            let tfhd_box = TfhdBox {
                track_id: track.track_id,
                base_data_offset: None,
                sample_description_index: resolved_track.sample_description_index,
                default_sample_duration: None,
                default_sample_size: None,
                default_sample_flags: None,
                duration_is_empty: false,
                default_base_is_moof: true,
            };

            let tfdt_box = TfdtBox {
                version: if track.decode_time > u32::MAX as u64 {
                    1
                } else {
                    0
                },
                base_media_decode_time: track.decode_time,
            };

            traf_boxes.push(TrafBox {
                tfhd_box,
                tfdt_box: Some(tfdt_box),
                trun_boxes: vec![trun_box],
                unknown_boxes: Vec::new(),
            });
        }

        Ok(MoofBox {
            mfhd_box,
            traf_boxes,
            unknown_boxes: Vec::new(),
        })
    }
}

fn resolve_segment_tracks<'a>(
    tracks: &mut Vec<TrackEntry>,
    samples: &'a [SegmentSample<'a>],
) -> Result<Vec<ResolvedSegmentTrack<'a>>, MuxError> {
    let mut ordered_kinds = Vec::new();
    for sample in samples {
        if !ordered_kinds.contains(&sample.track_kind) {
            ordered_kinds.push(sample.track_kind);
        }
    }

    let mut resolved_tracks = Vec::new();
    for track_kind in ordered_kinds {
        let track_samples: Vec<&SegmentSample<'a>> = samples
            .iter()
            .filter(|sample| sample.track_kind == track_kind)
            .collect();
        let first_sample = track_samples
            .first()
            .expect("bug: ordered track kind must have at least one sample");
        let track_index = ensure_track_entry(tracks, track_kind, first_sample.timescale)?;
        let track = &mut tracks[track_index];

        let mut current_sample_entry_index = track.current_sample_entry_index;
        let mut segment_sample_entry_index = None;
        let mut resolved_samples = Vec::new();
        let mut total_duration = 0u64;

        for sample in track_samples {
            if sample.timescale != track.timescale {
                return Err(MuxError::TimescaleMismatch {
                    track_kind,
                    expected: track.timescale,
                    actual: sample.timescale,
                });
            }

            let sample_entry_index = if let Some(sample_entry) = &sample.sample_entry {
                match track
                    .sample_entries
                    .iter()
                    .position(|known_entry| known_entry == sample_entry)
                {
                    Some(index) => index,
                    None => {
                        track.sample_entries.push(sample_entry.clone());
                        track.sample_entries.len() - 1
                    }
                }
            } else {
                current_sample_entry_index.ok_or(MuxError::MissingSampleEntry { track_kind })?
            };

            if let Some(expected_index) = segment_sample_entry_index {
                if expected_index != sample_entry_index {
                    return Err(MuxError::MixedSampleEntries { track_kind });
                }
            } else {
                segment_sample_entry_index = Some(sample_entry_index);
            }

            current_sample_entry_index = Some(sample_entry_index);
            total_duration = total_duration
                .checked_add(sample.duration as u64)
                .ok_or(MuxError::Overflow)?;
            resolved_samples.push(ResolvedSegmentSample {
                duration: sample.duration,
                keyframe: sample.keyframe,
                composition_time_offset: sample.composition_time_offset,
                data: sample.data,
            });
        }

        track.current_sample_entry_index = current_sample_entry_index;
        let sample_description_index = match segment_sample_entry_index {
            Some(0) => None,
            Some(index) => Some(u32::try_from(index + 1).map_err(|_| {
                MuxError::EncodeError(Error::invalid_data(
                    "sample_description_index exceeds u32::MAX",
                ))
            })?),
            None => None,
        };
        resolved_tracks.push(ResolvedSegmentTrack {
            track_index,
            samples: resolved_samples,
            total_duration,
            sample_description_index,
        });
    }

    Ok(resolved_tracks)
}

fn ensure_track_entry(
    tracks: &mut Vec<TrackEntry>,
    track_kind: TrackKind,
    timescale: NonZeroU32,
) -> Result<usize, MuxError> {
    if let Some(track_index) = tracks
        .iter()
        .position(|track| track.track_kind == track_kind)
    {
        let track = &tracks[track_index];
        if track.timescale != timescale {
            return Err(MuxError::TimescaleMismatch {
                track_kind,
                expected: track.timescale,
                actual: timescale,
            });
        }
        return Ok(track_index);
    }

    let track_id = u32::try_from(tracks.len() + 1).expect("track count exceeds u32::MAX");
    tracks.push(TrackEntry {
        track_kind,
        timescale,
        sample_entries: Vec::new(),
        track_id,
        decode_time: 0,
        current_sample_entry_index: None,
    });
    Ok(tracks.len() - 1)
}

/// SampleFlags を生成する
///
/// キーフレーム（同期サンプル）かどうかに応じて適切なフラグを設定する。
fn build_sample_flags(keyframe: bool) -> SampleFlags {
    if keyframe {
        // sample_depends_on=2 (独立している), sample_is_non_sync_sample=false
        SampleFlags::from_fields(0, 2, 0, 0, 0, false, 0)
    } else {
        // sample_depends_on=1 (他に依存している), sample_is_non_sync_sample=true
        SampleFlags::from_fields(0, 1, 0, 0, 0, true, 0)
    }
}
