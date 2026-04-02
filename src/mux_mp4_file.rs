//! MP4 ファイルのマルチプレックス実装を提供するモジュール
//!
//! このモジュールは、複数のメディアトラック（音声・映像）からのサンプルを
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
//! let sample_entry = todo!("使用するコーデックに合わせたサンプルエントリーを構築する");
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
    BoxHeader, BoxSize, Either, Encode, Error, FixedPointNumber, Mp4FileTime, TrackKind,
    Utf8String,
    boxes::{
        Brand, Co64Box, CttsBox, CttsEntry, DinfBox, FreeBox, FtypBox, HdlrBox, MdatBox, MdhdBox,
        MdiaBox, MinfBox, MoovBox, MvhdBox, SampleEntry, SmhdBox, StblBox, StcoBox, StscBox,
        StscEntry, StsdBox, StssBox, StszBox, SttsBox, TkhdBox, TrakBox, VmhdBox,
    },
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
    // - stss_box（同期サンプル）: キーフレームあたり ~4 バイト（最悪の場合はすべてキーフレーム）
    // - stco_box/co64_box（チャンクオフセット）: チャンクあたり ~8 バイト
    const BYTES_PER_SAMPLE: usize = 16;

    BASE_MOOV_OVERHEAD
        + (sample_count_per_track.len() * PER_TRACK_OVERHEAD)
        + (sample_count_per_track.iter().sum::<usize>() * BYTES_PER_SAMPLE)
}

/// [`Mp4FileMuxer`] 用のオプション
#[derive(Debug, Clone)]
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
}

impl Default for Mp4FileMuxerOptions {
    fn default() -> Self {
        Self {
            reserved_moov_box_size: 0,
            creation_timestamp: Duration::ZERO,
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
#[derive(Debug, Clone)]
pub struct Sample {
    /// サンプルのトラック種別
    pub track_kind: TrackKind,

    /// サンプルの詳細情報（コーデック種別など）
    ///
    /// 最初のサンプルでは必須。以降は省略可能で、
    /// 省略した場合は前のサンプルと同じ sample_entry が使用される
    pub sample_entry: Option<SampleEntry>,

    /// キーフレームかどうか
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
#[non_exhaustive]
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

    /// 同一セグメント内の同一トラックで複数の sample entry が混在している
    MixedSampleEntries {
        /// 対象トラックの種類
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
                write!(
                    f,
                    "{track_kind:?} track uses multiple sample entries within one segment"
                )
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

/// MP4 ファイルを生成するマルチプレックス処理を行うための構造体
///
/// この構造体は、複数のメディアトラック（音声・映像）からのサンプルを
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
    audio_chunks: Vec<Chunk>,
    video_chunks: Vec<Chunk>,
    audio_track_timescale: NonZeroU32,
    video_track_timescale: NonZeroU32,
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
            audio_chunks: Vec::new(),
            video_chunks: Vec::new(),

            // 以下の値は、トラックの最初のサンプルを処理する際に、
            // 実際の値で更新されるので、ここでは任意の初期値を指定しておけばいい
            audio_track_timescale: NonZeroU32::MIN,
            video_track_timescale: NonZeroU32::MIN,
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
    pub fn advance_position(&mut self, size: u64) -> Result<(), MuxError> {
        if self.finalized_boxes.is_some() {
            return Err(MuxError::AlreadyFinalized);
        }
        self.next_position += size;
        if size > 0 {
            self.last_sample_kind = None;
        }
        Ok(())
    }

    /// 映像ないし音声サンプルのデータを MP4 ファイルに追記したことを [`Mp4FileMuxer`] に通知する
    ///
    /// 実際のデータ追記処理自体は利用側の責務であり、
    /// このメソッド目的は、その追記結果などを伝えることで、
    /// [`Mp4FileMuxer`] が適切に、MP4ファイルの再生に必要なメタデータを構築できるようにすることである。
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
            size: sample.data_size as u32,
            composition_time_offset: sample.composition_time_offset,
        };

        let is_new_chunk_needed = self.is_new_chunk_needed(sample);

        let chunks = match sample.track_kind {
            TrackKind::Audio => {
                // 最初のサンプルのタイムスケールをトラックのタイムスケールにする
                if self.audio_chunks.is_empty() {
                    self.audio_track_timescale = sample.timescale;
                } else if self.audio_track_timescale != sample.timescale {
                    return Err(MuxError::TimescaleMismatch {
                        track_kind: TrackKind::Audio,
                        expected: self.audio_track_timescale,
                        actual: sample.timescale,
                    });
                }

                &mut self.audio_chunks
            }
            TrackKind::Video => {
                // 最初のサンプルのタイムスケールをトラックのタイムスケールにする
                if self.video_chunks.is_empty() {
                    self.video_track_timescale = sample.timescale;
                } else if self.video_track_timescale != sample.timescale {
                    return Err(MuxError::TimescaleMismatch {
                        track_kind: TrackKind::Video,
                        expected: self.video_track_timescale,
                        actual: sample.timescale,
                    });
                }

                &mut self.video_chunks
            }
        };

        if is_new_chunk_needed {
            let sample_entry = sample
                .sample_entry
                .clone()
                .or_else(|| chunks.last().map(|c| c.sample_entry.clone()))
                .ok_or(MuxError::MissingSampleEntry {
                    track_kind: sample.track_kind,
                })?;

            chunks.push(Chunk {
                offset: sample.data_offset,
                sample_entry,
                samples: Vec::new(),
            });
        }

        chunks.last_mut().expect("bug").samples.push(metadata);

        self.next_position += sample.data_size as u64;
        self.last_sample_kind = Some(sample.track_kind);
        Ok(())
    }

    fn is_new_chunk_needed(&self, sample: &Sample) -> bool {
        if self.last_sample_kind != Some(sample.track_kind) {
            return true;
        }

        let chunks = match sample.track_kind {
            TrackKind::Audio => &self.audio_chunks,
            TrackKind::Video => &self.video_chunks,
        };

        let Some(sample_entry) = &sample.sample_entry else {
            return false;
        };

        chunks
            .last()
            .is_none_or(|c| c.sample_entry != *sample_entry)
    }

    /// すべてのサンプルの追加が完了したことを [`Mp4FileMuxer`] に通知する
    ///
    /// このメソッドが呼び出されると、[`Mp4FileMuxer`] はそれまでの情報を用いて、
    /// MP4 ファイルの再生に必要な修正やメタデータの構築を行う。
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

        for chunk in self.audio_chunks.iter().chain(self.video_chunks.iter()) {
            match chunk.sample_entry {
                SampleEntry::Avc1(_) => has_avc1 = true,
                SampleEntry::Hev1(_) => has_hev1 = true,
                SampleEntry::Hvc1(_) => has_hvc1 = true,
                SampleEntry::Av01(_) => has_av01 = true,
                _ => {}
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
        let mut trak_boxes = Vec::new();

        if !self.audio_chunks.is_empty() {
            let track_id = trak_boxes.len() as u32 + 1;
            trak_boxes.push(self.build_audio_trak_box(track_id)?);
        }

        if !self.video_chunks.is_empty() {
            let track_id = trak_boxes.len() as u32 + 1;
            trak_boxes.push(self.build_video_trak_box(track_id)?);
        }

        let creation_time = Mp4FileTime::from_unix_time(self.options.creation_timestamp);
        let (timescale, duration) = self.calculate_total_duration();
        let mvhd_box = MvhdBox {
            creation_time,
            modification_time: creation_time,
            timescale,
            duration,
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

    fn build_audio_trak_box(&self, track_id: u32) -> Result<TrakBox, MuxError> {
        let total_duration = self
            .audio_chunks
            .iter()
            .flat_map(|c| c.samples.iter().map(|s| s.duration as u64))
            .sum::<u64>();

        let creation_time = Mp4FileTime::from_unix_time(self.options.creation_timestamp);
        let tkhd_box = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time,
            modification_time: creation_time,
            track_id,
            duration: total_duration,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: TkhdBox::DEFAULT_AUDIO_VOLUME,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: FixedPointNumber::default(),
            height: FixedPointNumber::default(),
        };

        Ok(TrakBox {
            tkhd_box,
            edts_box: None,
            mdia_box: self.build_audio_mdia_box()?,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_video_trak_box(&self, track_id: u32) -> Result<TrakBox, MuxError> {
        let total_duration = self
            .video_chunks
            .iter()
            .flat_map(|c| c.samples.iter().map(|s| s.duration as u64))
            .sum::<u64>();

        let (max_width, max_height) = self
            .video_chunks
            .iter()
            .filter_map(|c| c.sample_entry.video_resolution())
            .fold((0u16, 0u16), |(max_w, max_h), (w, h)| {
                (max_w.max(w), max_h.max(h))
            });

        let creation_time = Mp4FileTime::from_unix_time(self.options.creation_timestamp);
        let tkhd_box = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time,
            modification_time: creation_time,
            track_id,
            duration: total_duration,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: TkhdBox::DEFAULT_VIDEO_VOLUME,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: FixedPointNumber::new(max_width as i16, 0),
            height: FixedPointNumber::new(max_height as i16, 0),
        };

        Ok(TrakBox {
            tkhd_box,
            edts_box: None,
            mdia_box: self.build_video_mdia_box()?,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_audio_mdia_box(&self) -> Result<MdiaBox, MuxError> {
        let total_duration = self
            .audio_chunks
            .iter()
            .flat_map(|c| c.samples.iter().map(|s| s.duration as u64))
            .sum::<u64>();

        let creation_time = Mp4FileTime::from_unix_time(self.options.creation_timestamp);
        let mdhd_box = MdhdBox {
            creation_time,
            modification_time: creation_time,
            timescale: self.audio_track_timescale,
            duration: total_duration,
            language: MdhdBox::LANGUAGE_UNDEFINED,
        };

        let hdlr_box = HdlrBox {
            handler_type: HdlrBox::HANDLER_TYPE_SOUN,
            name: Utf8String::EMPTY.into_null_terminated_bytes(),
        };

        let minf_box = MinfBox {
            smhd_or_vmhd_box: Some(Either::A(SmhdBox::default())),
            dinf_box: DinfBox::LOCAL_FILE,
            stbl_box: self.build_stbl_box(&self.audio_chunks)?,
            unknown_boxes: Vec::new(),
        };

        Ok(MdiaBox {
            mdhd_box,
            hdlr_box,
            minf_box,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_video_mdia_box(&self) -> Result<MdiaBox, MuxError> {
        let total_duration = self
            .video_chunks
            .iter()
            .flat_map(|c| c.samples.iter().map(|s| s.duration as u64))
            .sum::<u64>();

        let creation_time = Mp4FileTime::from_unix_time(self.options.creation_timestamp);
        let mdhd_box = MdhdBox {
            creation_time,
            modification_time: creation_time,
            timescale: self.video_track_timescale,
            duration: total_duration,
            language: MdhdBox::LANGUAGE_UNDEFINED,
        };

        let hdlr_box = HdlrBox {
            handler_type: HdlrBox::HANDLER_TYPE_VIDE,
            name: Utf8String::EMPTY.into_null_terminated_bytes(),
        };

        let minf_box = MinfBox {
            smhd_or_vmhd_box: Some(Either::B(VmhdBox::default())),
            dinf_box: DinfBox::LOCAL_FILE,
            stbl_box: self.build_stbl_box(&self.video_chunks)?,
            unknown_boxes: Vec::new(),
        };

        Ok(MdiaBox {
            mdhd_box,
            hdlr_box,
            minf_box,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_stbl_box(&self, chunks: &[Chunk]) -> Result<StblBox, MuxError> {
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
        );
        let ctts_box = build_ctts_box(chunks)?;

        let stsc_box = StscBox {
            entries: chunks
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let sample_description_index = sample_entries
                        .iter()
                        .position(|entry| entry == &c.sample_entry)
                        .map(|idx| NonZeroU32::MIN.saturating_add(idx as u32))
                        .expect("sample_entry should exist in sample_entries");
                    StscEntry {
                        first_chunk: NonZeroU32::MIN.saturating_add(i as u32),
                        sample_per_chunk: c.samples.len() as u32,
                        sample_description_index,
                    }
                })
                .collect(),
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

        let is_all_keyframe = chunks.iter().all(|c| c.samples.iter().all(|s| s.keyframe));
        let stss_box = if is_all_keyframe {
            None
        } else {
            Some(StssBox {
                sample_numbers: chunks
                    .iter()
                    .flat_map(|c| c.samples.iter())
                    .enumerate()
                    .filter_map(|(i, s)| {
                        s.keyframe
                            .then_some(NonZeroU32::MIN.saturating_add(i as u32))
                    })
                    .collect(),
            })
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

    fn calculate_total_duration(&self) -> (NonZeroU32, u64) {
        let audio_duration = self
            .audio_chunks
            .iter()
            .flat_map(|c| c.samples.iter().map(|s| s.duration as u64))
            .sum::<u64>();
        let video_duration = self
            .video_chunks
            .iter()
            .flat_map(|c| c.samples.iter().map(|s| s.duration as u64))
            .sum::<u64>();

        let normalized_audio_duration =
            Duration::from_secs(audio_duration) / self.audio_track_timescale.get();
        let normalized_video_duration =
            Duration::from_secs(video_duration) / self.video_track_timescale.get();

        if normalized_audio_duration < normalized_video_duration {
            (self.video_track_timescale, video_duration)
        } else {
            (self.audio_track_timescale, audio_duration)
        }
    }
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
    use crate::{
        Uint,
        boxes::{
            AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, OpusBox, VisualSampleEntryFields,
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
        };
        let muxer =
            Mp4FileMuxer::with_options(options).expect("failed to create muxer with options");
        assert!(!muxer.initial_boxes_bytes().is_empty());
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
}
