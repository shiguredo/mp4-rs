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
use alloc::{format, vec, vec::Vec};

use crate::{
    BoxHeader, BoxSize, BoxType, Decode, Encode, Error, TrackKind,
    boxes::{
        FtypBox, HdlrBox, MdatBox, MoofBox, MoovBox, PsshBox, SaioBox, SaizBox,
        SampleEncryptionInfo, SampleEntry, SbgpBox, SeigEntry, SencBox, SencSample, SgpdBox,
        SgpdEntry, StypBox, Subsample, TencBox, TfhdBox, TrafBox, TrexBox, TrunBox, TrunSample,
        TrunSamples, UnknownBox,
    },
    boxes_cenc::SEIG_GROUPING_TYPE,
    demux_mp4_file::{DemuxError, Sample, TrackInfo},
};

#[derive(Debug, Clone)]
struct TrackRuntime {
    sample_entries: Vec<SampleEntry>,
    trex: TrexBox,
    current_sample_description_index: Option<u32>,

    /// 初期化セグメントの `stbl` にある `grouping_type` が `seig` の `sgpd`
    ///
    /// `traf` の `sbgp` が `1..=0x10000` の index で参照する。
    seig_sgpd: Option<SgpdBox>,
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
    encryption_info: Option<SampleEncryptionInfo>,
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
    pssh_boxes: Vec<PsshBox>,
    /// 直前の成功したメディアセグメントの先頭にあった `styp` ボックス
    styp: Option<StypBox>,
}

impl Fmp4SegmentDemuxer {
    /// 新しい [`Fmp4SegmentDemuxer`] インスタンスを生成する
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            track_infos: Vec::new(),
            track_runtimes: None,
            skipped_tracks: Vec::new(),
            pssh_boxes: Vec::new(),
            styp: None,
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

        let pssh_boxes = moov_box.pssh_boxes;
        let movie_timescale = moov_box.mvhd_box.timescale;
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

            // `seig` の track-level サンプルグループを取り出す。
            // `grouping_type` が `seig` の track-level `sbgp` は、トラック先頭からの
            // 通算サンプル番号が必要になるため対象外とする
            let seig_sgpd =
                track_level_seig_sgpd(&trak.mdia_box.minf_box.stbl_box.unknown_boxes, track_id)?;
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

            let edit_list = trak.edts_box.and_then(|edts_box| edts_box.elst_box);
            track_infos.push(TrackInfo {
                track_id,
                kind,
                duration: trak.mdia_box.mdhd_box.duration,
                timescale: trak.mdia_box.mdhd_box.timescale,
                movie_timescale,
                edit_list,
            });
            track_runtimes.push(TrackRuntime {
                sample_entries,
                trex,
                current_sample_description_index: None,
                seig_sgpd,
            });
        }

        self.track_infos = track_infos;
        self.track_runtimes = Some(track_runtimes);
        self.skipped_tracks = skipped_tracks;
        self.pssh_boxes = pssh_boxes;
        Ok(())
    }

    /// 初期化セグメントに含まれる `pssh` ボックス群を返す
    ///
    /// [`handle_init_segment()`](Self::handle_init_segment) を呼ぶ前は空スライスを返す。
    pub fn pssh_boxes(&self) -> &[PsshBox] {
        &self.pssh_boxes
    }

    /// 直前の成功したメディアセグメントの先頭にあった `styp` ボックスを返す
    ///
    /// 次のいずれかに該当する場合は `None` を返す。
    ///
    /// - [`handle_init_segment()`](Self::handle_init_segment) を呼ぶ前
    /// - 直前の成功したメディアセグメントの先頭に `styp` がない場合
    /// - 先頭の `styp` をデコードできなかった場合（その `styp` は読み飛ばす）
    ///
    /// 2 個目以降の `styp` と、`mdat` の後ろにある `styp` は無視する。
    /// [`handle_media_segment()`](Self::handle_media_segment) がエラーを返した場合、
    /// この値は直前の成功した呼び出しのまま変わらない。
    pub fn styp(&self) -> Option<&StypBox> {
        self.styp.as_ref()
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
    /// （`sidx` / `ssix` / `prft` / `free` など）は、種別を問わず中身を解釈せずに読み飛ばす。
    /// 例外として、メディアセグメントの先頭にある `styp` だけは型付きで読み、
    /// [`styp()`](Self::styp) から取得できるようにする。
    /// ただし、保持するのはボックス全体が入力内にあり、デコードに成功した場合だけである。
    /// デコードに失敗した場合は従来どおり読み飛ばしてエラーにしない。
    /// 2 個目以降の `styp` と `mdat` の後ろにある `styp` は無視する。
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
    /// - `tfhd` に `base_data_offset` が明示されている場合: その値を `data` スライス先頭からの相対値として使用する。
    ///   `handle_media_segment_with_file_offset` ではファイル先頭からの絶対値をスライス先頭からの相対値に変換する
    /// - `default_base_is_moof = true` かつ `base_data_offset` なし: moof 先頭を基準とする
    /// - `default_base_is_moof = false` かつ `base_data_offset` なし: 最初の `traf` は moof 先頭、2 番目以降は前の `traf` のデータ末尾を基準とする（ISO 14496-12 Section 8.8.8）
    ///
    /// # エラー返却時の内部状態
    ///
    /// エラーを返した場合も内部状態は変わらない。
    /// そのため、エラーの後も同じインスタンスを使い続けられる。
    /// その後に渡したメディアセグメントで `sample_entry` が `Some` になるサンプルは、
    /// エラーを返した呼び出しがなかった場合と同じになる。
    /// [`styp()`](Self::styp) が返す値も、エラーを返した呼び出しの前後で変わらない。
    pub fn handle_media_segment(&mut self, data: &[u8]) -> Result<Vec<Sample<'_>>, DemuxError> {
        self.handle_media_segment_inner(data, None)
    }

    /// ファイル全体を読み込むデマルチプレクサ向けにメディアセグメントを処理する
    ///
    /// `file_offset` は `data` の先頭がファイル先頭から何バイト目かを表す。
    /// `tfhd` の `base_data_offset` がファイル先頭からの絶対値である場合に、
    /// スライス先頭からの相対値へ変換するために使う。
    pub fn handle_media_segment_with_file_offset(
        &mut self,
        data: &[u8],
        file_offset: u64,
    ) -> Result<Vec<Sample<'_>>, DemuxError> {
        self.handle_media_segment_inner(data, Some(file_offset))
    }

    fn handle_media_segment_inner(
        &mut self,
        data: &[u8],
        file_offset: Option<u64>,
    ) -> Result<Vec<Sample<'_>>, DemuxError> {
        if data.is_empty() {
            return Err(DemuxError::DecodeError(Error::invalid_input(
                "empty media segment",
            )));
        }

        // ISO/IEC 14496-12:2022 では、メディアセグメントの `moof` の前に
        // `styp` (8.16.2) / `sidx` (8.16.3、複数可) / `ssix` (8.16.4) / `prft` (8.16.5) が置かれ得る。
        // `free` / `skip` (8.1.2) も置かれ得るうえ、未知のボックスは無視して読み飛ばすことになっている (4.2.2)。
        // そのため、これらを種別ごとに列挙せず、`moof` が出るまで汎用的に読み飛ばす。
        // ただし、先頭の `styp` だけは型付きで読む。8.16.2 は `styp` をセグメントの先頭ボックスと定めており、
        // 先頭以外の `styp` は無視してよいため、位置が 0 のものだけを対象にする。
        // ボックス全体が入力内にあり、デコードに成功した場合だけ保持し、失敗した場合は読み飛ばす
        // （壊れた `styp` を持つセグメントをエラーにしない）。
        // なお、ここでの扱いは ISO/IEC 14496-12:2022 に基づくものであり、将来の改訂で変わる可能性がある。
        let mut styp = None;
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
            if offset == 0
                && header.box_type == StypBox::TYPE
                && let Ok((styp_box, _)) = StypBox::decode(&data[offset..])
            {
                styp = Some(styp_box);
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
                        file_offset,
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

                // この `traf` のサンプルごとの暗号化情報を SAI (`senc` または `saiz` / `saio`)
                // から解決する。`tenc` は `stsd` のサンプルエントリーから取る
                let traf_sample_count =
                    traf.trun_boxes.iter().map(trun_sample_count).sum::<usize>();
                let encryption_infos = resolve_traf_encryption(
                    traf,
                    data,
                    moof_offset,
                    file_offset,
                    &track_runtime.sample_entries[sample_entry_index],
                    traf_sample_count,
                    track_runtime.seig_sgpd.as_ref(),
                )?;

                let base_media_decode_time = traf
                    .tfdt_box
                    .as_ref()
                    .map(|tfdt_box| tfdt_box.base_media_decode_time)
                    .unwrap_or(0);

                let base_data_offset =
                    traf_base_data_offset(traf, moof_offset, prev_traf_data_end, file_offset)?;

                let mut trun_decode_time = base_media_decode_time;
                let mut traf_data_end = base_data_offset;
                let mut prev_trun_data_end: Option<usize> = None;
                let mut traf_sample_index = 0usize;

                for trun in &traf.trun_boxes {
                    let trun_data_start =
                        resolve_trun_data_start(trun, base_data_offset, prev_trun_data_end)?;

                    let mut sample_data_offset = trun_data_start;

                    for (i, trun_row) in trun_sample_rows(&trun.samples).enumerate() {
                        let duration = trun_row
                            .and_then(|sample| sample.duration)
                            .or(traf.tfhd_box.default_sample_duration)
                            .unwrap_or(track_runtime.trex.default_sample_duration);
                        let size = usize::try_from(
                            resolve_sample_size(
                                trun_row.and_then(|sample| sample.size),
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
                            trun_row
                                .and_then(|sample| sample.flags)
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

                        let encryption_info = encryption_infos.get(traf_sample_index).cloned();
                        traf_sample_index += 1;

                        pending_samples.push(PendingSample {
                            track_index,
                            sample_entry_index,
                            emit_sample_entry,
                            timestamp: trun_decode_time,
                            duration,
                            keyframe,
                            data_offset: sample_data_offset as u64,
                            data_size: size,
                            composition_time_offset: trun_row
                                .and_then(|sample| sample.composition_time_offset),
                            encryption_info,
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

                    prev_trun_data_end = Some(sample_data_offset);
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
        // 先頭の `styp` も、すべての検証が通った後にまとめて書き戻す。
        // これにより、エラーを返した呼び出しの前後で `styp()` の値が変わらない
        self.styp = styp;

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
                    encryption_info: pending.encryption_info,
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
/// `trex` の `default_sample_size`）で決める。件数だけの run は既定サイズと件数の積でデータの長さを求め、
/// 件数が 0 の場合は既定サイズを決めない。
/// `trex` の既定値が要るのに `trex` がない場合はエラーになる
fn skipped_traf_data_end(
    traf: &TrafBox,
    skipped_track: &SkippedTrack,
    moof_offset: usize,
    prev_traf_data_end: Option<usize>,
    file_offset: Option<u64>,
) -> Result<usize, DemuxError> {
    let base_data_offset =
        traf_base_data_offset(traf, moof_offset, prev_traf_data_end, file_offset)?;

    // run のデータ長が溢れる場合は、データ末尾（開始位置 + データ長）も必ず溢れる。
    // そのため run のデータ長の計算とデータ末尾の計算で、同じエラーを返す
    let data_offset_overflow =
        || DemuxError::DecodeError(Error::invalid_data("sample data offset overflow"));
    let resolve_size = |trun_sample_size: Option<u32>| -> Result<usize, DemuxError> {
        let size = resolve_sample_size(
            trun_sample_size,
            &traf.tfhd_box,
            skipped_track.trex.as_ref(),
        )
        .ok_or_else(|| {
            DemuxError::DecodeError(Error::invalid_data(format!(
                "trex not found for skipped track_id={}",
                skipped_track.track_id,
            )))
        })?;
        usize::try_from(size).map_err(|_| {
            DemuxError::DecodeError(Error::invalid_data("sample size exceeds usize::MAX"))
        })
    };

    let mut traf_data_end = base_data_offset;
    let mut prev_trun_data_end: Option<usize> = None;
    for trun in &traf.trun_boxes {
        let sample_data_start =
            resolve_trun_data_start(trun, base_data_offset, prev_trun_data_end)?;

        let run_data_size = match &trun.samples {
            // 件数が 0 の run はデータがないので、既定サイズを決める必要がない（trex がなくてもエラーにしない）
            TrunSamples::CountOnly(0) => 0,
            // 件数だけの run は全サンプルが同じ既定サイズになるため、サンプルごとに走査せず積で求める。
            // u32 同士の積なので、usize が 64 ビットの環境では溢れない（32 ビット環境では溢れうる）
            TrunSamples::CountOnly(sample_count) => resolve_size(None)?
                .checked_mul(*sample_count as usize)
                .ok_or_else(data_offset_overflow)?,
            TrunSamples::PerSample(samples) => {
                samples.iter().try_fold(0usize, |total, trun_sample| {
                    total
                        .checked_add(resolve_size(trun_sample.size)?)
                        .ok_or_else(data_offset_overflow)
                })?
            }
        };
        let sample_data_end = sample_data_start
            .checked_add(run_data_size)
            .ok_or_else(data_offset_overflow)?;

        prev_trun_data_end = Some(sample_data_end);
        traf_data_end = traf_data_end.max(sample_data_end);
    }

    Ok(traf_data_end)
}

/// `trun` のデータの開始位置を決める
///
/// `data_offset` がある場合は `traf` の基準位置にその値を足した位置から始まり（ISO/IEC 14496-12:2022 の
/// 8.8.8.3）、ない場合は直前の run のデータ末尾から始まる（同 8.8.8.1）。
/// `traf` の最初の run に `data_offset` がない場合は基準位置から始まる。
/// この扱いは ISO/IEC 14496-12:2022 に基づくものであり、将来の改訂で変わる可能性がある
fn resolve_trun_data_start(
    trun: &TrunBox,
    base_data_offset: usize,
    prev_trun_data_end: Option<usize>,
) -> Result<usize, DemuxError> {
    match trun.data_offset {
        Some(data_offset) => base_data_offset
            .checked_add_signed(data_offset as isize)
            .ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("data_offset calculation overflow"))
            }),
        None => Ok(prev_trun_data_end.unwrap_or(base_data_offset)),
    }
}

/// `traf` のデータの基準位置を決める
///
/// `tfhd` に `base_data_offset` が明示されている場合はその値、`default_base_is_moof` が true の場合は
/// `moof` の先頭、どちらでもない場合は直前の `traf` のデータ末尾（最初の `traf` は `moof` の先頭）を使う
fn traf_base_data_offset(
    traf: &TrafBox,
    moof_offset: usize,
    prev_traf_data_end: Option<usize>,
    file_offset: Option<u64>,
) -> Result<usize, DemuxError> {
    if let Some(explicit_offset) = traf.tfhd_box.base_data_offset {
        // ファイル全体を読む場合は、絶対値の base_data_offset をスライス先頭からの相対値に直す
        let explicit_offset = match file_offset {
            Some(file_offset) => explicit_offset.checked_sub(file_offset).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data(
                    "base_data_offset is before the media segment",
                ))
            })?,
            None => explicit_offset,
        };
        return usize::try_from(explicit_offset).map_err(|_| {
            DemuxError::DecodeError(Error::invalid_data("base_data_offset exceeds usize::MAX"))
        });
    }
    if traf.tfhd_box.default_base_is_moof {
        return Ok(moof_offset);
    }
    Ok(prev_traf_data_end.unwrap_or(moof_offset))
}

/// `trun` のサンプル行を先頭から順に返す
///
/// 件数だけの run は各サンプル行にフィールドがないため、件数分の `None` を返す。
/// 空の [`TrunSample`] を件数分作らないようにするためである
fn trun_sample_rows(samples: &TrunSamples) -> impl Iterator<Item = Option<&TrunSample>> {
    // 2 つの形式を 1 つのイテレーターの型で返すため、件数分の `None` とサンプル行を連結する。
    // どちらか一方は必ず空になる
    let (count_only_rows, per_sample_rows): (u32, &[TrunSample]) = match samples {
        TrunSamples::CountOnly(sample_count) => (*sample_count, &[]),
        TrunSamples::PerSample(samples) => (0, samples),
    };
    (0..count_only_rows)
        .map(|_| None)
        .chain(per_sample_rows.iter().map(Some))
}

/// `trun` のサンプルサイズを決める
///
/// `trun` のサンプル行の値（`trun_sample_size`）、`tfhd` の `default_sample_size`、
/// `trex` の `default_sample_size` の順に使う。
/// `trex` の既定値が要るのに `trex` がない場合は `None` を返す
fn resolve_sample_size(
    trun_sample_size: Option<u32>,
    tfhd_box: &TfhdBox,
    trex_box: Option<&TrexBox>,
) -> Option<u32> {
    trun_sample_size
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

/// `unknown_boxes` から `grouping_type` が `seig` の指定種別のボックスを 1 つだけ取り出す
///
/// `grouping_type` が `seig` 以外のボックスは、意味の解釈をせず生バイトのまま扱うため無視する。
/// 同じ種別の `seig` のボックスが複数ある場合は、どのサンプルグループを使うべきか
/// 決められないため [`crate::ErrorKind::InvalidData`] を返す。
fn find_seig_sample_group_box<'a>(
    unknown_boxes: &'a [UnknownBox],
    box_type: BoxType,
    context: &str,
) -> Result<Option<&'a UnknownBox>, DemuxError> {
    let mut found = None;
    for unknown_box in unknown_boxes {
        if unknown_box.box_type == box_type
            && unknown_box.payload.get(4..8) == Some(&SEIG_GROUPING_TYPE[..])
        {
            if found.is_some() {
                return Err(DemuxError::DecodeError(Error::invalid_data(format!(
                    "multiple '{box_type}' boxes for the 'seig' grouping type in {context}",
                ))));
            }
            found = Some(unknown_box);
        }
    }
    Ok(found)
}

/// `seig` の `sgpd` を型付きボックスとしてデコードする
fn decode_seig_sgpd(unknown_box: &UnknownBox) -> Result<SgpdBox, DemuxError> {
    let bytes = unknown_box.encode_to_vec()?;
    let (sgpd_box, _) = SgpdBox::decode(&bytes)?;
    Ok(sgpd_box)
}

/// `stbl` から track-level の `seig` `sgpd` を取り出す
///
/// `grouping_type` が `seig` の track-level `sbgp` は、トラック先頭からの通算サンプル
/// 番号が必要になるため対象外として [`crate::ErrorKind::Unsupported`] を返す。
/// `seig` 以外の track-level `sbgp` は無視する。
fn track_level_seig_sgpd(
    unknown_boxes: &[UnknownBox],
    track_id: u32,
) -> Result<Option<SgpdBox>, DemuxError> {
    if find_seig_sample_group_box(unknown_boxes, SbgpBox::TYPE, "stbl")?.is_some() {
        return Err(DemuxError::DecodeError(Error::unsupported(format!(
            "track-level 'sbgp' box for the 'seig' grouping type is not supported (track_id={track_id})",
        ))));
    }
    match find_seig_sample_group_box(unknown_boxes, SgpdBox::TYPE, "stbl")? {
        Some(unknown_box) => Ok(Some(decode_seig_sgpd(unknown_box)?)),
        None => Ok(None),
    }
}

/// `sbgp` の 1 エントリーが適用されるサンプル範囲と、その実効 KID
#[derive(Debug, Clone, Copy)]
struct SeigSampleRun {
    /// この run が適用されるサンプル数
    sample_count: u32,

    /// 実効 KID (`None` は `tenc` の既定 KID を使う)
    kid: Option<[u8; 16]>,
}

/// `traf` の `seig` サンプルグループからサンプルごとの実効 KID を解決する
///
/// 返り値は `sbgp` のエントリー順の run である。`sbgp` が無い場合は [`None`] を返し、
/// すべてのサンプルは `tenc` の既定 KID を使う。`sbgp` の `sample_count` の合計が
/// `traf` のサンプル数より少ない場合、余りのサンプルは index `0` として扱う
/// (ISO/IEC 14496-12:2022 8.9.4)。
///
/// サンプルごとの実効 KID の解決は SAI の解釈とは独立して行う。`seig` 以外の
/// サンプルグループ種別は解釈しない。
fn resolve_seig_sample_group(
    traf: &TrafBox,
    track_seig_sgpd: Option<&SgpdBox>,
    track_encryption: Option<&TrackEncryption>,
    sample_count: usize,
) -> Result<Option<Vec<SeigSampleRun>>, DemuxError> {
    let sbgp_unknown_box = find_seig_sample_group_box(&traf.unknown_boxes, SbgpBox::TYPE, "traf")?;
    let sgpd_unknown_box = find_seig_sample_group_box(&traf.unknown_boxes, SgpdBox::TYPE, "traf")?;
    // `sbgp` が参照していなくても、壊れた `seig` の `sgpd` は早期に拒否する
    let fragment_sgpd = match sgpd_unknown_box {
        Some(unknown_box) => Some(decode_seig_sgpd(unknown_box)?),
        None => None,
    };
    let Some(sbgp_unknown_box) = sbgp_unknown_box else {
        return Ok(None);
    };
    let sbgp_bytes = sbgp_unknown_box.encode_to_vec()?;
    let (sbgp_box, _) = SbgpBox::decode(&sbgp_bytes)?;
    // `grouping_type_parameter` はサンプルグループの細分を表す。本実装は `seig` を
    // 1 種類しか扱わないため、0 以外は対象外とする
    if sbgp_box.grouping_type_parameter.unwrap_or(0) != 0 {
        return Err(DemuxError::DecodeError(Error::unsupported(
            "'sbgp' grouping_type_parameter must be 0 for the 'seig' grouping type",
        )));
    }
    let Some(track_encryption) = track_encryption else {
        return Err(DemuxError::DecodeError(Error::unsupported(
            "'seig' sample group requires a CENC protected track",
        )));
    };
    if track_encryption.tenc_box.default_is_protected == 0 {
        return Err(DemuxError::DecodeError(Error::unsupported(
            "'seig' sample group requires a track whose tenc default_is_protected is 1",
        )));
    }

    let mut total_sample_count = 0usize;
    for entry in &sbgp_box.entries {
        total_sample_count = total_sample_count
            .checked_add(usize::try_from(entry.sample_count).map_err(|_| {
                DemuxError::DecodeError(Error::invalid_data(
                    "'sbgp' sample_count exceeds usize::MAX",
                ))
            })?)
            .ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("'sbgp' sample_count total overflow"))
            })?;
    }
    if total_sample_count > sample_count {
        return Err(DemuxError::DecodeError(Error::invalid_data(format!(
            "'sbgp' sample_count total {total_sample_count} exceeds the number of samples in traf ({sample_count})"
        ))));
    }

    let mut runs = Vec::new();
    for entry in &sbgp_box.entries {
        let kid = resolve_sbgp_group_description_index(
            entry.group_description_index,
            track_seig_sgpd,
            fragment_sgpd.as_ref(),
            &track_encryption.tenc_box,
        )?;
        runs.push(SeigSampleRun {
            sample_count: entry.sample_count,
            kid,
        });
    }
    Ok(Some(runs))
}

/// `sbgp` の `group_description_index` を実効 KID に解決する
///
/// `0` はグループに属さないサンプルとして `tenc` の既定 KID を使う。`1..=0x10000` は
/// track-level の `sgpd`、`0x10001` 以上は fragment-local の `sgpd` の 1 始まり
/// エントリー番号として解決する (ISO/IEC 14496-12:2022 8.9.4)。
fn resolve_sbgp_group_description_index(
    group_description_index: u32,
    track_seig_sgpd: Option<&SgpdBox>,
    fragment_sgpd: Option<&SgpdBox>,
    tenc_box: &TencBox,
) -> Result<Option<[u8; 16]>, DemuxError> {
    if group_description_index == 0 {
        return Ok(None);
    }
    let (sgpd_box, entry_index, scope) = if group_description_index <= 0x1_0000 {
        let sgpd_box = track_seig_sgpd.ok_or_else(|| {
            DemuxError::DecodeError(Error::invalid_data(
                "'sbgp' refers to a track-level 'seig' 'sgpd' box but the init segment does not contain one",
            ))
        })?;
        (sgpd_box, group_description_index - 1, "track-level")
    } else {
        let sgpd_box = fragment_sgpd.ok_or_else(|| {
            DemuxError::DecodeError(Error::invalid_data(
                "'sbgp' refers to a fragment-local 'seig' 'sgpd' box but the traf does not contain one",
            ))
        })?;
        (
            sgpd_box,
            group_description_index - 0x1_0001,
            "fragment-local",
        )
    };
    let entry_index = usize::try_from(entry_index).map_err(|_| {
        DemuxError::DecodeError(Error::invalid_data(
            "'sbgp' group_description_index exceeds usize::MAX",
        ))
    })?;
    let entry = sgpd_box.entries.get(entry_index).ok_or_else(|| {
        DemuxError::DecodeError(Error::invalid_data(format!(
            "'sbgp' group_description_index refers to the {scope} 'sgpd' entry {} but it has only {} entries",
            entry_index + 1,
            sgpd_box.entries.len(),
        )))
    })?;
    let SgpdEntry::Seig(seig_entry) = entry else {
        return Err(DemuxError::DecodeError(Error::invalid_data(
            "'sbgp' refers to a 'sgpd' entry that is not a 'seig' entry",
        )));
    };
    check_seig_entry_against_tenc(seig_entry, tenc_box)?;
    // 実効 KID が tenc の既定 KID と同じ場合は None に正規化する
    if seig_entry.kid == tenc_box.default_kid {
        Ok(None)
    } else {
        Ok(Some(seig_entry.kid))
    }
}

/// `seig` エントリーが `tenc` の既定値と整合するかを検査する
///
/// 本実装は KID の切り替えだけを対象とする。`is_protected` / IV サイズの上書き、
/// constant IV、パターン暗号化パラメーターの上書きは対象外であり、`tenc` の既定値と
/// 異なる場合は [`crate::ErrorKind::Unsupported`] を返す。
fn check_seig_entry_against_tenc(entry: &SeigEntry, tenc_box: &TencBox) -> Result<(), DemuxError> {
    if entry.is_protected != tenc_box.default_is_protected {
        return Err(DemuxError::DecodeError(Error::unsupported(format!(
            "'seig' is_protected {} does not match tenc default_is_protected {}",
            entry.is_protected, tenc_box.default_is_protected,
        ))));
    }
    if entry.per_sample_iv_size != tenc_box.default_per_sample_iv_size {
        return Err(DemuxError::DecodeError(Error::unsupported(format!(
            "'seig' Per_Sample_IV_Size {} does not match tenc default_per_sample_iv_size {}",
            entry.per_sample_iv_size, tenc_box.default_per_sample_iv_size,
        ))));
    }
    if entry.constant_iv.is_some() {
        return Err(DemuxError::DecodeError(Error::unsupported(
            "'seig' constant IV is not supported",
        )));
    }
    // `seig` のパターン暗号化パラメーターは常に 0 / 0 であり、`tenc` に 0 以外の
    // パターン値がある場合はそれを上書きして復号結果が変わってしまう
    let crypt_byte_block = tenc_box
        .default_crypt_byte_block
        .map(|v| v.get())
        .unwrap_or(0);
    let skip_byte_block = tenc_box
        .default_skip_byte_block
        .map(|v| v.get())
        .unwrap_or(0);
    if crypt_byte_block != 0 || skip_byte_block != 0 {
        return Err(DemuxError::DecodeError(Error::unsupported(
            "'seig' pattern encryption parameters are not supported",
        )));
    }
    Ok(())
}

/// `seig` サンプルグループから解決した実効 KID をサンプルごとの暗号化情報に反映する
///
/// `runs` に含まれないサンプルは `tenc` の既定 KID (`kid: None`) のままになる。
fn apply_seig_sample_runs(
    encryption_infos: &mut TrafEncryptionInfos,
    runs: &[SeigSampleRun],
) -> Result<(), DemuxError> {
    match encryption_infos {
        TrafEncryptionInfos::None => {
            if runs.iter().any(|run| run.kid.is_some()) {
                return Err(DemuxError::DecodeError(Error::unsupported(
                    "'seig' KID rotation requires a CENC encrypted track",
                )));
            }
        }
        TrafEncryptionInfos::Constant(_) => {
            // constant IV のトラックは per-sample IV ではないため、KID の切り替えを
            // `SampleEncryptionInfo::kid` として表現できない
            if runs.iter().any(|run| run.kid.is_some()) {
                return Err(DemuxError::DecodeError(Error::unsupported(
                    "'seig' KID rotation requires per-sample IV",
                )));
            }
        }
        TrafEncryptionInfos::PerSample(infos) => {
            let mut sample_index = 0usize;
            for run in runs {
                let sample_count = usize::try_from(run.sample_count).map_err(|_| {
                    DemuxError::DecodeError(Error::invalid_data(
                        "'sbgp' sample_count exceeds usize::MAX",
                    ))
                })?;
                let end = sample_index.checked_add(sample_count).ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data(
                        "'sbgp' sample_count total overflow",
                    ))
                })?;
                let Some(target) = infos.get_mut(sample_index..end) else {
                    return Err(DemuxError::DecodeError(Error::invalid_data(
                        "'sbgp' sample_count total exceeds the number of samples in traf",
                    )));
                };
                for info in target {
                    info.kid = run.kid;
                }
                sample_index = end;
            }
        }
    }
    Ok(())
}

/// `traf` に含まれるサンプル数を返す
fn trun_sample_count(trun_box: &TrunBox) -> usize {
    match &trun_box.samples {
        TrunSamples::CountOnly(sample_count) => *sample_count as usize,
        TrunSamples::PerSample(samples) => samples.len(),
    }
}

/// `traf` の `unknown_boxes` から指定された種別のボックスを 1 つだけ取り出す
///
/// 同じ種別のボックスが複数ある場合は、どの SAI を使うべきか決められないため
/// [`crate::ErrorKind::InvalidData`] を返す。
fn find_single_unknown_box(
    traf: &TrafBox,
    box_type: BoxType,
) -> Result<Option<&UnknownBox>, DemuxError> {
    let mut found = None;
    for unknown_box in &traf.unknown_boxes {
        if unknown_box.box_type == box_type {
            if found.is_some() {
                return Err(DemuxError::DecodeError(Error::invalid_data(format!(
                    "multiple '{box_type}' boxes in traf"
                ))));
            }
            found = Some(unknown_box);
        }
    }
    Ok(found)
}

/// サンプルエントリーから取り出した CENC のトラック暗号化情報
#[derive(Debug)]
struct TrackEncryption {
    scheme_type: [u8; 4],
    tenc_box: TencBox,
}

impl TrackEncryption {
    /// サンプルエントリーから CENC のトラック暗号化情報を取り出す
    ///
    /// `Encv` / `Enca` 以外のサンプルエントリーと、`tenc` を持たないサンプルエントリーは
    /// `Ok(None)` になる。
    ///
    /// CENC 系以外の保護スキームの場合は、SAI を CENC として解釈できないため
    /// 対象外として [`crate::ErrorKind::Unsupported`] を返す。平文として `Ok(None)` を
    /// 返すと、暗号化されたサンプルを復号せずに返してしまう。
    fn from_sample_entry(sample_entry: &SampleEntry) -> Result<Option<Self>, DemuxError> {
        let sinf_box = match sample_entry {
            SampleEntry::Encv(encv_box) => &encv_box.sinf_box,
            SampleEntry::Enca(enca_box) => &enca_box.sinf_box,
            _ => return Ok(None),
        };
        let Some(schm_box) = &sinf_box.schm_box else {
            // 保護スキームが無いサンプルエントリーは保護情報を持たない。
            // ただし `tenc` を持っている場合は保護されているため、平文として扱わない
            let has_tenc = sinf_box
                .schi_box
                .as_ref()
                .and_then(|schi_box| schi_box.tenc_box.as_ref())
                .is_some();
            if has_tenc {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "'tenc' requires a 'schm' box",
                )));
            }
            return Ok(None);
        };
        if !crate::boxes_cenc::is_cenc_scheme(schm_box.scheme_type) {
            // CENC 以外の保護スキームは SAI の解釈方法が異なるため対象外とする。
            // 平文として扱うと、暗号化されたサンプルをそのまま返してしまう
            return Err(DemuxError::DecodeError(Error::unsupported(format!(
                "unsupported protection scheme '{}'",
                BoxType::Normal(schm_box.scheme_type),
            ))));
        }
        // `SinfBox` のデコードは CENC 系の `schm` に `tenc` を要求するため、
        // ここに来る入力には `tenc` がある。それでも平文として扱わないよう防御する
        let Some(tenc_box) = sinf_box
            .schi_box
            .as_ref()
            .and_then(|schi_box| schi_box.tenc_box.clone())
        else {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "CENC protected sample entry requires a 'tenc' box",
            )));
        };
        Ok(Some(Self {
            scheme_type: schm_box.scheme_type,
            tenc_box,
        }))
    }
}

/// `saiz` / `saio` から per-sample の SAI を復元するときに受理するサンプル数の上限
///
/// `saiz` / `saio` は `sample_count` を宣言するだけで、SAI が 0 バイトのサンプルも表せる。
/// そのため `sample_count` は入力データのサイズから制約されず、サンプル数分の `Vec` を
/// そのまま確保すると、小さな入力から巨大なメモリを確保できてしまう。
/// 入力サイズに比例しないメモリ確保を防ぐための上限であり、仕様から決まる値ではない。
const MAX_PER_SAMPLE_AUX_INFO_SAMPLES: usize = 1_048_576;

/// `traf` のサンプルごとの暗号化情報
///
/// 暗号化されていないトラックと constant IV を使うトラックでは、サンプル数分の
/// [`SampleEncryptionInfo`] を確保しない。`trun` の `sample_count` は宣言値であり、
/// 実際のサンプルデータの範囲を検査する前にサンプル数分の `Vec` を確保すると、
/// 小さな入力から巨大なメモリを確保できてしまうためである。
#[derive(Debug, Clone)]
enum TrafEncryptionInfos {
    /// 暗号化されていない (すべてのサンプルの `encryption_info` が `None`)
    None,

    /// すべてのサンプルが同じ暗号化情報を持つ (constant IV によるサンプル全体暗号化)
    Constant(SampleEncryptionInfo),

    /// サンプルごとに異なる暗号化情報を持つ
    PerSample(Vec<SampleEncryptionInfo>),
}

impl TrafEncryptionInfos {
    /// サンプル位置を指定して暗号化情報を返す
    fn get(&self, sample_index: usize) -> Option<&SampleEncryptionInfo> {
        match self {
            Self::None => None,
            Self::Constant(info) => Some(info),
            Self::PerSample(infos) => infos.get(sample_index),
        }
    }
}

/// `traf` のサンプルごとの暗号化情報を、`senc` または `saiz` / `saio` から解決する
///
/// - `senc` があれば `tenc` の IV サイズを使って [`SencBox::decode_with_iv_size()`] で読む
/// - `senc` が無く `saiz` / `saio` があれば、SAI を CENC の SampleAuxiliaryDataFormat として読む
/// - `tenc.default_is_protected = 1` かつ `default_per_sample_iv_size = 0` のトラックは、
///   SAI が無ければ constant IV のサンプル全体暗号化として扱う
/// - `tenc.default_is_protected = 0` のトラックに SAI がある場合と、
///   `default_is_protected = 1` かつ per-sample IV が必要なのに SAI が無い場合は、
///   選択的暗号化など対象外の構成なので [`crate::ErrorKind::InvalidData`] を返す
/// - 保護されていないトラックの `saiz` / `saio` は、`aux_info_type` が CENC 系でなければ
///   本ライブラリの対象外として無視する
///
/// 暗号化されていないトラックと constant IV のトラックでは、サンプル数分の
/// [`SampleEncryptionInfo`] を確保しない。`saiz` / `saio` からサンプルごとの情報を
/// 復元する場合は、[`TrafEncryptionInfos::PerSample`] を返す前にサンプル数の上限を検査する。
fn resolve_traf_encryption(
    traf: &TrafBox,
    data: &[u8],
    moof_offset: usize,
    file_offset: Option<u64>,
    sample_entry: &SampleEntry,
    sample_count: usize,
    track_seig_sgpd: Option<&SgpdBox>,
) -> Result<TrafEncryptionInfos, DemuxError> {
    let senc_unknown_box = find_single_unknown_box(traf, SencBox::TYPE)?;
    let saiz_unknown_box = find_single_unknown_box(traf, SaizBox::TYPE)?;
    let saio_unknown_box = find_single_unknown_box(traf, SaioBox::TYPE)?;
    let has_sai =
        senc_unknown_box.is_some() || saiz_unknown_box.is_some() || saio_unknown_box.is_some();
    let track_encryption = TrackEncryption::from_sample_entry(sample_entry)?;
    // KID の解決は SAI の解釈とは独立して行う。`seig` の `sbgp` が無ければ `None` であり、
    // すべてのサンプルが `tenc` の既定 KID を使う
    let seig_sample_runs = resolve_seig_sample_group(
        traf,
        track_seig_sgpd,
        track_encryption.as_ref(),
        sample_count,
    )?;

    if !has_sai {
        let Some(track_encryption) = track_encryption else {
            return Ok(TrafEncryptionInfos::None);
        };
        if track_encryption.tenc_box.default_is_protected == 0 {
            // 暗号化されていないサンプルとして扱う
            return Ok(TrafEncryptionInfos::None);
        }
        if track_encryption.tenc_box.default_per_sample_iv_size != 0 {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "encrypted track requires sample auxiliary information when per-sample IV is used",
            )));
        }
        // constant IV を使うサンプル全体暗号化なので、iv / subsamples が空の情報を
        // 全サンプルで共有する
        let mut encryption_infos = TrafEncryptionInfos::Constant(SampleEncryptionInfo {
            iv: Vec::new(),
            subsamples: Vec::new(),
            kid: None,
        });
        apply_seig_sample_runs(
            &mut encryption_infos,
            seig_sample_runs.as_deref().unwrap_or_default(),
        )?;
        return Ok(encryption_infos);
    }

    let Some(track_encryption) = track_encryption else {
        // 保護されていないトラックの SAI は、`aux_info_type` が CENC 系でなければ
        // 本ライブラリの対象外として無視する (ISO/IEC 14496-12:2022 8.7.8 / 8.7.9)。
        // 汎用 SAI では `aux_info_type_parameter` が 0 以外になり得るため、
        // CENC 向けの厳密なデコードを先に行わず、生のペイロードから `aux_info_type` を読む
        let aux_info_types = [
            saiz_unknown_box.and_then(raw_aux_info_type),
            saio_unknown_box.and_then(raw_aux_info_type),
        ];
        if aux_info_types
            .iter()
            .flatten()
            .any(|aux_info_type| !crate::boxes_cenc::is_cenc_scheme(*aux_info_type))
        {
            return Ok(TrafEncryptionInfos::None);
        }
        return Err(DemuxError::DecodeError(Error::invalid_data(
            "sample auxiliary information requires a CENC protected sample entry",
        )));
    };
    if track_encryption.tenc_box.default_is_protected == 0 {
        return Err(DemuxError::DecodeError(Error::invalid_data(
            "selective encryption is not supported: sample auxiliary information exists for a track whose tenc default_is_protected is 0",
        )));
    }
    // `senc` があっても `saiz` / `saio` の妥当性は確認する
    let saiz_box = decode_optional_box::<SaizBox>(saiz_unknown_box)?;
    let saio_box = decode_optional_box::<SaioBox>(saio_unknown_box)?;
    if let Some(saiz_box) = &saiz_box {
        check_aux_info_type("saiz", saiz_box.aux_info_type, track_encryption.scheme_type)?;
    }
    if let Some(saio_box) = &saio_box {
        check_aux_info_type("saio", saio_box.aux_info_type, track_encryption.scheme_type)?;
        // `saiz` / `saio` を持つ `traf` は、SAI の位置を解決するための基準が必須である
        // (ISO/IEC 14496-12:2022 8.8.14)。`senc` を優先する場合も、base の無い入力は不正とする
        saio_base_offset(traf, moof_offset, file_offset)?;
    }

    let iv_size = usize::from(track_encryption.tenc_box.default_per_sample_iv_size);

    if let Some(senc_unknown_box) = senc_unknown_box {
        let senc_bytes = senc_unknown_box.encode_to_vec()?;
        let (senc_box, _) = SencBox::decode_with_iv_size(
            &senc_bytes,
            track_encryption.tenc_box.default_per_sample_iv_size,
        )?;
        if senc_box.samples.len() != sample_count {
            return Err(DemuxError::DecodeError(Error::invalid_data(format!(
                "senc sample count {} does not match the number of samples in traf ({sample_count})",
                senc_box.samples.len()
            ))));
        }
        // `senc` は KID を持たないため、解決済みの実効 KID を後から付ける
        let samples: Vec<SampleEncryptionInfo> = senc_box
            .samples
            .into_iter()
            .map(|sample| SampleEncryptionInfo {
                iv: sample.iv,
                subsamples: sample.subsamples,
                kid: None,
            })
            .collect();
        let mut encryption_infos = TrafEncryptionInfos::PerSample(samples);
        apply_seig_sample_runs(
            &mut encryption_infos,
            seig_sample_runs.as_deref().unwrap_or_default(),
        )?;
        return Ok(encryption_infos);
    }

    let (Some(saiz_box), Some(saio_box)) = (saiz_box, saio_box) else {
        return Err(DemuxError::DecodeError(Error::invalid_data(
            "sample auxiliary information requires both 'saiz' and 'saio' when 'senc' is absent",
        )));
    };

    let saiz_sample_count = usize::try_from(saiz_box.sample_count).map_err(|_| {
        DemuxError::DecodeError(Error::invalid_data("saiz sample count exceeds usize::MAX"))
    })?;
    if saiz_sample_count != sample_count {
        return Err(DemuxError::DecodeError(Error::invalid_data(format!(
            "saiz sample count {saiz_sample_count} does not match the number of samples in traf ({sample_count})"
        ))));
    }
    // `saiz` / `saio` は `sample_count` を宣言するだけで、SAI が 0 バイトのサンプルも表せる。
    // サンプル数分の `Vec` を確保する前に、入力サイズに比例しない巨大な確保にならないか検査する。
    // `senc` を持つ場合は `SencBox` のデコードが入力サイズから `sample_count` を制約するため対象外
    if sample_count > MAX_PER_SAMPLE_AUX_INFO_SAMPLES {
        return Err(DemuxError::DecodeError(Error::invalid_data(format!(
            "sample auxiliary information sample count {sample_count} exceeds the maximum {MAX_PER_SAMPLE_AUX_INFO_SAMPLES}"
        ))));
    }
    let sample_info_sizes: Vec<usize> = if saiz_box.default_sample_info_size == 0 {
        saiz_box
            .sample_info_sizes
            .iter()
            .map(|size| usize::from(*size))
            .collect()
    } else {
        vec![usize::from(saiz_box.default_sample_info_size); sample_count]
    };

    // `saio` の offset は `tfhd` の base data offset と同じ基準で解決する
    // (ISO/IEC 14496-12:2022 8.8.14)
    let base = saio_base_offset(traf, moof_offset, file_offset)?;
    let mut sample_starts: Vec<usize> = Vec::new();
    if saio_box.offsets.len() == 1 {
        // 1 個の offset は「全 trun の SAI が連続している」ことを意味する
        let mut cursor = saio_offset_position(base, saio_box.offsets[0])?;
        for size in &sample_info_sizes {
            sample_starts.push(cursor);
            cursor = cursor.checked_add(*size).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data(
                    "sample auxiliary information offset overflow",
                ))
            })?;
        }
    } else if saio_box.offsets.len() == traf.trun_boxes.len() {
        // 複数ある場合は trun ごとに 1 個の offset を持つ
        let mut sample_index = 0usize;
        for (run_index, trun) in traf.trun_boxes.iter().enumerate() {
            let mut cursor = saio_offset_position(base, saio_box.offsets[run_index])?;
            for _ in 0..trun_sample_count(trun) {
                let size = *sample_info_sizes.get(sample_index).ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data(
                        "saiz sample info sizes are shorter than the number of samples in traf",
                    ))
                })?;
                sample_starts.push(cursor);
                cursor = cursor.checked_add(size).ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data(
                        "sample auxiliary information offset overflow",
                    ))
                })?;
                sample_index += 1;
            }
        }
    } else {
        return Err(DemuxError::DecodeError(Error::invalid_data(format!(
            "saio offset count {} must be 1 or equal to the number of trun boxes ({})",
            saio_box.offsets.len(),
            traf.trun_boxes.len()
        ))));
    }

    let mut samples = Vec::new();
    for (sample_index, start) in sample_starts.iter().enumerate() {
        let size = *sample_info_sizes.get(sample_index).ok_or_else(|| {
            DemuxError::DecodeError(Error::invalid_data(
                "saiz sample info sizes are shorter than the number of samples in traf",
            ))
        })?;
        let end = start.checked_add(size).ok_or_else(|| {
            DemuxError::DecodeError(Error::invalid_data(
                "sample auxiliary information offset overflow",
            ))
        })?;
        if end > data.len() {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "sample auxiliary information exceeds the media segment",
            )));
        }
        let sample = parse_sample_aux_info(&data[*start..end], iv_size)?;
        samples.push(SampleEncryptionInfo {
            iv: sample.iv,
            subsamples: sample.subsamples,
            kid: None,
        });
    }
    let mut encryption_infos = TrafEncryptionInfos::PerSample(samples);
    apply_seig_sample_runs(
        &mut encryption_infos,
        seig_sample_runs.as_deref().unwrap_or_default(),
    )?;
    Ok(encryption_infos)
}

/// `saiz` / `saio` を持つ `traf` の SAI の基準位置を返す
///
/// `tfhd` に `base_data_offset` がある場合はその値、`default_base_is_moof` が true の場合は
/// moof の先頭を使う。どちらも無い場合は、SAI の位置を決められないため
/// [`crate::ErrorKind::InvalidData`] を返す (ISO/IEC 14496-12:2022 8.8.14)。
fn saio_base_offset(
    traf: &TrafBox,
    moof_offset: usize,
    file_offset: Option<u64>,
) -> Result<usize, DemuxError> {
    if let Some(explicit_offset) = traf.tfhd_box.base_data_offset {
        // ファイル全体を読む場合は、絶対値の base_data_offset をスライス先頭からの相対値に直す
        let explicit_offset = match file_offset {
            Some(file_offset) => explicit_offset.checked_sub(file_offset).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data(
                    "base_data_offset is before the media segment",
                ))
            })?,
            None => explicit_offset,
        };
        return usize::try_from(explicit_offset).map_err(|_| {
            DemuxError::DecodeError(Error::invalid_data("base_data_offset exceeds usize::MAX"))
        });
    }
    if traf.tfhd_box.default_base_is_moof {
        return Ok(moof_offset);
    }
    Err(DemuxError::DecodeError(Error::invalid_data(
        "saiz/saio require base_data_offset or default_base_is_moof in tfhd",
    )))
}

/// SAI の基準位置と `saio` の offset から、SAI の先頭位置を求める
fn saio_offset_position(base: usize, offset: u64) -> Result<usize, DemuxError> {
    let offset = usize::try_from(offset).map_err(|_| {
        DemuxError::DecodeError(Error::invalid_data("saio offset exceeds usize::MAX"))
    })?;
    base.checked_add(offset).ok_or_else(|| {
        DemuxError::DecodeError(Error::invalid_data("saio offset calculation overflow"))
    })
}

/// SAI のバイト列を CENC の SampleAuxiliaryDataFormat として読む
///
/// `sample_info_size <= Per_Sample_IV_Size` の場合は IV のみ、それより大きい場合は
/// IV の後ろに `subsample_count` とサブサンプルが続く
/// (ISO/IEC 23001-7:2023 7.2.1 の `SampleEncryption` と同じ形式)。
///
/// SAI は KID を持たないため [`SencSample`] を返す。実効 KID は `seig` サンプルグループの
/// 解決処理で後から付ける。
///
/// IV より短い入力、サブサンプル表が切り詰められた入力、末尾に余分なバイトがある入力は
/// [`crate::ErrorKind::InvalidData`] を返す。
fn parse_sample_aux_info(bytes: &[u8], iv_size: usize) -> Result<SencSample, DemuxError> {
    if bytes.len() < iv_size {
        return Err(DemuxError::DecodeError(Error::invalid_data(
            "sample auxiliary information is shorter than the IV size",
        )));
    }
    let iv = bytes[..iv_size].to_vec();
    let rest = &bytes[iv_size..];
    if rest.is_empty() {
        return Ok(SencSample {
            iv,
            subsamples: Vec::new(),
        });
    }

    let mut offset = 0;
    let subsample_count = u16::decode_at(rest, &mut offset)?;
    // サブサンプル表が残りに収まるかを先に確認する。
    // 個々のフィールドを読みながら判定すると、切り詰められた入力を
    // InsufficientBuffer として返してしまう (この関数は InvalidData を返す)。
    // 1 サブサンプルは bytes_of_clear_data (2 バイト) + bytes_of_protected_data (4 バイト)
    let remaining = rest.len().saturating_sub(offset);
    if usize::from(subsample_count) > remaining / 6 {
        return Err(DemuxError::DecodeError(Error::invalid_data(
            "sample auxiliary information has a truncated subsample table",
        )));
    }
    let mut subsamples = Vec::new();
    for _ in 0..subsample_count {
        subsamples.push(Subsample {
            bytes_of_clear_data: u16::decode_at(rest, &mut offset)?,
            bytes_of_protected_data: u32::decode_at(rest, &mut offset)?,
        });
    }
    if offset != rest.len() {
        return Err(DemuxError::DecodeError(Error::invalid_data(
            "sample auxiliary information has trailing bytes",
        )));
    }
    Ok(SencSample { iv, subsamples })
}

/// 型付きボックスとしてデコードする
fn decode_optional_box<B: Decode>(
    unknown_box: Option<&UnknownBox>,
) -> Result<Option<B>, DemuxError> {
    let Some(unknown_box) = unknown_box else {
        return Ok(None);
    };
    let bytes = unknown_box.encode_to_vec()?;
    let (decoded, _) = B::decode(&bytes)?;
    Ok(Some(decoded))
}

/// `saiz` / `saio` のペイロードから `aux_info_type` を、型付きボックスにデコードせずに読む
///
/// [`UnknownBox::payload`] はボックスヘッダーを除いた中身で、先頭 4 バイトが FullBox の
/// version と flags である。flags の `0x000001` が立っている場合だけ、その直後の
/// 4 バイトが `aux_info_type` になる (ISO/IEC 14496-12:2022 8.7.8 / 8.7.9)。
/// 保護されていないトラックの汎用 SAI を無視する判定にだけ使う。汎用 SAI では
/// `aux_info_type_parameter` が 0 以外になり得るため、CENC 向けの厳密なデコードを
/// 先に行うと、無視すべき入力をエラーにしてしまう。
fn raw_aux_info_type(unknown_box: &UnknownBox) -> Option<[u8; 4]> {
    // version (1 バイト) + flags (3 バイト) をまとめて読み、flags の部分だけを使う
    let full_box_header = u32::from_be_bytes(unknown_box.payload.get(0..4)?.try_into().ok()?);
    if full_box_header & 0x0000_0001 == 0 {
        return None;
    }
    unknown_box.payload.get(4..8)?.try_into().ok()
}

/// `aux_info_type` がトラックの保護スキームと一致するかを検査する
///
/// `aux_info_type` を省略した場合は、ISO/IEC 14496-12:2022 8.7.8 / 8.7.9 により
/// トラックの保護スキームの 4CC が既定値になる。
fn check_aux_info_type(
    box_name: &str,
    aux_info_type: Option<[u8; 4]>,
    scheme_type: [u8; 4],
) -> Result<(), DemuxError> {
    if let Some(aux_info_type) = aux_info_type
        && aux_info_type != scheme_type
    {
        return Err(DemuxError::DecodeError(Error::invalid_data(format!(
            "'{box_name}' aux_info_type '{}' does not match the track protection scheme '{}'",
            BoxType::Normal(aux_info_type),
            BoxType::Normal(scheme_type),
        ))));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        ErrorKind,
        boxes::{EncvBox, FrmaBox, SchiBox, SchmBox, SinfBox, VisualSampleEntryFields},
    };

    /// `from_sample_entry()` に渡す `Encv` サンプルエントリーを組み立てる
    ///
    /// `SinfBox` のデコードが拒否する組み合わせも作れるよう、`schm_box` と `tenc_box` は
    /// 個別に指定する。
    fn create_encv_sample_entry(
        schm_box: Option<SchmBox>,
        tenc_box: Option<TencBox>,
    ) -> SampleEntry {
        SampleEntry::Encv(EncvBox {
            visual: VisualSampleEntryFields {
                data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                width: 320,
                height: 240,
                horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
                vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
                frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
                compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
                depth: VisualSampleEntryFields::DEFAULT_DEPTH,
            },
            sinf_box: SinfBox {
                frma_box: FrmaBox {
                    data_format: *b"hev1",
                },
                schm_box,
                schi_box: Some(SchiBox {
                    tenc_box,
                    unknown_boxes: Vec::new(),
                }),
                unknown_boxes: Vec::new(),
            },
            unknown_boxes: Vec::new(),
        })
    }

    /// テスト用の `tenc` を組み立てる
    fn create_tenc_box() -> TencBox {
        TencBox {
            default_is_protected: 1,
            default_per_sample_iv_size: 16,
            default_kid: [0x11; 16],
            default_constant_iv: None,
            default_crypt_byte_block: None,
            default_skip_byte_block: None,
        }
    }

    /// `tenc` が無い CENC 以外の保護スキームのサンプルエントリーが Unsupported になること
    ///
    /// `SinfBox` のデコードは CENC 以外のスキームに `tenc` を要求しないため、
    /// この組み合わせは実際の入力から到達できる。平文として扱わないことを固定する。
    #[test]
    fn non_cenc_scheme_without_tenc_is_unsupported() {
        let sample_entry = create_encv_sample_entry(
            Some(SchmBox {
                scheme_type: *b"piff",
                scheme_version: 0x0001_0000,
                scheme_uri: None,
            }),
            None,
        );
        let err = TrackEncryption::from_sample_entry(&sample_entry)
            .expect_err("CENC 以外の保護スキームを平文として解釈してしまった");
        match err {
            DemuxError::DecodeError(error) => {
                assert_eq!(
                    error.kind,
                    ErrorKind::Unsupported,
                    "エラー種別が Unsupported ではない (reason={})",
                    error.reason,
                );
                assert!(
                    error.reason.contains("unsupported protection scheme"),
                    "保護スキームの検査とは別のエラーである (reason={})",
                    error.reason,
                );
            }
            other => panic!("DecodeError を期待したが {other:?} だった"),
        }
    }

    /// `schm` を持たずに `tenc` を持つサンプルエントリーが InvalidData になること
    ///
    /// `SinfBox` のデコードはこの組み合わせを拒否するが、デコードより後段でも
    /// 保護されている入力を平文として扱わないことを固定する。
    #[test]
    fn sample_entry_with_tenc_without_schm_is_invalid_data() {
        let sample_entry = create_encv_sample_entry(None, Some(create_tenc_box()));
        let err = TrackEncryption::from_sample_entry(&sample_entry)
            .expect_err("保護スキームの無い tenc を平文として解釈してしまった");
        match err {
            DemuxError::DecodeError(error) => {
                assert_eq!(
                    error.kind,
                    ErrorKind::InvalidData,
                    "エラー種別が InvalidData ではない (reason={})",
                    error.reason,
                );
                assert!(
                    error.reason.contains("requires a 'schm' box"),
                    "保護スキームの有無の検査とは別のエラーである (reason={})",
                    error.reason,
                );
            }
            other => panic!("DecodeError を期待したが {other:?} だった"),
        }
    }

    /// サブサンプル表が切り詰められた SAI が InvalidData になること
    ///
    /// `subsample_count` が残りのバイト数に対して大きすぎる場合、個々のフィールドを
    /// 読みながら判定すると InsufficientBuffer が返ってしまう。関数の契約どおり
    /// InvalidData を返すことを固定する。
    #[test]
    fn parse_sample_aux_info_truncated_subsample_table_is_invalid_data() {
        // IV 8 バイト + subsample_count 2 + サブサンプル 1 個分 (6 バイト)
        let mut sai = alloc::vec![0x11; 8];
        sai.extend_from_slice(&2u16.to_be_bytes());
        sai.extend_from_slice(&[0x00, 0x05, 0x00, 0x00, 0x00, 0x0A]);

        let err = parse_sample_aux_info(&sai, 8)
            .expect_err("サブサンプル表が切り詰められた SAI を解釈してしまった");
        match err {
            DemuxError::DecodeError(error) => {
                assert_eq!(
                    error.kind,
                    ErrorKind::InvalidData,
                    "エラー種別が InvalidData ではない (reason={})",
                    error.reason,
                );
                assert!(
                    error.reason.contains("truncated subsample table"),
                    "サブサンプル表の検査とは別のエラーである (reason={})",
                    error.reason,
                );
            }
            other => panic!("DecodeError を期待したが {other:?} だった"),
        }
    }
}
