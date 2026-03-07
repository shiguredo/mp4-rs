//! Fragmented MP4 (fMP4) のデマルチプレックス機能を提供するモジュール
//!
//! このモジュールは、fMP4 形式の初期化セグメントとメディアセグメントを解析して
//! サンプルを取り出すための機能を提供する。
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
//! use shiguredo_mp4::demux_fmp4::Fmp4Demuxer;
//!
//! # fn main() -> Result<(), Box<dyn 'static + std::error::Error>> {
//! let mut demuxer = Fmp4Demuxer::new();
//!
//! // 初期化セグメントを処理する
//! let init_data: &[u8] = todo!("初期化セグメントのバイト列");
//! demuxer.handle_init_segment(init_data)?;
//!
//! let tracks = demuxer.tracks()?;
//! println!("{}個のトラックが見つかりました", tracks.len());
//!
//! // メディアセグメントを処理する
//! let segment_data: &[u8] = todo!("メディアセグメントのバイト列");
//! let samples = demuxer.handle_media_segment(segment_data)?;
//! for sample in &samples {
//!     let data = &segment_data[sample.data_offset as usize..
//!                              sample.data_offset as usize + sample.data_size];
//!     // data を処理...
//! }
//! # Ok(())
//! # }
//! ```
use alloc::{format, vec::Vec};
use core::num::NonZeroU32;

use crate::{
    BoxHeader, Decode, Error, TrackKind,
    boxes::{FtypBox, HdlrBox, MdatBox, MoofBox, MoovBox, SampleEntry, SidxBox, TrexBox},
};

/// fMP4 のトラック情報
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fmp4TrackInfo {
    /// トラック ID
    pub track_id: u32,

    /// トラックの種類
    pub kind: TrackKind,

    /// タイムスケール
    pub timescale: NonZeroU32,

    /// サンプルエントリー（コーデック情報）
    pub sample_entry: SampleEntry,
}

/// fMP4 メディアセグメントから取り出されたサンプル
#[derive(Debug, Clone)]
pub struct Fmp4DemuxSample {
    /// サンプルが属するトラックの ID
    pub track_id: u32,

    /// ベースデコード時間（タイムスケール単位）
    ///
    /// `tfdt` ボックスの `base_media_decode_time` に対応する。
    /// このサンプルが属する `trun` 内の先行サンプルの尺を累積した値を加算することで、
    /// このサンプル自身のデコード時間を計算できる。
    pub base_media_decode_time: u64,

    /// サンプルの尺（タイムスケール単位）
    pub duration: u32,

    /// キーフレームかどうか
    pub keyframe: bool,

    /// セグメントデータ内のサンプルデータ開始位置（バイト単位）
    ///
    /// `handle_media_segment()` に渡したスライスの先頭からのバイトオフセット
    pub data_offset: usize,

    /// サンプルデータのサイズ（バイト単位）
    pub data_size: usize,

    /// コンポジション時間オフセット（B フレーム向け）
    ///
    /// `trun` ボックスのサンプルに `sample_composition_time_offset` が含まれる場合に設定される。
    /// PTS = base_media_decode_time + `composition_time_offset` で計算できる。
    /// B フレームを含まない場合は `None` となる。
    pub composition_time_offset: Option<i32>,
}

/// デマルチプレックス処理中に発生するエラー
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Fmp4DemuxError {
    /// MP4 ボックスのデコード処理中に発生したエラー
    DecodeError(Error),

    /// 初期化セグメントが未処理
    NotInitialized,

    /// 既に初期化済み
    AlreadyInitialized,

    /// 指定された track_id に対応するトラックが見つからない
    UnknownTrackId(u32),
}

impl core::fmt::Display for Fmp4DemuxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Fmp4DemuxError::DecodeError(e) => write!(f, "Failed to decode MP4 box: {e}"),
            Fmp4DemuxError::NotInitialized => {
                write!(f, "Init segment has not been processed yet")
            }
            Fmp4DemuxError::AlreadyInitialized => {
                write!(f, "Init segment has already been processed")
            }
            Fmp4DemuxError::UnknownTrackId(id) => write!(f, "Unknown track_id: {id}"),
        }
    }
}

impl core::error::Error for Fmp4DemuxError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        if let Fmp4DemuxError::DecodeError(e) = self {
            Some(e)
        } else {
            None
        }
    }
}

impl From<Error> for Fmp4DemuxError {
    fn from(e: Error) -> Self {
        Self::DecodeError(e)
    }
}

#[derive(Debug, Clone)]
struct TrackEntry {
    track_id: u32,
    kind: TrackKind,
    timescale: NonZeroU32,
    sample_entry: SampleEntry,
    trex: TrexBox,
}

/// fMP4 デマルチプレックス処理を行うための構造体
///
/// 基本的な使用フロー:
/// 1. [`new()`](Self::new) でインスタンスを作成
/// 2. [`handle_init_segment()`](Self::handle_init_segment) で初期化セグメント（`ftyp` + `moov`）を処理
/// 3. [`handle_media_segment()`](Self::handle_media_segment) を繰り返し呼び出してサンプルを取得
#[derive(Debug, Clone)]
pub struct Fmp4Demuxer {
    tracks: Option<Vec<TrackEntry>>,
}

impl Fmp4Demuxer {
    /// 新しい [`Fmp4Demuxer`] インスタンスを生成する
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { tracks: None }
    }

    /// 初期化セグメント（`ftyp` + `moov`）を処理する
    ///
    /// このメソッドはトラック情報と `trex` のデフォルト値を初期化する。
    /// 2 回目以降の呼び出しは [`Fmp4DemuxError::AlreadyInitialized`] を返す。
    pub fn handle_init_segment(&mut self, data: &[u8]) -> Result<(), Fmp4DemuxError> {
        if self.tracks.is_some() {
            return Err(Fmp4DemuxError::AlreadyInitialized);
        }

        let mut offset = 0;

        // ftyp ボックスを読み飛ばす（存在しない場合はスキップ）
        if data.len() >= BoxHeader::MAX_SIZE {
            let (header, _) = BoxHeader::decode(data)?;
            if header.box_type == FtypBox::TYPE {
                let (_, ftyp_size) = FtypBox::decode(data)?;
                offset = ftyp_size;
            }
        }

        // moov ボックスを見つけて解析する
        let moov_box = loop {
            if offset >= data.len() {
                return Err(Fmp4DemuxError::DecodeError(Error::invalid_data(
                    "moov box not found in init segment",
                )));
            }
            let (header, _) = BoxHeader::decode(&data[offset..])?;
            if header.box_type == MoovBox::TYPE {
                let (moov, _) = MoovBox::decode(&data[offset..])?;
                break moov;
            }
            let box_size = header.box_size.get() as usize;
            if box_size == 0 {
                return Err(Fmp4DemuxError::DecodeError(Error::invalid_data(
                    "found box with size=0 before moov in init segment",
                )));
            }
            offset += box_size;
        };

        // mvex が必須
        let mvex_box = moov_box.mvex_box.ok_or_else(|| {
            Fmp4DemuxError::DecodeError(Error::invalid_data(
                "moov box does not contain mvex box (not an fMP4 init segment)",
            ))
        })?;

        // trex を track_id でインデックス化する
        let trex_list = mvex_box.trex_boxes;

        // trak から各トラックの情報を収集する
        let mut tracks = Vec::new();
        for trak in moov_box.trak_boxes {
            let track_id = trak.tkhd_box.track_id;

            let kind = match trak.mdia_box.hdlr_box.handler_type {
                HdlrBox::HANDLER_TYPE_VIDE => TrackKind::Video,
                HdlrBox::HANDLER_TYPE_SOUN => TrackKind::Audio,
                _ => continue,
            };

            let timescale = trak.mdia_box.mdhd_box.timescale;

            // stsd の最初のエントリーをサンプルエントリーとして使用する
            let sample_entry = trak
                .mdia_box
                .minf_box
                .stbl_box
                .stsd_box
                .entries
                .into_iter()
                .next()
                .ok_or_else(|| {
                    Fmp4DemuxError::DecodeError(Error::invalid_data(format!(
                        "stsd box is empty for track_id={track_id}"
                    )))
                })?;

            // 対応する trex を探す
            let trex = trex_list
                .iter()
                .find(|t| t.track_id == track_id)
                .cloned()
                .ok_or_else(|| {
                    Fmp4DemuxError::DecodeError(Error::invalid_data(format!(
                        "trex not found for track_id={track_id}"
                    )))
                })?;

            tracks.push(TrackEntry {
                track_id,
                kind,
                timescale,
                sample_entry,
                trex,
            });
        }

        self.tracks = Some(tracks);
        Ok(())
    }

    /// 初期化済みのトラック情報を返す
    ///
    /// 初期化セグメントを処理していない場合は [`Fmp4DemuxError::NotInitialized`] を返す。
    pub fn tracks(&self) -> Result<Vec<Fmp4TrackInfo>, Fmp4DemuxError> {
        let tracks = self.tracks.as_ref().ok_or(Fmp4DemuxError::NotInitialized)?;
        Ok(tracks
            .iter()
            .map(|t| Fmp4TrackInfo {
                track_id: t.track_id,
                kind: t.kind,
                timescale: t.timescale,
                sample_entry: t.sample_entry.clone(),
            })
            .collect())
    }

    /// メディアセグメント（`moof` + `mdat`）を処理してサンプルのリストを返す
    ///
    /// 返される [`Fmp4DemuxSample`] の `data_offset` は、
    /// `data` スライスの先頭からのバイトオフセットである。
    ///
    /// # サポートする `base_data_offset` モード
    ///
    /// - `tfhd` に `base_data_offset` が明示されている場合: その値を `data` スライス先頭からの相対値として使用する
    /// - `default_base_is_moof = true` かつ `base_data_offset` なし: moof 先頭を基準とする
    /// - `default_base_is_moof = false` かつ `base_data_offset` なし: 最初の `traf` は moof 先頭、2 番目以降は前の `traf` のデータ末尾を基準とする（ISO 14496-12 Section 8.8.8）
    pub fn handle_media_segment(
        &self,
        data: &[u8],
    ) -> Result<Vec<Fmp4DemuxSample>, Fmp4DemuxError> {
        let tracks = self.tracks.as_ref().ok_or(Fmp4DemuxError::NotInitialized)?;

        let mut offset = 0;

        // sidx ボックスが存在する場合は読み飛ばす
        if data.len() >= BoxHeader::MAX_SIZE {
            let (header, _) = BoxHeader::decode(&data[offset..])?;
            if header.box_type == SidxBox::TYPE {
                let box_size = header.box_size.get() as usize;
                if box_size == 0 {
                    return Err(Fmp4DemuxError::DecodeError(Error::invalid_data(
                        "sidx box has size=0",
                    )));
                }
                offset += box_size;
            }
        }

        // moof ボックスを解析する
        if offset >= data.len() {
            return Err(Fmp4DemuxError::DecodeError(Error::invalid_input(
                "empty media segment",
            )));
        }
        let (header, _) = BoxHeader::decode(&data[offset..])?;
        if header.box_type != MoofBox::TYPE {
            return Err(Fmp4DemuxError::DecodeError(Error::invalid_data(format!(
                "expected moof box but got {:?}",
                header.box_type
            ))));
        }
        let moof_offset = offset;
        let (moof, moof_size) = MoofBox::decode(&data[offset..])?;
        offset += moof_size;

        // mdat ボックスを確認する（オフセット計算のためにヘッダーだけ読む）
        if offset >= data.len() {
            return Err(Fmp4DemuxError::DecodeError(Error::invalid_data(
                "mdat box not found after moof",
            )));
        }
        let (mdat_header, _) = BoxHeader::decode(&data[offset..])?;
        if mdat_header.box_type != MdatBox::TYPE {
            return Err(Fmp4DemuxError::DecodeError(Error::invalid_data(format!(
                "expected mdat box after moof but got {:?}",
                mdat_header.box_type
            ))));
        }

        let mut samples = Vec::new();

        // ISO 14496-12: default_base_is_moof=false の場合、2 番目以降の traf の
        // base_data_offset は前の traf のデータ末尾を使用する
        let mut prev_traf_data_end: Option<usize> = None;

        for traf in &moof.traf_boxes {
            let track_id = traf.tfhd_box.track_id;
            let track = tracks
                .iter()
                .find(|t| t.track_id == track_id)
                .ok_or(Fmp4DemuxError::UnknownTrackId(track_id))?;

            // base_media_decode_time を tfdt から取得する（なければ 0）
            let base_media_decode_time = traf
                .tfdt_box
                .as_ref()
                .map(|b| b.base_media_decode_time)
                .unwrap_or(0);

            // base_data_offset を決定する
            //
            // 優先順位:
            //   1. tfhd に base_data_offset が明示されている場合はそれを使う
            //   2. default_base_is_moof が true の場合は moof 先頭位置を使う
            //   3. default_base_is_moof が false の場合:
            //      - 最初の traf: moof 先頭位置
            //      - 2 番目以降: 前の traf のデータ末尾
            let base_data_offset: usize =
                if let Some(explicit_offset) = traf.tfhd_box.base_data_offset {
                    explicit_offset as usize
                } else if traf.tfhd_box.default_base_is_moof {
                    moof_offset
                } else {
                    prev_traf_data_end.unwrap_or(moof_offset)
                };

            // trun 内のサンプルを解析する
            let mut trun_decode_time = base_media_decode_time;
            let mut traf_data_end = base_data_offset;

            for trun in &traf.trun_boxes {
                let trun_data_start = base_data_offset
                    .checked_add_signed(trun.data_offset.unwrap_or(0) as isize)
                    .ok_or_else(|| {
                        Fmp4DemuxError::DecodeError(Error::invalid_data(
                            "data_offset calculation overflow",
                        ))
                    })?;

                let mut sample_data_offset = trun_data_start;

                for (i, trun_sample) in trun.samples.iter().enumerate() {
                    // duration: trun > tfhd > trex の優先順位で解決する
                    let duration = trun_sample
                        .duration
                        .or(traf.tfhd_box.default_sample_duration)
                        .unwrap_or(track.trex.default_sample_duration);

                    // size: trun > tfhd > trex の優先順位で解決する
                    let size = trun_sample
                        .size
                        .or(traf.tfhd_box.default_sample_size)
                        .unwrap_or(track.trex.default_sample_size)
                        as usize;

                    // flags: first_sample_flags (i=0 かつ存在する場合) > trun > tfhd > trex
                    let flags = if i == 0
                        && let Some(first_flags) = trun.first_sample_flags
                    {
                        first_flags
                    } else {
                        trun_sample
                            .flags
                            .or(traf.tfhd_box.default_sample_flags)
                            .unwrap_or(track.trex.default_sample_flags)
                    };

                    // sample_is_non_sync_sample が false のとき = キーフレーム
                    let keyframe = !flags.sample_is_non_sync_sample();

                    samples.push(Fmp4DemuxSample {
                        track_id,
                        base_media_decode_time: trun_decode_time,
                        duration,
                        keyframe,
                        data_offset: sample_data_offset,
                        data_size: size,
                        composition_time_offset: trun_sample.composition_time_offset,
                    });

                    trun_decode_time = trun_decode_time.saturating_add(duration as u64);
                    sample_data_offset = sample_data_offset.checked_add(size).ok_or_else(|| {
                        Fmp4DemuxError::DecodeError(Error::invalid_data(
                            "sample data offset overflow",
                        ))
                    })?;
                }

                // この trun のデータ末尾を更新する
                traf_data_end = traf_data_end.max(sample_data_offset);
            }

            prev_traf_data_end = Some(traf_data_end);
        }

        Ok(samples)
    }
}
