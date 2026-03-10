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
//! # Examples
//!
//! ```no_run
//! use std::num::NonZeroU32;
//!
//! use shiguredo_mp4::TrackKind;
//! use shiguredo_mp4::mux::{Fmp4SegmentMuxer, SegmentSample, SegmentTrackConfig};
//!
//! # fn main() -> Result<(), Box<dyn 'static + std::error::Error>> {
//! // トラック設定を定義する
//! let sample_entry = todo!("使用するコーデックに合わせたサンプルエントリーを構築する");
//! let tracks = vec![SegmentTrackConfig {
//!     track_kind: TrackKind::Video,
//!     timescale: NonZeroU32::new(90000).expect("non-zero"),
//!     sample_entry,
//! }];
//!
//! let mut muxer = Fmp4SegmentMuxer::new(tracks)?;
//!
//! // 初期化セグメントを取得する
//! let init_bytes = muxer.init_segment_bytes()?;
//!
//! // メディアセグメントを生成する
//! let sample_data = vec![0u8; 1024];
//! let samples = vec![SegmentSample {
//!     track_index: 0,
//!     duration: 3000,
//!     keyframe: true,
//!     composition_time_offset: None,
//!     data: &sample_data,
//! }];
//! let segment_bytes = muxer.create_media_segment(&samples)?;
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
};

/// fMP4 マルチプレックス処理中に発生するエラー
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum SegmentMuxError {
    /// MP4 ボックスのエンコード処理中に発生したエラー
    EncodeError(Error),

    /// トラックが指定されていない
    EmptyTracks,

    /// サンプルが指定されていない
    EmptySamples,

    /// track_index が範囲外
    InvalidTrackIndex {
        /// 指定されたインデックス
        index: usize,

        /// 有効なトラック数
        track_count: usize,
    },

    /// 内部カウンタのオーバーフロー
    Overflow,
}

impl core::fmt::Display for SegmentMuxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SegmentMuxError::EncodeError(e) => write!(f, "Failed to encode MP4 box: {e}"),
            SegmentMuxError::EmptyTracks => write!(f, "No tracks specified"),
            SegmentMuxError::EmptySamples => write!(f, "No samples in segment"),
            SegmentMuxError::InvalidTrackIndex { index, track_count } => {
                write!(
                    f,
                    "track_index {index} is out of range (track count: {track_count})",
                )
            }
            SegmentMuxError::Overflow => write!(f, "Internal counter overflow"),
        }
    }
}

impl core::error::Error for SegmentMuxError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        if let SegmentMuxError::EncodeError(e) = self {
            Some(e)
        } else {
            None
        }
    }
}

impl From<Error> for SegmentMuxError {
    fn from(e: Error) -> Self {
        SegmentMuxError::EncodeError(e)
    }
}

/// fMP4 マルチプレックス時のトラック設定
#[derive(Debug, Clone)]
pub struct SegmentTrackConfig {
    /// トラックの種類
    pub track_kind: TrackKind,

    /// タイムスケール
    pub timescale: NonZeroU32,

    /// サンプルエントリー（コーデック情報）
    pub sample_entry: SampleEntry,
}

/// fMP4 メディアセグメントに追加するサンプル
#[derive(Debug, Clone)]
pub struct SegmentSample<'a> {
    /// `Fmp4SegmentMuxer::new()` に渡したトラックリストのインデックス
    pub track_index: usize,

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
    pub data: &'a [u8],
}

#[derive(Debug, Clone)]
struct TrackEntry {
    config: SegmentTrackConfig,
    track_id: u32,
    /// 累積デコード時間（タイムスケール単位）
    decode_time: u64,
}

/// tfra ボックス用の 1 セグメント分のエントリ
#[derive(Debug, Clone)]
struct TfraSegmentEntry {
    /// このセグメントの先頭サンプルのデコード時間
    time: u64,
    /// このセグメントの moof ボックスのバイトオフセット
    moof_offset: u64,
    /// moof 内でのこのトラックの traf の 1 ベースインデックス
    traf_number: u32,
}

/// fMP4 ファイルを生成するマルチプレックス処理を行うための構造体
///
/// この構造体は、複数のメディアトラック（音声・映像）からのサンプルを
///  fMP4 形式の初期化セグメントとメディアセグメントに変換する。
///
/// 基本的な使用フロー：
/// 1. [`new()`](Self::new) でインスタンスを作成（トラック設定を指定）
/// 2. [`init_segment_bytes()`](Self::init_segment_bytes) で初期化セグメントを取得
/// 3. [`create_media_segment()`](Self::create_media_segment) を繰り返し呼び出してメディアセグメントを生成
/// 4. 必要に応じて [`mfra_bytes()`](Self::mfra_bytes) でランダムアクセスインデックスを取得
#[derive(Debug, Clone)]
pub struct Fmp4SegmentMuxer {
    tracks: Vec<TrackEntry>,
    creation_timestamp: Duration,
    sequence_number: u32,
    /// init_segment_bytes / create_media_segment で書き出したバイト数の累計
    bytes_written: u64,
    /// トラックごとの tfra エントリ（tracks と同じインデックス）
    tfra_entries: Vec<Vec<TfraSegmentEntry>>,
}

impl Fmp4SegmentMuxer {
    /// [`Fmp4SegmentMuxer`] インスタンスを生成する
    ///
    /// `tracks` は空にできない。空の場合は [`SegmentMuxError::EmptyTracks`] が返される。
    pub fn new(tracks: Vec<SegmentTrackConfig>) -> Result<Self, SegmentMuxError> {
        Self::with_creation_timestamp(tracks, Duration::ZERO)
    }

    /// 作成タイムスタンプを指定して [`Fmp4SegmentMuxer`] インスタンスを生成する
    pub fn with_creation_timestamp(
        tracks: Vec<SegmentTrackConfig>,
        creation_timestamp: Duration,
    ) -> Result<Self, SegmentMuxError> {
        if tracks.is_empty() {
            return Err(SegmentMuxError::EmptyTracks);
        }

        let tracks: Vec<TrackEntry> = tracks
            .into_iter()
            .enumerate()
            .map(|(i, config)| {
                let track_id = u32::try_from(i + 1).expect("track count exceeds u32::MAX");
                TrackEntry {
                    config,
                    track_id,
                    decode_time: 0,
                }
            })
            .collect();

        let track_count = tracks.len();
        Ok(Self {
            tracks,
            creation_timestamp,
            sequence_number: 0,
            bytes_written: 0,
            tfra_entries: vec![Vec::new(); track_count],
        })
    }

    /// 初期化セグメント（`ftyp` + `moov`）のバイト列を返す
    pub fn init_segment_bytes(&mut self) -> Result<Vec<u8>, SegmentMuxError> {
        let ftyp = self.build_ftyp();
        let moov = self.build_init_moov()?;

        let mut bytes = ftyp.encode_to_vec()?;
        bytes.extend_from_slice(&moov.encode_to_vec()?);
        self.bytes_written = self
            .bytes_written
            .checked_add(bytes.len() as u64)
            .ok_or(SegmentMuxError::Overflow)?;
        Ok(bytes)
    }

    /// メディアセグメント（`moof` + `mdat`）のバイト列を生成する
    ///
    /// `samples` に含まれるサンプルは `track_index` でグループ化される。
    /// 同一セグメント内の同一トラックのサンプルは、渡された順序で `mdat` に格納される。
    pub fn create_media_segment(
        &mut self,
        samples: &[SegmentSample],
    ) -> Result<Vec<u8>, SegmentMuxError> {
        self.build_media_segment_bytes(samples)
    }

    /// `sidx` ボックスを先頭に付加したメディアセグメントを生成する
    ///
    /// `sidx` はセグメントインデックスボックスであり、
    /// MPEG-DASH などのアダプティブストリーミングで利用される。
    ///
    /// `sidx` の `reference_id` は最初のトラックの track_id を使用する。
    pub fn create_media_segment_with_sidx(
        &mut self,
        samples: &[SegmentSample],
    ) -> Result<Vec<u8>, SegmentMuxError> {
        // sidx を構築するために必要な情報を build_media_segment_bytes の呼び出し前に確定する
        if samples.is_empty() {
            return Err(SegmentMuxError::EmptySamples);
        }
        let first_track_index = samples[0].track_index;

        // このセグメントに含まれるサンプルの合計尺
        let subsegment_duration: u64 = samples
            .iter()
            .filter(|s| s.track_index == first_track_index)
            .map(|s| s.duration as u64)
            .sum();

        // 最初のサンプルがキーフレームかどうか
        let first_sample_is_keyframe = samples
            .iter()
            .find(|s| s.track_index == first_track_index)
            .map(|s| s.keyframe)
            .unwrap_or(false);

        // decode_time は build_media_segment_bytes の呼び出しで更新されるため、先に保存する
        let earliest_presentation_time = self.tracks[first_track_index].decode_time;

        // メディアセグメントを生成してサイズを計測する
        let media_segment = self.build_media_segment_bytes(samples)?;
        let media_segment_size = media_segment.len();

        let reference_track = &self.tracks[first_track_index];

        let referenced_size = u32::try_from(media_segment_size).map_err(|_| {
            SegmentMuxError::EncodeError(Error::invalid_data(
                "referenced_size overflow: media segment size exceeds u32 max",
            ))
        })?;
        let subsegment_duration_u32 = u32::try_from(subsegment_duration).map_err(|_| {
            SegmentMuxError::EncodeError(Error::invalid_data(
                "subsegment_duration overflow: duration exceeds u32 max",
            ))
        })?;

        let sidx_box = SidxBox {
            reference_id: reference_track.track_id,
            timescale: reference_track.config.timescale.get(),
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
    ) -> Result<Vec<u8>, SegmentMuxError> {
        if samples.is_empty() {
            return Err(SegmentMuxError::EmptySamples);
        }

        // track_index の範囲チェック
        for sample in samples {
            if sample.track_index >= self.tracks.len() {
                return Err(SegmentMuxError::InvalidTrackIndex {
                    index: sample.track_index,
                    track_count: self.tracks.len(),
                });
            }
        }

        let moof_offset = self.bytes_written;

        self.sequence_number = self.sequence_number.checked_add(1).ok_or_else(|| {
            SegmentMuxError::EncodeError(Error::invalid_data("sequence number overflow"))
        })?;

        // 出現するトラックの一覧を順序を保ちながら重複なく収集する（O(n) で処理する）
        let mut seen = vec![false; self.tracks.len()];
        let mut ordered_track_indices: Vec<usize> = Vec::new();
        for sample in samples {
            if !seen[sample.track_index] {
                seen[sample.track_index] = true;
                ordered_track_indices.push(sample.track_index);
            }
        }

        // mdat ペイロードを構築する（トラック順にサンプルデータを連結）
        let mut mdat_payload: Vec<u8> = Vec::new();
        for &ti in &ordered_track_indices {
            for sample in samples.iter().filter(|s| s.track_index == ti) {
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
        let placeholder_offsets = vec![0i32; self.tracks.len()];
        let moof_size = self
            .build_moof(samples, &ordered_track_indices, &placeholder_offsets)?
            .encode_to_vec()?
            .len();

        // 各トラックのサンプルデータの data_offset (moof 先頭からの相対値) を計算する
        let mut track_data_offsets = vec![0i32; self.tracks.len()];
        let mut accumulated_data_size = moof_size + mdat_header_size;

        for &ti in &ordered_track_indices {
            track_data_offsets[ti] = i32::try_from(accumulated_data_size).map_err(|_| {
                SegmentMuxError::EncodeError(Error::invalid_data(
                    "data_offset overflow: moof + mdat header exceeds i32 max",
                ))
            })?;
            let track_data_size: usize = samples
                .iter()
                .filter(|s| s.track_index == ti)
                .map(|s| s.data.len())
                .sum();
            accumulated_data_size += track_data_size;
        }

        // 正しい data_offset で moof を構築する
        let moof = self.build_moof(samples, &ordered_track_indices, &track_data_offsets)?;
        let moof_bytes = moof.encode_to_vec()?;

        // moof + mdat を結合する
        let mut segment = moof_bytes;
        segment.extend_from_slice(&mdat_header_bytes);
        segment.extend_from_slice(&mdat_payload);

        // tfra エントリを記録してから decode_time を更新する
        for (traf_pos, &ti) in ordered_track_indices.iter().enumerate() {
            let entry = TfraSegmentEntry {
                time: self.tracks[ti].decode_time,
                moof_offset,
                traf_number: u32::try_from(traf_pos + 1).expect("traf count exceeds u32::MAX"),
            };
            self.tfra_entries[ti].push(entry);
        }

        // 各トラックの decode_time を更新する
        for ti in 0..self.tracks.len() {
            let total_duration: u64 = samples
                .iter()
                .filter(|s| s.track_index == ti)
                .map(|s| s.duration as u64)
                .sum();
            self.tracks[ti].decode_time = self.tracks[ti]
                .decode_time
                .checked_add(total_duration)
                .ok_or(SegmentMuxError::Overflow)?;
        }

        self.bytes_written = self
            .bytes_written
            .checked_add(segment.len() as u64)
            .ok_or(SegmentMuxError::Overflow)?;
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
    pub fn mfra_bytes(&self) -> Result<Vec<u8>, SegmentMuxError> {
        let mut tfra_boxes = Vec::new();

        for (ti, entries) in self.tfra_entries.iter().enumerate() {
            if entries.is_empty() {
                continue;
            }
            let track = &self.tracks[ti];

            // time / moof_offset が u32 に収まるか否かで version を決める
            let needs_v1 = entries
                .iter()
                .any(|e| e.time > u32::MAX as u64 || e.moof_offset > u32::MAX as u64);
            let version = if needs_v1 { 1 } else { 0 };

            let tfra_entries: Vec<TfraEntry> = entries
                .iter()
                .map(|e| TfraEntry {
                    time: e.time,
                    moof_offset: e.moof_offset,
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
            SegmentMuxError::EncodeError(Error::invalid_data(
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
            match &track.config.sample_entry {
                SampleEntry::Avc1(_) => has_avc1 = true,
                SampleEntry::Hev1(_) => has_hev1 = true,
                SampleEntry::Hvc1(_) => has_hvc1 = true,
                SampleEntry::Av01(_) => has_av01 = true,
                _ => {}
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

    fn build_init_moov(&self) -> Result<MoovBox, SegmentMuxError> {
        let creation_time = Mp4FileTime::from_unix_time(self.creation_timestamp);

        let trak_boxes: Result<Vec<_>, SegmentMuxError> = self
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
    ) -> Result<TrakBox, SegmentMuxError> {
        let visual = match &entry.config.sample_entry {
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
                    SegmentMuxError::EncodeError(crate::Error::invalid_data(
                        "video width exceeds i16::MAX",
                    ))
                })?;
                let h = i16::try_from(v.height).map_err(|_| {
                    SegmentMuxError::EncodeError(crate::Error::invalid_data(
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

        let handler_type = match entry.config.track_kind {
            TrackKind::Video => HdlrBox::HANDLER_TYPE_VIDE,
            TrackKind::Audio => HdlrBox::HANDLER_TYPE_SOUN,
        };

        let hdlr_box = HdlrBox {
            handler_type,
            name: Utf8String::EMPTY.into_null_terminated_bytes(),
        };

        let smhd_or_vmhd = match entry.config.track_kind {
            TrackKind::Audio => Some(Either::A(SmhdBox::default())),
            TrackKind::Video => Some(Either::B(VmhdBox::default())),
        };

        // fMP4 の初期化セグメントでは stbl は stsd のみ持てばよく、
        // 他のサンプルテーブルは空にする
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![entry.config.sample_entry.clone()],
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
            timescale: entry.config.timescale,
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
        samples: &[SegmentSample],
        ordered_track_indices: &[usize],
        data_offsets: &[i32],
    ) -> Result<MoofBox, SegmentMuxError> {
        let mfhd_box = MfhdBox {
            sequence_number: self.sequence_number,
        };

        let mut traf_boxes = Vec::new();
        for &ti in ordered_track_indices {
            let track = &self.tracks[ti];
            let track_samples: Vec<&SegmentSample> =
                samples.iter().filter(|s| s.track_index == ti).collect();

            // いずれかのサンプルに CTO がある場合は全サンプルに明示的な値を設定する
            // (TrunBox のフラグは trun 全体に適用されるため)
            let has_any_cto = track_samples
                .iter()
                .any(|s| s.composition_time_offset.is_some());

            let trun_samples: Vec<TrunSample> = track_samples
                .iter()
                .map(|s| -> Result<TrunSample, SegmentMuxError> {
                    Ok(TrunSample {
                        duration: Some(s.duration),
                        size: Some(u32::try_from(s.data.len()).map_err(|_| {
                            SegmentMuxError::EncodeError(Error::invalid_data(
                                "sample data size exceeds u32::MAX",
                            ))
                        })?),
                        flags: Some(build_sample_flags(s.keyframe)),
                        composition_time_offset: if has_any_cto {
                            Some(s.composition_time_offset.unwrap_or(0))
                        } else {
                            None
                        },
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let trun_box = TrunBox {
                data_offset: Some(data_offsets[ti]),
                first_sample_flags: None,
                samples: trun_samples,
            };

            let tfhd_box = TfhdBox {
                track_id: track.track_id,
                base_data_offset: None,
                sample_description_index: None,
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
