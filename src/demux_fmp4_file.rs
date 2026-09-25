//! 完全な fMP4 ファイルを incremental にデマルチプレックスするモジュール
//!
//! このモジュールは、`ftyp` + `moov` + `moof`/`mdat` で構成される fMP4 ファイルを
//! 段階的に読み進めながらサンプルを取り出すための機能を提供する。
//!
//! ストリーミング向けの [`crate::demux::Fmp4SegmentDemuxer`] とは異なり、
//! ひとつのファイル内に並んだ複数のセグメントを順番に処理する。
//!
//! # 制限事項
//!
//! `tfhd` の `base_data_offset` フィールドにファイル先頭からの絶対オフセットが
//! 記録されている形式には対応していない。
//!
//! `moof` と `mdat` の間にあるトップレベルボックス（`free` など）は、種別を問わず中身を解釈せずに読み飛ばす。
//! `moof` の後ろで `mdat` より先に別の `moof` が出た場合や、
//! `moof` と `mdat` の間にサイズが 0 のボックス（32 ビットの size=0、または size=1 + largesize=0）がある場合は、
//! `mdat` が存在しない壊れたファイルとしてエラーになる。
//!
//! # Examples
//!
//! ```no_run
//! use shiguredo_mp4::demux::{DemuxError, Fmp4FileDemuxer, Input};
//!
//! # fn main() -> Result<(), Box<dyn 'static + std::error::Error>> {
//! let file_data: Vec<u8> = todo!("bytes of the fMP4 file");
//! let mut demuxer = Fmp4FileDemuxer::new();
//!
//! // `required_input()` が要求する範囲をファイルから取り出して渡す
//! let feed = |demuxer: &mut Fmp4FileDemuxer| {
//!     while let Some(required) = demuxer.required_input() {
//!         let start = required.position as usize;
//!         // 要求範囲がファイル末尾を超える場合は、ファイル末尾までを渡す
//!         let end = required
//!             .size
//!             .map(|size| start.saturating_add(size))
//!             .unwrap_or(file_data.len())
//!             .min(file_data.len());
//!         demuxer.handle_input(Input {
//!             position: required.position,
//!             data: file_data.get(start..end).unwrap_or(&[]),
//!         });
//!     }
//! };
//!
//! feed(&mut demuxer);
//! let tracks = demuxer.tracks()?;
//! println!("Found {} track(s)", tracks.len());
//!
//! loop {
//!     match demuxer.next_sample() {
//!         Ok(Some(sample)) => {
//!             println!(
//!                 "track_id={}, timestamp={}, size={}",
//!                 sample.track.track_id,
//!                 sample.timestamp,
//!                 sample.data_size,
//!             );
//!         }
//!         Ok(None) => break,
//!         // 次のメディアセグメントやファイル末尾を読むための入力が必要になったら、要求された範囲を渡して続ける
//!         Err(DemuxError::InputRequired(_)) => feed(&mut demuxer),
//!         Err(e) => return Err(e.into()),
//!     }
//! }
//! # Ok(())
//! # }
//! ```
use alloc::{collections::VecDeque, format, vec::Vec};
use core::cmp::Ordering;

use crate::{
    BoxHeader, Decode, Error, TrackKind,
    boxes::{FtypBox, HdlrBox, MdatBox, MoofBox, MoovBox, SampleEntry},
    demux_fmp4_segment::Fmp4SegmentDemuxer,
    demux_mp4_file::{DemuxError, Input, RequiredInput, Sample, TrackInfo},
};

#[derive(Debug, Clone)]
struct TrackRuntime {
    sample_entry: Option<SampleEntry>,
}

#[derive(Debug, Clone)]
struct PendingSample {
    track_index: usize,
    timestamp: u64,
    duration: u32,
    keyframe: bool,
    data_offset: u64,
    data_size: usize,
    composition_time_offset: Option<i64>,
    sample_entry: Option<SampleEntry>,
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    ReadFtypBoxHeader,
    ReadFtypBox {
        box_size: usize,
    },
    ReadMoovBoxHeader {
        offset: u64,
    },
    ReadMoovBox {
        offset: u64,
        box_size: usize,
    },
    ReadTopLevelBoxHeader {
        offset: u64,
    },
    ReadMoofBox {
        offset: u64,
        box_size: usize,
    },
    ReadMdatBoxHeader {
        moof_offset: u64,
        mdat_offset: u64,
    },
    ReadMediaSegment {
        moof_offset: u64,
        segment_size: Option<usize>,
        next_offset: Option<u64>,
    },
    EndOfFile,
}

/// 完全な fMP4 ファイルを incremental にデマルチプレックスする構造体
#[derive(Debug, Clone)]
pub struct Fmp4FileDemuxer {
    phase: Phase,
    inner: Fmp4SegmentDemuxer,
    track_infos: Vec<TrackInfo>,
    track_runtimes: Vec<TrackRuntime>,
    pending_samples: VecDeque<PendingSample>,
    handle_input_error: Option<DemuxError>,
}

impl Fmp4FileDemuxer {
    /// 新しい [`Fmp4FileDemuxer`] を生成する
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            phase: Phase::ReadFtypBoxHeader,
            inner: Fmp4SegmentDemuxer::new(),
            track_infos: Vec::new(),
            track_runtimes: Vec::new(),
            pending_samples: VecDeque::new(),
            handle_input_error: None,
        }
    }

    /// 次の処理を進めるために必要な入力範囲を返す
    pub fn required_input(&self) -> Option<RequiredInput> {
        if self.handle_input_error.is_some() || !self.pending_samples.is_empty() {
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
            Phase::ReadMoovBoxHeader { offset } => Some(RequiredInput {
                position: offset,
                size: Some(BoxHeader::MAX_SIZE),
            }),
            Phase::ReadMoovBox { offset, box_size } => Some(RequiredInput {
                position: offset,
                size: Some(box_size),
            }),
            Phase::ReadTopLevelBoxHeader { offset } => Some(RequiredInput {
                position: offset,
                size: Some(BoxHeader::MAX_SIZE),
            }),
            Phase::ReadMoofBox { offset, box_size } => Some(RequiredInput {
                position: offset,
                size: Some(box_size),
            }),
            Phase::ReadMdatBoxHeader { mdat_offset, .. } => Some(RequiredInput {
                position: mdat_offset,
                size: Some(BoxHeader::MAX_SIZE),
            }),
            Phase::ReadMediaSegment {
                moof_offset,
                segment_size,
                ..
            } => Some(RequiredInput {
                position: moof_offset,
                size: segment_size,
            }),
            Phase::EndOfFile => None,
        }
    }

    /// ファイルデータを入力として受け取り、デマルチプレックス処理を進める
    ///
    /// このメソッドは [`Fmp4FileDemuxer::required_input()`] で要求された位置を含むファイルデータを受け取り、
    /// デマルチプレックス処理を進める。
    ///
    /// [`Fmp4FileDemuxer::required_input()`] が指定した範囲よりも多くのデータを渡す分には問題はない。
    /// 入力ファイル全体のデータを渡してもよい。
    /// ただし、[`Mp4FileDemuxer::handle_input()`](crate::demux::Mp4FileDemuxer::handle_input) と異なり、
    /// ファイル全体を渡す場合も [`Fmp4FileDemuxer::required_input()`] が `Some` を返す間は
    /// このメソッドを繰り返し呼び出す必要がある。
    /// [`Fmp4FileDemuxer::next_sample()`] が [`DemuxError::InputRequired`] を返した後も同じである。
    ///
    /// 入力が要求された範囲の終端より手前で終わっている場合は、入力の終端をファイルの終端とみなす。
    /// そのため、ファイルの途中で切れた入力を渡すと、切れた位置までのデータだけが処理される。
    /// 入力の終端が要求された位置と一致する場合は、そこでファイルの終端に達したものとして処理する。
    ///
    /// 入力が要求された位置を含まない場合（要求された位置が入力の終端より後ろにある場合や、
    /// 入力が要求された位置より後ろから始まる場合）は、入力をエラーとして扱い、エラー状態に遷移する。
    /// エラー状態に遷移した後は、[`Fmp4FileDemuxer::tracks()`] や [`Fmp4FileDemuxer::next_sample()`] の
    /// 次の呼び出しがそのエラーを返す
    pub fn handle_input(&mut self, input: Input) {
        if self.handle_input_error.is_none()
            && let Some(required) = self.required_input()
            && !Self::input_is_acceptable(required, input)
        {
            let reason = format!(
                "handle_input() error: expected input starting at position {}, but got {} bytes starting at position {}",
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

    /// 初期化済みのトラック情報を返す
    pub fn tracks(&mut self) -> Result<&[TrackInfo], DemuxError> {
        if let Some(e) = self.handle_input_error.take() {
            return Err(e);
        }
        if !self.is_initialized() {
            return Err(DemuxError::InputRequired(
                self.required_input()
                    .expect("bug: required input missing before initialization"),
            ));
        }
        Ok(&self.track_infos)
    }

    /// 次のサンプルを返す
    pub fn next_sample(&mut self) -> Result<Option<Sample<'_>>, DemuxError> {
        if let Some(e) = self.handle_input_error.take() {
            return Err(e);
        }

        if let Some(pending) = self.pending_samples.pop_front() {
            return Ok(Some(self.build_sample(pending)));
        }

        match self.phase {
            Phase::EndOfFile => Ok(None),
            _ => Err(DemuxError::InputRequired(
                self.required_input()
                    .expect("bug: required input missing before next_sample"),
            )),
        }
    }

    fn handle_input_inner(&mut self, input: Input) -> Result<(), DemuxError> {
        match self.phase {
            Phase::ReadFtypBoxHeader => self.read_ftyp_box_header(input),
            Phase::ReadFtypBox { .. } => self.read_ftyp_box(input),
            Phase::ReadMoovBoxHeader { .. } => self.read_moov_box_header(input),
            Phase::ReadMoovBox { .. } => self.read_moov_box(input),
            Phase::ReadTopLevelBoxHeader { .. } => self.read_top_level_box_header(input),
            Phase::ReadMoofBox { .. } => self.read_moof_box(input),
            Phase::ReadMdatBoxHeader { .. } => self.read_mdat_box_header(input),
            Phase::ReadMediaSegment { .. } => self.read_media_segment(input),
            Phase::EndOfFile => Ok(()),
        }
    }

    fn read_ftyp_box_header(&mut self, input: Input) -> Result<(), DemuxError> {
        let required_size = BoxHeader::MAX_SIZE;
        let data = self.available_bytes(input, 0, required_size)?;
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
            panic!("bug");
        };

        let data = self.available_bytes(input, 0, box_size)?;
        let (_ftyp_box, ftyp_box_size) = FtypBox::decode(&data[..box_size])?;
        self.phase = Phase::ReadMoovBoxHeader {
            offset: ftyp_box_size as u64,
        };
        Ok(())
    }

    fn read_moov_box_header(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadMoovBoxHeader { offset } = self.phase else {
            panic!("bug");
        };

        let data = self.available_bytes(input, offset, BoxHeader::MAX_SIZE)?;
        let (header, _) = BoxHeader::decode(data)?;
        let box_size = usize::try_from(header.box_size.get()).map_err(|_| {
            DemuxError::DecodeError(Error::invalid_data("box size exceeds usize::MAX"))
        })?;

        if box_size == 0 {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "moov box not found",
            )));
        }

        if header.box_type == MoovBox::TYPE {
            self.phase = Phase::ReadMoovBox { offset, box_size };
        } else {
            self.phase = Phase::ReadMoovBoxHeader {
                offset: offset.checked_add(box_size as u64).ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data("box offset overflow"))
                })?,
            };
        }
        Ok(())
    }

    fn read_moov_box(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadMoovBox { offset, box_size } = self.phase else {
            panic!("bug");
        };

        let data = self.available_bytes(input, offset, box_size)?;
        let (moov_box, _) = MoovBox::decode(&data[..box_size])?;

        self.track_infos.clear();
        self.track_runtimes.clear();
        for trak in moov_box.trak_boxes {
            let kind = match trak.mdia_box.hdlr_box.handler_type {
                HdlrBox::HANDLER_TYPE_VIDE => TrackKind::Video,
                HdlrBox::HANDLER_TYPE_SOUN => TrackKind::Audio,
                // 字幕トラックのハンドラー種別は `subt` (stpp) / `text` (wvtt / tx3g) の 2 種類
                HdlrBox::HANDLER_TYPE_SUBT | HdlrBox::HANDLER_TYPE_TEXT => TrackKind::Subtitle,
                _ => continue,
            };

            let track_id = trak.tkhd_box.track_id;
            let timescale = trak.mdia_box.mdhd_box.timescale;
            let duration = trak.mdia_box.mdhd_box.duration;
            self.track_infos.push(TrackInfo {
                track_id,
                kind,
                duration,
                timescale,
            });
            self.track_runtimes
                .push(TrackRuntime { sample_entry: None });
        }

        self.inner.handle_init_segment(&data[..box_size])?;
        self.phase = Phase::ReadTopLevelBoxHeader {
            offset: offset.checked_add(box_size as u64).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("moov offset overflow"))
            })?,
        };
        Ok(())
    }

    fn read_top_level_box_header(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadTopLevelBoxHeader { offset } = self.phase else {
            panic!("bug");
        };

        let Some(data) = input.slice_range(offset, None) else {
            return Err(DemuxError::InputRequired(RequiredInput {
                position: offset,
                size: Some(BoxHeader::MAX_SIZE),
            }));
        };
        if data.is_empty() {
            self.phase = Phase::EndOfFile;
            return Ok(());
        }
        if data.len() < BoxHeader::MIN_SIZE {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "incomplete top-level box header",
            )));
        }

        let (header, _) = BoxHeader::decode(data)?;
        let box_size = usize::try_from(header.box_size.get()).map_err(|_| {
            DemuxError::DecodeError(Error::invalid_data("box size exceeds usize::MAX"))
        })?;

        if box_size == 0 {
            self.phase = Phase::EndOfFile;
            return Ok(());
        }

        if header.box_type == MoofBox::TYPE {
            self.phase = Phase::ReadMoofBox { offset, box_size };
        } else {
            self.phase = Phase::ReadTopLevelBoxHeader {
                offset: offset.checked_add(box_size as u64).ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data("box offset overflow"))
                })?,
            };
        }
        Ok(())
    }

    fn read_moof_box(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadMoofBox { offset, box_size } = self.phase else {
            panic!("bug");
        };

        let data = self.available_bytes(input, offset, box_size)?;
        let (moof_box, moof_size) = MoofBox::decode(&data[..box_size])?;
        for traf in &moof_box.traf_boxes {
            if traf.tfhd_box.base_data_offset.is_some() {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "tfhd with absolute base_data_offset is not supported in Fmp4FileDemuxer",
                )));
            }
        }

        let mdat_offset = offset
            .checked_add(moof_size as u64)
            .ok_or_else(|| DemuxError::DecodeError(Error::invalid_data("mdat offset overflow")))?;
        self.phase = Phase::ReadMdatBoxHeader {
            moof_offset: offset,
            mdat_offset,
        };
        Ok(())
    }

    fn read_mdat_box_header(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadMdatBoxHeader {
            moof_offset,
            mdat_offset,
        } = self.phase
        else {
            panic!("bug");
        };

        let Some(data) = input.slice_range(mdat_offset, None) else {
            return Err(DemuxError::InputRequired(RequiredInput {
                position: mdat_offset,
                size: Some(BoxHeader::MAX_SIZE),
            }));
        };
        if data.is_empty() {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "mdat box not found after moof",
            )));
        }
        if data.len() < BoxHeader::MIN_SIZE {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "incomplete mdat box header",
            )));
        }

        let (mdat_header, _) = BoxHeader::decode(data)?;
        if mdat_header.box_type != MdatBox::TYPE {
            // ISO/IEC 14496-12:2022 の 4.2.2 は、認識できない種別のボックスを無視して読み飛ばすことを求めている。
            // `free` / `skip` (8.1.2) も `moof` と `mdat` の間に置かれ得るため、
            // `mdat` が出るまでトップレベルボックスを種別を問わず読み飛ばす。
            // `moof` が出た場合は、`mdat` が存在しない壊れたファイルとしてエラーにする。
            // なお、ここでの扱いは ISO/IEC 14496-12:2022 に基づくものであり、将来の改訂で変わる可能性がある。
            if mdat_header.box_type == MoofBox::TYPE {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "expected mdat box after moof",
                )));
            }
            let box_size = usize::try_from(mdat_header.box_size.get()).map_err(|_| {
                DemuxError::DecodeError(Error::invalid_data("box size exceeds usize::MAX"))
            })?;
            // 32 ビットの size=0 はコンテナの最後のボックスなので、その後ろに `mdat` は存在し得ず、
            // size=1 + largesize=0 は仕様上の意味が定められていない。どちらも読み飛ばし先を決められない。
            // この検査がないと `mdat_offset` が進まず、`required_input()` が同じ範囲を要求し続ける
            if box_size == 0 {
                return Err(DemuxError::DecodeError(Error::invalid_data(
                    "found box with size=0 between moof and mdat in media segment",
                )));
            }
            let next_offset = mdat_offset.checked_add(box_size as u64).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("box offset overflow"))
            })?;
            self.phase = Phase::ReadMdatBoxHeader {
                moof_offset,
                mdat_offset: next_offset,
            };
            return Ok(());
        }

        let (segment_size, next_offset) = if mdat_header.box_size.get() == 0 {
            (None, None)
        } else {
            let mdat_size = usize::try_from(mdat_header.box_size.get()).map_err(|_| {
                DemuxError::DecodeError(Error::invalid_data("mdat box size exceeds usize::MAX"))
            })?;
            let mdat_end = mdat_offset.checked_add(mdat_size as u64).ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("mdat offset overflow"))
            })?;
            // メディアセグメントの範囲は `moof` の先頭から `mdat` の末尾までとする。
            // `moof` と `mdat` の間に読み飛ばしたボックスがある場合も、その分を範囲に含める。
            // `mdat_offset` は `moof` の直後から始まり読み飛ばしで増えるだけなので、`moof_offset` 以上になる
            let segment_size = usize::try_from(mdat_end - moof_offset).map_err(|_| {
                DemuxError::DecodeError(Error::invalid_data("segment size exceeds usize::MAX"))
            })?;
            (Some(segment_size), Some(mdat_end))
        };

        self.phase = Phase::ReadMediaSegment {
            moof_offset,
            segment_size,
            next_offset,
        };
        Ok(())
    }

    fn read_media_segment(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadMediaSegment {
            moof_offset,
            segment_size,
            next_offset,
        } = self.phase
        else {
            panic!("bug");
        };

        let data = match segment_size {
            Some(segment_size) => self.available_bytes(input, moof_offset, segment_size)?,
            None => {
                let Some(data) = input.slice_range(moof_offset, None) else {
                    return Err(DemuxError::InputRequired(RequiredInput {
                        position: moof_offset,
                        size: None,
                    }));
                };
                if data.is_empty() {
                    return Err(DemuxError::DecodeError(Error::invalid_data(
                        "input ended before the required range was available",
                    )));
                }
                data
            }
        };
        let pending_samples = {
            let track_infos = &self.track_infos;
            let raw_samples = self.inner.handle_media_segment(data)?;
            Self::build_pending_samples(track_infos, moof_offset, raw_samples)?
        };
        self.pending_samples.extend(pending_samples);

        self.phase = if let Some(next_offset) = next_offset {
            Phase::ReadTopLevelBoxHeader {
                offset: next_offset,
            }
        } else {
            Phase::EndOfFile
        };
        Ok(())
    }

    /// 入力から `position` 以降のデータを取り出し、`required_size` バイトに切り詰めて返す
    ///
    /// 入力が要求された位置を含まない場合は [`DemuxError::InputRequired`] を返す。
    /// 入力が要求された位置を含むが、要求された範囲の終端より手前で終わっている場合は、
    /// 入力の終端をファイルの終端とみなして [`DemuxError::DecodeError`] を返す
    fn available_bytes<'a>(
        &self,
        input: Input<'a>,
        position: u64,
        required_size: usize,
    ) -> Result<&'a [u8], DemuxError> {
        let Some(data) = input.slice_range(position, None) else {
            return Err(DemuxError::InputRequired(RequiredInput {
                position,
                size: Some(required_size),
            }));
        };
        if data.len() < required_size {
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "input ended before the required range was available",
            )));
        }
        // 要求より多いデータが渡された場合に、後続のメディアセグメントまで処理しないように切り詰める
        Ok(&data[..required_size])
    }

    /// 受け取った入力が、要求された位置を含んでいるかどうかを確認する
    ///
    /// 要求された範囲を満たす入力だけでなく、要求された位置を含みながら
    /// 要求された範囲の終端より手前で終わっている入力も受け付ける。
    /// 後者は入力の終端をファイルの終端とみなす（`available_bytes` と同じ判定である）。
    /// 要求された位置が入力の終端より後ろにある場合と、入力が要求された位置より後ろから始まる場合は
    /// 受け付けない
    fn input_is_acceptable(required: RequiredInput, input: Input) -> bool {
        // `available_bytes` と同じく、`Input::slice_range()` が要求された位置を含むかどうかで判定する
        input.slice_range(required.position, None).is_some()
    }

    fn is_initialized(&self) -> bool {
        matches!(
            self.phase,
            Phase::ReadTopLevelBoxHeader { .. }
                | Phase::ReadMoofBox { .. }
                | Phase::ReadMdatBoxHeader { .. }
                | Phase::ReadMediaSegment { .. }
                | Phase::EndOfFile
        )
    }

    fn build_pending_samples(
        track_infos: &[TrackInfo],
        segment_offset: u64,
        raw_samples: Vec<Sample<'_>>,
    ) -> Result<Vec<PendingSample>, DemuxError> {
        let mut pending_samples = Vec::new();

        for raw_sample in raw_samples {
            let track_index = track_infos
                .iter()
                .position(|track_info| track_info.track_id == raw_sample.track.track_id)
                .ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data(format!(
                        "track_id={} not found in init segment",
                        raw_sample.track.track_id
                    )))
                })?;
            let data_offset = segment_offset
                .checked_add(raw_sample.data_offset)
                .ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data(
                        "sample data absolute offset overflow",
                    ))
                })?;

            pending_samples.push(PendingSample {
                track_index,
                timestamp: raw_sample.timestamp,
                duration: raw_sample.duration,
                keyframe: raw_sample.keyframe,
                data_offset,
                data_size: raw_sample.data_size,
                composition_time_offset: raw_sample.composition_time_offset,
                sample_entry: raw_sample.sample_entry.cloned(),
            });
        }

        pending_samples.sort_by(|lhs, rhs| compare_pending_samples(track_infos, lhs, rhs));
        Ok(pending_samples)
    }

    fn build_sample(&mut self, pending: PendingSample) -> Sample<'_> {
        let PendingSample {
            track_index,
            timestamp,
            duration,
            keyframe,
            data_offset,
            data_size,
            composition_time_offset,
            sample_entry,
        } = pending;

        let has_sample_entry = sample_entry.is_some();
        if let Some(sample_entry) = sample_entry {
            self.track_runtimes[track_index].sample_entry = Some(sample_entry);
        }

        let track_info = &self.track_infos[track_index];
        let track_runtime = &self.track_runtimes[track_index];

        Sample {
            track: track_info,
            sample_entry: has_sample_entry.then_some(
                track_runtime
                    .sample_entry
                    .as_ref()
                    .expect("bug: sample entry must be cached before borrowing"),
            ),
            keyframe,
            timestamp,
            duration,
            data_offset,
            data_size,
            composition_time_offset,
        }
    }
}

fn compare_pending_samples(
    track_infos: &[TrackInfo],
    lhs: &PendingSample,
    rhs: &PendingSample,
) -> Ordering {
    let lhs_scaled =
        u128::from(lhs.timestamp) * u128::from(track_infos[rhs.track_index].timescale.get());
    let rhs_scaled =
        u128::from(rhs.timestamp) * u128::from(track_infos[lhs.track_index].timescale.get());

    lhs_scaled
        .cmp(&rhs_scaled)
        .then_with(|| lhs.track_index.cmp(&rhs.track_index))
        .then_with(|| lhs.data_offset.cmp(&rhs.data_offset))
}
