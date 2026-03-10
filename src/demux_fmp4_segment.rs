//! Fragmented MP4 (fMP4) のデマルチプレックス機能を提供するモジュール
//!
//! このモジュールは、fMP4 形式の初期化セグメントとメディアセグメントを解析して
//! サンプルを取り出すための機能を提供する。
//!
//! # fMP4 の構造
//!
//! fMP4 は以下の 2 種類のセグメントで構成される:
//!
//! - **初期化セグメント**: `ftyp` + `moov` (`mvex` / `trex` を含む)
//! - **メディアセグメント**: `moof` + `mdat` のペア（繰り返し）
//!
//! # Examples
//!
//! ```no_run
//! use shiguredo_mp4::demux::Fmp4SegmentDemuxer;
//!
//! # fn main() -> Result<(), Box<dyn 'static + std::error::Error>> {
//! let mut demuxer = Fmp4SegmentDemuxer::new();
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
//!     let data = &segment_data[sample.data_offset as usize
//!         ..sample.data_offset as usize + sample.data_size];
//!     // data を処理...
//! }
//! # Ok(())
//! # }
//! ```
use alloc::{format, vec::Vec};

use crate::{
    BoxHeader, Decode, Error, TrackKind,
    boxes::{FtypBox, HdlrBox, MdatBox, MoofBox, MoovBox, SampleEntry, SidxBox, TfhdBox, TrexBox},
    demux_mp4_file::{DemuxError, Sample, TrackInfo},
};

#[derive(Debug, Clone)]
struct TrackRuntime {
    sample_entries: Vec<SampleEntry>,
    trex: TrexBox,
    current_sample_description_index: Option<u32>,
}

#[derive(Debug, Clone)]
struct PendingSample {
    track_index: usize,
    sample_entry_index: usize,
    emit_sample_entry: bool,
    timestamp: u64,
    duration: u32,
    keyframe: bool,
    data_offset: u64,
    data_size: usize,
    composition_time_offset: Option<i64>,
}

/// fMP4 デマルチプレックス処理を行うための構造体
///
/// 基本的な使用フロー:
/// 1. [`new()`](Self::new) でインスタンスを作成
/// 2. [`handle_init_segment()`](Self::handle_init_segment) で初期化セグメント（`ftyp` + `moov`）を処理
/// 3. [`handle_media_segment()`](Self::handle_media_segment) を繰り返し呼び出してサンプルを取得
#[derive(Debug, Clone)]
pub struct Fmp4SegmentDemuxer {
    track_infos: Vec<TrackInfo>,
    track_runtimes: Option<Vec<TrackRuntime>>,
}

impl Fmp4SegmentDemuxer {
    /// 新しい [`Fmp4SegmentDemuxer`] インスタンスを生成する
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            track_infos: Vec::new(),
            track_runtimes: None,
        }
    }

    /// 初期化セグメント（`ftyp` + `moov`）を処理する
    ///
    /// このメソッドはトラック情報と `trex` のデフォルト値を初期化する。
    /// 2 回目以降の呼び出しは [`DemuxError::InvalidState`] を返す。
    pub fn handle_init_segment(&mut self, data: &[u8]) -> Result<(), DemuxError> {
        if self.track_runtimes.is_some() {
            return Err(DemuxError::InvalidState(
                "Init segment has already been processed",
            ));
        }

        let mut offset = 0;

        if data.len() >= BoxHeader::MIN_SIZE {
            let (header, _) = BoxHeader::decode(data)?;
            if header.box_type == FtypBox::TYPE {
                let (_, ftyp_size) = FtypBox::decode(data)?;
                offset = ftyp_size;
            }
        }

        let moov_box = loop {
            if offset >= data.len() {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "moov box not found in init segment",
                )));
            }
            let (header, _) = BoxHeader::decode(&data[offset..])?;
            if header.box_type == MoovBox::TYPE {
                let (moov, _) = MoovBox::decode(&data[offset..])?;
                break moov;
            }
            let box_size = usize::try_from(header.box_size.get()).map_err(|_| {
                DemuxError::DecodeError(Error::invalid_data("box size exceeds usize::MAX"))
            })?;
            if box_size == 0 {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "found box with size=0 before moov in init segment",
                )));
            }
            offset = offset.checked_add(box_size).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("box offset overflow in init segment"))
            })?;
        };

        let mvex_box = moov_box.mvex_box.ok_or_else(|| {
            DemuxError::DecodeError(Error::invalid_data(
                "moov box does not contain mvex box (not an fMP4 init segment)",
            ))
        })?;
        let trex_list = mvex_box.trex_boxes;

        let mut track_infos = Vec::new();
        let mut track_runtimes = Vec::new();
        for trak in moov_box.trak_boxes {
            let track_id = trak.tkhd_box.track_id;
            let kind = match trak.mdia_box.hdlr_box.handler_type {
                HdlrBox::HANDLER_TYPE_VIDE => TrackKind::Video,
                HdlrBox::HANDLER_TYPE_SOUN => TrackKind::Audio,
                _ => continue,
            };

            let sample_entries = trak.mdia_box.minf_box.stbl_box.stsd_box.entries;
            if sample_entries.is_empty() {
                return Err(DemuxError::DecodeError(Error::invalid_data(format!(
                    "stsd box is empty for track_id={track_id}",
                ))));
            }

            let trex = trex_list
                .iter()
                .find(|trex_box| trex_box.track_id == track_id)
                .cloned()
                .ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data(format!(
                        "trex not found for track_id={track_id}",
                    )))
                })?;

            track_infos.push(TrackInfo {
                track_id,
                kind,
                duration: trak.mdia_box.mdhd_box.duration,
                timescale: trak.mdia_box.mdhd_box.timescale,
            });
            track_runtimes.push(TrackRuntime {
                sample_entries,
                trex,
                current_sample_description_index: None,
            });
        }

        self.track_infos = track_infos;
        self.track_runtimes = Some(track_runtimes);
        Ok(())
    }

    /// 初期化済みのトラック情報を返す
    pub fn tracks(&self) -> Result<&[TrackInfo], DemuxError> {
        if self.track_runtimes.is_none() {
            return Err(DemuxError::InvalidState(
                "Init segment has not been processed yet",
            ));
        }
        Ok(&self.track_infos)
    }

    /// メディアセグメント（`moof` + `mdat`）を処理してサンプルのリストを返す
    ///
    /// 返される [`Sample`] の `data_offset` は、
    /// `data` スライスの先頭からのバイトオフセットである。
    ///
    /// `sample_entry` は各トラックの最初のサンプル、または
    /// sample description index が変わったサンプルでのみ `Some` になる。
    ///
    /// # 制限事項
    ///
    /// 1 回の呼び出しで処理できるのは単一の `moof` + `mdat` ペアのみ。
    /// セグメント内に複数の `moof` + `mdat` ペアが含まれる場合は未対応。
    /// 先頭に `sidx` ボックスが存在する場合は自動的にスキップされる。
    ///
    /// # サポートする `base_data_offset` モード
    ///
    /// - `tfhd` に `base_data_offset` が明示されている場合: その値を `data` スライス先頭からの相対値として使用する
    /// - `default_base_is_moof = true` かつ `base_data_offset` なし: moof 先頭を基準とする
    /// - `default_base_is_moof = false` かつ `base_data_offset` なし: 最初の `traf` は moof 先頭、2 番目以降は前の `traf` のデータ末尾を基準とする（ISO 14496-12 Section 8.8.8）
    pub fn handle_media_segment(&mut self, data: &[u8]) -> Result<Vec<Sample<'_>>, DemuxError> {
        let mut offset = 0;

        if data.len().saturating_sub(offset) >= BoxHeader::MIN_SIZE {
            let (header, _) = BoxHeader::decode(&data[offset..])?;
            if header.box_type == SidxBox::TYPE {
                let box_size = usize::try_from(header.box_size.get()).map_err(|_| {
                    DemuxError::DecodeError(Error::invalid_data("sidx box size exceeds usize::MAX"))
                })?;
                if box_size == 0 {
                    return Err(DemuxError::DecodeError(Error::invalid_data(
                        "sidx box has size=0",
                    )));
                }
                offset = offset.checked_add(box_size).ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data("sidx box offset overflow"))
                })?;
            }
        }

        if offset >= data.len() {
            return Err(DemuxError::DecodeError(Error::invalid_input(
                "empty media segment",
            )));
        }
        let (header, _) = BoxHeader::decode(&data[offset..])?;
        if header.box_type != MoofBox::TYPE {
            return Err(DemuxError::DecodeError(Error::invalid_data(format!(
                "expected moof box but got {:?}",
                header.box_type
            ))));
        }
        let moof_offset = offset;
        let (moof, moof_size) = MoofBox::decode(&data[offset..])?;
        offset = offset
            .checked_add(moof_size)
            .ok_or_else(|| DemuxError::DecodeError(Error::invalid_data("moof offset overflow")))?;

        if offset >= data.len() {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "mdat box not found after moof",
            )));
        }
        let (mdat_header, _) = BoxHeader::decode(&data[offset..])?;
        if mdat_header.box_type != MdatBox::TYPE {
            return Err(DemuxError::DecodeError(Error::invalid_data(format!(
                "expected mdat box after moof but got {:?}",
                mdat_header.box_type
            ))));
        }

        let pending_samples = {
            let track_infos = &self.track_infos;
            let track_runtimes = self
                .track_runtimes
                .as_mut()
                .ok_or(DemuxError::InvalidState(
                    "Init segment has not been processed yet",
                ))?;

            let mut pending_samples = Vec::new();
            let mut prev_traf_data_end: Option<usize> = None;

            for traf in &moof.traf_boxes {
                let track_index = track_infos
                    .iter()
                    .position(|track_info| track_info.track_id == traf.tfhd_box.track_id)
                    .ok_or_else(|| {
                        DemuxError::DecodeError(Error::invalid_data(format!(
                            "unknown track_id in media segment: {}",
                            traf.tfhd_box.track_id
                        )))
                    })?;
                let track_runtime = &mut track_runtimes[track_index];

                let sample_description_index =
                    resolve_sample_description_index(&traf.tfhd_box, &track_runtime.trex)?;
                let sample_entry_index = checked_sample_entry_index(
                    track_runtime,
                    sample_description_index,
                    track_infos[track_index].track_id,
                )?;
                let mut emit_sample_entry = track_runtime.current_sample_description_index
                    != Some(sample_description_index);

                let base_media_decode_time = traf
                    .tfdt_box
                    .as_ref()
                    .map(|tfdt_box| tfdt_box.base_media_decode_time)
                    .unwrap_or(0);

                let base_data_offset = if let Some(explicit_offset) = traf.tfhd_box.base_data_offset
                {
                    usize::try_from(explicit_offset).map_err(|_| {
                        DemuxError::DecodeError(Error::invalid_data(
                            "base_data_offset exceeds usize::MAX",
                        ))
                    })?
                } else if traf.tfhd_box.default_base_is_moof {
                    moof_offset
                } else {
                    prev_traf_data_end.unwrap_or(moof_offset)
                };

                let mut trun_decode_time = base_media_decode_time;
                let mut traf_data_end = base_data_offset;

                for trun in &traf.trun_boxes {
                    let trun_data_start = base_data_offset
                        .checked_add_signed(trun.data_offset.unwrap_or(0) as isize)
                        .ok_or_else(|| {
                            DemuxError::DecodeError(Error::invalid_data(
                                "data_offset calculation overflow",
                            ))
                        })?;

                    let mut sample_data_offset = trun_data_start;

                    for (i, trun_sample) in trun.samples.iter().enumerate() {
                        let duration = trun_sample
                            .duration
                            .or(traf.tfhd_box.default_sample_duration)
                            .unwrap_or(track_runtime.trex.default_sample_duration);
                        let size = usize::try_from(
                            trun_sample
                                .size
                                .or(traf.tfhd_box.default_sample_size)
                                .unwrap_or(track_runtime.trex.default_sample_size),
                        )
                        .map_err(|_| {
                            DemuxError::DecodeError(Error::invalid_data(
                                "sample size exceeds usize::MAX",
                            ))
                        })?;

                        let flags = if i == 0
                            && let Some(first_sample_flags) = trun.first_sample_flags
                        {
                            first_sample_flags
                        } else {
                            trun_sample
                                .flags
                                .or(traf.tfhd_box.default_sample_flags)
                                .unwrap_or(track_runtime.trex.default_sample_flags)
                        };
                        let keyframe = !flags.sample_is_non_sync_sample();

                        let sample_data_end =
                            sample_data_offset.checked_add(size).ok_or_else(|| {
                                DemuxError::DecodeError(Error::invalid_data(
                                    "sample data offset overflow",
                                ))
                            })?;
                        if sample_data_end > data.len() {
                            return Err(DemuxError::DecodeError(Error::invalid_data(
                                "sample data range exceeds segment boundary",
                            )));
                        }

                        pending_samples.push(PendingSample {
                            track_index,
                            sample_entry_index,
                            emit_sample_entry,
                            timestamp: trun_decode_time,
                            duration,
                            keyframe,
                            data_offset: sample_data_offset as u64,
                            data_size: size,
                            composition_time_offset: trun_sample
                                .composition_time_offset
                                .map(i64::from),
                        });
                        track_runtime.current_sample_description_index =
                            Some(sample_description_index);
                        emit_sample_entry = false;

                        trun_decode_time = trun_decode_time
                            .checked_add(duration as u64)
                            .ok_or_else(|| {
                                DemuxError::DecodeError(Error::invalid_data(
                                    "trun decode time overflow",
                                ))
                            })?;
                        sample_data_offset = sample_data_end;
                    }

                    traf_data_end = traf_data_end.max(sample_data_offset);
                }

                prev_traf_data_end = Some(traf_data_end);
            }

            pending_samples
        };

        let track_runtimes = self
            .track_runtimes
            .as_ref()
            .expect("bug: track_runtimes must exist after initialization");
        Ok(pending_samples
            .into_iter()
            .map(|pending| {
                let track = &self.track_infos[pending.track_index];
                let sample_entry = if pending.emit_sample_entry {
                    Some(
                        &track_runtimes[pending.track_index].sample_entries
                            [pending.sample_entry_index],
                    )
                } else {
                    None
                };
                Sample {
                    track,
                    sample_entry,
                    keyframe: pending.keyframe,
                    timestamp: pending.timestamp,
                    duration: pending.duration,
                    data_offset: pending.data_offset,
                    data_size: pending.data_size,
                    composition_time_offset: pending.composition_time_offset,
                }
            })
            .collect())
    }
}

fn resolve_sample_description_index(
    tfhd_box: &TfhdBox,
    trex_box: &TrexBox,
) -> Result<u32, DemuxError> {
    let sample_description_index = tfhd_box
        .sample_description_index
        .unwrap_or(trex_box.default_sample_description_index);
    if sample_description_index == 0 {
        return Err(DemuxError::DecodeError(Error::invalid_data(
            "sample_description_index must be greater than zero",
        )));
    }
    Ok(sample_description_index)
}

fn checked_sample_entry_index(
    track_runtime: &TrackRuntime,
    sample_description_index: u32,
    track_id: u32,
) -> Result<usize, DemuxError> {
    let Some(sample_entry_index) = usize::try_from(sample_description_index - 1).ok() else {
        return Err(DemuxError::DecodeError(Error::invalid_data(
            "sample_description_index exceeds usize::MAX",
        )));
    };
    if sample_entry_index >= track_runtime.sample_entries.len() {
        return Err(DemuxError::DecodeError(Error::invalid_data(format!(
            "sample_description_index={sample_description_index} is out of range for track_id={track_id}",
        ))));
    }
    Ok(sample_entry_index)
}
