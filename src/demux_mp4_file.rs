//! MP4 ファイルのデマルチプレックス（分離）機能を提供するモジュール
//!
//! このモジュールは、MP4ファイルに含まれる複数のメディアトラック（音声・映像）から
//! 時系列順にサンプルを抽出するための機能を提供する。
//!
//! # Examples
//!
//! MP4 ファイル全体をメモリに読み込んでデマルチプレックスする例：
//!
//! ```no_run
//! use shiguredo_mp4::demux::{Input, Mp4FileDemuxer};
//!
//! // MP4 ファイル全体をメモリに読み込む
//! let file_data = std::fs::read("sample.mp4").expect("ファイル読み込み失敗");
//!
//! // デマルチプレックス処理を初期化し、ファイルデータ全体を提供する
//! let mut demuxer = Mp4FileDemuxer::new();
//! let input = Input {
//!     position: 0,
//!     data: &file_data,
//! };
//! demuxer.handle_input(input);
//!
//! // トラック情報を取得する
//! let tracks = demuxer.tracks().expect("トラック取得失敗");
//! println!("{}個のトラックが見つかりました", tracks.len());
//! for track in tracks {
//!     println!("トラックID: {}, 種類: {:?}, 尺: {}, タイムスケール: {}",
//!              track.track_id, track.kind, track.duration, track.timescale);
//! }
//!
//! // 時系列順にサンプルを抽出する
//! while let Some(sample) = demuxer.next_sample().expect("サンプル読み込み失敗") {
//!     println!("サンプル - トラックID: {}, タイムスタンプ: {}, サイズ: {} バイト",
//!              sample.track.track_id, sample.timestamp, sample.data_size);
//!     // sample.data_offset の位置から sample.data_size バイトのサンプルデータにアクセス
//!     let sample_data = &file_data[sample.data_offset as usize..
//!                                  sample.data_offset as usize + sample.data_size];
//!     // sample_data を処理...
//! }
//! ```
use alloc::{borrow::ToOwned, format, vec::Vec};
use core::{num::NonZeroU32, time::Duration};

use crate::{
    BoxHeader, Decode, Error, TrackKind,
    aux::{SampleTableAccessor, SampleTableAccessorError},
    boxes::{FtypBox, HdlrBox, MoovBox, SampleEntry, StblBox},
};

/// メディアトラックの情報を表す構造体
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrackInfo {
    /// トラックID
    pub track_id: u32,

    /// トラックの種類
    pub kind: TrackKind,

    /// トラックの尺（タイムスケール単位）
    ///
    /// 秒単位の尺は、この値を `timescale` で割ることで求められる。
    ///
    /// fMP4 の場合は init segment 由来の値であり、実際には 0 になることが多い。
    /// その場合は「未確定ないし実質不明相当」とみなしてよい。
    pub duration: u64,

    /// トラックで使用されているタイムスケール
    pub timescale: NonZeroU32,
}

/// デマルチプレックス処理で抽出されたメディアサンプルを表す構造体
///
/// この構造体は各サンプル（フレーム単位の音声または映像データ）の
/// メタデータとデータ位置情報を保持する
#[derive(Debug, Clone)]
pub struct Sample<'a> {
    /// サンプルが属するトラックの情報
    pub track: &'a TrackInfo,

    /// サンプルの詳細情報
    ///
    /// 前のサンプルから変更がない場合には None になる（最初のサンプルは常に Some となる）
    pub sample_entry: Option<&'a SampleEntry>,

    /// キーフレームであるかの判定
    pub keyframe: bool,

    /// サンプルのタイムスタンプ（トラックのタイムスケール単位）
    ///
    /// 秒単位のタイムスタンプは、この値を `track.timescale` で割ることで求められる
    ///
    /// なお、この値は「同じトラック内の前方に位置するサンプルの尺の累積値」を計算することで求めらている。
    /// すなわち、この値は decode timestamp を表す。
    /// PTS は `timestamp + composition_time_offset` で計算できる。
    pub timestamp: u64,

    /// サンプルの尺（トラックのタイムスケール単位）
    ///
    /// 秒単位の尺は、この値を `track.timescale` で割ることで求められる
    pub duration: u32,

    /// 入力データ内におけるサンプルデータの開始位置（バイト単位）
    ///
    /// file demuxer ではファイル先頭からの絶対位置、
    /// segment demuxer では `handle_media_segment()` に渡した入力バッファ先頭からの相対位置を表す。
    pub data_offset: u64,

    /// サンプルデータのサイズ（バイト単位）
    pub data_size: usize,

    /// コンポジション時間オフセット（トラックのタイムスケール単位）
    ///
    /// 通常 MP4 の `ctts` と fMP4 の `trun` を共通の sample API で扱うため、
    /// `i64` に widening している。
    /// 仕様上すべての入力が 64 bit 必須という意味ではない。
    ///
    /// `ctts` ボックスが存在する場合や、fMP4 の `trun` に
    /// `sample_composition_time_offset` が含まれる場合に設定される。
    /// PTS = timestamp + composition_time_offset で計算できる。
    /// `ctts` ボックスがない場合は `None` となる。
    pub composition_time_offset: Option<i64>,
}

/// デマルチプレックスに必要な入力データの情報を表す構造体
///
/// この構造体は、デマルチプレックス処理を進めるために読み込む必要があるデータの
/// 位置とサイズを示す。この情報をもとに呼び出し元がファイルなどからデータを読み込み、
/// [`Mp4FileDemuxer::handle_input()`] に渡す
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequiredInput {
    /// 必要なデータの開始位置（バイト単位）
    pub position: u64,

    /// 必要なデータのサイズ（バイト単位）
    ///
    ///
    /// ここで指定されたサイズはあくまでもヒントであり、厳密に一致したサイズのデータを提供する必要はない
    /// （特に指定サイズよりも大きいデータを呼び出し元がすでに保持している場合には、それをそのまま渡した方が効率がいい）
    ///
    /// `None` はファイルなどの末尾までを意味する
    pub size: Option<usize>,
}

impl RequiredInput {
    const fn new(position: u64, size: Option<usize>) -> Self {
        Self { position, size }
    }

    /// この [`RequiredInput`] インスタンスを、提供されたデータを使用して [`Input`] に変換する
    pub const fn to_input<'a>(self, data: &'a [u8]) -> Input<'a> {
        Input {
            position: self.position,
            data,
        }
    }

    /// 引数の [`Input`] が、この [`RequiredInput`] の要求を満たしているかどうかを確認する
    pub fn is_satisfied_by(self, input: Input) -> bool {
        let Some(offset) = self.position.checked_sub(input.position) else {
            // 入力データの開始位置が、要求位置よりも後ろにある
            return false;
        };

        if offset > input.data.len() as u64 {
            // 入力データの終端位置が、要求位置よりも前にある
            return false;
        }

        // [NOTE] ここまで来たら「要求位置が入力データの範囲の含まれていること」は確実

        let Some(required_size) = self.size else {
            // 要求サイズがない場合はここで終了（入力にファイル終端までのデータが含まれていると想定する）
            return true;
        };

        let Some(end) = offset.checked_add(required_size as u64) else {
            // 基本はここには来ないはずだけど、入力データが壊れていて
            // required_size に極端に大きな値が設定される可能性もなくはないので、
            // 念のためにハンドリングしておく
            return false;
        };

        if end > input.data.len() as u64 {
            // 要求の終端位置が入力データに含まれていなかった（入力データの終端位置より後ろだった）
            return false;
        }

        true
    }
}

/// [`Mp4FileDemuxer::handle_input()`] に渡す入力データを表す構造体
///
/// [`Mp4FileDemuxer`] 自体は I/O 操作を行わず、各メソッドで I/O 操作が必要になった場合には、
/// [`DemuxError::InputRequired`] を通して呼び出し元への要求が発行される。
/// この構造体は、その要求をうけた呼び出し元が I/O 操作の結果を [`Mp4FileDemuxer`] に伝えるために使用される。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Input<'a> {
    /// バッファ内のデータがファイル内で始まる位置（バイト単位）
    pub position: u64,

    /// ファイルデータのバッファ
    pub data: &'a [u8],
}

impl<'a> Input<'a> {
    pub(crate) fn slice_range(self, position: u64, size: Option<usize>) -> Option<&'a [u8]> {
        let offset = position.checked_sub(self.position)? as usize;
        if offset > self.data.len() {
            return None;
        }

        if let Some(size) = size {
            let end = offset.checked_add(size)?;
            self.data.get(offset..end)
        } else {
            Some(&self.data[offset..])
        }
    }
}

#[derive(Debug, Clone)]
struct TrackState {
    table: SampleTableAccessor<StblBox>,
    timescale: NonZeroU32,
    next_sample_index: NonZeroU32,
}

/// MP4 デマルチプレックス処理中に発生するエラーを表す列挙型
#[non_exhaustive]
#[derive(Clone)]
pub enum DemuxError {
    /// MP4 ボックスのデコード処理中に発生したエラー
    DecodeError(Error),

    /// サンプルテーブル処理中に発生したエラー
    SampleTableError(SampleTableAccessorError),

    /// 操作に対する内部状態が不正
    InvalidState(&'static str),

    /// 入力データの読み込みが必要なことを示すエラー
    ///
    /// このエラーが返された場合、呼び出し元は指定された位置とサイズのファイルデータを
    /// 読み込み、[`Mp4FileDemuxer::handle_input()`] に渡す必要がある
    ///
    /// なお I/O が必要になる可能性がある各メソッドを使用する前に [`Mp4FileDemuxer::required_input()`] を呼び出すことで、
    /// 事前に必要なデータを [`Mp4FileDemuxer`] に供給するが可能となる
    InputRequired(RequiredInput),
}

impl DemuxError {
    fn input_required(position: u64, size: Option<usize>) -> Self {
        Self::InputRequired(RequiredInput::new(position, size))
    }
}

impl From<Error> for DemuxError {
    fn from(error: Error) -> Self {
        DemuxError::DecodeError(error)
    }
}

impl From<SampleTableAccessorError> for DemuxError {
    fn from(error: SampleTableAccessorError) -> Self {
        DemuxError::SampleTableError(error)
    }
}

impl core::fmt::Debug for DemuxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self}")
    }
}

impl core::fmt::Display for DemuxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DemuxError::DecodeError(error) => {
                write!(f, "Failed to decode MP4 box: {error}")
            }
            DemuxError::SampleTableError(error) => {
                write!(f, "Sample table error: {error}")
            }
            DemuxError::InvalidState(message) => write!(f, "{message}"),
            DemuxError::InputRequired(required) => match required.size {
                Some(s) => write!(
                    f,
                    "Need input data: {s} bytes at position {}",
                    required.position
                ),
                None => write!(
                    f,
                    "Need input data: from position {} to end of file",
                    required.position
                ),
            },
        }
    }
}

impl core::error::Error for DemuxError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            DemuxError::DecodeError(error) => Some(error),
            DemuxError::SampleTableError(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    ReadFtypBoxHeader,
    ReadFtypBox {
        box_size: Option<usize>,
    },
    ReadMoovBoxHeader {
        offset: u64,
    },
    ReadMoovBox {
        offset: u64,
        box_size: Option<usize>,
    },
    Initialized,
}

/// MP4 ファイルをデマルチプレックスして、メディアサンプルを取得するための構造体
///
/// この構造体は段階的にファイルデータを処理し、複数のメディアトラックから
/// 時系列順にサンプルを抽出する機能を提供する。
///
/// なお、この構造体自体は I/O 操作は行わないため、
/// ファイル読み込みなどを必要に応じて行うのは利用側の責務となっている。
///
/// また、この構造体はストリーミング用途での使用は想定していない。
#[derive(Debug, Clone)]
pub struct Mp4FileDemuxer {
    phase: Phase,
    track_infos: Vec<TrackInfo>,
    tracks: Vec<TrackState>,
    handle_input_error: Option<DemuxError>,
}

impl Mp4FileDemuxer {
    /// 新しい [`Mp4FileDemuxer`] インスタンスを生成する
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            phase: Phase::ReadFtypBoxHeader,
            track_infos: Vec::new(),
            tracks: Vec::new(),
            handle_input_error: None,
        }
    }

    /// 次の処理を進めるために必要な I/O の位置とサイズを返す
    ///
    /// デマルチプレックス処理が初期化済みの場合は `None` を返す。
    /// それ以外の場合は、必要なファイルデータの情報を返す。
    pub fn required_input(&self) -> Option<RequiredInput> {
        if self.handle_input_error.is_some() {
            return None;
        }

        match self.phase {
            Phase::ReadFtypBoxHeader => Some(RequiredInput::new(0, Some(BoxHeader::MAX_SIZE))),
            Phase::ReadFtypBox { box_size } => Some(RequiredInput::new(0, box_size)),
            Phase::ReadMoovBoxHeader { offset } => {
                Some(RequiredInput::new(offset, Some(BoxHeader::MAX_SIZE)))
            }
            Phase::ReadMoovBox { offset, box_size } => Some(RequiredInput::new(offset, box_size)),
            Phase::Initialized => None,
        }
    }

    /// ファイルデータを入力として受け取り、デマルチプレックス処理を進める
    ///
    /// このメソッドは [`Mp4FileDemuxer::required_input()`] で要求された位置に対応するファイルデータを受け取り、
    /// デマルチプレックス処理を進める
    ///
    /// # NOTE
    ///
    /// `input` 引数では [`Mp4FileDemuxer::required_input()`] が指定した範囲を包含する入力データを
    /// 渡す必要がある
    ///
    /// このメソッドはデータの部分的な消費を行わないため、呼び出し元が必要なデータを
    /// 一度に全て渡す必要がある（つまり、呼び出し元で固定長のバッファを使って複数回に分けてデータを供給することはできない）
    ///
    /// もし、異なる範囲や、不十分なデータサイズの入力（つまり [`RequiredInput::is_satisfied_by()`] が `false` になる入力）が渡された場合には、
    /// [`Mp4FileDemuxer`] はエラー状態に遷移する
    ///
    /// エラー状態に遷移した後は、 [`Mp4FileDemuxer::required_input()`] は常に `None` を返し、
    /// [`Mp4FileDemuxer::tracks()] や [`Mp4FileDemuxer::next_sample()] の次の呼び出しはエラーを返すようになる
    ///
    /// なお [`Mp4FileDemuxer::required_input()`] で指定された範囲よりも多くのデータを渡す分には問題はない
    /// （極端なケースでは、入力ファイル全体のデータを一度に渡してしまっても構わない）
    pub fn handle_input(&mut self, input: Input) {
        if self.handle_input_error.is_none()
            && let Some(required) = self.required_input()
            && !required.is_satisfied_by(input)
        {
            let size_desc = required
                .size
                .map(|s| format!("at least {s} bytes"))
                .unwrap_or_else(|| "data up to EOF".to_owned());
            let reason = format!(
                "handle_input() error: provided input does not contain the required data (expected {size_desc} starting at position {}, but got {} bytes starting at position {})",
                required.position,
                input.data.len(),
                input.position,
            );
            self.handle_input_error = Some(DemuxError::DecodeError(Error::invalid_input(reason)));
            return;
        }

        if let Err(e) = self.handle_input_inner(input)
            && !matches!(e, DemuxError::InputRequired(_))
        {
            // 入力処理中に（入力不足以外の）エラーが出た場合は required_input() との
            // 相互呼び出しで無限ループするのを避けるためにエラー情報を覚えておく
            // （このメソッド自体が Result を返すと、呼び出し元のハンドリングが複雑になるのでそれは避ける）
            self.handle_input_error = Some(e);
        }
    }

    fn handle_input_inner(&mut self, input: Input) -> Result<(), DemuxError> {
        match self.phase {
            Phase::ReadFtypBoxHeader => self.read_ftyp_box_header(input),
            Phase::ReadFtypBox { .. } => self.read_ftyp_box(input),
            Phase::ReadMoovBoxHeader { .. } => self.read_moov_box_header(input),
            Phase::ReadMoovBox { .. } => self.read_moov_box(input),
            Phase::Initialized => Ok(()),
        }
    }

    fn read_ftyp_box_header(&mut self, input: Input) -> Result<(), DemuxError> {
        assert!(matches!(self.phase, Phase::ReadFtypBoxHeader));

        let data_size = Some(BoxHeader::MAX_SIZE);
        let Some(data) = input.slice_range(0, data_size) else {
            return Err(DemuxError::input_required(0, data_size));
        };
        let (header, _header_size) = BoxHeader::decode(data)?;
        header.box_type.expect(FtypBox::TYPE)?;

        let box_size = Some(header.box_size.get() as usize).filter(|n| *n > 0);
        self.phase = Phase::ReadFtypBox { box_size };
        self.handle_input_inner(input)
    }

    fn read_ftyp_box(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadFtypBox { box_size } = self.phase else {
            panic!("bug");
        };
        let Some(data) = input.slice_range(0, box_size) else {
            return Err(DemuxError::input_required(0, box_size));
        };
        let (_ftyp_box, ftyp_box_size) = FtypBox::decode(data)?;
        self.phase = Phase::ReadMoovBoxHeader {
            offset: ftyp_box_size as u64,
        };
        self.handle_input_inner(input)
    }

    fn read_moov_box_header(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadMoovBoxHeader { offset } = self.phase else {
            panic!("bug");
        };

        let data_size = Some(BoxHeader::MAX_SIZE);
        let Some(data) = input.slice_range(offset, data_size) else {
            return Err(DemuxError::input_required(offset, data_size));
        };

        let (header, _header_size) = BoxHeader::decode(data)?;
        let box_size = Some(header.box_size.get()).filter(|n| *n > 0);

        if header.box_type != MoovBox::TYPE {
            let Some(box_size) = box_size else {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "moov box not found",
                )));
            };
            let offset = offset.checked_add(box_size).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("box offset overflow"))
            })?;
            self.phase = Phase::ReadMoovBoxHeader { offset };
        } else {
            let box_size = box_size.map(|n| n as usize);
            self.phase = Phase::ReadMoovBox { offset, box_size };
        }
        self.handle_input_inner(input)
    }

    fn read_moov_box(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadMoovBox { offset, box_size } = self.phase else {
            panic!("bug");
        };

        let Some(data) = input.slice_range(offset, box_size) else {
            return Err(DemuxError::input_required(offset, box_size));
        };
        let (moov_box, _moov_box_size) = MoovBox::decode(data)?;

        for trak_box in moov_box.trak_boxes {
            let track_id = trak_box.tkhd_box.track_id;
            let kind = match trak_box.mdia_box.hdlr_box.handler_type {
                HdlrBox::HANDLER_TYPE_VIDE => TrackKind::Video,
                HdlrBox::HANDLER_TYPE_SOUN => TrackKind::Audio,
                _ => continue,
            };
            let timescale = trak_box.mdia_box.mdhd_box.timescale;
            let table = SampleTableAccessor::new(trak_box.mdia_box.minf_box.stbl_box)?;
            self.track_infos.push(TrackInfo {
                track_id,
                kind,
                duration: trak_box.mdia_box.mdhd_box.duration,
                timescale,
            });
            self.tracks.push(TrackState {
                table,
                timescale,
                next_sample_index: NonZeroU32::MIN,
            })
        }

        self.phase = Phase::Initialized;
        Ok(())
    }

    /// MP4 ファイル内のすべてのメディアトラック情報を取得する
    ///
    /// なお、トラック情報を取得するために I/O 操作が必要な場合は [`DemuxError::InputRequired`] が返される。
    /// その場合、呼び出し元は指定された位置とサイズのファイルデータを読み込み、
    /// [`Mp4FileDemuxer::handle_input()`] に渡した後、再度このメソッドを呼び出す必要がある。
    pub fn tracks(&mut self) -> Result<&[TrackInfo], DemuxError> {
        self.ensure_initialized()?;
        Ok(&self.track_infos)
    }

    /// 時系列順に次のサンプルを取得する
    ///
    /// すべてのトラックから、まだ取得していないものの中で、
    /// 最も早いタイムスタンプを持つサンプルを返す。
    /// サンプルが存在しない場合は `None` が返される。
    ///
    /// なお、次のサンプルの情報を取得するために I/O 操作が必要な場合は [`DemuxError::InputRequired`] が返される。
    /// その場合、呼び出し元は指定された位置とサイズのファイルデータを読み込み、
    /// [`Mp4FileDemuxer::handle_input()`] に渡した後、再度このメソッドを呼び出す必要がある。
    pub fn next_sample(&mut self) -> Result<Option<Sample<'_>>, DemuxError> {
        self.ensure_initialized()?;

        let mut earliest_sample: Option<(Duration, usize)> = None;

        // 全トラックの中で最も早いタイムスタンプを持つサンプルを探す
        for (track_index, track) in self.tracks.iter().enumerate() {
            let Some(sample_accessor) = track.table.get_sample(track.next_sample_index) else {
                continue;
            };
            let timestamp =
                Duration::from_secs(sample_accessor.timestamp()) / track.timescale.get();
            if earliest_sample.as_ref().is_some_and(|s| timestamp >= s.0) {
                continue;
            }

            earliest_sample = Some((timestamp, track_index));
        }

        // 最も早いサンプルを提供したトラックを進める
        if let Some((_timestamp, track_index)) = earliest_sample {
            let sample_index = {
                let track = &mut self.tracks[track_index];
                let sample_index = track.next_sample_index;
                track.next_sample_index = sample_index.checked_add(1).ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data("sample index overflow"))
                })?;
                sample_index
            };
            let sample = self.build_sample(track_index, sample_index);
            Ok(Some(sample))
        } else {
            Ok(None)
        }
    }

    /// 時系列順に前のサンプルを取得する
    ///
    /// すべてのトラックのうち、現在位置より前にあるサンプルから、
    /// 最も遅いタイムスタンプのものを返す。
    /// 同一タイムスタンプのサンプルが複数ある場合は、シーク後の [`Mp4FileDemuxer::next_sample()`] の走査対象に含まれる。
    /// サンプルが存在しない場合は `None` が返される。
    ///
    /// なお、前のサンプルの情報を取得するために I/O 操作が必要な場合は [`DemuxError::InputRequired`] が返される。
    /// その場合、呼び出し元は指定された位置とサイズのファイルデータを読み込み、
    /// [`Mp4FileDemuxer::handle_input()`] に渡した後、再度このメソッドを呼び出す必要がある。
    pub fn prev_sample(&mut self) -> Result<Option<Sample<'_>>, DemuxError> {
        self.ensure_initialized()?;

        let mut latest_sample: Option<(Duration, usize, NonZeroU32)> = None;

        // 全トラックの中で最も遅いタイムスタンプを持つサンプルを探す
        for (track_index, track) in self.tracks.iter().enumerate() {
            let Some(prev_sample_index) = track
                .next_sample_index
                .get()
                .checked_sub(1)
                .and_then(NonZeroU32::new)
            else {
                continue;
            };
            let Some(sample_accessor) = track.table.get_sample(prev_sample_index) else {
                continue;
            };
            let timestamp =
                Duration::from_secs(sample_accessor.timestamp()) / track.timescale.get();
            if latest_sample.as_ref().is_some_and(|s| timestamp <= s.0) {
                continue;
            }
            latest_sample = Some((timestamp, track_index, prev_sample_index));
        }

        if let Some((_timestamp, track_index, sample_index)) = latest_sample {
            self.tracks[track_index].next_sample_index = sample_index;
            let sample = self.build_sample(track_index, sample_index);
            Ok(Some(sample))
        } else {
            Ok(None)
        }
    }

    /// 指定した時刻にシークする
    ///
    /// 各トラックで指定時刻を含むサンプルを選び、次回の [`Mp4FileDemuxer::next_sample()`] が
    /// その位置から開始されるようにする。
    /// つまり次の [`Mp4FileDemuxer::next_sample()`] で返されるサンプルのタイムスタンプは、
    /// 「`position` で指定した位置と同じか、少し前になることがある」ということを意味する。
    ///
    pub fn seek(&mut self, position: Duration) -> Result<(), DemuxError> {
        self.ensure_initialized()?;

        for track in &mut self.tracks {
            let target_timestamp = duration_to_timestamp(position, track.timescale)?;
            if let Some(sample) = track.table.get_sample_by_timestamp(target_timestamp) {
                track.next_sample_index = sample.index();
            } else {
                let next_index = track.table.sample_count().checked_add(1).ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data("sample index overflow"))
                })?;
                track.next_sample_index = NonZeroU32::new(next_index).expect("bug");
            }
        }

        Ok(())
    }

    fn build_sample(&self, track_index: usize, sample_index: NonZeroU32) -> Sample<'_> {
        let track = &self.tracks[track_index];
        let sample_accessor = track.table.get_sample(sample_index).expect("bug");
        let sample_entry = sample_accessor.chunk().sample_entry();
        let sample_entry_index = sample_accessor.chunk().sample_entry_index();

        // サンプルエントリーに変更があるかどうかをチェックする
        let is_new_sample_entry = if let Some(prev_sample_index) =
            sample_index.get().checked_sub(1).and_then(NonZeroU32::new)
        {
            let prev_sample_accessor = track.table.get_sample(prev_sample_index).expect("bug");
            if prev_sample_accessor.chunk().sample_entry_index() == sample_entry_index {
                // サンプルエントリーのインデックスが等しい場合には、内容も常に等しい
                false
            } else {
                prev_sample_accessor.chunk().sample_entry() != sample_entry
            }
        } else {
            // 最初のサンプル
            true
        };

        Sample {
            track: &self.track_infos[track_index],
            sample_entry: is_new_sample_entry.then_some(sample_entry),
            keyframe: sample_accessor.is_sync_sample(),
            timestamp: sample_accessor.timestamp(),
            duration: sample_accessor.duration(),
            data_offset: sample_accessor.data_offset(),
            data_size: sample_accessor.data_size() as usize,
            composition_time_offset: sample_accessor.composition_time_offset(),
        }
    }

    fn ensure_initialized(&mut self) -> Result<(), DemuxError> {
        if let Some(e) = self.handle_input_error.take() {
            Err(e)
        } else if let Some(required) = self.required_input() {
            Err(DemuxError::InputRequired(required))
        } else {
            Ok(())
        }
    }
}

fn duration_to_timestamp(duration: Duration, timescale: NonZeroU32) -> Result<u64, DemuxError> {
    let timescale = u64::from(timescale.get());
    let secs = duration.as_secs();
    let subsec_nanos = u64::from(duration.subsec_nanos());
    let secs_part = secs
        .checked_mul(timescale)
        .ok_or_else(|| DemuxError::DecodeError(Error::invalid_data("timestamp overflow")))?;
    let nanos_part = subsec_nanos
        .checked_mul(timescale)
        .ok_or_else(|| DemuxError::DecodeError(Error::invalid_data("timestamp overflow")))?
        / 1_000_000_000;
    secs_part
        .checked_add(nanos_part)
        .ok_or_else(|| DemuxError::DecodeError(Error::invalid_data("timestamp overflow")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_tracks_from_file_data(file_data: &[u8]) -> Vec<TrackInfo> {
        let input = Input {
            position: 0,
            data: file_data,
        };
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(input);

        let tracks = demuxer.tracks().expect("failed to get tracks").to_vec();

        let mut sample_count = 0;
        let mut keyframe_count = 0;
        while let Some(sample) = demuxer.next_sample().expect("failed to read samples") {
            assert!(sample.data_size > 0);
            assert!(sample.duration > 0);
            sample_count += 1;
            if sample.keyframe {
                keyframe_count += 1;
            }
        }
        assert_ne!(sample_count, 0);
        assert_ne!(keyframe_count, 0);

        tracks
    }

    #[test]
    fn test_read_aac_audio() {
        let tracks =
            read_tracks_from_file_data(include_bytes!("../tests/testdata/beep-aac-audio.mp4"));

        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Audio));
    }

    #[test]
    fn test_read_opus_audio() {
        let tracks =
            read_tracks_from_file_data(include_bytes!("../tests/testdata/beep-opus-audio.mp4"));

        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Audio));
    }

    #[test]
    fn test_read_h264_video() {
        let tracks =
            read_tracks_from_file_data(include_bytes!("../tests/testdata/black-h264-video.mp4"));

        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Video));
    }

    #[test]
    fn test_read_h265_video() {
        let tracks =
            read_tracks_from_file_data(include_bytes!("../tests/testdata/black-h265-video.mp4"));

        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Video));
    }

    #[test]
    fn test_read_vp9_video() {
        let tracks =
            read_tracks_from_file_data(include_bytes!("../tests/testdata/black-vp9-video.mp4"));

        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Video));
    }

    #[test]
    fn test_read_av1_video() {
        let tracks =
            read_tracks_from_file_data(include_bytes!("../tests/testdata/black-av1-video.mp4"));

        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Video));
    }
}
