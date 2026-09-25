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
//!   - `moof` の前に `styp` / `sidx` など、`moof` と `mdat` の間や `mdat` の後ろに `free` などのボックスが置かれることがある
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
    BoxHeader, BoxSize, Decode, Error, TrackKind,
    boxes::{
        FtypBox, HdlrBox, MdatBox, MoofBox, MoovBox, SampleEntry, TfhdBox, TrafBox, TrexBox,
        TrunSample,
    },
    demux_mp4_file::{DemuxError, Sample, TrackInfo},
};

#[derive(Debug, Clone)]
struct TrackRuntime {
    sample_entries: Vec<SampleEntry>,
    trex: TrexBox,
    current_sample_description_index: Option<u32>,
}

/// 初期化セグメントで読み飛ばしたトラック
///
/// 映像・音声・字幕以外のハンドラー種別のトラックは、サンプルを返さないためトラック情報に登録しない。
/// ただし、メディアセグメントの `traf` を読み飛ばすために、track_id と `trex` を記録する
#[derive(Debug, Clone)]
struct SkippedTrack {
    track_id: u32,

    /// `trex` は `moov` の各トラックに 1 つずつ必須とされている（ISO/IEC 14496-12:2022 の 8.8.3.1）が、
    /// 読み飛ばすトラックでは初期化セグメントの時点で欠落をエラーにしないため `Option` で持つ
    trex: Option<TrexBox>,
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
    skipped_tracks: Vec<SkippedTrack>,
}

impl Fmp4SegmentDemuxer {
    /// 新しい [`Fmp4SegmentDemuxer`] インスタンスを生成する
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            track_infos: Vec::new(),
            track_runtimes: None,
            skipped_tracks: Vec::new(),
        }
    }

    /// 初期化セグメント（`ftyp` + `moov`）を処理する
    ///
    /// このメソッドはトラック情報と `trex` のデフォルト値を初期化する。
    /// 2 回目以降の呼び出しは [`DemuxError::InvalidState`] を返す。
    ///
    /// ハンドラー種別が `vide` / `soun` / `subt` / `text` 以外のトラックは、
    /// サンプルを返さないためトラック情報に登録せずに読み飛ばす。
    /// 読み飛ばしたトラックの track_id と `trex` は、メディアセグメントでそのトラックの `traf` を
    /// 読み飛ばすために記録する。`trex` がなくても初期化は成功する。
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
        let mut skipped_tracks = Vec::new();
        for trak in moov_box.trak_boxes {
            let track_id = trak.tkhd_box.track_id;
            let kind = match trak.mdia_box.hdlr_box.handler_type {
                HdlrBox::HANDLER_TYPE_VIDE => TrackKind::Video,
                HdlrBox::HANDLER_TYPE_SOUN => TrackKind::Audio,
                // 字幕トラックのハンドラー種別は `subt` (stpp) / `text` (wvtt / tx3g) の 2 種類
                HdlrBox::HANDLER_TYPE_SUBT | HdlrBox::HANDLER_TYPE_TEXT => TrackKind::Subtitle,
                _ => {
                    // 対応していないトラックはサンプルを返さないため、トラック情報には登録しない。
                    // メディアセグメントの `traf` を読み飛ばすために、track_id と `trex` だけ記録する。
                    // `trex` の欠落はここではエラーにしない
                    let trex = trex_list
                        .iter()
                        .find(|trex_box| trex_box.track_id == track_id)
                        .cloned();
                    skipped_tracks.push(SkippedTrack { track_id, trex });
                    continue;
                }
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
        self.skipped_tracks = skipped_tracks;
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
    /// `moof` より前、`moof` と `mdat` の間、`mdat` の後ろにあるトップレベルボックス
    /// （`styp` / `sidx` / `ssix` / `prft` / `free` など）は、種別を問わず中身を解釈せずに読み飛ばす。
    /// `moof` より前にある `ftyp` / `moov` / `mdat` も同じく読み飛ばし、`moov` があってもその内容は反映しない。
    /// トラックの設定には、常に [`handle_init_segment()`](Self::handle_init_segment) で処理した内容を使う。
    ///
    /// 読み飛ばしたボックスの分は `data_offset` に足さない。
    /// `trun` の `data_offset` は `tfhd` で決まる基準（ISO/IEC 14496-12:2022 の 8.8.7.1）に足す値であり（同 8.8.8.3）、
    /// `moof` と `mdat` の間にボックスがあるファイルでは、書き手がその分を含めた値を `trun` に書く。
    ///
    /// 返される [`Sample`] の `data_offset` は、
    /// `data` スライスの先頭からのバイトオフセットである。
    /// 読み飛ばしたボックスがあっても、基準は `moof` の先頭ではなく `data` スライスの先頭のままである。
    ///
    /// `sample_entry` は各トラックの最初のサンプル、または
    /// sample description index が変わったサンプルでのみ `Some` になる。
    ///
    /// # 対応していないトラック
    ///
    /// [`handle_init_segment()`](Self::handle_init_segment) で読み飛ばしたトラックの `traf` からは
    /// サンプルを返さない。ただし `default_base_is_moof = false` かつ `base_data_offset` なしの場合は、
    /// 2 番目以降の `traf` の基準位置が直前の `traf` のデータ末尾になるため、
    /// 読み飛ばす `traf` についてもデータ末尾を計算しておく。
    /// その計算で `trex` の既定値（`default_sample_size`）が要るのに `trex` がない場合はエラーになる。
    /// `moov` に存在しない track_id の `traf` もエラーになる。
    ///
    /// # 制限事項
    ///
    /// 1 回の呼び出しで処理できるのは単一の `moof` + `mdat` ペアのみ。
    /// `mdat` の後ろに `moof` がある場合（入力に `moof` + `mdat` のペアが複数含まれる場合）や、
    /// `moof` の後ろで `mdat` より先に別の `moof` が出た場合はエラーになる。
    ///
    /// 次の入力もエラーになる。
    ///
    /// - `moof` が見つからない場合
    /// - `mdat` が見つからないまま入力の末尾に達した場合
    /// - `moof` より前にサイズが 0 のボックス（32 ビットの size=0、または size=1 + largesize=0）がある場合
    /// - `moof` と `mdat` の間にサイズが 0 のボックスがある場合
    /// - `mdat` の後ろに、size=1 + largesize=0 のボックス、宣言サイズが入力の末尾を超えるボックス、
    ///   ボックスヘッダーに満たない端数がある場合
    ///
    /// `mdat` の後ろにある 32 ビットの size=0 のボックスは、入力の末尾まで続くボックスとして受け付ける。
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

        // `moof` の後ろも、`mdat` が出るまでトップレベルボックスを種別を問わず読み飛ばす。
        // ISO/IEC 14496-12:2022 の 4.2.2 は認識できない種別のボックスを無視して読み飛ばすことを求めており、
        // `free` / `skip` (8.1.2) も `moof` と `mdat` の間に置かれ得る。
        // ただし、`mdat` より先に `moof` が出た場合は、1 回の呼び出しで扱えるのは 1 組だけなのでエラーにする。
        // なお、ここでの扱いは ISO/IEC 14496-12:2022 に基づくものであり、将来の改訂で変わる可能性がある。
        let (mdat_header, mdat_offset) = loop {
            if offset >= data.len() {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "mdat box not found after moof",
                )));
            }
            let (header, _) = BoxHeader::decode(&data[offset..])?;
            if header.box_type == MdatBox::TYPE {
                break (header, offset);
            }
            if header.box_type == MoofBox::TYPE {
                return Err(DemuxError::DecodeError(Error::invalid_data(format!(
                    "expected mdat box after moof but got {:?}",
                    header.box_type
                ))));
            }
            let box_size = usize::try_from(header.box_size.get()).map_err(|_| {
                DemuxError::DecodeError(Error::invalid_data("box size exceeds usize::MAX"))
            })?;
            // size=0 のボックスは読み飛ばし先を決められない。
            // 32 ビットの size=0 はコンテナの最後のボックスなので、その後ろに `mdat` は存在し得ず、
            // size=1 + largesize=0 は仕様上の意味が定められていない。
            // `moof` より前の読み飛ばしと同じ理由でエラーにする。
            // この検査がないと `offset` が進まず、ループが終わらなくなる
            if box_size == 0 {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "found box with size=0 between moof and mdat in media segment",
                )));
            }
            offset = offset.checked_add(box_size).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("box offset overflow in media segment"))
            })?;
        };
        let mdat_end = if mdat_header.box_size.get() == 0 {
            data.len()
        } else {
            mdat_offset
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
                    .position(|track_info| track_info.track_id == traf.tfhd_box.track_id);

                // 初期化セグメントで読み飛ばしたトラックの `traf` はサンプルを返さない。
                // ただし `default_base_is_moof = false` の場合は次の `traf` の基準位置が
                // この `traf` のデータ末尾になるため、`default_base_is_moof` の値によらずデータ末尾を計算する
                let Some(track_index) = track_index else {
                    let skipped_track = self
                        .skipped_tracks
                        .iter()
                        .find(|skipped| skipped.track_id == traf.tfhd_box.track_id)
                        .ok_or_else(|| {
                            DemuxError::DecodeError(Error::invalid_data(format!(
                                "unknown track_id in media segment: {}",
                                traf.tfhd_box.track_id
                            )))
                        })?;
                    prev_traf_data_end = Some(skipped_traf_data_end(
                        traf,
                        skipped_track,
                        moof_offset,
                        prev_traf_data_end,
                    )?);
                    continue;
                };

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

                let base_data_offset =
                    traf_base_data_offset(traf, moof_offset, prev_traf_data_end)?;

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
                            resolve_sample_size(
                                trun_sample,
                                &traf.tfhd_box,
                                Some(&track_runtime.trex),
                            )
                            .expect("bug: a supported track always has a trex box"),
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

        // `mdat` の後ろも、入力の末尾まで `moof` 以外のトップレベルボックスを読み飛ばす。
        // この検査を `traf` のループの後ろに置くのは、エラーを返した場合に内部状態を変えないためである
        // （作業用の sample description index の書き戻しは、この検査より後ろにある）。
        // 32 ビットの size=0 のボックスは、4.2.2 によりコンテナの最後のボックスなので、
        // 入力の末尾まで続くものとして受け付ける（`Fmp4FileDemuxer` が `mdat` の後ろの size=0 を受け付けるのと揃える）。
        // size=1 + largesize=0 は仕様上の意味が定められておらず、読み飛ばし先を決められない。
        // なお、ここでの扱いは ISO/IEC 14496-12:2022 に基づくものであり、将来の改訂で変わる可能性がある。
        let mut trailing_offset = mdat_end;
        while trailing_offset < data.len() {
            // ボックスヘッダーに満たない端数は、`BoxHeader` のデコードエラー
            // （`ErrorKind::InsufficientBuffer`）をそのまま返す。`moof` より前や `moof` と `mdat` の間の
            // 読み飛ばしでも同じ扱いであり、ここだけ別のエラーに変換しない
            let (header, _) = BoxHeader::decode(&data[trailing_offset..])?;
            if header.box_type == MoofBox::TYPE {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "found moof box after mdat in media segment",
                )));
            }
            if header.box_size == BoxSize::VARIABLE_SIZE {
                break;
            }
            let box_size = usize::try_from(header.box_size.get()).map_err(|_| {
                DemuxError::DecodeError(Error::invalid_data("box size exceeds usize::MAX"))
            })?;
            // size=1 + largesize=0
            if box_size == 0 {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "found box with size=0 after mdat in media segment",
                )));
            }
            let next_offset = trailing_offset.checked_add(box_size).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("box offset overflow in media segment"))
            })?;
            if next_offset > data.len() {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "box after mdat exceeds media segment boundary",
                )));
            }
            trailing_offset = next_offset;
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

/// 読み飛ばす `traf` のデータ末尾を計算する
///
/// サンプルは返さないが、`default_base_is_moof = false` かつ `base_data_offset` なしの場合は、
/// 次の `traf` の基準位置がこの `traf` のデータ末尾になるため、サンプルサイズからデータ末尾だけを求める。
/// サンプルサイズは対応しているトラックと同じ順（`trun`、`tfhd` の `default_sample_size`、
/// `trex` の `default_sample_size`）で決める。
/// `trex` の既定値が要るのに `trex` がない場合はエラーになる
fn skipped_traf_data_end(
    traf: &TrafBox,
    skipped_track: &SkippedTrack,
    moof_offset: usize,
    prev_traf_data_end: Option<usize>,
) -> Result<usize, DemuxError> {
    let base_data_offset = traf_base_data_offset(traf, moof_offset, prev_traf_data_end)?;

    let mut traf_data_end = base_data_offset;
    for trun in &traf.trun_boxes {
        let mut sample_data_offset = base_data_offset
            .checked_add_signed(trun.data_offset.unwrap_or(0) as isize)
            .ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("data_offset calculation overflow"))
            })?;

        for trun_sample in &trun.samples {
            let size =
                resolve_sample_size(trun_sample, &traf.tfhd_box, skipped_track.trex.as_ref())
                    .ok_or_else(|| {
                        DemuxError::DecodeError(Error::invalid_data(format!(
                            "trex not found for skipped track_id={}",
                            skipped_track.track_id,
                        )))
                    })?;
            let size = usize::try_from(size).map_err(|_| {
                DemuxError::DecodeError(Error::invalid_data("sample size exceeds usize::MAX"))
            })?;
            sample_data_offset = sample_data_offset.checked_add(size).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("sample data offset overflow"))
            })?;
        }

        traf_data_end = traf_data_end.max(sample_data_offset);
    }

    Ok(traf_data_end)
}

/// `traf` のデータの基準位置を決める
///
/// `tfhd` に `base_data_offset` が明示されている場合はその値、`default_base_is_moof` が true の場合は
/// `moof` の先頭、どちらでもない場合は直前の `traf` のデータ末尾（最初の `traf` は `moof` の先頭）を使う
fn traf_base_data_offset(
    traf: &TrafBox,
    moof_offset: usize,
    prev_traf_data_end: Option<usize>,
) -> Result<usize, DemuxError> {
    if let Some(explicit_offset) = traf.tfhd_box.base_data_offset {
        return usize::try_from(explicit_offset).map_err(|_| {
            DemuxError::DecodeError(Error::invalid_data("base_data_offset exceeds usize::MAX"))
        });
    }
    if traf.tfhd_box.default_base_is_moof {
        return Ok(moof_offset);
    }
    Ok(prev_traf_data_end.unwrap_or(moof_offset))
}

/// `trun` のサンプルサイズを決める
///
/// `trun` の値、`tfhd` の `default_sample_size`、`trex` の `default_sample_size` の順に使う。
/// `trex` の既定値が要るのに `trex` がない場合は `None` を返す
fn resolve_sample_size(
    trun_sample: &TrunSample,
    tfhd_box: &TfhdBox,
    trex_box: Option<&TrexBox>,
) -> Option<u32> {
    trun_sample
        .size
        .or(tfhd_box.default_sample_size)
        .or_else(|| trex_box.map(|trex_box| trex_box.default_sample_size))
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
