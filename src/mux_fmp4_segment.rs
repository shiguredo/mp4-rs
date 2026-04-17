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
//! fMP4 の sample entry 自体は `stsd` にしか格納できないが、
//! [`Fmp4SegmentMuxer`] は `create_media_segment_metadata()` に渡されたサンプルから
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
//! use shiguredo_mp4::mux::{Fmp4SegmentMuxer, Sample};
//!
//! # fn main() -> Result<(), Box<dyn 'static + std::error::Error>> {
//! let sample_entry = todo!("使用するコーデックに合わせたサンプルエントリーを構築する");
//! let mut muxer = Fmp4SegmentMuxer::new()?;
//!
//! // 返り値は moof + mdat header であり、payload 自体は含まれない
//! let samples = vec![Sample {
//!     track_kind: TrackKind::Video,
//!     timescale: NonZeroU32::new(90000).expect("non-zero"),
//!     sample_entry: Some(sample_entry),
//!     duration: 3000,
//!     keyframe: true,
//!     composition_time_offset: None,
//!     data_offset: 0,
//!     data_size: 1024,
//! }];
//! let segment_bytes = muxer.create_media_segment_metadata(&samples)?;
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
    mux_mp4_file::{MuxError, Sample},
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
struct ResolvedSegmentTrack {
    track_index: usize,
    samples: Vec<ResolvedSegmentSample>,
    total_duration: u64,
    sample_description_index: Option<u32>,
    first_data_offset: u64,
    payload_end: u64,
}

#[derive(Debug)]
struct ResolvedSegmentSample {
    duration: u32,
    keyframe: bool,
    composition_time_offset: Option<i32>,
    data_size: usize,
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
/// 2. [`create_media_segment_metadata()`](Self::create_media_segment_metadata) を繰り返し呼び出してトラック情報と sample entry を蓄積しつつメディアセグメントを生成
/// 3. [`init_segment_bytes()`](Self::init_segment_bytes) で、その時点までに観測した内容を反映した初期化セグメントを取得
/// 4. 必要に応じて [`mfra_bytes()`](Self::mfra_bytes) でランダムアクセスインデックスを取得
#[derive(Debug, Clone)]
pub struct Fmp4SegmentMuxer {
    tracks: Vec<TrackEntry>,
    options: SegmentMuxerOptions,
    sequence_number: u32,
    /// `create_media_segment_metadata*()` で表現した media segment のバイト数累計
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
    /// [`create_media_segment_metadata()`](Self::create_media_segment_metadata) ないし
    /// [`create_media_segment_metadata_with_sidx()`](Self::create_media_segment_metadata_with_sidx) で
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

    /// メディアセグメント先頭のメタデータ（`moof` + `mdat` ヘッダー）のバイト列を生成する
    ///
    /// `samples` に含まれるサンプルは `track_kind` でグループ化して扱われる。
    /// 同一セグメント内の同一トラックのサンプルは、`data_offset` の昇順で
    /// `mdat` payload 領域内に連続して配置されている必要がある。
    /// トラック間の payload 配置順は `data_offset` に従って決定される。
    ///
    /// 返り値に含まれるのは `moof` と `mdat` ヘッダーのみであり、
    /// `mdat` payload そのものは含まれない。
    /// 呼び出し側は、返り値の直後に `samples` が参照する payload 群を
    /// `data_offset` / `data_size` の指定どおりに配置する必要がある。
    ///
    /// `samples[*].data_offset` の基準は、
    /// [`crate::mux::Mp4FileMuxer::append_sample()`] で使う「ファイル全体の絶対位置」ではなく、
    /// 「今回のセグメントに属する `mdat` payload 領域の先頭からの相対位置」である。
    ///
    /// `samples[*].composition_time_offset` は公開 API では demuxer と揃えて `i64` だが、
    /// この muxer が `trun` に書けるのは `i32::MIN ..= i32::MAX` の範囲に限られる。
    /// 範囲外の値を指定した場合はエラーになる。
    ///
    /// 現実装は `1 track = 1 traf = 1 trun` を前提としている。
    /// そのため、ひとつのトラックに属する payload を複数の離れた範囲へ分割して
    /// 配置することはサポートしていない。
    ///
    /// このメソッドは、メディアセグメントを生成するだけでなく、
    /// `init_segment_bytes()` の構築に必要なトラック情報と sample entry も内部に蓄積する。
    pub fn create_media_segment_metadata(
        &mut self,
        samples: &[Sample],
    ) -> Result<Vec<u8>, MuxError> {
        let (segment, _) = self.build_media_segment_bytes(samples)?;
        Ok(segment)
    }

    /// `sidx` ボックスを先頭に付加したメディアセグメント先頭メタデータを生成する
    ///
    /// `sidx` はセグメントインデックスボックスであり、
    /// MPEG-DASH などのアダプティブストリーミングで利用される。
    ///
    /// `sidx` の `reference_id` は最初のサンプルのトラック種別に対応する track_id を使用する。
    ///
    /// このメソッドも [`create_media_segment_metadata()`](Self::create_media_segment_metadata) と同様に、
    /// 観測したトラック情報と sample entry を内部に蓄積する。
    pub fn create_media_segment_metadata_with_sidx(
        &mut self,
        samples: &[Sample],
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
        let (media_segment, mdat_payload_size) = self.build_media_segment_bytes(samples)?;
        let media_segment_size = media_segment
            .len()
            .checked_add(usize::try_from(mdat_payload_size).map_err(|_| MuxError::Overflow)?)
            .ok_or(MuxError::Overflow)?;
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
        samples: &[Sample],
    ) -> Result<(Vec<u8>, u64), MuxError> {
        if samples.is_empty() {
            return Err(MuxError::EmptySamples);
        }
        let moof_relative_offset = self.media_bytes_written;
        let sequence_number = self.sequence_number.checked_add(1).ok_or_else(|| {
            MuxError::EncodeError(Error::invalid_data("sequence number overflow"))
        })?;
        let mut next_tracks = self.tracks.clone();
        let resolved_tracks = resolve_segment_tracks(&mut next_tracks, samples)?;

        // mdat ヘッダーを先に確定する
        // ペイロードサイズに応じて U32 (8 バイトヘッダー) か U64 (16 バイトヘッダー) を選択する
        let mdat_payload_size = resolved_tracks
            .iter()
            .map(|track| track.payload_end)
            .max()
            .ok_or(MuxError::EmptySamples)?;
        let mdat_box_size_value = BoxHeader::MIN_SIZE as u64 + mdat_payload_size;
        let (mdat_box_size, mdat_header_size) = if mdat_box_size_value <= u32::MAX as u64 {
            (
                BoxSize::U32(mdat_box_size_value as u32),
                BoxHeader::MIN_SIZE,
            )
        } else {
            // 拡張サイズが必要: ヘッダーが 16 バイトになるため合計値を再計算する
            let extended_box_size = 16u64 + mdat_payload_size;
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
        let data_offset_base = u64::try_from(moof_size + mdat_header_size).map_err(|_| {
            MuxError::EncodeError(Error::invalid_data(
                "data_offset base overflow: moof + mdat header exceeds u64 max",
            ))
        })?;

        for resolved_track in &resolved_tracks {
            let track_data_offset = data_offset_base
                .checked_add(resolved_track.first_data_offset)
                .ok_or(MuxError::Overflow)?;
            track_data_offsets[resolved_track.track_index] = i32::try_from(track_data_offset)
                .map_err(|_| {
                    MuxError::EncodeError(Error::invalid_data(
                        "data_offset overflow: moof + mdat header exceeds i32 max",
                    ))
                })?;
        }

        // 正しい data_offset で moof を構築する
        let moof = self.build_moof(
            &next_tracks,
            &resolved_tracks,
            sequence_number,
            &track_data_offsets,
        )?;
        let moof_bytes = moof.encode_to_vec()?;

        // 返り値には moof + mdat ヘッダーのみを含める。
        let mut segment = moof_bytes;
        segment.extend_from_slice(&mdat_header_bytes);

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
            .and_then(|written| written.checked_add(mdat_payload_size))
            .ok_or(MuxError::Overflow)?;
        self.sequence_number = sequence_number;
        self.tracks = next_tracks;
        self.tfra_entries = next_tfra_entries;
        Ok((segment, mdat_payload_size))
    }

    /// ランダムアクセスインデックス（`mfra`）のバイト列を生成する
    ///
    /// `mfra` ボックスはファイルの末尾に付加することで、
    /// ランダムアクセスを高速化するために利用される。
    ///
    /// `mfra` 内の `tfra.moof_offset` は、
    /// このメソッドを呼んだ時点での [`init_segment_bytes()`](Self::init_segment_bytes)
    /// のサイズを先頭オフセットとして計算される。
    ///
    /// したがって、`mfra` を実際に付加するファイルでは、
    /// このメソッドで前提にした init segment を先頭に配置する必要がある。
    /// 途中で観測済みトラックや sample entry が増えた場合は init segment の内容とサイズも
    /// 変わり得るため、最終的に先頭へ配置する init segment を確定させた後で
    /// `mfra_bytes()` を呼ぶこと。
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
        resolved_tracks: &[ResolvedSegmentTrack],
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
                        size: Some(u32::try_from(sample.data_size).map_err(|_| {
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

fn resolve_segment_tracks(
    tracks: &mut Vec<TrackEntry>,
    samples: &[Sample],
) -> Result<Vec<ResolvedSegmentTrack>, MuxError> {
    let mut ordered_kinds = Vec::new();
    for sample in samples {
        if !ordered_kinds.contains(&sample.track_kind) {
            ordered_kinds.push(sample.track_kind);
        }
    }

    let mut resolved_tracks = Vec::new();
    for track_kind in ordered_kinds {
        let track_samples: Vec<&Sample> = samples
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
        let mut expected_next_data_offset: Option<u64> = None;
        let mut first_data_offset = None;
        let mut payload_end = 0u64;

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
            if first_data_offset.is_none() {
                first_data_offset = Some(sample.data_offset);
            }
            if let Some(expected_offset) = expected_next_data_offset
                && expected_offset != sample.data_offset
            {
                return Err(MuxError::EncodeError(Error::invalid_input(
                    "sample data for the same track must be contiguous in the segment payload",
                )));
            }
            expected_next_data_offset = Some(
                sample
                    .data_offset
                    .checked_add(sample.data_size as u64)
                    .ok_or(MuxError::Overflow)?,
            );
            payload_end = expected_next_data_offset.expect("offset must be set");

            resolved_samples.push(ResolvedSegmentSample {
                duration: sample.duration,
                keyframe: sample.keyframe,
                composition_time_offset: sample
                    .composition_time_offset
                    .map(|offset| {
                        i32::try_from(offset).map_err(|_| {
                            MuxError::EncodeError(Error::invalid_input(
                                "composition_time_offset for fMP4 must be within i32 range",
                            ))
                        })
                    })
                    .transpose()?,
                data_size: sample.data_size,
            });
        }

        track.current_sample_entry_index = current_sample_entry_index;
        let sample_description_index = match segment_sample_entry_index {
            // ISO 14496-12 では tfhd.sample_description_index を省略した場合、
            // trex.default_sample_description_index が適用される。
            // build_init_moov() では各トラックの default_sample_description_index に 1 を
            // 設定しているため、0-based index=0 のときは tfhd 側を省略してよい。
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
            first_data_offset: first_data_offset.expect("track must contain at least one sample"),
            payload_end,
        });
    }

    resolved_tracks.sort_by_key(|track| track.first_data_offset);
    let mut expected_track_offset = 0u64;
    for track in &resolved_tracks {
        if track.first_data_offset != expected_track_offset {
            return Err(MuxError::EncodeError(Error::invalid_input(
                "track payload ranges must be contiguous and ordered by data_offset",
            )));
        }
        expected_track_offset = track.payload_end;
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
