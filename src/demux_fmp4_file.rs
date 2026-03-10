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
//! # Examples
//!
//! ```no_run
//! use shiguredo_mp4::demux::{Fmp4FileDemuxer, Input};
//!
//! # fn main() -> Result<(), Box<dyn 'static + std::error::Error>> {
//! let file_data: Vec<u8> = todo!("fMP4 ファイルのバイト列");
//! let mut demuxer = Fmp4FileDemuxer::new();
//!
//! while let Some(required) = demuxer.required_input() {
//!     let start = required.position as usize;
//!     let end = start.saturating_add(required.size.unwrap_or(file_data.len() - start));
//!     demuxer.handle_input(Input {
//!         position: required.position,
//!         data: file_data.get(start..end).unwrap_or(&[]),
//!     });
//! }
//!
//! let tracks = demuxer.tracks()?;
//! println!("{}個のトラックが見つかりました", tracks.len());
//!
//! while let Some(sample) = demuxer.next_sample()? {
//!     println!(
//!         "track_id={}, timestamp={}, size={}",
//!         sample.track.track_id,
//!         sample.timestamp,
//!         sample.data_size,
//!     );
//! }
//! # Ok(())
//! # }
//! ```
use alloc::{collections::VecDeque, format, vec::Vec};
use core::cmp::Ordering;

use crate::{
    BoxHeader, Decode, Error, TrackKind,
    boxes::{FtypBox, HdlrBox, MdatBox, MoofBox, MoovBox, SampleEntry},
    demux_fmp4_segment::{Fmp4SegmentDemuxer, SegmentDemuxError, SegmentSample},
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
        moof_size: usize,
        mdat_offset: u64,
    },
    ReadMediaSegment {
        moof_offset: u64,
        segment_size: usize,
        next_offset: u64,
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
                size: Some(segment_size),
            }),
            Phase::EndOfFile => None,
        }
    }

    /// ファイルデータを入力として受け取り、デマルチプレックス処理を進める
    pub fn handle_input(&mut self, input: Input) {
        if self.handle_input_error.is_none()
            && let Some(required) = self.required_input()
            && !self.input_is_acceptable(required, input)
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

        self.inner
            .handle_init_segment(&data[..box_size])
            .map_err(map_segment_error)?;
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
            moof_size,
            mdat_offset,
        };
        Ok(())
    }

    fn read_mdat_box_header(&mut self, input: Input) -> Result<(), DemuxError> {
        let Phase::ReadMdatBoxHeader {
            moof_offset,
            moof_size,
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
            return Err(DemuxError::DecodeError(Error::invalid_data(
                "expected mdat box after moof",
            )));
        }
        let mdat_size = usize::try_from(mdat_header.box_size.get()).map_err(|_| {
            DemuxError::DecodeError(Error::invalid_data("mdat box size exceeds usize::MAX"))
        })?;
        if mdat_size == 0 {
            return Err(DemuxError::DecodeError(Error::unsupported(
                "mdat box with size=0 is not supported in Fmp4FileDemuxer",
            )));
        }

        let segment_size = moof_size
            .checked_add(mdat_size)
            .ok_or_else(|| DemuxError::DecodeError(Error::invalid_data("segment size overflow")))?;
        let next_offset = moof_offset
            .checked_add(segment_size as u64)
            .ok_or_else(|| {
                DemuxError::DecodeError(Error::invalid_data("segment offset overflow"))
            })?;

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

        let data = self.available_bytes(input, moof_offset, segment_size)?;
        let raw_samples = self
            .inner
            .handle_media_segment(&data[..segment_size])
            .map_err(map_segment_error)?;
        self.enqueue_segment_samples(moof_offset, raw_samples)?;

        self.phase = Phase::ReadTopLevelBoxHeader {
            offset: next_offset,
        };
        Ok(())
    }

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

    fn enqueue_segment_samples(
        &mut self,
        segment_offset: u64,
        raw_samples: Vec<SegmentSample>,
    ) -> Result<(), DemuxError> {
        let mut pending_samples = Vec::new();

        for raw_sample in raw_samples {
            let track_index = self
                .track_infos
                .iter()
                .position(|track_info| track_info.track_id == raw_sample.track_id)
                .ok_or_else(|| {
                    DemuxError::DecodeError(Error::invalid_data(format!(
                        "track_id={} not found in init segment",
                        raw_sample.track_id
                    )))
                })?;
            let data_offset = segment_offset
                .checked_add(raw_sample.data_offset as u64)
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
                composition_time_offset: raw_sample.composition_time_offset.map(i64::from),
                sample_entry: raw_sample.sample_entry,
            });
        }

        pending_samples.sort_by(|lhs, rhs| compare_pending_samples(&self.track_infos, lhs, rhs));
        self.pending_samples.extend(pending_samples);
        Ok(())
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

fn map_segment_error(error: SegmentDemuxError) -> DemuxError {
    match error {
        SegmentDemuxError::DecodeError(error) => DemuxError::DecodeError(error),
        SegmentDemuxError::NotInitialized | SegmentDemuxError::AlreadyInitialized => {
            DemuxError::DecodeError(Error::invalid_data("unexpected fMP4 segment demuxer state"))
        }
        SegmentDemuxError::UnknownTrackId(track_id) => DemuxError::DecodeError(
            Error::invalid_data(format!("unknown track_id in media segment: {track_id}")),
        ),
    }
}
