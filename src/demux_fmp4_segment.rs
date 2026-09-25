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
//!   - `moof` の前に `styp` / `sidx` などのボックスが置かれることがある
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
//! let init_data: &[u8] = todo!("bytes of the init segment");
//! demuxer.handle_init_segment(init_data)?;
//!
//! let tracks = demuxer.tracks()?;
//! println!("Found {} track(s)", tracks.len());
//!
//! // メディアセグメントを処理する
//! let segment_data: &[u8] = todo!("bytes of a media segment");
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
    boxes::{FtypBox, HdlrBox, MdatBox, MoofBox, MoovBox, SampleEntry, TfhdBox, TrexBox},
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
                // 字幕トラックのハンドラー種別は `subt` (stpp) / `text` (wvtt / tx3g) の 2 種類
                HdlrBox::HANDLER_TYPE_SUBT | HdlrBox::HANDLER_TYPE_TEXT => TrackKind::Subtitle,
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
    /// `moof` より前にあるトップレベルボックス（`styp` / `sidx` / `ssix` / `prft` / `free` など）は、
    /// 種別を問わず中身を解釈せずに読み飛ばす。
    /// `ftyp` / `moov` / `mdat` も同じく読み飛ばし、`moov` があってもその内容は反映しない。
    /// トラックの設定には、常に [`handle_init_segment()`](Self::handle_init_segment) で処理した内容を使う。
    ///
    /// 返される [`Sample`] の `data_offset` は、
    /// `data` スライスの先頭からのバイトオフセットである。
    /// `moof` より前のボックスを読み飛ばした場合も、
    /// `data_offset` の基準は `moof` の先頭ではなく `data` スライスの先頭のままである。
    ///
    /// `sample_entry` は各トラックの最初のサンプル、または
    /// sample description index が変わったサンプルでのみ `Some` になる。
    ///
    /// # 制限事項
    ///
    /// 1 回の呼び出しで処理できるのは単一の `moof` + `mdat` ペアのみ。
    /// セグメント内に複数の `moof` + `mdat` ペアが含まれる場合や、
    /// `mdat` の後ろに追加データが存在する場合はエラーになる。
    /// `moof` が見つからない場合や、`moof` より前にサイズが 0 のボックス
    /// （size=0、または size=1 + largesize=0）がある場合もエラーになる。
    ///
    /// # サポートする `base_data_offset` モード
    ///
    /// - `tfhd` に `base_data_offset` が明示されている場合: その値を `data` スライス先頭からの相対値として使用する
    /// - `default_base_is_moof = true` かつ `base_data_offset` なし: moof 先頭を基準とする
    /// - `default_base_is_moof = false` かつ `base_data_offset` なし: 最初の `traf` は moof 先頭、2 番目以降は前の `traf` のデータ末尾を基準とする（ISO 14496-12 Section 8.8.8）
    ///
    /// # エラー返却時の内部状態
    ///
    /// エラーを返した場合も内部状態は変わらない。
    /// そのため、エラーの後も同じインスタンスを使い続けられる。
    /// その後に渡したメディアセグメントで `sample_entry` が `Some` になるサンプルは、
    /// エラーを返した呼び出しがなかった場合と同じになる。
    pub fn handle_media_segment(&mut self, data: &[u8]) -> Result<Vec<Sample<'_>>, DemuxError> {
        if data.is_empty() {
            return Err(DemuxError::DecodeError(Error::invalid_input(
                "empty media segment",
            )));
        }

        // ISO/IEC 14496-12:2022 では、メディアセグメントの `moof` の前に
        // `styp` (8.16.2) / `sidx` (8.16.3、複数可) / `ssix` (8.16.4) / `prft` (8.16.5) が置かれ得る。
        // `free` / `skip` (8.1.2) も置かれ得るうえ、未知のボックスは無視して読み飛ばすことになっている (4.2.2)。
        // そのため、これらを種別ごとに列挙せず、`moof` が出るまで汎用的に読み飛ばす。
        // `styp` が先頭にあるかどうかも検証しない (8.16.2 では、先頭にない `styp` は無視してよい)。
        // なお、ここでの扱いは ISO/IEC 14496-12:2022 に基づくものであり、将来の改訂で変わる可能性がある。
        let mut offset = 0;
        loop {
            if offset >= data.len() {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "moof box not found in media segment",
                )));
            }
            let (header, _) = BoxHeader::decode(&data[offset..])?;
            if header.box_type == MoofBox::TYPE {
                break;
            }
            let box_size = usize::try_from(header.box_size.get()).map_err(|_| {
                DemuxError::DecodeError(Error::invalid_data("box size exceeds usize::MAX"))
            })?;
            // size=0 のボックスはトップレベルのコンテナの最後のボックスでなければならない (4.2.2)。
            // メディアセグメント単体を渡すこの API では入力全体をそのコンテナとみなすため、その後ろに `moof` は存在し得ない。
            // size=1 + largesize=0 は仕様上の意味が定められておらず、読み飛ばし先を決められない。
            // どちらもボックスサイズは 0 になるため、ここでまとめてエラーにする。
            // この検査がないと `offset` が進まず、ループが終わらなくなる。
            if box_size == 0 {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "found box with size=0 before moof in media segment",
                )));
            }
            offset = offset.checked_add(box_size).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("box offset overflow in media segment"))
            })?;
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
        let mdat_end = if mdat_header.box_size.get() == 0 {
            data.len()
        } else {
            offset
                .checked_add(usize::try_from(mdat_header.box_size.get()).map_err(|_| {
                    DemuxError::DecodeError(Error::invalid_data("mdat box size exceeds usize::MAX"))
                })?)
                .ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data("mdat box end overflow"))
                })?
        };
        if mdat_end > data.len() {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "mdat box exceeds segment boundary",
            )));
        }

        let (pending_samples, current_sample_description_indices) = {
            let track_infos = &self.track_infos;
            let track_runtimes = self
                .track_runtimes
                .as_ref()
                .ok_or(DemuxError::InvalidState(
                    "Init segment has not been processed yet",
                ))?;

            // エラーを返した場合に内部状態を変えないように、各トラックの直前の sample description index を
            // 作業用に複製し、このメディアセグメントの処理中はこちらだけを読み書きする。
            // `track_runtimes` へは、すべての検証が通った後にまとめて書き戻す。
            //
            // `emit_sample_entry` の判定で `track_runtimes` の値と比べてはいけない。
            // ISO/IEC 14496-12:2022 の 8.8.6.1 は 1 つの `moof` に同じトラックの `traf` を複数置くことを認めており、
            // 2 番目以降の `traf` は、同じ呼び出しで先に処理した `traf` の値と比べる必要があるためである。
            // なお、この扱いは ISO/IEC 14496-12:2022 に基づくものであり、将来の改訂で変わる可能性がある。
            let mut current_sample_description_indices: Vec<Option<u32>> = track_runtimes
                .iter()
                .map(|track_runtime| track_runtime.current_sample_description_index)
                .collect();

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
                let track_runtime = &track_runtimes[track_index];

                let sample_description_index =
                    resolve_sample_description_index(&traf.tfhd_box, &track_runtime.trex)?;
                let sample_entry_index = checked_sample_entry_index(
                    track_runtime,
                    sample_description_index,
                    track_infos[track_index].track_id,
                )?;
                let mut emit_sample_entry = current_sample_description_indices[track_index]
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
                        if sample_data_end > mdat_end {
                            return Err(DemuxError::DecodeError(Error::invalid_data(
                                "sample data range exceeds mdat boundary",
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
                            composition_time_offset: trun_sample.composition_time_offset,
                        });
                        current_sample_description_indices[track_index] =
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

            (pending_samples, current_sample_description_indices)
        };

        if mdat_end != data.len() {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "media segment contains trailing data after mdat",
            )));
        }

        // ここより後でエラーを返すことはないので、作業用の sample description index を書き戻す
        let track_runtimes = self
            .track_runtimes
            .as_mut()
            .expect("bug: track_runtimes must exist after initialization");
        for (track_runtime, sample_description_index) in track_runtimes
            .iter_mut()
            .zip(current_sample_description_indices)
        {
            track_runtime.current_sample_description_index = sample_description_index;
        }

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
