//! MP4 ファイルの種別を incremental に判定するモジュール
//!
//! このモジュールは、巨大ファイルや non-faststart な MP4 ファイルに対しても、
//! 必要な位置だけを段階的に読み込みながら MP4 / fragmented MP4 を判定するための
//! 機能を提供する。
//!
//! 判定は `moov` ボックス内の `mvex` ボックスの有無に基づいて行う。
//!
//! # Examples
//!
//! ```no_run
//! use shiguredo_mp4::demux::{Input, Mp4FileKind, Mp4FileKindDetector};
//!
//! # fn main() -> Result<(), Box<dyn 'static + std::error::Error>> {
//! let file_data: Vec<u8> = todo!("MP4 ファイルのバイト列");
//! let mut detector = Mp4FileKindDetector::new();
//!
//! while let Some(required) = detector.required_input() {
//!     let start = required.position as usize;
//!     let end = required
//!         .size
//!         .map(|size| start.saturating_add(size))
//!         .unwrap_or(file_data.len())
//!         .min(file_data.len());
//!     detector.handle_input(Input {
//!         position: required.position,
//!         data: file_data.get(start..end).unwrap_or(&[]),
//!     });
//!
//!     if let Some(kind) = detector.file_kind()? {
//!         match kind {
//!             Mp4FileKind::Mp4 => println!("regular MP4"),
//!             Mp4FileKind::FragmentedMp4 => println!("fragmented MP4"),
//!         }
//!         break;
//!     }
//! }
//! # Ok(())
//! # }
//! ```
use alloc::{format, string::String};

use crate::{
    BoxHeader, Decode, Error,
    boxes::{FtypBox, MoovBox},
    demux_mp4_file::{DemuxError, Input, RequiredInput},
};

/// MP4 ファイルの種別を表す列挙型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mp4FileKind {
    /// 通常の MP4 ファイル
    Mp4,

    /// `mvex` を含む fragmented MP4 ファイル
    FragmentedMp4,
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    ReadFtypBoxHeader,
    ReadFtypBox {
        box_size: usize,
    },
    ReadTopLevelBoxHeader {
        offset: u64,
    },
    ReadMoovBox {
        offset: u64,
        box_size: Option<usize>,
    },
    Detected {
        kind: Mp4FileKind,
    },
}

/// MP4 ファイルの種別を incremental に判定する構造体
#[derive(Debug, Clone)]
pub struct Mp4FileKindDetector {
    phase: Phase,
    handle_input_error: Option<DemuxError>,
}

impl Default for Mp4FileKindDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl Mp4FileKindDetector {
    /// 新しい [`Mp4FileKindDetector`] を生成する
    pub const fn new() -> Self {
        Self {
            phase: Phase::ReadFtypBoxHeader,
            handle_input_error: None,
        }
    }

    /// 次の判定を進めるために必要な入力範囲を返す
    pub fn required_input(&self) -> Option<RequiredInput> {
        if self.handle_input_error.is_some() {
            return None;
        }

        match self.phase {
            Phase::ReadFtypBoxHeader => Some(RequiredInput {
                position: 0,
                size: Some(BoxHeader::MAX_SIZE),
            }),
            Phase::ReadFtypBox { box_size } => Some(RequiredInput {
                position: 0,
                size: Some(box_size),
            }),
            Phase::ReadTopLevelBoxHeader { offset } => Some(RequiredInput {
                position: offset,
                size: Some(BoxHeader::MAX_SIZE),
            }),
            Phase::ReadMoovBox { offset, box_size } => Some(RequiredInput {
                position: offset,
                size: box_size,
            }),
            Phase::Detected { .. } => None,
        }
    }

    /// ファイルデータを入力として受け取り、判定処理を進める
    pub fn handle_input(&mut self, input: Input) {
        if self.handle_input_error.is_none()
            && let Some(required) = self.required_input()
            && !self.input_is_acceptable(required, input)
        {
            let size_desc = required
                .size
                .map(|size| format!("at least {size} bytes"))
                .unwrap_or_else(|| String::from("data up to EOF"));
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
            self.handle_input_error = Some(e);
        }
    }

    /// 判定結果を返す
    ///
    /// まだ `moov` まで到達していない場合は `Ok(None)` を返す。
    pub fn file_kind(&self) -> Result<Option<Mp4FileKind>, DemuxError> {
        if let Some(e) = &self.handle_input_error {
            return Err(e.clone());
        }

        let kind = match self.phase {
            Phase::Detected { kind } => Some(kind),
            _ => None,
        };
        Ok(kind)
    }

    fn handle_input_inner(&mut self, input: Input) -> Result<(), DemuxError> {
        match self.phase {
            Phase::ReadFtypBoxHeader => self.read_ftyp_box_header(input),
            Phase::ReadFtypBox { .. } => self.read_ftyp_box(input),
            Phase::ReadTopLevelBoxHeader { .. } => self.read_top_level_box_header(input),
            Phase::ReadMoovBox { .. } => self.read_moov_box(input),
            Phase::Detected { .. } => Ok(()),
        }
    }

    fn read_ftyp_box_header(&mut self, input: Input) -> Result<(), DemuxError> {
        let data = self.available_bytes(input, 0, Some(BoxHeader::MAX_SIZE))?;
        let (header, _) = BoxHeader::decode(data)?;
        header.box_type.expect(FtypBox::TYPE)?;

        let box_size = usize::try_from(header.box_size.get()).map_err(|_| {
            DemuxError::DecodeError(Error::invalid_data("ftyp box size exceeds usize::MAX"))
        })?;
        if box_size == 0 {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "ftyp box size must be non-zero",
            )));
        }

        self.phase = Phase::ReadFtypBox { box_size };
        Ok(())
    }

    fn read_ftyp_box(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadFtypBox { box_size } = self.phase else {
            unreachable!("bug: invalid phase for read_ftyp_box");
        };

        let data = self.available_bytes(input, 0, Some(box_size))?;
        let (_ftyp_box, ftyp_box_size) = FtypBox::decode(&data[..box_size])?;
        self.phase = Phase::ReadTopLevelBoxHeader {
            offset: ftyp_box_size as u64,
        };
        Ok(())
    }

    fn read_top_level_box_header(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadTopLevelBoxHeader { offset } = self.phase else {
            unreachable!("bug: invalid phase for read_top_level_box_header");
        };

        if input.position == offset && input.data.is_empty() {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "moov box not found before EOF",
            )));
        }

        let data = self.available_bytes(input, offset, Some(BoxHeader::MAX_SIZE))?;
        let (header, _) = BoxHeader::decode(data)?;

        if header.box_type == MoovBox::TYPE {
            let box_size = if header.box_size.get() == 0 {
                None
            } else {
                Some(usize::try_from(header.box_size.get()).map_err(|_| {
                    DemuxError::DecodeError(Error::invalid_data("moov box size exceeds usize::MAX"))
                })?)
            };
            self.phase = Phase::ReadMoovBox { offset, box_size };
            return Ok(());
        }

        if header.box_size.get() == 0 {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "top-level box with size=0 before moov is not supported",
            )));
        }

        let next_offset = offset
            .checked_add(header.box_size.get())
            .ok_or_else(|| DemuxError::DecodeError(Error::invalid_data("box offset overflow")))?;
        self.phase = Phase::ReadTopLevelBoxHeader {
            offset: next_offset,
        };
        Ok(())
    }

    fn read_moov_box(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadMoovBox { offset, box_size } = self.phase else {
            unreachable!("bug: invalid phase for read_moov_box");
        };

        let data = self.available_bytes(input, offset, box_size)?;
        let decode_input = if let Some(box_size) = box_size {
            &data[..box_size]
        } else {
            data
        };
        let (moov_box, _) = MoovBox::decode(decode_input)?;
        let kind = if moov_box.mvex_box.is_some() {
            Mp4FileKind::FragmentedMp4
        } else {
            Mp4FileKind::Mp4
        };
        self.phase = Phase::Detected { kind };
        Ok(())
    }

    fn available_bytes<'a>(
        &self,
        input: Input<'a>,
        position: u64,
        required_size: Option<usize>,
    ) -> Result<&'a [u8], DemuxError> {
        let Some(data) = input.slice_range(position, None) else {
            return Err(DemuxError::InputRequired(RequiredInput {
                position,
                size: required_size,
            }));
        };

        if let Some(required_size) = required_size
            && data.len() < required_size
        {
            if input.position == position {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "input ended before the required range was available",
                )));
            }
            return Err(DemuxError::InputRequired(RequiredInput {
                position,
                size: Some(required_size),
            }));
        }

        Ok(data)
    }

    fn input_is_acceptable(&self, required: RequiredInput, input: Input) -> bool {
        required.is_satisfied_by(input)
            || (input.position == required.position
                && required
                    .size
                    .is_some_and(|required_size| input.data.len() < required_size))
    }
}
