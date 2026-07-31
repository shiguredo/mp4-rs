//! MP4 ファイルのマルチプレックス実装を提供するモジュール
//!
//! このモジュールは、複数のメディアトラック（音声・映像・字幕）からのサンプルを
//! 時系列順に統合して、MP4 ファイルを生成するための機能を提供する。
//!
//! # Examples
//!
//! 基本的なワークフロー例：
//!
//! ```no_run
//! use std::fs::File;
//! use std::io::{Write, Seek, SeekFrom};
//! use std::num::NonZeroU32;
//! use std::time::Duration;
//!
//! use shiguredo_mp4::mux::{Mp4FileMuxer, Sample};
//! use shiguredo_mp4::TrackKind;
//!
//! # fn main() -> Result<(), Box<dyn 'static + std::error::Error>> {
//! let mut muxer = Mp4FileMuxer::new()?;
//!
//! // 初期ボックス情報を出力ファイルに書きこむ
//! let initial_bytes = muxer.initial_boxes_bytes();
//! let mut file = File::create("output.mp4")?;
//! file.write_all(initial_bytes)?;
//!
//! // サンプルを追加
//! // => データをファイルに追記してから、それをマルチプレクサーに伝える
//! let sample_data = vec![0; 1024];
//! file.write_all(&sample_data)?;
//!
//! let sample_entry = todo!("build a sample entry for the codec being used");
//! let sample = Sample {
//!     track_kind: TrackKind::Video,
//!     sample_entry: Some(sample_entry),
//!     keyframe: true,
//!     timescale: NonZeroU32::MIN.saturating_add(30 - 1),
//!     duration: 1,
//!     composition_time_offset: None,
//!     data_offset: initial_bytes.len() as u64,
//!     data_size: sample_data.len(),
//! };
//! muxer.append_sample(&sample)?;
//!
//! // マルチプレックス処理を完了
//! let finalized = muxer.finalize()?;
//!
//! // ファイナライズ後のボックス情報をファイルに書きこむ
//! for (offset, bytes) in finalized.offset_and_bytes_pairs() {
//!     file.seek(SeekFrom::Start(offset))?;
//!     file.write_all(bytes)?;
//! }
//! # Ok(())
//! # }
//! ```
use alloc::{vec, vec::Vec};
use core::{num::NonZeroU32, time::Duration};

use crate::{
    BoxHeader, BoxSize, Either, Encode, Error, FixedPointNumber, LanguageCode, Mp4FileTime,
    TrackKind, Utf8String,
    boxes::{
        Brand, Co64Box, CttsBox, CttsEntry, DinfBox, FreeBox, FtypBox, HdlrBox, MdatBox, MdhdBox,
        MdiaBox, MediaHeader, MinfBox, MoovBox, MvhdBox, SampleEntry, StblBox, StcoBox, StscBox,
        StscEntry, StsdBox, StssBox, StszBox, SttsBox, TkhdBox, TrakBox, VmhdBox,
    },
    mux_fmp4_segment::{TrakDerivation, derive_trak_attributes, subtitle_trak_attributes},
};

// ftyp 更新時に必要となる free 予約の最小値は以下:
// - 追加ブランド最大 4 個 × 4 bytes = 16 bytes
// - free ボックス再構築に必要な最小ヘッダーサイズ = 8 bytes
// => 最低 24 bytes
//
// ここでは将来のブランド追加やレイアウト変更に備えて余裕を持たせ、64 bytes を予約する。
const RESERVED_FTYP_UPDATE_FREE_PAYLOAD_SIZE: usize = 64;

/// MP4 ファイルの moov ボックスの最大サイズを見積もる
///
/// [`Mp4FileMuxerOptions::reserved_moov_box_size`] に設定する値を簡易的に決定するために使用できる関数。
/// トラックごとのサンプル数から、faststart 形式で必要なメタデータ領域を概算で計算する。
pub fn estimate_maximum_moov_box_size(sample_count_per_track: &[usize]) -> usize {
    // moov ボックスの基本的なオーバーヘッド（mvhd_box とボックスヘッダーなど）
    const BASE_MOOV_OVERHEAD: usize = 512;

    // トラックあたりのオーバーヘッド（tkhd_box、mdia_box など）
    const PER_TRACK_OVERHEAD: usize = 1024;

    // サンプルあたりの概算バイト数：
    // - stts_box（時間-サンプル）: エントリあたり ~8 バイト
    // - stsc_box（サンプル-チャンク）: チャンクあたり ~12 バイト（通常はサンプルより少ない）
    // - stsz_box（サンプルサイズ）: サンプルあたり ~4 バイト
    // - stss_box（同期サンプル）: キーフレームあたり ~4 バイト
    //   （全サンプルがキーフレームの場合は stss_box 自体が省略されるため、
    //   最悪ケースは 1 サンプルだけが非キーフレームのとき）
    // - stco_box/co64_box（チャンクオフセット）: チャンクあたり ~8 バイト
    const BYTES_PER_SAMPLE: usize = 16;

    BASE_MOOV_OVERHEAD
        + (sample_count_per_track.len() * PER_TRACK_OVERHEAD)
        + (sample_count_per_track.iter().sum::<usize>() * BYTES_PER_SAMPLE)
}

/// トラック単位のメタデータ（`mdhd.language` / `hdlr.name`）
///
/// プレイヤーが複数トラックの一覧をユーザーに提示して選択させる際の
/// 主要な表示情報になる（特に字幕トラックで意味を持つ）。
///
/// [`Mp4FileMuxerOptions`] と [`crate::mux::SegmentMuxerOptions`] の両方から
/// 参照される共有型。両 muxer 共通の型だが、既存の [`Sample`] / [`MuxError`] と
/// 同じく `mux_mp4_file` に定義し、[`crate::mux::Fmp4SegmentMuxer`] 側は
/// `use` で取り込む先例に倣う。
///
/// [`Default`] は「未指定相当」の値（`und` + 空文字列）を返し、
/// 生成される MP4 のバイト列は本フィールド追加前と一致する
#[derive(Debug, Clone, Default)]
pub struct TrackMetadata {
    /// 未指定時は [`LanguageCode::UNDEFINED`]（`*b"und"`）
    pub language: LanguageCode,

    /// 未指定時は空文字列
    ///
    /// muxer は `HdlrBox::name` に「末尾 null 1 バイト付き UTF-8」として書き出す
    pub name: Utf8String,
}

/// [`Mp4FileMuxer`] 用のオプション
#[derive(Debug, Clone, Default)]
pub struct Mp4FileMuxerOptions {
    /// faststart 形式用に事前に確保する moov ボックスのサイズ（バイト単位）
    ///
    /// faststart とは、MP4 ファイルの再生に必要なメタデータを含む moov ボックスを
    /// ファイルの先頭付近に配置する形式である。
    /// これにより、動画プレイヤーが再生を開始する際に、ファイル末尾へのシークを行ったり、
    /// ファイル全体をロードする必要がなくなり、再生開始までの時間が短くなることが期待できる。
    ///
    /// なお、実際の moov ボックスのサイズがここで指定した値よりも大きい場合は、
    /// moov ボックスはファイル末尾に配置され、faststart 形式は無効になる。
    ///
    /// デフォルト値は 0（faststart は常に無効となる）。
    //
    // [NOTE]
    // ftyp 更新用の余白（`RESERVED_FTYP_UPDATE_FREE_PAYLOAD_SIZE`）は常に確保されるため、
    // `reserved_moov_box_size = 0` でも理論上は非常に小さい moov なら faststart になる余地がある。
    // ただし実際の moov は通常もっと大きくなるため、実運用ではほぼ該当しない。
    pub reserved_moov_box_size: usize,

    /// ファイル作成時刻（構築される MP4 ファイル内のメタデータとして使われる）
    ///
    /// デフォルト値は UNIX エポック（1970年1月1日 00:00:00 UTC）
    pub creation_timestamp: Duration,

    /// 音声トラックのメタデータ（`mdhd.language` / `hdlr.name`）
    ///
    /// 現状は同じ `TrackKind` の全トラックに共通の値が適用される
    /// （同一 `TrackKind` に複数トラックを追加した場合、両方に同じメタデータが刺さる）。
    /// トラックごとの個別指定は将来の対応
    pub audio_track: TrackMetadata,

    /// 映像トラックのメタデータ（`mdhd.language` / `hdlr.name`）
    ///
    /// 同一 `TrackKind` 内での扱いは [`Self::audio_track`] を参照
    pub video_track: TrackMetadata,

    /// 字幕トラックのメタデータ（`mdhd.language` / `hdlr.name`）
    ///
    /// 同一 `TrackKind` 内での扱いは [`Self::audio_track`] を参照
    pub subtitle_track: TrackMetadata,
}

impl Mp4FileMuxerOptions {
    /// [`TrackKind`] に対応するトラックメタデータを返す
    pub(crate) fn track_metadata(&self, kind: TrackKind) -> &TrackMetadata {
        match kind {
            TrackKind::Audio => &self.audio_track,
            TrackKind::Video => &self.video_track,
            TrackKind::Subtitle => &self.subtitle_track,
        }
    }
}

/// [`Mp4FileMuxer::finalize()`] の結果として得られる、MP4 ファイル構築の完了に必要なボックス情報
#[derive(Debug, Clone)]
pub struct FinalizedBoxes {
    head_boxes_bytes: Vec<u8>,
    moov_box_offset: u64,
    moov_box_bytes: Vec<u8>,
    mdat_box_offset: u64,
    mdat_box_header_bytes: Vec<u8>,
    moov_box: MoovBox,
}

impl FinalizedBoxes {
    /// 構築された MP4 ファイルで faststart が有効になっているかどうかを返す
    pub fn is_faststart_enabled(&self) -> bool {
        self.moov_box_offset < self.mdat_box_offset
    }

    /// 最終的な moov ボックスのサイズを返す（バイト単位）
    pub fn moov_box_size(&self) -> usize {
        self.moov_box_bytes.len()
    }

    /// MP4 ファイルの構築を完了するために、ファイルに書きこむべきボックスのオフセットとバイト列の組を返す
    pub fn offset_and_bytes_pairs(&self) -> impl Iterator<Item = (u64, &[u8])> {
        [
            Some((0, self.head_boxes_bytes.as_slice())),
            (self.moov_box_offset >= self.mdat_box_offset)
                .then_some((self.moov_box_offset, self.moov_box_bytes.as_slice())),
            Some((self.mdat_box_offset, self.mdat_box_header_bytes.as_slice())),
        ]
        .into_iter()
        .flatten()
    }

    /// 構築された moov ボックスを返す
    pub fn moov_box(&self) -> &MoovBox {
        &self.moov_box
    }
}

/// MP4 ファイルに追加するメディアサンプル
///
/// 字幕トラック（[`TrackKind::Subtitle`]）では
/// [`Self::composition_time_offset`] = [`None`] を推奨する。
#[derive(Debug, Clone)]
pub struct Sample {
    /// サンプルのトラック種別
    pub track_kind: TrackKind,

    /// サンプルの詳細情報（コーデック種別など）
    ///
    /// 最初のサンプルでは必須。以降は省略可能で、
    /// 省略した場合は前のサンプルと同じ sample_entry が使用される
    pub sample_entry: Option<SampleEntry>,

    /// キーフレーム（同期サンプル）かどうか
    ///
    /// 音声・字幕では各サンプルが独立してデコード可能なのが通例のため、`true` を指定するのが正規である。
    ///
    /// [`Mp4FileMuxer`] では、この値が `stss` ボックスの生成に使われる。
    ///
    /// - トラック内の全サンプルが `true` の場合、`stss` は省略される（全サンプルが同期サンプル）
    /// - 一部だけ `true` の場合、`true` のサンプル番号だけが `stss` に列挙される
    /// - 全サンプルが `false` の場合:
    ///   - 音声・字幕: `stss` を省略する（全サンプル同期として扱う）
    ///   - 映像: [`Mp4FileMuxer::finalize()`] が [`MuxError::NoSyncSamples`] を返す
    ///
    /// [`crate::mux::Fmp4SegmentMuxer`] では `stss` は使わず、
    /// `trun` の `SampleFlags` および `sidx` の SAP 判定に使われる。
    /// 映像の全サンプルが `false` でも [`MuxError::NoSyncSamples`] にはならない。
    pub keyframe: bool,

    /// サンプルのタイムスケール（時間単位）
    ///
    /// `duration` フィールドの値は、このタイムスケール単位での長さを表す
    ///
    /// # Examples
    ///
    /// - 映像サンプル（30 fps）: `timescale = 30` なら `duration = 1` は 1/30 秒
    /// - 音声サンプル（48 kHz）: `timescale = 48000` なら `duration = 1920` は 1920/48000 秒
    ///
    /// # NOTE
    ///
    /// 同じトラック内のすべてのサンプルは同じタイムスケール値を使用する必要がある
    ///
    /// 異なるタイムスケール値を指定すると
    /// [`Mp4FileMuxer::append_sample()`] 呼び出し時に [`MuxError::TimescaleMismatch`] エラーが発生する
    pub timescale: NonZeroU32,

    /// サンプルの尺（タイムスケール単位）
    ///
    /// # NOTE
    ///
    /// MP4 ではサンプルのタイムスタンプを直接指定する方法がなく、
    /// あるサンプルのタイムスタンプは「それ以前のサンプルの尺の累積」として表現される。
    ///
    /// そのため、映像および音声サンプルの冒頭ないし途中でタイムスタンプのギャップが発生する場合には
    /// 利用側で以下のような対処が求められる:
    /// - 映像:
    ///   - 黒画像などを生成してギャップ分を補完するか、サンプルの尺を調整する
    ///   - たとえば、ギャップが発生した直前のサンプルの尺にギャップ期間分を加算する
    /// - 音声:
    ///   - 無音などを生成してギャップ分を補完する
    ///   - 音声はサンプルデータに対する尺の長さが固定なので、映像のように MP4 レイヤーで尺の調整はできない
    ///
    /// なお、MP4 の枠組みでもギャップを表現するためのボックスは存在するが
    /// プレイヤーの対応がまちまちであるため [`Mp4FileMuxer`] では現状サポートしておらず、
    /// 上述のような個々のプレイヤーの実装への依存性が低い方法を推奨している。
    pub duration: u32,

    /// コンポジション時間オフセット（トラックのタイムスケール単位）
    ///
    /// B フレームを含む映像などで PTS と DTS がずれる場合に指定する。
    /// 値の意味は `PTS = DTS + composition_time_offset` である。
    ///
    /// [`Mp4FileMuxer`] では `ctts` ボックスの生成に使われる。
    /// [`crate::mux::Fmp4SegmentMuxer`] では `trun` の
    /// `sample_composition_time_offset` の生成に使われる。
    ///
    /// 公開 API では demuxer と揃えて `i64` で受け取るが、
    /// 実際に MP4 / fMP4 のボックスへ書ける範囲はより狭い。
    ///
    /// - [`Mp4FileMuxer`]:
    ///   - 負値は `i32::MIN ..= -1`
    ///   - 非負値は `0 ..= u32::MAX`
    /// - [`crate::mux::Fmp4SegmentMuxer`]:
    ///   - `i32::MIN ..= i32::MAX`
    ///
    /// 上記の範囲を超える値を指定した場合、mux 時にエラーになる。
    pub composition_time_offset: Option<i64>,

    /// ファイル内におけるサンプルデータの開始位置（バイト単位）
    pub data_offset: u64,

    /// サンプルデータのサイズ（バイト単位）
    pub data_size: usize,
}

/// マルチプレックス処理中に発生するエラー
pub enum MuxError {
    /// MP4 ボックスのエンコード処理中に発生したエラー
    EncodeError(Error),

    /// まだトラックが観測されていない
    EmptyTracks,

    /// サンプルが指定されていない
    EmptySamples,

    /// ファイルポジションの不一致
    PositionMismatch {
        /// 期待されたポジション
        expected: u64,

        /// 実際のポジション
        actual: u64,
    },

    /// 必須の sample_entry が欠落している
    MissingSampleEntry {
        /// サンプルエントリーが不在であるトラック種別
        track_kind: TrackKind,
    },

    /// マルチプレックスが既にファイナライズ済み
    AlreadyFinalized,

    /// 同じトラック内のタイムスケール値の不一致
    TimescaleMismatch {
        /// 不一致が発生したトラック種別
        track_kind: TrackKind,

        /// 期待されたタイムスケール
        expected: NonZeroU32,

        /// 実際に提供されたタイムスケール
        actual: NonZeroU32,
    },

    /// 同一トラック内に両立しないサンプルエントリーが混在している
    ///
    /// [`crate::mux::Fmp4SegmentMuxer`] では、1 つのセグメント内の同一トラックで
    /// 複数のサンプルエントリーが使われた場合に返す。
    ///
    /// [`Mp4FileMuxer`] では、字幕トラック内でハンドラー種別ないしメディアヘッダーが
    /// 異なるサンプルエントリー（たとえば `stpp` と `tx3g`）が混在した場合に返す。
    /// これらはトラック単位で 1 つしか持てないため、混在すると規格上整合しない MP4 になる
    MixedSampleEntries {
        /// 対象トラックの種類
        track_kind: TrackKind,
    },

    /// トラックに同期サンプルが 1 つも存在しない
    ///
    /// [`Mp4FileMuxer::finalize()`] 時、映像トラックの全サンプルが `keyframe = false` のときに返す。
    /// エントリー 0 個の `stss`（同期サンプルなし）を出力する代わりに拒否する。
    /// 音声・字幕では同条件でも `stss` を省略して全サンプル同期として扱うため、このエラーにはならない
    NoSyncSamples {
        /// 同期サンプルが無いトラック種別
        track_kind: TrackKind,
    },

    /// マルチプレックス処理中の内部カウンタがオーバーフローした
    Overflow,
}

impl From<Error> for MuxError {
    fn from(error: Error) -> Self {
        MuxError::EncodeError(error)
    }
}

impl core::fmt::Debug for MuxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self}")
    }
}

impl core::fmt::Display for MuxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MuxError::EncodeError(error) => {
                write!(f, "Failed to encode MP4 box: {error}")
            }
            MuxError::EmptyTracks => write!(f, "No tracks have been observed yet"),
            MuxError::EmptySamples => write!(f, "No samples in segment"),
            MuxError::PositionMismatch { expected, actual } => {
                write!(
                    f,
                    "Position mismatch: expected {expected}, but got {actual}"
                )
            }
            MuxError::MissingSampleEntry { track_kind } => {
                write!(
                    f,
                    "Missing sample entry for first sample of {track_kind:?} track",
                )
            }
            MuxError::AlreadyFinalized => {
                write!(f, "Muxer has already been finalized")
            }
            MuxError::TimescaleMismatch {
                track_kind,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Timescale mismatch for {track_kind:?} track: expected {expected}, but got {actual}",
                )
            }
            MuxError::MixedSampleEntries { track_kind } => {
                write!(f, "{track_kind:?} track uses incompatible sample entries")
            }
            MuxError::NoSyncSamples { track_kind } => {
                write!(f, "{track_kind:?} track has no sync samples")
            }
            MuxError::Overflow => write!(f, "Internal counter overflow"),
        }
    }
}

impl core::error::Error for MuxError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        if let MuxError::EncodeError(error) = self {
            Some(error)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
struct SampleMetadata {
    duration: u32,
    keyframe: bool,
    size: u32,
    composition_time_offset: Option<i64>,
}

#[derive(Debug, Clone)]
struct Chunk {
    offset: u64,
    sample_entry: SampleEntry,
    samples: Vec<SampleMetadata>,
}

/// トラック種別ごとの [`Chunk`] 群とタイムスケールをまとめた内部エントリ
///
/// `mux_fmp4_segment` の同名 private 型と概念は同じだが、`Mp4FileMuxer` は
/// フラグメント固有のフィールド（`decode_time` / `current_sample_entry_index`）を持たない。
/// また `track_id` とサンプルエントリー一覧は保持せず、
/// それぞれ `build_moov_box` と `build_stbl_box` が構築時に `chunks` から導出する。
/// そのため両者は共通化せず、モジュール private の別型として定義する
#[derive(Debug, Clone)]
struct TrackEntry {
    track_kind: TrackKind,
    timescale: NonZeroU32,
    chunks: Vec<Chunk>,
}

/// MP4 ファイルを生成するマルチプレックス処理を行うための構造体
///
/// この構造体は、複数のメディアトラック（音声・映像・字幕）からのサンプルを
/// 時系列順に統合して、MP4 ファイルを生成するための主要な処理を行う。
///
/// 基本的な使用フロー：
/// 1. [`new()`](Self::new) または [`with_options()`](Self::with_options) でインスタンスを作成
/// 2. [`initial_boxes_bytes()`](Self::initial_boxes_bytes) で得られたバイト列をファイルに書きこむ
/// 3. [`append_sample()`](Self::append_sample) でサンプルを追加
/// 4. [`finalize()`](Self::finalize) でマルチプレックス処理を完了する
///
/// なお、この構造体自体はファイル書き込みなどの I/O 操作は行わず、
/// そのために必要な情報を提供するだけとなっている（I/O 操作を行うのは利用側の責務）。
///
/// また、この構造体の目的は「MP4 ファイル構築の典型的なユースケースをカバーして簡単に行えるようにすること」であり、
/// 細かい制御は行えないようになっている。
/// もし構築する MP4 ファイルの細部までコントロールしたい場合には、この構造体経由ではなく、
/// 利用側で MP4 ボックス群を直接構築することを推奨する。
#[derive(Debug, Clone)]
pub struct Mp4FileMuxer {
    options: Mp4FileMuxerOptions,
    initial_boxes_bytes: Vec<u8>,
    mdat_box_offset: u64,
    next_position: u64,
    last_sample_kind: Option<TrackKind>,
    finalized_boxes: Option<FinalizedBoxes>,
    tracks: Vec<TrackEntry>,
}

impl Mp4FileMuxer {
    /// [`Mp4FileMuxer`] インスタンスを生成する
    pub fn new() -> Result<Self, MuxError> {
        Self::with_options(Mp4FileMuxerOptions::default())
    }

    /// 指定したオプションで [`Mp4FileMuxer`] インスタンスを生成する
    pub fn with_options(options: Mp4FileMuxerOptions) -> Result<Self, MuxError> {
        let mut this = Self {
            options,
            initial_boxes_bytes: Vec::new(),
            mdat_box_offset: 0,
            next_position: 0,
            last_sample_kind: None,
            finalized_boxes: None,
            tracks: Vec::new(),
        };
        this.build_initial_boxes()?;
        Ok(this)
    }

    fn build_initial_boxes(&mut self) -> Result<(), MuxError> {
        // ftyp ボックスを構築
        let ftyp_box = FtypBox {
            major_brand: Brand::ISOM,
            minor_version: 0,
            compatible_brands: vec![Brand::ISOM, Brand::ISO2, Brand::MP41],
        };

        // ftyp ボックスをヘッダーバイト列に追加
        self.initial_boxes_bytes = ftyp_box.encode_to_vec()?;

        // ftyp 更新用の余白と moov 用の予約領域を、共有 free ボックスとして確保する
        // （finalize 時に実際の利用状況に合わせて先頭領域を書き換える）
        let shared_free_payload_size = self
            .options
            .reserved_moov_box_size
            .saturating_add(RESERVED_FTYP_UPDATE_FREE_PAYLOAD_SIZE);
        let free_box = FreeBox {
            payload: vec![0; shared_free_payload_size],
        };
        self.initial_boxes_bytes
            .extend_from_slice(&free_box.encode_to_vec()?);

        self.mdat_box_offset = self.initial_boxes_bytes.len() as u64;

        // 可変長の mdat ボックスのヘッダーを書きこむ
        //
        // [NOTE]
        // mdat ボックスのペイロードサイズが 4 GB を越えても大丈夫なように
        // 常に `BoxSize::LARGE_VARIABLE_SIZE` を使用している
        //
        // 初期化時には `BoxSize::VARIABLE_SIZE` を使用して、ファイナライズの時に
        // 実際のペイロードサイズに応じて mdat ヘッダーの領域を調整することも可能ではあるが、
        // 処理が複雑になる割にサイズ的なメリットが薄い（4 バイト削減できるかどうか）ので、
        // ここではシンプルな方法を採用している
        let mdat_box_header = BoxHeader::new(MdatBox::TYPE, BoxSize::LARGE_VARIABLE_SIZE);
        self.initial_boxes_bytes
            .extend_from_slice(&mdat_box_header.encode_to_vec()?);

        // サンプルのデータが mdat ボックスに追記されていくように、ポジションを更新
        self.next_position = self.initial_boxes_bytes.len() as u64;

        Ok(())
    }

    /// 構築する MP4 ファイルに含まれる初期ボックス群を表すバイト列を取得する
    ///
    /// 利用側は [`Mp4FileMuxer::append_sample()`] を呼び出す前に、このメソッドが返す内容で
    /// 出力先を初期化しておく必要がある
    pub fn initial_boxes_bytes(&self) -> &[u8] {
        &self.initial_boxes_bytes
    }

    /// サンプルデータ以外のバイト列（fMP4 フラグメントヘッダ等）のサイズ分だけ
    /// 内部の書き込み位置を進める
    ///
    /// OBS の Hybrid MP4 のように、サンプルデータの間に moof / mdat ヘッダなどの
    /// 非サンプルデータが挿入される場合に使用する。
    ///
    /// `size` が 0 より大きい場合は、次の [`append_sample()`](Self::append_sample) 呼び出し時に
    /// 強制的に新しいチャンクが開始される。
    /// これは、非サンプルデータの挿入によりチャンク内のサンプルデータの連続性が
    /// 失われるためである。
    ///
    /// `size` が 0 の場合は何も行わない。
    pub fn advance_position(&mut self, size: u64) -> Result<(), MuxError> {
        if self.finalized_boxes.is_some() {
            return Err(MuxError::AlreadyFinalized);
        }
        self.next_position = self
            .next_position
            .checked_add(size)
            .ok_or(MuxError::Overflow)?;
        if size > 0 {
            self.last_sample_kind = None;
        }
        Ok(())
    }

    /// 映像・音声・字幕サンプルのデータを MP4 ファイルに追記したことを [`Mp4FileMuxer`] に通知する
    ///
    /// 実際のデータ追記処理自体は利用側の責務であり、
    /// このメソッド目的は、その追記結果などを伝えることで、
    /// [`Mp4FileMuxer`] が適切に、MP4ファイルの再生に必要なメタデータを構築できるようにすることである。
    ///
    /// # エラー返却時の内部状態
    ///
    /// エラーを返した場合も内部状態は変わらない。
    /// 呼び出し側は内容を補正したうえで再呼び出しできる。
    pub fn append_sample(&mut self, sample: &Sample) -> Result<(), MuxError> {
        if self.finalized_boxes.is_some() {
            return Err(MuxError::AlreadyFinalized);
        }
        if self.next_position != sample.data_offset {
            return Err(MuxError::PositionMismatch {
                expected: self.next_position,
                actual: sample.data_offset,
            });
        }

        let metadata = SampleMetadata {
            duration: sample.duration,
            keyframe: sample.keyframe,
            size: u32::try_from(sample.data_size).map_err(|_| {
                MuxError::EncodeError(Error::invalid_data("sample data size exceeds u32::MAX"))
            })?,
            composition_time_offset: sample.composition_time_offset,
        };

        let is_new_chunk_needed = self.is_new_chunk_needed(sample);

        // 新規チャンクが必要な場合のみサンプルエントリーを解決する
        // （不要ならスキップして self.tracks への副作用を発生させない）
        let resolved_sample_entry = if is_new_chunk_needed {
            let entry = sample
                .sample_entry
                .clone()
                .or_else(|| {
                    self.tracks
                        .iter()
                        .find(|t| t.track_kind == sample.track_kind)
                        .and_then(|t| t.chunks.last().map(|c| c.sample_entry.clone()))
                })
                .ok_or(MuxError::MissingSampleEntry {
                    track_kind: sample.track_kind,
                })?;
            Some(entry)
        } else {
            None
        };

        // 字幕トラックの hdlr / media_header は先頭のサンプルエントリーだけで決まるため、
        // 対応表上の組が異なるサンプルエントリーが混在すると stsd と矛盾した trak になる。
        // finalize() の時点では「どのサンプルが原因か」を示せないので、投入時点で拒否する。
        // 映像トラックは複数のサンプルエントリー（解像度違いなど）を前提とした設計なので対象外
        if sample.track_kind == TrackKind::Subtitle
            && let Some(new_entry) = &resolved_sample_entry
            && let Some(first_entry) = self
                .tracks
                .iter()
                .find(|t| t.track_kind == TrackKind::Subtitle)
                .and_then(|t| t.chunks.first())
                .map(|c| &c.sample_entry)
            && subtitle_trak_attributes(first_entry) != subtitle_trak_attributes(new_entry)
        {
            return Err(MuxError::MixedSampleEntries {
                track_kind: TrackKind::Subtitle,
            });
        }

        // tracks への副作用より先に Overflow を確定させ、エラー時に内部状態を不変に保つ
        let next_position = self
            .next_position
            .checked_add(sample.data_size as u64)
            .ok_or(MuxError::Overflow)?;

        // サンプルエントリーの解決を先に済ませてから ensure_track_entry を呼ぶことで
        // MissingSampleEntry エラー時に self.tracks を完全に不変に保つ。
        // ここより後に ? 付きの失敗経路を足すと、TrackEntry push 後にエラー返却できてしまい
        // 「エラー時は内部状態不変」の契約が壊れる
        let track_index = self.ensure_track_entry(sample.track_kind, sample.timescale)?;

        if let Some(sample_entry) = resolved_sample_entry {
            self.tracks[track_index].chunks.push(Chunk {
                offset: sample.data_offset,
                sample_entry,
                samples: Vec::new(),
            });
        }

        self.tracks[track_index]
            .chunks
            .last_mut()
            .expect("bug")
            .samples
            .push(metadata);

        self.next_position = next_position;
        self.last_sample_kind = Some(sample.track_kind);
        Ok(())
    }

    /// 指定した [`TrackKind`] の [`TrackEntry`] の位置を返す（無ければ新規に追加する）
    ///
    /// 既存の entry がある場合は `timescale` の一致を検証し、不一致なら
    /// [`MuxError::TimescaleMismatch`] を返す
    fn ensure_track_entry(
        &mut self,
        track_kind: TrackKind,
        timescale: NonZeroU32,
    ) -> Result<usize, MuxError> {
        if let Some(index) = self.tracks.iter().position(|t| t.track_kind == track_kind) {
            let track = &self.tracks[index];
            if track.timescale != timescale {
                return Err(MuxError::TimescaleMismatch {
                    track_kind,
                    expected: track.timescale,
                    actual: timescale,
                });
            }
            return Ok(index);
        }
        self.tracks.push(TrackEntry {
            track_kind,
            timescale,
            chunks: Vec::new(),
        });
        Ok(self.tracks.len() - 1)
    }

    fn is_new_chunk_needed(&self, sample: &Sample) -> bool {
        if self.last_sample_kind != Some(sample.track_kind) {
            return true;
        }

        let Some(sample_entry) = &sample.sample_entry else {
            return false;
        };

        // 上の早期リターンを通過している時点で該当トラックは必ず存在するが、
        // 型システム上は Option を経由する。
        // 仮に存在しなかったとしても「まだチャンクが無い = 新規チャンクが必要」が正しいので、
        // 既定値には true を使う
        self.tracks
            .iter()
            .find(|t| t.track_kind == sample.track_kind)
            .map(|t| {
                t.chunks
                    .last()
                    .is_none_or(|c| c.sample_entry != *sample_entry)
            })
            .unwrap_or(true)
    }

    /// すべてのサンプルの追加が完了したことを [`Mp4FileMuxer`] に通知する
    ///
    /// このメソッドが呼び出されると、[`Mp4FileMuxer`] はそれまでの情報を用いて、
    /// MP4 ファイルの再生に必要な修正やメタデータの構築を行う。
    ///
    /// 映像トラックの全サンプルが `keyframe = false` の場合は
    /// [`MuxError::NoSyncSamples`] を返す。
    ///
    /// 利用側は、このメソッドが返した結果を、出力先に反映する必要がある。
    pub fn finalize(&mut self) -> Result<&FinalizedBoxes, MuxError> {
        if self.finalized_boxes.is_some() {
            return Err(MuxError::AlreadyFinalized);
        }

        // moov ボックスを構築
        let moov_box = self.build_moov_box()?;
        let moov_box_bytes = moov_box.encode_to_vec()?;
        let ftyp_box_bytes = self.build_final_ftyp_box().encode_to_vec()?;
        let (head_boxes_bytes, moov_box_offset) =
            self.build_head_boxes_bytes(&ftyp_box_bytes, &moov_box_bytes)?;

        // mdat ボックスヘッダーのサイズ部分を確定する
        let mdat_box_size = self.next_position - self.mdat_box_offset;
        let mdat_box_header = BoxHeader::new(MdatBox::TYPE, BoxSize::U64(mdat_box_size));
        let mdat_box_header_bytes = mdat_box_header.encode_to_vec()?;

        self.finalized_boxes = Some(FinalizedBoxes {
            head_boxes_bytes,
            moov_box_offset,
            moov_box_bytes,
            mdat_box_offset: self.mdat_box_offset,
            mdat_box_header_bytes,
            moov_box,
        });

        Ok(self.finalized_boxes.as_ref().expect("infallible"))
    }

    /// ファイナライズされたボックス情報を取得する
    ///
    /// ファイナライズ結果を後から取得したい時のためのメソッド。
    /// [`Mp4FileMuxer::finalize()`] の呼び出し前は `None` が返される。
    pub fn finalized_boxes(&self) -> Option<&FinalizedBoxes> {
        self.finalized_boxes.as_ref()
    }

    fn build_final_ftyp_box(&self) -> FtypBox {
        let mut has_avc1 = false;
        let mut has_hev1 = false;
        let mut has_hvc1 = false;
        let mut has_av01 = false;

        for track in &self.tracks {
            for chunk in &track.chunks {
                match chunk.sample_entry {
                    SampleEntry::Avc1(_) => has_avc1 = true,
                    SampleEntry::Hev1(_) => has_hev1 = true,
                    SampleEntry::Hvc1(_) => has_hvc1 = true,
                    SampleEntry::Av01(_) => has_av01 = true,
                    _ => {}
                }
            }
        }

        let mut compatible_brands = vec![Brand::ISOM, Brand::ISO2, Brand::MP41];
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
            major_brand: Brand::ISOM,
            minor_version: 0,
            compatible_brands,
        }
    }

    fn build_head_boxes_bytes(
        &self,
        ftyp_box_bytes: &[u8],
        moov_box_bytes: &[u8],
    ) -> Result<(Vec<u8>, u64), MuxError> {
        let head_region_size =
            usize::try_from(self.mdat_box_offset).expect("mdat_box_offset should fit in usize");

        // moov を先頭領域に配置できる場合は faststart にする
        if let Some(required_head_size) = ftyp_box_bytes.len().checked_add(moov_box_bytes.len())
            && required_head_size <= head_region_size
        {
            let trailing_size = head_region_size - required_head_size;
            if trailing_size == 0 || trailing_size >= BoxHeader::MIN_SIZE {
                let mut head_boxes_bytes = ftyp_box_bytes.to_vec();
                let moov_box_offset = ftyp_box_bytes.len() as u64;
                head_boxes_bytes.extend_from_slice(moov_box_bytes);
                if trailing_size > 0 {
                    let free_box_bytes = Self::build_free_box_bytes(trailing_size)?;
                    head_boxes_bytes.extend_from_slice(&free_box_bytes);
                }
                return Ok((head_boxes_bytes, moov_box_offset));
            }
        }

        // 先頭に moov を置けない場合は、ftyp + free に再構成して moov は末尾に追記する
        let trailing_size = head_region_size
            .checked_sub(ftyp_box_bytes.len())
            .expect("bug: finalized ftyp should fit in reserved head region");

        let mut head_boxes_bytes = ftyp_box_bytes.to_vec();
        if trailing_size > 0 {
            let free_box_bytes = Self::build_free_box_bytes(trailing_size)?;
            head_boxes_bytes.extend_from_slice(&free_box_bytes);
        }
        Ok((head_boxes_bytes, self.next_position))
    }

    fn build_free_box_bytes(total_size: usize) -> Result<Vec<u8>, MuxError> {
        assert!(
            total_size >= BoxHeader::MIN_SIZE,
            "bug: free box size should be larger than BoxHeader::MIN_SIZE",
        );

        let box_size = if let Ok(box_size) = u32::try_from(total_size) {
            BoxSize::U32(box_size)
        } else {
            BoxSize::U64(total_size as u64)
        };
        let header = BoxHeader::new(FreeBox::TYPE, box_size);
        let payload_size = total_size
            .checked_sub(header.external_size())
            .expect("free box total size should be larger than its header size");

        let mut bytes = header.encode_to_vec()?;
        bytes.extend_from_slice(&vec![0; payload_size]);
        Ok(bytes)
    }

    fn build_moov_box(&self) -> Result<MoovBox, MuxError> {
        // 各トラックの `tkhd` の `duration` は `mvhd` の `timescale` 単位で書く必要があるため、
        // trak ボックスの構築よりも先に `mvhd` に入れる `timescale` を確定させる
        // （`calculate_total_duration()` は `self.tracks` しか参照しないので、ここで先に呼んでよい）
        let (movie_timescale, movie_duration) = self.calculate_total_duration();

        let mut trak_boxes = Vec::new();

        // 空 chunks の TrackEntry はスキップする
        //
        // append_sample() はサンプルエントリーの解決を ensure_track_entry() よりも前に行うため、
        // chunks が空のままの TrackEntry は現状の実装では生成されない。
        // ここは将来その不変条件が崩れたときに不正な trak を出力しないための防御であり、
        // 現時点で実際にスキップされる要素は無い
        for entry in self.tracks.iter().filter(|t| !t.chunks.is_empty()) {
            let track_id = trak_boxes.len() as u32 + 1;
            trak_boxes.push(self.build_trak_box(entry, track_id, movie_timescale)?);
        }

        let creation_time = Mp4FileTime::from_unix_time(self.options.creation_timestamp);
        let mvhd_box = MvhdBox {
            creation_time,
            modification_time: creation_time,
            timescale: movie_timescale,
            duration: movie_duration,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: trak_boxes.len() as u32 + 1,
        };

        Ok(MoovBox {
            mvhd_box,
            trak_boxes,
            mvex_box: None,
            unknown_boxes: Vec::new(),
        })
    }

    /// 指定した [`TrackEntry`] から `trak` ボックスを構築する
    ///
    /// `movie_timescale` には `mvhd` ボックスに書くのと同じ値を渡すこと。
    /// `tkhd` の `duration` の単位はこの値で決まるため、異なる値を渡すと
    /// 仕様違反の尺を持つ MP4 がエラーもなく出力される
    ///
    /// `entry.chunks` が非空であることを不変条件として要求する
    /// （空の場合は `derive_trak_derivation` 内の `expect` で panic する）。
    /// 呼び出し側の `build_moov_box` が空 chunks の [`TrackEntry`] をスキップしているため、
    /// この不変条件は常に満たされる
    fn build_trak_box(
        &self,
        entry: &TrackEntry,
        track_id: u32,
        movie_timescale: NonZeroU32,
    ) -> Result<TrakBox, MuxError> {
        let total_duration = total_sample_duration(entry);
        let tkhd_duration =
            convert_duration_to_movie_timescale(total_duration, entry.timescale, movie_timescale)?;

        let derived = self.derive_trak_derivation(entry)?;

        let creation_time = Mp4FileTime::from_unix_time(self.options.creation_timestamp);
        let tkhd_box = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time,
            modification_time: creation_time,
            track_id,
            duration: tkhd_duration,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: derived.volume,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: derived.width,
            height: derived.height,
        };

        Ok(TrakBox {
            tkhd_box,
            edts_box: None,
            mdia_box: self.build_mdia_box(entry, &derived)?,
            unknown_boxes: Vec::new(),
        })
    }

    /// トラック種別ごとの派生属性（tkhd / hdlr / media_header 用）を導出する
    ///
    /// 映像トラックだけは複数のサンプルエントリーの最大幅・高さを採用する既存挙動を維持するため、
    /// `derive_trak_attributes` を呼ばず本メソッド内で [`TrakDerivation`] を直接組み立てる
    fn derive_trak_derivation(&self, entry: &TrackEntry) -> Result<TrakDerivation, MuxError> {
        let first_sample_entry = &entry
            .chunks
            .first()
            .expect("bug: derive_trak_derivation called with empty chunks")
            .sample_entry;

        match entry.track_kind {
            TrackKind::Video => {
                let (max_width, max_height) = entry
                    .chunks
                    .iter()
                    .filter_map(|c| c.sample_entry.video_resolution())
                    .fold((0u16, 0u16), |(max_w, max_h), (w, h)| {
                        (max_w.max(w), max_h.max(h))
                    });

                let width = i16::try_from(max_width).map_err(|_| {
                    MuxError::EncodeError(Error::invalid_data("video width exceeds i16::MAX"))
                })?;
                let height = i16::try_from(max_height).map_err(|_| {
                    MuxError::EncodeError(Error::invalid_data("video height exceeds i16::MAX"))
                })?;

                Ok(TrakDerivation {
                    volume: TkhdBox::DEFAULT_VIDEO_VOLUME,
                    width: FixedPointNumber::new(width, 0),
                    height: FixedPointNumber::new(height, 0),
                    handler_type: HdlrBox::HANDLER_TYPE_VIDE,
                    media_header: MediaHeader::Vmhd(VmhdBox::default()),
                })
            }
            TrackKind::Audio | TrackKind::Subtitle => {
                derive_trak_attributes(entry.track_kind, first_sample_entry)
            }
        }
    }

    fn build_mdia_box(
        &self,
        entry: &TrackEntry,
        derived: &TrakDerivation,
    ) -> Result<MdiaBox, MuxError> {
        let total_duration = total_sample_duration(entry);
        let metadata = self.options.track_metadata(entry.track_kind);

        let creation_time = Mp4FileTime::from_unix_time(self.options.creation_timestamp);
        let mdhd_box = MdhdBox {
            creation_time,
            modification_time: creation_time,
            timescale: entry.timescale,
            duration: total_duration,
            language: metadata.language.as_bytes(),
        };

        let hdlr_box = HdlrBox {
            handler_type: derived.handler_type,
            name: metadata.name.clone().into_null_terminated_bytes(),
        };

        let minf_box = MinfBox {
            media_header: Some(derived.media_header.clone()),
            dinf_box: DinfBox::LOCAL_FILE,
            stbl_box: self.build_stbl_box(entry.track_kind, &entry.chunks)?,
            unknown_boxes: Vec::new(),
        };

        Ok(MdiaBox {
            mdhd_box,
            hdlr_box,
            minf_box,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_stbl_box(&self, track_kind: TrackKind, chunks: &[Chunk]) -> Result<StblBox, MuxError> {
        // [NOTE]
        // 典型的にはユニークなサンプルエントリーの数は高々数個なので、線形探索を行う
        // （`HashMap`は nostd 環境で使えず、`BTreeMap`には`Ord`実装が必要なので使用していない）
        let mut sample_entries = Vec::new();
        for chunk in chunks {
            if sample_entries.contains(&chunk.sample_entry) {
                continue;
            }
            sample_entries.push(chunk.sample_entry.clone());
        }

        let stsd_box = StsdBox {
            entries: sample_entries.clone(),
        };

        let stts_box = SttsBox::from_sample_deltas(
            chunks
                .iter()
                .flat_map(|c| c.samples.iter().map(|s| s.duration)),
        )?;
        let ctts_box = build_ctts_box(chunks)?;

        let stsc_box = StscBox {
            entries: chunks
                .iter()
                .enumerate()
                .map(|(i, c)| -> Result<StscEntry, MuxError> {
                    // sample_entries は直前のループで全 chunk の entry を収集済みなので必ず見つかる
                    let idx = sample_entries
                        .iter()
                        .position(|entry| entry == &c.sample_entry)
                        .expect("sample_entry should exist in sample_entries");
                    // 0-based の position を 1-based の sample_description_index へ変換する
                    let idx = u32::try_from(idx).map_err(|_| {
                        MuxError::EncodeError(Error::invalid_data(
                            "sample description index exceeds u32::MAX",
                        ))
                    })?;
                    let sample_description_index =
                        NonZeroU32::MIN.checked_add(idx).ok_or(MuxError::Overflow)?;

                    // 0-based の列挙インデックスを 1-based の first_chunk へ変換する
                    let i = u32::try_from(i).map_err(|_| {
                        MuxError::EncodeError(Error::invalid_data("chunk index exceeds u32::MAX"))
                    })?;
                    Ok(StscEntry {
                        first_chunk: NonZeroU32::MIN.checked_add(i).ok_or(MuxError::Overflow)?,
                        sample_per_chunk: u32::try_from(c.samples.len()).map_err(|_| {
                            MuxError::EncodeError(Error::invalid_data(
                                "samples per chunk exceeds u32::MAX",
                            ))
                        })?,
                        sample_description_index,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        };

        let stsz_box = StszBox::Variable {
            entry_sizes: chunks
                .iter()
                .flat_map(|c| c.samples.iter().map(|s| s.size))
                .collect(),
        };

        let stco_or_co64_box = if self.next_position > u32::MAX as u64 {
            Either::B(Co64Box {
                chunk_offsets: chunks.iter().map(|c| c.offset).collect(),
            })
        } else {
            Either::A(StcoBox {
                chunk_offsets: chunks.iter().map(|c| c.offset as u32).collect(),
            })
        };

        // ISO/IEC 14496-12: stss の不在は全サンプルが同期サンプルであることを意味する
        let is_all_keyframe = chunks.iter().all(|c| c.samples.iter().all(|s| s.keyframe));
        let stss_box = if is_all_keyframe {
            None
        } else {
            // enumerate は filter 前のグローバル 0-based 番号を保持したまま 1-based へ変換する
            let sample_numbers = chunks
                .iter()
                .flat_map(|c| c.samples.iter())
                .enumerate()
                .filter(|&(_, s)| s.keyframe)
                .map(|(i, _)| {
                    let i = u32::try_from(i).map_err(|_| {
                        MuxError::EncodeError(Error::invalid_data("sample index exceeds u32::MAX"))
                    })?;
                    NonZeroU32::MIN.checked_add(i).ok_or(MuxError::Overflow)
                })
                .collect::<Result<Vec<_>, _>>()?;

            if sample_numbers.is_empty() {
                // entry_count = 0 の stss は「同期サンプルなし」を意味し、不在（全同期）と真逆である。
                // 音声・字幕は本来すべて同期なので省略で救済し、映像は誤った宣言を出さず拒否する
                match track_kind {
                    TrackKind::Audio | TrackKind::Subtitle => None,
                    TrackKind::Video => {
                        return Err(MuxError::NoSyncSamples { track_kind });
                    }
                }
            } else {
                Some(StssBox { sample_numbers })
            }
        };

        Ok(StblBox {
            stsd_box,
            stts_box,
            ctts_box,
            cslg_box: None,
            stsc_box,
            stsz_box,
            stco_or_co64_box,
            stss_box,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        })
    }

    /// 正規化した尺（実時間）が最長のトラックの `(timescale, duration)` を返す
    ///
    /// 正規化した尺が同着の場合は、先に [`Self::append_sample`] されたトラックを採用する。
    ///
    /// `chunks` が空のトラックは `build_moov_box` では `trak` の生成対象から外れるが、
    /// ここでは除外していない。尺の合計が 0 になるため、尺を持つトラックが他にあれば選ばれず、
    /// すべてのトラックの尺が 0 ならどれが選ばれても換算結果は 0 になるためである
    ///
    /// トラックが 1 つも無い場合は `(NonZeroU32::MIN, 0)` を返す
    /// （このとき `trak` ボックスも 0 個になるため、`mvhd` に入る値は任意でよい）
    fn calculate_total_duration(&self) -> (NonZeroU32, u64) {
        let mut best: Option<(NonZeroU32, u64, Duration)> = None;
        for track in &self.tracks {
            let duration = total_sample_duration(track);
            let normalized = Duration::from_secs(duration) / track.timescale.get();

            match best {
                Some((_, _, best_normalized)) if best_normalized >= normalized => {}
                _ => best = Some((track.timescale, duration, normalized)),
            }
        }
        best.map(|(ts, dur, _)| (ts, dur))
            .unwrap_or((NonZeroU32::MIN, 0))
    }
}

/// [`TrackEntry`] が持つ全サンプルの尺の合計を返す（そのトラックの `timescale` 単位）
///
/// `tkhd` / `mdhd` / `mvhd` に入る尺はいずれもこの値から導出される。
/// `tkhd` の `duration` は `mdhd` の `duration` を換算した値でなければならないため、
/// 数え方が箇所ごとに食い違うと静かに不整合な MP4 が出力される。それを避けるため 1 箇所に集約する
fn total_sample_duration(entry: &TrackEntry) -> u64 {
    entry
        .chunks
        .iter()
        .flat_map(|c| c.samples.iter().map(|s| s.duration as u64))
        .sum()
}

/// `mdhd` の `timescale` 単位の尺を、`mvhd` の `timescale` 単位に換算する
///
/// [ISO/IEC 14496-12] TrackHeaderBox class では、`tkhd` ボックスの `duration` は
/// ファイル全体の時間軸を定める `mvhd` ボックスの `timescale` 単位で表すと定められている。
/// 一方 `mdhd` ボックスの `duration` はトラック固有の `timescale` 単位なので、換算が必要になる。
///
/// 端数は切り上げる。切り捨てを使うと、換算結果が 1 未満になるトラックで `duration` が 0 に潰れ、
/// 尺が 0 のトラックとみなしてサンプルを読み出さないプレイヤーが存在するためである。
/// 代償として尺は最大 `1 / movie_timescale` 秒だけ過大になるが、
/// 過大側では読み出せるサンプルが減らないため、打ち切りより害が小さいと判断している。
/// なお尺の合計が 0 のトラックは、切り上げても `duration` が 0 のままになる。
///
/// 換算結果が `mvhd` ボックスの `duration` を数 tick 上回ることがある。
/// 採用トラックの選択が正規化した尺のナノ秒粒度の比較で行われるため、
/// わずかに長いトラックが同着と判定されて採用されないことがあるためである。
///
/// なお、ここでの扱いは上記規格の現行版に基づくものであり、将来の改訂で変わる可能性がある。
fn convert_duration_to_movie_timescale(
    media_duration: u64,
    media_timescale: NonZeroU32,
    movie_timescale: NonZeroU32,
) -> Result<u64, MuxError> {
    // `u64` と `u32` の積は高々 2 の 96 乗なので、`u128` で計算すれば中間結果はオーバーフローしない
    let converted = (media_duration as u128 * movie_timescale.get() as u128)
        .div_ceil(media_timescale.get() as u128);

    // 呼び出し側が最長トラックの `timescale` を渡すため、換算結果は `mvhd` の `duration` を
    // 数 tick 上回る程度にしかならない。`u64` を超えるには採用トラックの尺の合計が
    // `u64::MAX` 近傍である必要があり、現実の入力では到達しない防御的な分岐である
    u64::try_from(converted).map_err(|_| {
        MuxError::EncodeError(Error::invalid_data(
            "converted track duration exceeds u64::MAX",
        ))
    })
}

fn build_ctts_box(chunks: &[Chunk]) -> Result<Option<CttsBox>, MuxError> {
    let has_any_cto = chunks.iter().any(|chunk| {
        chunk
            .samples
            .iter()
            .any(|sample| sample.composition_time_offset.is_some())
    });
    if !has_any_cto {
        return Ok(None);
    }

    let version = if chunks.iter().any(|chunk| {
        chunk.samples.iter().any(|sample| {
            sample
                .composition_time_offset
                .is_some_and(|offset| offset < 0)
        })
    }) {
        1
    } else {
        0
    };

    let mut entries: Vec<CttsEntry> = Vec::new();
    for offset in chunks
        .iter()
        .flat_map(|chunk| chunk.samples.iter())
        .map(|sample| sample.composition_time_offset.unwrap_or(0))
    {
        if offset < i64::from(i32::MIN) {
            return Err(MuxError::EncodeError(Error::invalid_input(
                "composition_time_offset must be greater than or equal to i32::MIN",
            )));
        }
        if version == 1 && offset > i64::from(i32::MAX) {
            return Err(MuxError::EncodeError(Error::invalid_input(
                "composition_time_offset exceeds i32::MAX (ctts version 1 requires i32 range)",
            )));
        }
        if version == 0 && offset > i64::from(u32::MAX) {
            return Err(MuxError::EncodeError(Error::invalid_input(
                "composition_time_offset must be less than or equal to u32::MAX",
            )));
        }
        if let Some(last) = entries.last_mut()
            && last.sample_offset == offset
        {
            last.sample_count = last.sample_count.checked_add(1).ok_or(MuxError::Overflow)?;
            continue;
        }
        entries.push(CttsEntry {
            sample_count: 1,
            sample_offset: offset,
        });
    }

    Ok(Some(CttsBox { version, entries }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    use crate::{
        Uint,
        boxes::{
            AudioSampleEntryFields, Avc1Box, AvccBox, BoxRecord, DopsBox, FtabBox, OpusBox,
            StppBox, StszBox, StyleRecord, Tx3gBox, VisualSampleEntryFields,
        },
    };

    #[test]
    fn test_muxer_creation() {
        // オプションなし
        let muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        assert!(!muxer.initial_boxes_bytes().is_empty());
        assert!(muxer.finalized_boxes().is_none());

        // オプションあり
        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size: 4096,
            creation_timestamp: Duration::from_secs(0),
            ..Default::default()
        };
        let muxer =
            Mp4FileMuxer::with_options(options).expect("failed to create muxer with options");
        assert!(!muxer.initial_boxes_bytes().is_empty());
    }

    /// `TrackMetadata::default()` が生成する `mdhd.language` / `hdlr.name` のバイト列は
    /// フィールド追加前の muxer 出力（`MdhdBox::LANGUAGE_UNDEFINED` および
    /// null 終端 1 バイトの空文字列）と完全一致する
    ///
    /// 完了条件「Options のフィールドを指定しなかった場合、生成される MP4 のバイト列が
    /// 現行と完全一致すること」を回帰防止として固定する。
    /// あわせて [`crate::LanguageCode::UNDEFINED`] と
    /// [`crate::boxes::MdhdBox::LANGUAGE_UNDEFINED`] が同値であることも担保する
    #[test]
    fn test_default_track_metadata_bytes() {
        let metadata = TrackMetadata::default();

        // mdhd.language は `*b"und"`
        assert_eq!(metadata.language.as_bytes(), MdhdBox::LANGUAGE_UNDEFINED);
        assert_eq!(
            LanguageCode::UNDEFINED.as_bytes(),
            MdhdBox::LANGUAGE_UNDEFINED
        );

        // hdlr.name は末尾 null 1 バイトのみ
        assert_eq!(metadata.name.into_null_terminated_bytes(), vec![0u8]);
    }

    /// サンプル追加とファイナライズの基本的なワークフローテスト
    #[test]
    fn test_append_sample_and_finalize() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        // H.264 ビデオサンプルを作成
        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer
            .append_sample(&sample)
            .expect("failed to append sample");

        // 別のサンプルを追加
        let sample2 = Sample {
            track_kind: TrackKind::Video,
            sample_entry: None,
            keyframe: false,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size + 1024,
            data_size: 512,
        };
        muxer
            .append_sample(&sample2)
            .expect("failed to append sample");

        // マルチプレクサーをファイナライズ
        let finalized = muxer.finalize().expect("failed to finalize");
        assert!(!finalized.moov_box_bytes.is_empty());
        assert!(!finalized.mdat_box_header_bytes.is_empty());
    }

    /// ポジション不一致エラーのテスト
    #[test]
    fn test_position_mismatch_error() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size + 100, // 誤ったオフセット
            data_size: 1024,
        };
        assert!(matches!(
            muxer.append_sample(&sample),
            Err(MuxError::PositionMismatch { expected, actual })
            if expected == initial_size && actual == initial_size + 100
        ));
    }

    /// サンプルエントリー不在エラーのテスト
    #[test]
    fn test_missing_sample_entry_error() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        // サンプルエントリーなしの最初のサンプルは失敗するはず
        let sample = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: None,
            keyframe: false,
            timescale: NonZeroU32::MIN.saturating_add(1000 - 1),
            duration: 20,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 512,
        };
        assert!(matches!(
            muxer.append_sample(&sample),
            Err(MuxError::MissingSampleEntry {
                track_kind: TrackKind::Audio
            })
        ));
    }

    /// MissingSampleEntry エラーの返却がトラック状態に副作用を残さないことを検証するテスト
    ///
    /// append_sample() はサンプルエントリーの解決を ensure_track_entry() よりも前に行うため、
    /// MissingSampleEntry で失敗したサンプルの timescale は記録されない。
    /// これを公開 API から観測できる形として、
    /// 失敗した直後に別の timescale のサンプルを投入しても
    /// TimescaleMismatch にならずに受理されることを確認する
    #[test]
    fn test_missing_sample_entry_error_leaves_tracks_unchanged() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        // 新規音声トラックの初回サンプルで sample_entry = None を渡し MissingSampleEntry を発生させる
        let bad_sample = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: None,
            keyframe: false,
            timescale: NonZeroU32::MIN.saturating_add(1000 - 1),
            duration: 20,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 256,
        };
        assert!(matches!(
            muxer.append_sample(&bad_sample),
            Err(MuxError::MissingSampleEntry {
                track_kind: TrackKind::Audio
            })
        ));

        // ここで別 timescale を持つ Sample を再投入する。
        // 内部状態が不変であれば TimescaleMismatch は発生せず、新規トラックとして受け入れられる
        let good_sample = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: Some(create_opus_sample_entry()),
            keyframe: false,
            timescale: NonZeroU32::MIN.saturating_add(48000 - 1),
            duration: 1024,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 256,
        };
        muxer
            .append_sample(&good_sample)
            .expect("failed to append sample after MissingSampleEntry");
    }

    /// ファイナライズ済みエラーのテスト
    #[test]
    fn test_already_finalized_error() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer
            .append_sample(&sample)
            .expect("failed to append sample");
        muxer.finalize().expect("failed to finalize");

        // ファイナライズ後に別のサンプルを追加しようとする
        let sample2 = Sample {
            track_kind: TrackKind::Video,
            sample_entry: None,
            keyframe: false,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size + 1024,
            data_size: 512,
        };
        assert!(matches!(
            muxer.append_sample(&sample2),
            Err(MuxError::AlreadyFinalized)
        ));
    }

    /// data_size が u32::MAX の境界値で append_sample と finalize が成功するテスト
    ///
    /// Mp4FileMuxer はメタデータのみを管理して実バイト列は確保しないため、
    /// data_size に u32::MAX を渡しても 4 GiB の確保は発生しない。
    /// next_position が u32::MAX を超えるため、finalize は Co64Box 経路を通る。
    #[test]
    fn test_append_sample_data_size_u32_max_succeeds() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: u32::MAX as usize,
        };
        muxer
            .append_sample(&sample)
            .expect("failed to append sample with u32::MAX data_size");
        let finalized = muxer.finalize().expect("failed to finalize muxer");
        assert!(!finalized.moov_box_bytes.is_empty());
    }

    /// faststart 有効化 (with_options) 経路でも data_size の u32 境界が同様に防御されるテスト
    #[test]
    fn test_append_sample_data_size_u32_max_with_faststart() {
        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size: 8192,
            ..Default::default()
        };
        let mut muxer =
            Mp4FileMuxer::with_options(options).expect("failed to create muxer with options");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: u32::MAX as usize,
        };
        muxer
            .append_sample(&sample)
            .expect("failed to append sample with u32::MAX data_size under faststart");
        let finalized = muxer.finalize().expect("failed to finalize muxer");
        assert!(finalized.is_faststart_enabled());
    }

    /// data_size が u32::MAX を超える場合のエラーテスト
    // 32-bit プラットフォームでは usize で u32::MAX + 1 を表現できず構造的に到達不能なため cfg で限定する
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn test_append_sample_data_size_exceeds_u32_max() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: u32::MAX as usize + 1,
        };
        let err = muxer
            .append_sample(&sample)
            .expect_err("expected encode error for data_size exceeding u32::MAX");
        assert!(matches!(err, MuxError::EncodeError(_)));
        // MuxError::EncodeError は他原因でも返るためメッセージ内容まで確認する
        let message = format!("{err}");
        assert!(
            message.contains("sample data size exceeds u32::MAX"),
            "unexpected error message: {message}",
        );
    }

    /// append_sample がエラーを返した後にミューサ状態が変化していないことを検証するテスト
    // 32-bit プラットフォームでは usize で u32::MAX + 1 を表現できず構造的に到達不能なため cfg で限定する
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn test_append_sample_error_keeps_muxer_state() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let bad_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: u32::MAX as usize + 1,
        };
        muxer
            .append_sample(&bad_sample)
            .expect_err("expected encode error for data_size exceeding u32::MAX");

        // エラー後でも next_position は初期値のままなので、同じ data_offset で再投入できる
        let good_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer
            .append_sample(&good_sample)
            .expect("failed to append sample after error");
        muxer.finalize().expect("failed to finalize muxer");
    }

    /// Overflow 後も内部状態が不変で、収まる data_size なら再投入できることを検証するテスト
    ///
    /// advance_position で次の書き込み位置を u64::MAX 付近まで進め、
    /// Overflow を起こしたあとに同じ data_offset かつ収まる data_size で再投入する。
    /// Overflow 直後に tracks / chunk / sample 数が増えていないことと、
    /// finalize 後の stsz エントリ数が先行サンプル + 1 であることを確認し、二重登録が無いことを示す。
    #[test]
    fn test_append_sample_overflow_keeps_muxer_state() {
        let mut muxer = Mp4FileMuxer::new().expect("ミューサの作成に失敗した");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::MIN.saturating_add(30 - 1);
        let first_size = 1024usize;

        let first_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale,
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: first_size,
        };
        muxer
            .append_sample(&first_sample)
            .expect("最初のサンプルの追加に失敗した");

        let overflow_offset = u64::MAX - 10;
        let after_first = initial_size + first_size as u64;
        muxer
            .advance_position(overflow_offset - after_first)
            .expect("書き込み位置の前進に失敗した");

        // data_size = 100 では overflow_offset + 100 が u64::MAX を超える
        let overflowing_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: None,
            keyframe: false,
            timescale,
            duration: 1,
            composition_time_offset: None,
            data_offset: overflow_offset,
            data_size: 100,
        };
        let err = muxer
            .append_sample(&overflowing_sample)
            .expect_err("オーバーフローする data_size ではエラーが返るべき");
        assert!(matches!(err, MuxError::Overflow), "予期しないエラー: {err}");

        // Overflow 直後に tracks / chunk / sample が増えていないことを直接確認する
        // （finalize 後の stsz だけでなく、中間状態の残留も回帰として捕まえる）
        assert_eq!(muxer.tracks.len(), 1, "Overflow で TrackEntry が増えている");
        assert_eq!(
            muxer.tracks[0].chunks.len(),
            1,
            "Overflow で Chunk が増えている"
        );
        assert_eq!(
            muxer.tracks[0].chunks[0].samples.len(),
            1,
            "Overflow で sample metadata が増えている"
        );

        let fitting_size = 5usize;
        let fitting_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: None,
            keyframe: false,
            timescale,
            duration: 1,
            composition_time_offset: None,
            data_offset: overflow_offset,
            data_size: fitting_size,
        };
        muxer
            .append_sample(&fitting_sample)
            .expect("Overflow 後の再投入に失敗した");

        let finalized = muxer.finalize().expect("finalize に失敗した");
        let stsz_box = &finalized.moov_box().trak_boxes[0]
            .mdia_box
            .minf_box
            .stbl_box
            .stsz_box;
        let StszBox::Variable { entry_sizes } = stsz_box else {
            panic!("stsz は Variable であるべき");
        };
        assert_eq!(
            entry_sizes.as_slice(),
            &[first_size as u32, fitting_size as u32],
            "Overflow 後の再投入で二重登録が起きている"
        );
    }

    /// timescale 不一致と Overflow が同時に成立するとき Overflow が先に返ることを検証するテスト
    #[test]
    fn test_append_sample_overflow_before_timescale_mismatch() {
        let mut muxer = Mp4FileMuxer::new().expect("ミューサの作成に失敗した");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;
        let first_size = 1024usize;

        let first_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: first_size,
        };
        muxer
            .append_sample(&first_sample)
            .expect("最初のサンプルの追加に失敗した");

        let overflow_offset = u64::MAX - 10;
        let after_first = initial_size + first_size as u64;
        muxer
            .advance_position(overflow_offset - after_first)
            .expect("書き込み位置の前進に失敗した");

        // timescale 不一致かつ加算オーバーフローする入力
        let conflicting_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: None,
            keyframe: false,
            timescale: NonZeroU32::MIN.saturating_add(60 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: overflow_offset,
            data_size: 100,
        };
        let err = muxer
            .append_sample(&conflicting_sample)
            .expect_err("エラーが返るべき");
        assert!(
            matches!(err, MuxError::Overflow),
            "TimescaleMismatch より Overflow が先に返るべき: {err}"
        );
    }

    /// `build_stbl_box` が使う 1-based 変換の算術境界を固定する
    ///
    /// `u32::MAX` 個のチャンクを実際に生成するのは非現実的なため、
    /// 実装が依存する `NonZeroU32::MIN.checked_add` の戻り値だけを直接確認する。
    /// `finalize()` 経由で `MuxError::Overflow` が返ることまでは検証しない。
    #[test]
    fn test_nonzero_u32_min_checked_add_overflows_at_u32_max() {
        assert!(
            NonZeroU32::MIN.checked_add(u32::MAX).is_none(),
            "NonZeroU32::MIN + u32::MAX はオーバーフローするべき"
        );
        // 最大の合法な 0-based インデックス（u32::MAX - 1）では 1-based 値が u32::MAX になる
        assert_eq!(
            NonZeroU32::MIN.checked_add(u32::MAX - 1).map(|v| v.get()),
            Some(u32::MAX),
            "NonZeroU32::MIN + (u32::MAX - 1) は u32::MAX になるべき"
        );
    }

    /// `finalize` 経路で stsc の first_chunk と stss の sample_numbers が 1-based になることを検証する
    #[test]
    fn test_build_stbl_box_one_based_indices_for_chunks_and_keyframes() {
        let mut muxer = Mp4FileMuxer::new().expect("ミューサの作成に失敗した");
        let mut offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::MIN.saturating_add(30 - 1);
        let sample_size = 100usize;

        // チャンク 1: キーフレーム
        muxer
            .append_sample(&Sample {
                track_kind: TrackKind::Video,
                sample_entry: Some(create_avc1_sample_entry()),
                keyframe: true,
                timescale,
                duration: 1,
                composition_time_offset: None,
                data_offset: offset,
                data_size: sample_size,
            })
            .expect("1 つ目のサンプルの追加に失敗した");
        offset += sample_size as u64;

        // 非サンプルデータを挟んで強制的に新チャンクを開始する
        muxer
            .advance_position(8)
            .expect("書き込み位置の前進に失敗した");
        offset += 8;

        // チャンク 2: 非キーフレーム（stss を出させる）
        muxer
            .append_sample(&Sample {
                track_kind: TrackKind::Video,
                sample_entry: None,
                keyframe: false,
                timescale,
                duration: 1,
                composition_time_offset: None,
                data_offset: offset,
                data_size: sample_size,
            })
            .expect("2 つ目のサンプルの追加に失敗した");
        offset += sample_size as u64;

        muxer
            .advance_position(8)
            .expect("書き込み位置の前進に失敗した");
        offset += 8;

        // チャンク 3: キーフレーム
        muxer
            .append_sample(&Sample {
                track_kind: TrackKind::Video,
                sample_entry: None,
                keyframe: true,
                timescale,
                duration: 1,
                composition_time_offset: None,
                data_offset: offset,
                data_size: sample_size,
            })
            .expect("3 つ目のサンプルの追加に失敗した");

        let finalized = muxer.finalize().expect("finalize に失敗した");
        let stbl = &finalized.moov_box().trak_boxes[0]
            .mdia_box
            .minf_box
            .stbl_box;

        let first_chunks: Vec<u32> = stbl
            .stsc_box
            .entries
            .iter()
            .map(|e| e.first_chunk.get())
            .collect();
        assert_eq!(
            first_chunks.as_slice(),
            &[1, 2, 3],
            "first_chunk は 1-based の連番であるべき"
        );

        let stss = stbl
            .stss_box
            .as_ref()
            .expect("混在キーフレームでは stss が出るべき");
        let sample_numbers: Vec<u32> = stss.sample_numbers.iter().map(|n| n.get()).collect();
        assert_eq!(
            sample_numbers.as_slice(),
            &[1, 3],
            "sample_numbers はキーフレームの 1-based 番号であるべき"
        );
    }

    /// `muxer` に解像度 `width` x `height` の AVC1 サンプルを 1 つ追加して `finalize()` を呼ぶ
    fn finalize_after_appending_video_sample(
        muxer: &mut Mp4FileMuxer,
        width: u16,
        height: u16,
    ) -> Result<(), MuxError> {
        let initial_size = muxer.initial_boxes_bytes().len() as u64;
        let mut entry = create_avc1_sample_entry();
        let SampleEntry::Avc1(avc1) = &mut entry else {
            panic!("create_avc1_sample_entry must return SampleEntry::Avc1");
        };
        avc1.visual.width = width;
        avc1.visual.height = height;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(entry),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer
            .append_sample(&sample)
            .expect("failed to append sample");
        muxer.finalize().map(|_| ())
    }

    /// 映像解像度の幅と高さが i16::MAX (32767) の境界値で finalize が成功するテスト
    #[test]
    fn test_finalize_video_resolution_i16_max_succeeds() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        finalize_after_appending_video_sample(&mut muxer, i16::MAX as u16, i16::MAX as u16)
            .expect("failed to finalize at i16::MAX resolution");
    }

    /// faststart 有効化 (with_options) 経路でも i16::MAX 境界値で finalize が成功するテスト
    #[test]
    fn test_finalize_video_resolution_i16_max_with_faststart() {
        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size: 8192,
            ..Default::default()
        };
        let mut muxer =
            Mp4FileMuxer::with_options(options).expect("failed to create muxer with options");
        finalize_after_appending_video_sample(&mut muxer, i16::MAX as u16, i16::MAX as u16)
            .expect("failed to finalize at i16::MAX resolution under faststart");
    }

    /// 映像幅が i16::MAX を超える場合に width 側のエラーメッセージが返ることを検証するテスト
    #[test]
    fn test_finalize_video_width_exceeds_i16_max() {
        // i16::MAX を超える最小値の u16 (= 32768) を渡す
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let err = finalize_after_appending_video_sample(&mut muxer, 32_768, 1)
            .expect_err("expected encode error for width exceeding i16::MAX");
        assert!(matches!(err, MuxError::EncodeError(_)));
        // MuxError::EncodeError は他原因でも返るためメッセージ内容まで確認する
        let message = format!("{err}");
        assert!(
            message.contains("video width exceeds i16::MAX"),
            "unexpected error message: {message}",
        );
    }

    /// 映像高さが i16::MAX を超える場合に height 側のエラーメッセージが返ることを検証するテスト
    #[test]
    fn test_finalize_video_height_exceeds_i16_max() {
        // i16::MAX を超える最小値の u16 (= 32768) を渡す
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let err = finalize_after_appending_video_sample(&mut muxer, 1, 32_768)
            .expect_err("expected encode error for height exceeding i16::MAX");
        assert!(matches!(err, MuxError::EncodeError(_)));
        // MuxError::EncodeError は他原因でも返るためメッセージ内容まで確認する
        let message = format!("{err}");
        assert!(
            message.contains("video height exceeds i16::MAX"),
            "unexpected error message: {message}",
        );
    }

    /// 音声と映像の複数トラックのテスト
    #[test]
    fn test_audio_and_video_tracks() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        // ビデオサンプルを追加
        let video_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer
            .append_sample(&video_sample)
            .expect("failed to append video sample");

        // オーディオサンプルを追加
        let audio_sample = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: Some(create_opus_sample_entry()),
            keyframe: false,
            timescale: NonZeroU32::MIN.saturating_add(1000 - 1),
            duration: 20,
            composition_time_offset: None,
            data_offset: initial_size + 1024,
            data_size: 256,
        };
        muxer
            .append_sample(&audio_sample)
            .expect("failed to append audio sample");

        let finalized = muxer.finalize().expect("failed to finalize");
        assert!(!finalized.moov_box_bytes.is_empty());
    }

    /// 音声・映像・字幕の 3 トラックの mux テスト
    #[test]
    fn test_audio_video_subtitle_tracks() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        // 映像サンプルを追加
        let video_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer
            .append_sample(&video_sample)
            .expect("failed to append video sample");

        // 音声サンプルを追加
        let audio_sample = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: Some(create_opus_sample_entry()),
            keyframe: false,
            timescale: NonZeroU32::MIN.saturating_add(1000 - 1),
            duration: 20,
            composition_time_offset: None,
            data_offset: initial_size + 1024,
            data_size: 256,
        };
        muxer
            .append_sample(&audio_sample)
            .expect("failed to append audio sample");

        // 字幕サンプルを追加
        let subtitle_sample = Sample {
            track_kind: TrackKind::Subtitle,
            sample_entry: Some(create_stpp_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(1000 - 1),
            duration: 500,
            composition_time_offset: None,
            data_offset: initial_size + 1024 + 256,
            data_size: 128,
        };
        muxer
            .append_sample(&subtitle_sample)
            .expect("failed to append subtitle sample");

        let finalized = muxer.finalize().expect("failed to finalize");
        assert!(!finalized.moov_box_bytes.is_empty());
        // 3 トラック分の trak_box が構築されていることを確認
        assert_eq!(finalized.moov_box().trak_boxes.len(), 3);
        // next_track_id は最後に振った track_id の次の値になる
        assert_eq!(finalized.moov_box().mvhd_box.next_track_id, 4);
    }

    /// 全サンプルが非キーフレームの音声トラックでは空の `stss` を出さず省略すること
    #[test]
    fn test_audio_all_non_keyframe_omits_empty_stss() {
        let mut muxer = Mp4FileMuxer::new().expect("ミューサの作成に失敗した");
        let mut offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::MIN.saturating_add(1000 - 1);
        let sample_size = 256usize;

        for i in 0..3 {
            muxer
                .append_sample(&Sample {
                    track_kind: TrackKind::Audio,
                    sample_entry: (i == 0).then(create_opus_sample_entry),
                    keyframe: false,
                    timescale,
                    duration: 20,
                    composition_time_offset: None,
                    data_offset: offset,
                    data_size: sample_size,
                })
                .expect("音声サンプルの追加に失敗した");
            offset += sample_size as u64;
        }

        let finalized = muxer.finalize().expect("finalize に失敗した");
        let stss = &finalized.moov_box().trak_boxes[0]
            .mdia_box
            .minf_box
            .stbl_box
            .stss_box;
        assert!(
            stss.is_none(),
            "全非キーフレームの音声トラックで空の stss が出力された"
        );
    }

    /// 全サンプルが非キーフレームの字幕トラックでは空の `stss` を出さず省略すること
    #[test]
    fn test_subtitle_all_non_keyframe_omits_empty_stss() {
        let mut muxer = Mp4FileMuxer::new().expect("ミューサの作成に失敗した");
        let mut offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::MIN.saturating_add(1000 - 1);
        let sample_size = 128usize;

        for i in 0..3 {
            muxer
                .append_sample(&Sample {
                    track_kind: TrackKind::Subtitle,
                    sample_entry: (i == 0).then(create_stpp_sample_entry),
                    keyframe: false,
                    timescale,
                    duration: 500,
                    composition_time_offset: None,
                    data_offset: offset,
                    data_size: sample_size,
                })
                .expect("字幕サンプルの追加に失敗した");
            offset += sample_size as u64;
        }

        let finalized = muxer.finalize().expect("finalize に失敗した");
        let stss = &finalized.moov_box().trak_boxes[0]
            .mdia_box
            .minf_box
            .stbl_box
            .stss_box;
        assert!(
            stss.is_none(),
            "全非キーフレームの字幕トラックで空の stss が出力された"
        );
    }

    /// 全サンプルが非キーフレームの映像トラックは空の `stss` を出さずエラーにすること
    #[test]
    fn test_video_all_non_keyframe_rejects_empty_stss() {
        let mut muxer = Mp4FileMuxer::new().expect("ミューサの作成に失敗した");
        let mut offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::MIN.saturating_add(30 - 1);
        let sample_size = 1024usize;

        for i in 0..3 {
            muxer
                .append_sample(&Sample {
                    track_kind: TrackKind::Video,
                    sample_entry: (i == 0).then(create_avc1_sample_entry),
                    keyframe: false,
                    timescale,
                    duration: 1,
                    composition_time_offset: None,
                    data_offset: offset,
                    data_size: sample_size,
                })
                .expect("映像サンプルの追加に失敗した");
            offset += sample_size as u64;
        }

        let err = muxer
            .finalize()
            .expect_err("空 stss 相当の映像トラックを受け入れた");
        assert!(
            matches!(
                err,
                MuxError::NoSyncSamples {
                    track_kind: TrackKind::Video
                }
            ),
            "予期しないエラー: {err}"
        );
    }

    /// 字幕トラック用の [`Sample`] を組み立てる
    fn subtitle_sample(sample_entry: SampleEntry, data_offset: u64) -> Sample {
        Sample {
            track_kind: TrackKind::Subtitle,
            sample_entry: Some(sample_entry),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(1000 - 1),
            duration: 500,
            composition_time_offset: None,
            data_offset,
            data_size: 128,
        }
    }

    /// 字幕トラックにハンドラー種別が異なるサンプルエントリーを混ぜると拒否されることを検証するテスト
    ///
    /// stpp は `subt` + `sthd`、tx3g は `text` + `nmhd` に対応する。
    /// `hdlr` と `media_header` はトラック単位で 1 つしか持てないため、
    /// 混在を許すと `stsd` と矛盾した `trak` が無警告で生成されてしまう
    #[test]
    fn test_mixed_subtitle_sample_entries_error() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        muxer
            .append_sample(&subtitle_sample(create_stpp_sample_entry(), initial_size))
            .expect("failed to append stpp sample");

        // tx3g は stpp と対応表上の組が異なるので拒否される
        let mixed = subtitle_sample(create_tx3g_sample_entry(), initial_size + 128);
        assert!(matches!(
            muxer.append_sample(&mixed),
            Err(MuxError::MixedSampleEntries {
                track_kind: TrackKind::Subtitle
            })
        ));

        // 拒否されても内部状態は不変なので、同じ形式のサンプルなら続けて投入できる
        muxer
            .append_sample(&subtitle_sample(
                create_stpp_sample_entry(),
                initial_size + 128,
            ))
            .expect("failed to append stpp sample after rejection");

        let finalized = muxer.finalize().expect("failed to finalize");
        assert_eq!(finalized.moov_box().trak_boxes.len(), 1);
    }

    /// 同じ対応表の組に属するサンプルエントリー同士は混在させても受け入れられることを検証するテスト
    ///
    /// namespace が異なる stpp はどちらも `subt` + `sthd` に対応するため、
    /// `stsd` に 2 エントリーを並べても `trak` の属性と矛盾しない
    #[test]
    fn test_same_group_subtitle_sample_entries_are_accepted() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        muxer
            .append_sample(&subtitle_sample(create_stpp_sample_entry(), initial_size))
            .expect("failed to append stpp sample");

        let another_stpp = SampleEntry::Stpp(StppBox {
            data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
            namespace: Utf8String::new("http://www.w3.org/ns/ttml#parameter")
                .expect("null 文字を含まない"),
            schema_location: Utf8String::EMPTY,
            auxiliary_mime_types: Utf8String::EMPTY,
            unknown_boxes: vec![],
        });
        muxer
            .append_sample(&subtitle_sample(another_stpp, initial_size + 128))
            .expect("namespace 違いの stpp は受け入れられるべき");

        let finalized = muxer.finalize().expect("failed to finalize");
        let trak = &finalized.moov_box().trak_boxes[0];
        assert_eq!(trak.mdia_box.minf_box.stbl_box.stsd_box.entries.len(), 2);
        assert_eq!(
            trak.mdia_box.hdlr_box.handler_type,
            HdlrBox::HANDLER_TYPE_SUBT
        );
    }

    /// 映像トラックは複数のサンプルエントリーを引き続き受け入れることを検証するテスト
    ///
    /// 字幕トラック向けの混在拒否が映像トラックの既存挙動（解像度違いの許容）に
    /// 波及していないことを確認する
    #[test]
    fn test_video_track_still_accepts_multiple_sample_entries() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let mut first = create_avc1_sample_entry();
        let SampleEntry::Avc1(avc1) = &mut first else {
            panic!("create_avc1_sample_entry must return SampleEntry::Avc1");
        };
        avc1.visual.width = 640;
        avc1.visual.height = 480;

        let mut second = create_avc1_sample_entry();
        let SampleEntry::Avc1(avc1) = &mut second else {
            panic!("create_avc1_sample_entry must return SampleEntry::Avc1");
        };
        avc1.visual.width = 1920;
        avc1.visual.height = 1080;

        for (i, entry) in [first, second].into_iter().enumerate() {
            let sample = Sample {
                track_kind: TrackKind::Video,
                sample_entry: Some(entry),
                keyframe: true,
                timescale: NonZeroU32::MIN.saturating_add(30 - 1),
                duration: 1,
                composition_time_offset: None,
                data_offset: initial_size + (i as u64 * 1024),
                data_size: 1024,
            };
            muxer
                .append_sample(&sample)
                .expect("映像トラックは複数サンプルエントリーを受け入れるべき");
        }

        let finalized = muxer.finalize().expect("failed to finalize");
        let trak = &finalized.moov_box().trak_boxes[0];
        assert_eq!(trak.mdia_box.minf_box.stbl_box.stsd_box.entries.len(), 2);
        // tkhd には全サンプルエントリーの最大値が入る既存挙動を維持する
        assert_eq!(trak.tkhd_box.width, FixedPointNumber::new(1920, 0));
        assert_eq!(trak.tkhd_box.height, FixedPointNumber::new(1080, 0));
    }

    /// `mvhd` に正規化した尺が最長のトラックの timescale / duration が採用されることを検証するテスト
    ///
    /// 映像は 1/30 秒、音声は 5 秒にして音声を最長にしている。
    /// タイムスケール単位の生の値では映像 1 < 音声 5000 だが、
    /// 比較はタイムスケールで正規化した実時間で行われる必要がある
    #[test]
    fn test_mvhd_uses_longest_track() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let video_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer
            .append_sample(&video_sample)
            .expect("failed to append video sample");

        let audio_sample = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: Some(create_opus_sample_entry()),
            keyframe: false,
            timescale: NonZeroU32::MIN.saturating_add(1000 - 1),
            duration: 5000,
            composition_time_offset: None,
            data_offset: initial_size + 1024,
            data_size: 256,
        };
        muxer
            .append_sample(&audio_sample)
            .expect("failed to append audio sample");

        let finalized = muxer.finalize().expect("failed to finalize");
        let mvhd_box = &finalized.moov_box().mvhd_box;
        assert_eq!(
            mvhd_box.timescale.get(),
            1000,
            "最長トラック（音声）の timescale が採用されていない"
        );
        assert_eq!(
            mvhd_box.duration, 5000,
            "最長トラック（音声）の尺が採用されていない"
        );

        // trak は append_sample() の呼び出し順（映像 → 音声）で並ぶ。
        // 映像は media timescale 30 で尺 1 なので、movie timescale 1000 では
        // ceil(1 * 1000 / 30) = 34 になる（換算前の生値 1 のままなら 34 倍短い尺になる）
        let trak_boxes = &finalized.moov_box().trak_boxes;
        assert_eq!(
            trak_boxes[0].tkhd_box.duration, 34,
            "映像の tkhd の duration が mvhd の timescale 単位に換算されていない"
        );
        assert_eq!(
            trak_boxes[1].tkhd_box.duration, 5000,
            "mvhd に採用された音声の tkhd の duration は換算しても変わらない"
        );
    }

    /// 正規化した尺が同着の場合に先に追加したトラックが `mvhd` に採用されることを検証するテスト
    ///
    /// 映像 30/30 と音声 1000/1000 でどちらもちょうど 1 秒にして同着にする。
    /// 映像を先に追加しているので映像側の値が採用される
    #[test]
    fn test_mvhd_tie_breaks_by_append_order() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let video_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 30,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer
            .append_sample(&video_sample)
            .expect("failed to append video sample");

        let audio_sample = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: Some(create_opus_sample_entry()),
            keyframe: false,
            timescale: NonZeroU32::MIN.saturating_add(1000 - 1),
            duration: 1000,
            composition_time_offset: None,
            data_offset: initial_size + 1024,
            data_size: 256,
        };
        muxer
            .append_sample(&audio_sample)
            .expect("failed to append audio sample");

        let finalized = muxer.finalize().expect("failed to finalize");
        let mvhd_box = &finalized.moov_box().mvhd_box;
        assert_eq!(
            mvhd_box.timescale.get(),
            30,
            "同着時は先に追加した映像トラックの timescale が採用されるべき"
        );
        assert_eq!(
            mvhd_box.duration, 30,
            "同着時は先に追加した映像トラックの尺が採用されるべき"
        );

        // 音声は media timescale 1000 で尺 1000 なので、movie timescale 30 では
        // ceil(1000 * 30 / 1000) = 30 になる（換算前の生値 1000 のままなら 33 倍長い尺になる）
        let trak_boxes = &finalized.moov_box().trak_boxes;
        assert_eq!(
            trak_boxes[0].tkhd_box.duration, 30,
            "mvhd に採用された映像の tkhd の duration は換算しても変わらない"
        );
        assert_eq!(
            trak_boxes[1].tkhd_box.duration, 30,
            "音声の tkhd の duration が mvhd の timescale 単位に換算されていない"
        );
    }

    /// faststart 機能の有効化テスト
    #[test]
    fn test_faststart_enabled() {
        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size: 8192,
            ..Default::default()
        };
        let mut muxer =
            Mp4FileMuxer::with_options(options).expect("failed to create muxer with options");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::MIN.saturating_add(30 - 1),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer
            .append_sample(&sample)
            .expect("failed to append sample");

        let finalized = muxer.finalize().expect("failed to finalize");
        assert!(finalized.is_faststart_enabled());
    }

    /// 複数ビデオサンプルのテスト
    #[test]
    fn test_multiple_video_samples() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let mut sample_entry = Some(create_avc1_sample_entry());
        for i in 0..5 {
            let sample = Sample {
                track_kind: TrackKind::Video,
                sample_entry: sample_entry.take(),
                keyframe: i % 2 == 0,
                timescale: NonZeroU32::MIN.saturating_add(30 - 1),
                duration: 1,
                composition_time_offset: None,
                data_offset: initial_size + (i as u64 * 1024),
                data_size: 1024,
            };
            muxer
                .append_sample(&sample)
                .expect("failed to append sample");
        }

        let finalized = muxer.finalize().expect("failed to finalize");
        assert!(!finalized.moov_box_bytes.is_empty());
    }

    fn create_avc1_sample_entry() -> SampleEntry {
        SampleEntry::Avc1(Avc1Box {
            visual: VisualSampleEntryFields {
                data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                width: 1920,
                height: 1080,
                horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
                vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
                frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
                compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
                depth: VisualSampleEntryFields::DEFAULT_DEPTH,
            },
            avcc_box: AvccBox {
                avc_profile_indication: 66,
                profile_compatibility: 0,
                avc_level_indication: 30,
                length_size_minus_one: Uint::new(3),
                sps_list: vec![],
                pps_list: vec![],
                chroma_format: None,
                bit_depth_luma_minus8: None,
                bit_depth_chroma_minus8: None,
                sps_ext_list: vec![],
            },
            unknown_boxes: vec![],
        })
    }

    fn create_opus_sample_entry() -> SampleEntry {
        SampleEntry::Opus(OpusBox {
            audio: AudioSampleEntryFields {
                data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                channelcount: 2,
                samplesize: AudioSampleEntryFields::DEFAULT_SAMPLESIZE,
                samplerate: FixedPointNumber::new(48000u16, 0),
            },
            dops_box: DopsBox {
                output_channel_count: 2,
                pre_skip: 312,
                input_sample_rate: 48000,
                output_gain: 0,
            },
            unknown_boxes: vec![],
        })
    }

    fn create_stpp_sample_entry() -> SampleEntry {
        SampleEntry::Stpp(StppBox {
            data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
            namespace: Utf8String::new("http://www.w3.org/ns/ttml").expect("null 文字を含まない"),
            schema_location: Utf8String::EMPTY,
            auxiliary_mime_types: Utf8String::EMPTY,
            unknown_boxes: vec![],
        })
    }

    fn create_tx3g_sample_entry() -> SampleEntry {
        SampleEntry::Tx3g(Tx3gBox {
            data_reference_index: Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX,
            display_flags: 0,
            horizontal_justification: 0,
            vertical_justification: 0,
            background_color_rgba: [0, 0, 0, 0],
            default_text_box: BoxRecord::default(),
            default_style: StyleRecord::default(),
            ftab_box: FtabBox::default(),
            unknown_boxes: vec![],
        })
    }
}
