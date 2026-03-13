use std::{num::NonZeroU32, time::Duration};

use orfail::OrFail;
use serde::Serialize;
use shiguredo_mp4::{
    Decode, Either, Encode, FixedPointNumber, Mp4File, Mp4FileTime, Utf8String,
    aux::SampleTableAccessor,
    boxes::{
        Brand, DinfBox, FtypBox, HdlrBox, MdatBox, MdhdBox, MdiaBox, MinfBox, MoovBox, MvhdBox,
        RootBox, SampleEntry, SmhdBox, StblBox, StcoBox, StscBox, StscEntry, StsdBox, StssBox,
        StszBox, SttsBox, TkhdBox, TrakBox, VmhdBox,
    },
};

// 出力側はマイクロ秒に決め打ち
const OUTPUT_TIMESCALE: NonZeroU32 = NonZeroU32::MIN.saturating_add(1_000_000 - 1);

#[derive(Debug)]
pub struct OutputMp4Builder {
    tracks: Vec<Track>,
    chunk_offsets: Vec<u32>,
    file_size: u32,
}

impl OutputMp4Builder {
    pub fn new(tracks: Vec<Track>) -> Self {
        Self {
            tracks,
            chunk_offsets: Vec::new(),
            file_size: 0,
        }
    }

    pub fn build(mut self) -> orfail::Result<Mp4File> {
        let ftyp_box = self.build_ftyp_box();
        let mdat_box = self.build_mdat_box();
        let moov_box = self.build_moov_box().or_fail()?;
        Ok(Mp4File {
            ftyp_box,
            boxes: vec![RootBox::Mdat(mdat_box), RootBox::Moov(moov_box)],
        })
    }

    fn build_ftyp_box(&mut self) -> FtypBox {
        let ftyp_box = FtypBox {
            major_brand: Brand::ISOM,
            minor_version: 0,
            compatible_brands: vec![
                Brand::ISOM,
                Brand::ISO2,
                Brand::MP41,
                Brand::AVC1,
                Brand::AV01,
            ],
        };
        self.file_size += ftyp_box.encode_to_vec().map(|b| b.len()).expect("bug") as u32;
        ftyp_box
    }

    fn build_mdat_box(&mut self) -> MdatBox {
        let mut mdat_box = MdatBox {
            payload: Vec::new(),
        };
        self.file_size += mdat_box.encode_to_vec().map(|b| b.len()).expect("bug") as u32;

        for track in &self.tracks {
            for chunk in &track.chunks {
                self.chunk_offsets.push(self.file_size);
                for sample in &chunk.samples {
                    mdat_box.payload.extend_from_slice(&sample.data);
                    self.file_size += sample.data.len() as u32;
                }
            }
        }
        mdat_box
    }

    fn build_moov_box(&mut self) -> orfail::Result<MoovBox> {
        let mvhd_box = MvhdBox {
            creation_time: Mp4FileTime::default(),
            modification_time: Mp4FileTime::default(),
            timescale: OUTPUT_TIMESCALE,
            duration: self
                .tracks
                .iter()
                .map(|t| t.duration().as_micros())
                .max()
                .unwrap_or_default() as u64,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: self.tracks.len() as u32 + 1,
        };
        let mut trak_boxes = Vec::new();
        for (i, track) in std::mem::take(&mut self.tracks).iter().enumerate() {
            let trak_box = self.build_trak_box(i as u32 + 1, track).or_fail()?;
            trak_boxes.push(trak_box);
        }
        Ok(MoovBox {
            mvhd_box,
            trak_boxes,
            mvex_box: None,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_trak_box(&mut self, track_id: u32, track: &Track) -> orfail::Result<TrakBox> {
        let tkhd_box = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time: Mp4FileTime::default(),
            modification_time: Mp4FileTime::default(),
            track_id,
            duration: track.duration().as_micros() as u64,
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
            mdia_box: self.build_mdia_box(track).or_fail()?,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_mdia_box(&mut self, track: &Track) -> orfail::Result<MdiaBox> {
        let mdhd_box = MdhdBox {
            creation_time: Mp4FileTime::default(),
            modification_time: Mp4FileTime::default(),
            timescale: OUTPUT_TIMESCALE,
            duration: track.duration().as_micros() as u64,
            language: MdhdBox::LANGUAGE_UNDEFINED,
        };
        let hdlr_box = HdlrBox {
            handler_type: if track.is_audio {
                HdlrBox::HANDLER_TYPE_SOUN
            } else {
                HdlrBox::HANDLER_TYPE_VIDE
            },
            name: Utf8String::EMPTY.into_null_terminated_bytes(),
        };
        let minf_box = MinfBox {
            smhd_or_vmhd_box: if track.is_audio {
                Some(Either::A(SmhdBox::default()))
            } else {
                Some(Either::B(VmhdBox::default()))
            },
            dinf_box: DinfBox::LOCAL_FILE,
            stbl_box: self.build_stbl_box(track).or_fail()?,
            unknown_boxes: Vec::new(),
        };
        Ok(MdiaBox {
            mdhd_box,
            hdlr_box,
            minf_box,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_stbl_box(&mut self, track: &Track) -> orfail::Result<StblBox> {
        let stsd_box = StsdBox {
            entries: track
                .chunks
                .iter()
                .map(|c| c.sample_entry.clone())
                .collect(),
        };
        let stts_box =
            SttsBox::from_sample_deltas(track.samples().map(|s| s.duration.as_micros() as u32));
        let stsc_box = StscBox {
            entries: track
                .chunks
                .iter()
                .enumerate()
                .map(|(i, c)| StscEntry {
                    first_chunk: NonZeroU32::MIN.saturating_add(i as u32),
                    sample_per_chunk: c.samples.len() as u32,
                    sample_description_index: NonZeroU32::MIN.saturating_add(i as u32),
                })
                .collect(),
        };
        let stsz_box = StszBox::Variable {
            entry_sizes: track.samples().map(|s| s.data.len() as u32).collect(),
        };
        let stco_box = StcoBox {
            chunk_offsets: self.chunk_offsets.drain(0..track.chunks.len()).collect(),
        };

        let stss_box = (!track.is_audio).then(|| StssBox {
            sample_numbers: track
                .samples()
                .enumerate()
                .filter(|(_, s)| s.keyframe)
                .map(|(i, _)| NonZeroU32::MIN.saturating_add(i as u32))
                .collect(),
        });

        Ok(StblBox {
            stsd_box,
            stts_box,
            ctts_box: None,
            cslg_box: None,
            stsc_box,
            stsz_box,
            stco_or_co64_box: Either::A(stco_box),
            stss_box,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Mp4FileSummary {
    pub duration: u32,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug)]
pub struct InputMp4 {
    pub tracks: Vec<Track>,
}

impl InputMp4 {
    pub fn parse(mp4_file_bytes: &[u8]) -> orfail::Result<Self> {
        let (mp4_file, _) = Mp4File::decode(mp4_file_bytes).or_fail()?;
        let moov_box = mp4_file
            .boxes
            .iter()
            .find_map(|b| {
                if let RootBox::Moov(b) = b {
                    Some(b)
                } else {
                    None
                }
            })
            .or_fail_with(|()| "'moov' box not found".to_owned())?;

        let mut tracks = Vec::new();
        for trak_box in &moov_box.trak_boxes {
            let is_audio = trak_box.mdia_box.hdlr_box.handler_type == HdlrBox::HANDLER_TYPE_SOUN;
            let timescale = trak_box.mdia_box.mdhd_box.timescale;
            let sample_table =
                SampleTableAccessor::new(&trak_box.mdia_box.minf_box.stbl_box).or_fail()?;

            tracks.push(Track {
                is_audio,
                chunks: sample_table
                    .chunks()
                    .map(|c| Chunk {
                        sample_entry: c.sample_entry().clone(),
                        samples: c
                            .samples()
                            .map(|s| {
                                let offset = s.data_offset() as usize;
                                let size = s.data_size() as usize;
                                Sample {
                                    description: None,
                                    duration: Duration::from_secs(s.duration() as u64)
                                        / timescale.get(),
                                    keyframe: s.is_sync_sample(),
                                    data: mp4_file_bytes[offset..][..size].to_vec(),
                                }
                            })
                            .collect(),
                    })
                    .collect(),
            });
        }

        Ok(Self { tracks })
    }

    pub fn summary(&self) -> Mp4FileSummary {
        let duration = self
            .tracks
            .iter()
            .map(|t| t.duration())
            .max()
            .unwrap_or_default()
            .as_secs() as u32;
        let (width, height) = self
            .tracks
            .iter()
            .filter_map(|t| {
                if let Some(SampleEntry::Avc1(x)) = t.chunks.first().map(|c| &c.sample_entry) {
                    Some((x.visual.width, x.visual.height))
                } else {
                    None
                }
            })
            .max()
            .unwrap_or_default();
        Mp4FileSummary {
            duration,
            width,
            height,
        }
    }
}

#[derive(Debug)]
pub struct Track {
    pub is_audio: bool,
    pub chunks: Vec<Chunk>,
}

impl Track {
    pub fn duration(&self) -> Duration {
        self.chunks
            .iter()
            .flat_map(|c| c.samples.iter().map(|s| s.duration))
            .sum()
    }

    fn samples(&self) -> impl '_ + Iterator<Item = &Sample> {
        self.chunks.iter().flat_map(|c| c.samples.iter())
    }
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub sample_entry: SampleEntry,
    pub samples: Vec<Sample>,
}

#[derive(Debug, Clone)]
pub struct Sample {
    pub description: Option<Vec<u8>>,
    pub duration: Duration,
    pub keyframe: bool,
    pub data: Vec<u8>,
}
