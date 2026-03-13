//! moov とその下に配置されるボックスをまとめたモジュール
//!
//! このモジュールは内部的なもので、構造体などの外部への提供は boxes モジュールを通して行う
use alloc::{boxed::Box, format, vec::Vec};
use core::num::NonZeroU32;

use crate::{
    BaseBox, BoxHeader, BoxType, Decode, Either, Encode, Error, FixedPointNumber, FullBox,
    FullBoxFlags, FullBoxHeader, Mp4FileTime, Result, SampleFlags, Utf8String,
    basic_types::as_box_object,
    boxes::{SampleEntry, UnknownBox, check_mandatory_box, with_box_type},
    descriptors::EsDescriptor,
};

/// [ISO/IEC 14496-12] MovieBox class
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct MoovBox {
    pub mvhd_box: MvhdBox,
    pub trak_boxes: Vec<TrakBox>,
    pub mvex_box: Option<MvexBox>,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl MoovBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"moov");
}

impl Encode for MoovBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.mvhd_box.encode(&mut buf[offset..])?;
        for b in &self.trak_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        if let Some(b) = &self.mvex_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for MoovBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut mvhd_box = None;
            let mut trak_boxes = Vec::new();
            let mut mvex_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    MvhdBox::TYPE if mvhd_box.is_none() => {
                        mvhd_box = Some(MvhdBox::decode_at(payload, &mut offset)?);
                    }
                    TrakBox::TYPE => {
                        trak_boxes.push(TrakBox::decode_at(payload, &mut offset)?);
                    }
                    MvexBox::TYPE if mvex_box.is_none() => {
                        mvex_box = Some(MvexBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    mvhd_box: check_mandatory_box(mvhd_box, "mvhd", "moov")?,
                    trak_boxes,
                    mvex_box,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for MoovBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.mvhd_box).map(as_box_object))
                .chain(self.trak_boxes.iter().map(as_box_object))
                .chain(self.mvex_box.iter().map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-12] MovieHeaderBox class (親: [`MoovBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct MvhdBox {
    pub creation_time: Mp4FileTime,
    pub modification_time: Mp4FileTime,
    pub timescale: NonZeroU32,
    pub duration: u64,
    pub rate: FixedPointNumber<i16, u16>,
    pub volume: FixedPointNumber<i8, u8>,
    pub matrix: [i32; 9],
    pub next_track_id: u32,
}

impl MvhdBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"mvhd");

    /// [`MvhdBox::rate`] のデフォルト値（通常の再生速度）
    pub const DEFAULT_RATE: FixedPointNumber<i16, u16> = FixedPointNumber::new(1, 0);

    /// [`MvhdBox::volume`] のデフォルト値（最大音量）
    pub const DEFAULT_VOLUME: FixedPointNumber<i8, u8> = FixedPointNumber::new(1, 0);

    /// [`MvhdBox::matrix`] のデフォルト値
    pub const DEFAULT_MATRIX: [i32; 9] = [0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000];
}

impl Encode for MvhdBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        if self.full_box_version() == 1 {
            offset += self.creation_time.as_secs().encode(&mut buf[offset..])?;
            offset += self
                .modification_time
                .as_secs()
                .encode(&mut buf[offset..])?;
            offset += self.timescale.encode(&mut buf[offset..])?;
            offset += self.duration.encode(&mut buf[offset..])?;
        } else {
            offset += (self.creation_time.as_secs() as u32).encode(&mut buf[offset..])?;
            offset += (self.modification_time.as_secs() as u32).encode(&mut buf[offset..])?;
            offset += self.timescale.encode(&mut buf[offset..])?;
            offset += (self.duration as u32).encode(&mut buf[offset..])?;
        }
        offset += self.rate.encode(&mut buf[offset..])?;
        offset += self.volume.encode(&mut buf[offset..])?;
        offset += [0u8; 2 + 4 * 2].encode(&mut buf[offset..])?;
        offset += self.matrix.encode(&mut buf[offset..])?;
        offset += [0u8; 4 * 6].encode(&mut buf[offset..])?;
        offset += self.next_track_id.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for MvhdBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;

            let mut this = Self {
                creation_time: Mp4FileTime::default(),
                modification_time: Mp4FileTime::default(),
                timescale: NonZeroU32::MIN,
                duration: 0,
                rate: Self::DEFAULT_RATE,
                volume: Self::DEFAULT_VOLUME,
                matrix: Self::DEFAULT_MATRIX,
                next_track_id: 0,
            };

            if full_header.version == 1 {
                this.creation_time =
                    u64::decode_at(payload, &mut offset).map(Mp4FileTime::from_secs)?;
                this.modification_time =
                    u64::decode_at(payload, &mut offset).map(Mp4FileTime::from_secs)?;
                this.timescale = NonZeroU32::decode_at(payload, &mut offset)?;
                this.duration = u64::decode_at(payload, &mut offset)?;
            } else {
                this.creation_time = u32::decode_at(payload, &mut offset)
                    .map(|v| Mp4FileTime::from_secs(v as u64))?;
                this.modification_time = u32::decode_at(payload, &mut offset)
                    .map(|v| Mp4FileTime::from_secs(v as u64))?;
                this.timescale = NonZeroU32::decode_at(payload, &mut offset)?;
                this.duration = u32::decode_at(payload, &mut offset).map(|v| v as u64)?;
            }

            this.rate = FixedPointNumber::decode_at(payload, &mut offset)?;
            this.volume = FixedPointNumber::decode_at(payload, &mut offset)?;
            let _ = <[u8; 2 + 4 * 2]>::decode_at(payload, &mut offset)?;
            this.matrix = <[i32; 9]>::decode_at(payload, &mut offset)?;
            let _ = <[u8; 4 * 6]>::decode_at(payload, &mut offset)?;
            this.next_track_id = u32::decode_at(payload, &mut offset)?;

            Ok((this, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for MvhdBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for MvhdBox {
    fn full_box_version(&self) -> u8 {
        if self.creation_time.as_secs() > u32::MAX as u64
            || self.modification_time.as_secs() > u32::MAX as u64
            || self.duration > u32::MAX as u64
        {
            1
        } else {
            0
        }
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] TrackBox class (親: [`MoovBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct TrakBox {
    pub tkhd_box: TkhdBox,
    pub edts_box: Option<EdtsBox>,
    pub mdia_box: MdiaBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl TrakBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"trak");
}

impl Encode for TrakBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.tkhd_box.encode(&mut buf[offset..])?;
        if let Some(b) = &self.edts_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        offset += self.mdia_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for TrakBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut tkhd_box = None;
            let mut edts_box = None;
            let mut mdia_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    TkhdBox::TYPE if tkhd_box.is_none() => {
                        tkhd_box = Some(TkhdBox::decode_at(payload, &mut offset)?);
                    }
                    EdtsBox::TYPE if edts_box.is_none() => {
                        edts_box = Some(EdtsBox::decode_at(payload, &mut offset)?);
                    }
                    MdiaBox::TYPE if mdia_box.is_none() => {
                        mdia_box = Some(MdiaBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    tkhd_box: check_mandatory_box(tkhd_box, "tkhd", "trak")?,
                    edts_box,
                    mdia_box: check_mandatory_box(mdia_box, "mdia", "trak")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for TrakBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.tkhd_box).map(as_box_object))
                .chain(self.edts_box.iter().map(as_box_object))
                .chain(core::iter::once(&self.mdia_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-12] TrackHeaderBox class (親: [`TrakBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct TkhdBox {
    pub flag_track_enabled: bool,
    pub flag_track_in_movie: bool,
    pub flag_track_in_preview: bool,
    pub flag_track_size_is_aspect_ratio: bool,

    pub creation_time: Mp4FileTime,
    pub modification_time: Mp4FileTime,
    pub track_id: u32,
    pub duration: u64,
    pub layer: i16,
    pub alternate_group: i16,
    pub volume: FixedPointNumber<i8, u8>,
    pub matrix: [i32; 9],
    pub width: FixedPointNumber<i16, u16>,
    pub height: FixedPointNumber<i16, u16>,
}

impl TkhdBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"tkhd");

    /// [`TkhdBox::layer`] のデフォルト値
    pub const DEFAULT_LAYER: i16 = 0;

    /// [`TkhdBox::alternate_group`] のデフォルト値
    pub const DEFAULT_ALTERNATE_GROUP: i16 = 0;

    /// 音声用の [`TkhdBox::volume`] のデフォルト値（最大音量）
    pub const DEFAULT_AUDIO_VOLUME: FixedPointNumber<i8, u8> = FixedPointNumber::new(1, 0);

    /// 映像用の [`TkhdBox::volume`] のデフォルト値（無音）
    pub const DEFAULT_VIDEO_VOLUME: FixedPointNumber<i8, u8> = FixedPointNumber::new(0, 0);

    /// [`TkhdBox::matrix`] のデフォルト値
    pub const DEFAULT_MATRIX: [i32; 9] = [0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000];
}

impl Encode for TkhdBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        if self.full_box_version() == 1 {
            offset += self.creation_time.as_secs().encode(&mut buf[offset..])?;
            offset += self
                .modification_time
                .as_secs()
                .encode(&mut buf[offset..])?;
            offset += self.track_id.encode(&mut buf[offset..])?;
            offset += [0u8; 4].encode(&mut buf[offset..])?;
            offset += self.duration.encode(&mut buf[offset..])?;
        } else {
            offset += (self.creation_time.as_secs() as u32).encode(&mut buf[offset..])?;
            offset += (self.modification_time.as_secs() as u32).encode(&mut buf[offset..])?;
            offset += self.track_id.encode(&mut buf[offset..])?;
            offset += [0u8; 4].encode(&mut buf[offset..])?;
            offset += (self.duration as u32).encode(&mut buf[offset..])?;
        }
        offset += [0u8; 4 * 2].encode(&mut buf[offset..])?;
        offset += self.layer.encode(&mut buf[offset..])?;
        offset += self.alternate_group.encode(&mut buf[offset..])?;
        offset += self.volume.encode(&mut buf[offset..])?;
        offset += [0u8; 2].encode(&mut buf[offset..])?;
        offset += self.matrix.encode(&mut buf[offset..])?;
        offset += self.width.encode(&mut buf[offset..])?;
        offset += self.height.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for TkhdBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;

            let mut this = Self {
                flag_track_enabled: false,
                flag_track_in_movie: false,
                flag_track_in_preview: false,
                flag_track_size_is_aspect_ratio: false,
                creation_time: Mp4FileTime::default(),
                modification_time: Mp4FileTime::default(),
                track_id: 0,
                duration: 0,
                layer: Self::DEFAULT_LAYER,
                alternate_group: Self::DEFAULT_ALTERNATE_GROUP,
                volume: Self::DEFAULT_AUDIO_VOLUME,
                matrix: Self::DEFAULT_MATRIX,
                width: FixedPointNumber::new(0, 0),
                height: FixedPointNumber::new(0, 0),
            };

            this.flag_track_enabled = full_header.flags.is_set(0);
            this.flag_track_in_movie = full_header.flags.is_set(1);
            this.flag_track_in_preview = full_header.flags.is_set(2);
            this.flag_track_size_is_aspect_ratio = full_header.flags.is_set(3);

            if full_header.version == 1 {
                this.creation_time =
                    u64::decode_at(payload, &mut offset).map(Mp4FileTime::from_secs)?;
                this.modification_time =
                    u64::decode_at(payload, &mut offset).map(Mp4FileTime::from_secs)?;
                this.track_id = u32::decode_at(payload, &mut offset)?;
                let _ = <[u8; 4]>::decode_at(payload, &mut offset)?;
                this.duration = u64::decode_at(payload, &mut offset)?;
            } else {
                this.creation_time = u32::decode_at(payload, &mut offset)
                    .map(|v| Mp4FileTime::from_secs(v as u64))?;
                this.modification_time = u32::decode_at(payload, &mut offset)
                    .map(|v| Mp4FileTime::from_secs(v as u64))?;
                this.track_id = u32::decode_at(payload, &mut offset)?;
                let _ = <[u8; 4]>::decode_at(payload, &mut offset)?;
                this.duration = u32::decode_at(payload, &mut offset).map(|v| v as u64)?;
            }

            let _ = <[u8; 4 * 2]>::decode_at(payload, &mut offset)?;
            this.layer = i16::decode_at(payload, &mut offset)?;
            this.alternate_group = i16::decode_at(payload, &mut offset)?;
            this.volume = FixedPointNumber::decode_at(payload, &mut offset)?;
            let _ = <[u8; 2]>::decode_at(payload, &mut offset)?;
            this.matrix = <[i32; 9]>::decode_at(payload, &mut offset)?;
            this.width = FixedPointNumber::decode_at(payload, &mut offset)?;
            this.height = FixedPointNumber::decode_at(payload, &mut offset)?;

            Ok((this, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for TkhdBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for TkhdBox {
    fn full_box_version(&self) -> u8 {
        if self.creation_time.as_secs() > u32::MAX as u64
            || self.modification_time.as_secs() > u32::MAX as u64
            || self.duration > u32::MAX as u64
        {
            1
        } else {
            0
        }
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::from_flags([
            (0, self.flag_track_enabled),
            (1, self.flag_track_in_movie),
            (2, self.flag_track_in_preview),
            (3, self.flag_track_size_is_aspect_ratio),
        ])
    }
}

/// [ISO/IEC 14496-12] EditBox class (親: [`TrakBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct EdtsBox {
    pub elst_box: Option<ElstBox>,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl EdtsBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"edts");
}

impl Encode for EdtsBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        if let Some(b) = &self.elst_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for EdtsBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut elst_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    ElstBox::TYPE if elst_box.is_none() => {
                        elst_box = Some(ElstBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    elst_box,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for EdtsBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(self.elst_box.iter().map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [`ElstBox`] に含まれるエントリー
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct ElstEntry {
    pub edit_duration: u64,
    pub media_time: i64,
    pub media_rate: FixedPointNumber<i16, i16>,
}

/// [ISO/IEC 14496-12] EditListBox class (親: [`EdtsBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct ElstBox {
    pub entries: Vec<ElstEntry>,
}

impl ElstBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"elst");
}

impl Encode for ElstBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;

        let version = self.full_box_version();
        offset += (self.entries.len() as u32).encode(&mut buf[offset..])?;
        for entry in &self.entries {
            if version == 1 {
                offset += entry.edit_duration.encode(&mut buf[offset..])?;
                offset += entry.media_time.encode(&mut buf[offset..])?;
            } else {
                offset += (entry.edit_duration as u32).encode(&mut buf[offset..])?;
                offset += (entry.media_time as i32).encode(&mut buf[offset..])?;
            }
            offset += entry.media_rate.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for ElstBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;

            let mut entries = Vec::new();
            let count = u32::decode_at(payload, &mut offset)?;
            for _ in 0..count {
                let edit_duration;
                let media_time;
                if full_header.version == 1 {
                    edit_duration = u64::decode_at(payload, &mut offset)?;
                    media_time = i64::decode_at(payload, &mut offset)?;
                } else {
                    edit_duration = u32::decode_at(payload, &mut offset)? as u64;
                    media_time = i32::decode_at(payload, &mut offset)? as i64;
                }
                let media_rate = FixedPointNumber::decode_at(payload, &mut offset)?;
                entries.push(ElstEntry {
                    edit_duration,
                    media_time,
                    media_rate,
                });
            }

            Ok((Self { entries }, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for ElstBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for ElstBox {
    fn full_box_version(&self) -> u8 {
        let large = self.entries.iter().any(|x| {
            u32::try_from(x.edit_duration).is_err() || i32::try_from(x.media_time).is_err()
        });
        if large { 1 } else { 0 }
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] MediaBox class (親: [`TrakBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct MdiaBox {
    pub mdhd_box: MdhdBox,
    pub hdlr_box: HdlrBox,
    pub minf_box: MinfBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl MdiaBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"mdia");
}

impl Encode for MdiaBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.mdhd_box.encode(&mut buf[offset..])?;
        offset += self.hdlr_box.encode(&mut buf[offset..])?;
        offset += self.minf_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for MdiaBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut mdhd_box = None;
            let mut hdlr_box = None;
            let mut minf_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    MdhdBox::TYPE if mdhd_box.is_none() => {
                        mdhd_box = Some(MdhdBox::decode_at(payload, &mut offset)?);
                    }
                    HdlrBox::TYPE if hdlr_box.is_none() => {
                        hdlr_box = Some(HdlrBox::decode_at(payload, &mut offset)?);
                    }
                    MinfBox::TYPE if minf_box.is_none() => {
                        minf_box = Some(MinfBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    mdhd_box: check_mandatory_box(mdhd_box, "mdhd", "mdia")?,
                    hdlr_box: check_mandatory_box(hdlr_box, "hdlr", "mdia")?,
                    minf_box: check_mandatory_box(minf_box, "minf", "mdia")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for MdiaBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.mdhd_box).map(as_box_object))
                .chain(core::iter::once(&self.hdlr_box).map(as_box_object))
                .chain(core::iter::once(&self.minf_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-12] MediaHeaderBox class (親: [`MdiaBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct MdhdBox {
    pub creation_time: Mp4FileTime,
    pub modification_time: Mp4FileTime,
    pub timescale: NonZeroU32,
    pub duration: u64,

    /// ISO-639-2/T language code
    pub language: [u8; 3],
}

impl MdhdBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"mdhd");

    /// 未定義を表す言語コード
    pub const LANGUAGE_UNDEFINED: [u8; 3] = *b"und";
}

impl Encode for MdhdBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        if self.full_box_version() == 1 {
            offset += self.creation_time.as_secs().encode(&mut buf[offset..])?;
            offset += self
                .modification_time
                .as_secs()
                .encode(&mut buf[offset..])?;
            offset += self.timescale.encode(&mut buf[offset..])?;
            offset += self.duration.encode(&mut buf[offset..])?;
        } else {
            offset += (self.creation_time.as_secs() as u32).encode(&mut buf[offset..])?;
            offset += (self.modification_time.as_secs() as u32).encode(&mut buf[offset..])?;
            offset += self.timescale.encode(&mut buf[offset..])?;
            offset += (self.duration as u32).encode(&mut buf[offset..])?;
        }

        let mut language: u16 = 0;
        for l in &self.language {
            let Some(code) = l.checked_sub(0x60) else {
                return Err(Error::invalid_input(format!(
                    "Invalid language code: {:?}",
                    self.language
                )));
            };
            language = (language << 5) | code as u16;
        }
        offset += language.encode(&mut buf[offset..])?;
        offset += [0u8; 2].encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for MdhdBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;

            let mut this = Self {
                creation_time: Mp4FileTime::default(),
                modification_time: Mp4FileTime::default(),
                timescale: NonZeroU32::MIN,
                duration: 0,
                language: Self::LANGUAGE_UNDEFINED,
            };

            if full_header.version == 1 {
                this.creation_time =
                    u64::decode_at(payload, &mut offset).map(Mp4FileTime::from_secs)?;
                this.modification_time =
                    u64::decode_at(payload, &mut offset).map(Mp4FileTime::from_secs)?;
                this.timescale = NonZeroU32::decode_at(payload, &mut offset)?;
                this.duration = u64::decode_at(payload, &mut offset)?;
            } else {
                this.creation_time = u32::decode_at(payload, &mut offset)
                    .map(|v| Mp4FileTime::from_secs(v as u64))?;
                this.modification_time = u32::decode_at(payload, &mut offset)
                    .map(|v| Mp4FileTime::from_secs(v as u64))?;
                this.timescale = NonZeroU32::decode_at(payload, &mut offset)?;
                this.duration = u32::decode_at(payload, &mut offset).map(|v| v as u64)?;
            }

            let language = u16::decode_at(payload, &mut offset)?;
            this.language = [
                ((language >> 10) & 0b11111) as u8 + 0x60,
                ((language >> 5) & 0b11111) as u8 + 0x60,
                (language & 0b11111) as u8 + 0x60,
            ];

            let _ = <[u8; 2]>::decode_at(payload, &mut offset)?;

            Ok((this, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for MdhdBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for MdhdBox {
    fn full_box_version(&self) -> u8 {
        if self.creation_time.as_secs() > u32::MAX as u64
            || self.modification_time.as_secs() > u32::MAX as u64
            || self.duration > u32::MAX as u64
        {
            1
        } else {
            0
        }
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] HandlerBox class (親: [`MdiaBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct HdlrBox {
    pub handler_type: [u8; 4],

    /// ハンドラ名
    ///
    /// ISO の仕様書上はここは [`Utf8String`] であるべきだが、
    /// 中身が UTF-8 ではなかったり、
    /// null 終端文字列ではなく先頭にサイズバイトを格納する形式で
    /// MP4 ファイルを作成する実装が普通に存在するため、
    /// ここでは単なるバイト列として扱っている
    pub name: Vec<u8>,
}

impl HdlrBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"hdlr");

    /// 音声用のハンドラー種別
    pub const HANDLER_TYPE_SOUN: [u8; 4] = *b"soun";

    /// 映像用のハンドラー種別
    pub const HANDLER_TYPE_VIDE: [u8; 4] = *b"vide";
}

impl Encode for HdlrBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += [0u8; 4].encode(&mut buf[offset..])?;
        offset += self.handler_type.encode(&mut buf[offset..])?;
        offset += [0u8; 4 * 3].encode(&mut buf[offset..])?;
        offset += self.name.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for HdlrBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let _ = <[u8; 4]>::decode_at(payload, &mut offset)?;
            let handler_type = <[u8; 4]>::decode_at(payload, &mut offset)?;
            let _ = <[u8; 4 * 3]>::decode_at(payload, &mut offset)?;
            let name = payload[offset..].to_vec();

            Ok((
                Self { handler_type, name },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for HdlrBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for HdlrBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] MediaInformationBox class (親: [`MdiaBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct MinfBox {
    // 音声・映像トラック以外の場合は None になる
    pub smhd_or_vmhd_box: Option<Either<SmhdBox, VmhdBox>>,
    pub dinf_box: DinfBox,
    pub stbl_box: StblBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl MinfBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"minf");
}

impl Encode for MinfBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        if let Some(smhd_or_vmhd_box) = &self.smhd_or_vmhd_box {
            match smhd_or_vmhd_box {
                Either::A(b) => offset += b.encode(&mut buf[offset..])?,
                Either::B(b) => offset += b.encode(&mut buf[offset..])?,
            }
        }
        offset += self.dinf_box.encode(&mut buf[offset..])?;
        offset += self.stbl_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for MinfBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut smhd_box = None;
            let mut vmhd_box = None;
            let mut dinf_box = None;
            let mut stbl_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    SmhdBox::TYPE if smhd_box.is_none() => {
                        smhd_box = Some(SmhdBox::decode_at(payload, &mut offset)?);
                    }
                    VmhdBox::TYPE if vmhd_box.is_none() => {
                        vmhd_box = Some(VmhdBox::decode_at(payload, &mut offset)?);
                    }
                    DinfBox::TYPE if dinf_box.is_none() => {
                        dinf_box = Some(DinfBox::decode_at(payload, &mut offset)?);
                    }
                    StblBox::TYPE if stbl_box.is_none() => {
                        stbl_box = Some(StblBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    smhd_or_vmhd_box: smhd_box.map(Either::A).or(vmhd_box.map(Either::B)),
                    dinf_box: check_mandatory_box(dinf_box, "dinf", "minf")?,
                    stbl_box: check_mandatory_box(stbl_box, "stbl", "minf")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for MinfBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(self.smhd_or_vmhd_box.iter().map(as_box_object))
                .chain(core::iter::once(&self.dinf_box).map(as_box_object))
                .chain(core::iter::once(&self.stbl_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-12] SoundMediaHeaderBox class (親: [`MinfBox`]）
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct SmhdBox {
    pub balance: FixedPointNumber<u8, u8>,
}

impl SmhdBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"smhd");

    /// [`SmhdBox::balance`] のデフォルト値（中央）
    pub const DEFAULT_BALANCE: FixedPointNumber<u8, u8> = FixedPointNumber::new(0, 0);
}

impl Encode for SmhdBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += self.balance.encode(&mut buf[offset..])?;
        offset += [0u8; 2].encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for SmhdBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let balance = FixedPointNumber::decode_at(payload, &mut offset)?;
            let _ = <[u8; 2]>::decode_at(payload, &mut offset)?;

            Ok((Self { balance }, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for SmhdBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for SmhdBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] VideoMediaHeaderBox class (親: [`MinfBox`]）
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct VmhdBox {
    pub graphicsmode: u16,
    pub opcolor: [u16; 3],
}

impl VmhdBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"vmhd");

    /// [`VmhdBox::graphicsmode`] のデフォルト値（コピー）
    pub const DEFAULT_GRAPHICSMODE: u16 = 0;

    /// [`VmhdBox::opcolor`] のデフォルト値
    pub const DEFAULT_OPCOLOR: [u16; 3] = [0, 0, 0];
}

impl Encode for VmhdBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += self.graphicsmode.encode(&mut buf[offset..])?;
        offset += self.opcolor.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for VmhdBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;

            // [NOTE]
            // ISO/IEC 14496-12 の仕様には「vmhd ボックスの flags は 1 になる」と記載があるが、
            // 実際には 0 となるファイルも存在するため、ここではそのチェックを行わないようにしている

            let graphicsmode = u16::decode_at(payload, &mut offset)?;
            let opcolor = <[u16; 3]>::decode_at(payload, &mut offset)?;

            Ok((
                Self {
                    graphicsmode,
                    opcolor,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for VmhdBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for VmhdBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(1)
    }
}

/// [ISO/IEC 14496-12] DataInformationBox class (親: [`MinfBox`]）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct DinfBox {
    pub dref_box: DrefBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl DinfBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"dinf");

    /// メディアデータが同じファイル内に格納されていることを示す [`DinfBox`] の値
    pub const LOCAL_FILE: Self = Self {
        dref_box: DrefBox::LOCAL_FILE,
        unknown_boxes: Vec::new(),
    };
}

impl Encode for DinfBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.dref_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for DinfBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut dref_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    DrefBox::TYPE if dref_box.is_none() => {
                        dref_box = Some(DrefBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    dref_box: check_mandatory_box(dref_box, "dref", "dinf")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for DinfBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.dref_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-12] DataReferenceBox class (親: [`DinfBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct DrefBox {
    pub url_box: Option<UrlBox>,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl DrefBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"dref");

    /// メディアデータが同じファイル内に格納されていることを示す [`DrefBox`] の値
    pub const LOCAL_FILE: Self = Self {
        url_box: Some(UrlBox::LOCAL_FILE),
        unknown_boxes: Vec::new(),
    };
}

impl Encode for DrefBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        let entry_count = (self.url_box.is_some() as usize + self.unknown_boxes.len()) as u32;
        offset += entry_count.encode(&mut buf[offset..])?;
        if let Some(b) = &self.url_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for DrefBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let entry_count = u32::decode_at(payload, &mut offset)?;

            let mut url_box = None;
            let mut unknown_boxes = Vec::new();

            for _ in 0..entry_count {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    UrlBox::TYPE if url_box.is_none() => {
                        url_box = Some(UrlBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    url_box,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for DrefBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(self.url_box.iter().map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

impl FullBox for DrefBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] DataEntryUrlBox class (親: [`DrefBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct UrlBox {
    pub location: Option<Utf8String>,
}

impl UrlBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"url ");

    /// メディアデータが同じファイル内に格納されていることを示す [`UrlBox`] の値
    pub const LOCAL_FILE: Self = Self { location: None };
}

impl Encode for UrlBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        if let Some(l) = &self.location {
            offset += l.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for UrlBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let location = if full_header.flags.is_set(0) {
                None
            } else {
                Some(Utf8String::decode_at(payload, &mut offset)?)
            };

            Ok((Self { location }, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for UrlBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for UrlBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(self.location.is_none() as u32)
    }
}

/// [ISO/IEC 14496-12] SampleTableBox class (親: [`MinfBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct StblBox {
    pub stsd_box: StsdBox,
    pub stts_box: SttsBox,
    pub ctts_box: Option<CttsBox>,
    pub cslg_box: Option<CslgBox>,
    pub stsc_box: StscBox,
    pub stsz_box: StszBox,
    pub stco_or_co64_box: Either<StcoBox, Co64Box>,
    pub stss_box: Option<StssBox>,
    pub sdtp_box: Option<SdtpBox>,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl StblBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"stbl");
}

impl Encode for StblBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.stsd_box.encode(&mut buf[offset..])?;
        offset += self.stts_box.encode(&mut buf[offset..])?;
        if let Some(b) = &self.ctts_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        if let Some(b) = &self.cslg_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        offset += self.stsc_box.encode(&mut buf[offset..])?;
        offset += self.stsz_box.encode(&mut buf[offset..])?;
        match &self.stco_or_co64_box {
            Either::A(b) => offset += b.encode(&mut buf[offset..])?,
            Either::B(b) => offset += b.encode(&mut buf[offset..])?,
        }
        if let Some(b) = &self.stss_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        if let Some(b) = &self.sdtp_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for StblBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut stsd_box = None;
            let mut stts_box = None;
            let mut ctts_box = None;
            let mut cslg_box = None;
            let mut stsc_box = None;
            let mut stsz_box = None;
            let mut stco_box = None;
            let mut co64_box = None;
            let mut stss_box = None;
            let mut sdtp_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    StsdBox::TYPE if stsd_box.is_none() => {
                        stsd_box = Some(StsdBox::decode_at(payload, &mut offset)?);
                    }
                    SttsBox::TYPE if stts_box.is_none() => {
                        stts_box = Some(SttsBox::decode_at(payload, &mut offset)?);
                    }
                    CttsBox::TYPE if ctts_box.is_none() => {
                        ctts_box = Some(CttsBox::decode_at(payload, &mut offset)?);
                    }
                    CslgBox::TYPE if cslg_box.is_none() => {
                        cslg_box = Some(CslgBox::decode_at(payload, &mut offset)?);
                    }
                    StscBox::TYPE if stsc_box.is_none() => {
                        stsc_box = Some(StscBox::decode_at(payload, &mut offset)?);
                    }
                    StszBox::TYPE if stsz_box.is_none() => {
                        stsz_box = Some(StszBox::decode_at(payload, &mut offset)?);
                    }
                    StcoBox::TYPE if stco_box.is_none() => {
                        stco_box = Some(StcoBox::decode_at(payload, &mut offset)?);
                    }
                    Co64Box::TYPE if co64_box.is_none() => {
                        co64_box = Some(Co64Box::decode_at(payload, &mut offset)?);
                    }
                    StssBox::TYPE if stss_box.is_none() => {
                        stss_box = Some(StssBox::decode_at(payload, &mut offset)?);
                    }
                    SdtpBox::TYPE if sdtp_box.is_none() => {
                        sdtp_box = Some(SdtpBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    stsd_box: check_mandatory_box(stsd_box, "stsd", "stbl")?,
                    stts_box: check_mandatory_box(stts_box, "stts", "stbl")?,
                    ctts_box,
                    cslg_box,
                    stsc_box: check_mandatory_box(stsc_box, "stsc", "stbl")?,
                    stsz_box: check_mandatory_box(stsz_box, "stsz", "stbl")?,
                    stco_or_co64_box: check_mandatory_box(
                        stco_box.map(Either::A).or(co64_box.map(Either::B)),
                        "stco' or 'co64",
                        "stbl",
                    )?,
                    stss_box,
                    sdtp_box,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for StblBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.stsd_box).map(as_box_object))
                .chain(core::iter::once(&self.stts_box).map(as_box_object))
                .chain(self.ctts_box.iter().map(as_box_object))
                .chain(self.cslg_box.iter().map(as_box_object))
                .chain(core::iter::once(&self.stsc_box).map(as_box_object))
                .chain(core::iter::once(&self.stsz_box).map(as_box_object))
                .chain(core::iter::once(&self.stco_or_co64_box).map(as_box_object))
                .chain(self.stss_box.iter().map(as_box_object))
                .chain(self.sdtp_box.iter().map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

impl AsRef<StblBox> for StblBox {
    fn as_ref(&self) -> &StblBox {
        self
    }
}

/// [ISO/IEC 14496-12] SampleDescriptionBox class (親: [`StblBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct StsdBox {
    pub entries: Vec<SampleEntry>,
}

impl StsdBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"stsd");
}

impl Encode for StsdBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        let entry_count = (self.entries.len()) as u32;
        offset += entry_count.encode(&mut buf[offset..])?;
        for b in &self.entries {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for StsdBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let entry_count = u32::decode_at(payload, &mut offset)?;

            let mut entries = Vec::new();
            for _ in 0..entry_count {
                entries.push(SampleEntry::decode_at(payload, &mut offset)?);
            }

            Ok((Self { entries }, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for StsdBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(self.entries.iter().map(as_box_object))
    }
}

impl FullBox for StsdBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [`SttsBox`] が保持するエントリー
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct SttsEntry {
    pub sample_count: u32,
    pub sample_delta: u32,
}

/// [ISO/IEC 14496-12] TimeToSampleBox class (親: [`StblBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct SttsBox {
    pub entries: Vec<SttsEntry>,
}

impl SttsBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"stts");

    /// サンプル群の尺を走査するイテレーターを受け取って、対応する [`SttsBox`] インスタンスを作成する
    pub fn from_sample_deltas<I>(sample_deltas: I) -> Self
    where
        I: IntoIterator<Item = u32>,
    {
        let mut entries = Vec::<SttsEntry>::new();
        for sample_delta in sample_deltas {
            if let Some(last) = entries.last_mut()
                && last.sample_delta == sample_delta
            {
                last.sample_count += 1;
                continue;
            }
            entries.push(SttsEntry {
                sample_count: 1,
                sample_delta,
            });
        }
        Self { entries }
    }
}

impl Encode for SttsBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += (self.entries.len() as u32).encode(&mut buf[offset..])?;
        for entry in &self.entries {
            offset += entry.sample_count.encode(&mut buf[offset..])?;
            offset += entry.sample_delta.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for SttsBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let count = u32::decode_at(payload, &mut offset)? as usize;

            let mut entries = Vec::new();
            for _ in 0..count {
                entries.push(SttsEntry {
                    sample_count: u32::decode_at(payload, &mut offset)?,
                    sample_delta: u32::decode_at(payload, &mut offset)?,
                });
            }

            Ok((Self { entries }, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for SttsBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for SttsBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [`CttsBox`] が保持するエントリー
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CttsEntry {
    /// このエントリーが適用されるサンプル数
    pub sample_count: u32,

    /// 合成時刻オフセット
    ///
    /// version 0 では非負値、version 1 では負値も許容される。
    pub sample_offset: i64,
}

/// [ISO/IEC 14496-12] CompositionOffsetBox class (親: [`StblBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CttsBox {
    /// full box version
    pub version: u8,

    /// エントリー列
    pub entries: Vec<CttsEntry>,
}

impl CttsBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"ctts");
}

impl Encode for CttsBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let version = self.full_box_version();
        if version > 1 {
            return Err(Error::invalid_input(format!(
                "Invalid ctts box version: {version}"
            )));
        }

        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += (self.entries.len() as u32).encode(&mut buf[offset..])?;
        for entry in &self.entries {
            offset += entry.sample_count.encode(&mut buf[offset..])?;
            if version == 1 {
                let sample_offset = i32::try_from(entry.sample_offset).map_err(|_| {
                    Error::invalid_input(format!(
                        "ctts version 1 requires sample_offset to be in i32 range, got {}",
                        entry.sample_offset
                    ))
                })?;
                offset += sample_offset.encode(&mut buf[offset..])?;
            } else {
                let sample_offset = u32::try_from(entry.sample_offset).map_err(|_| {
                    Error::invalid_input(format!(
                        "ctts version 0 requires non-negative sample_offset, got {}",
                        entry.sample_offset
                    ))
                })?;
                offset += sample_offset.encode(&mut buf[offset..])?;
            }
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for CttsBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            if full_header.version > 1 {
                return Err(Error::invalid_data(format!(
                    "Invalid ctts box version: {}",
                    full_header.version
                )));
            }
            let count = u32::decode_at(payload, &mut offset)? as usize;

            let mut entries = Vec::new();
            for _ in 0..count {
                let sample_count = u32::decode_at(payload, &mut offset)?;
                let sample_offset = if full_header.version == 1 {
                    i32::decode_at(payload, &mut offset)? as i64
                } else {
                    u32::decode_at(payload, &mut offset)? as i64
                };
                entries.push(CttsEntry {
                    sample_count,
                    sample_offset,
                });
            }

            Ok((
                Self {
                    version: full_header.version,
                    entries,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for CttsBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for CttsBox {
    fn full_box_version(&self) -> u8 {
        self.version
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] CompositionToDecodeBox class (親: [`StblBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CslgBox {
    /// full box version
    pub version: u8,

    /// composition to decode time shift
    pub composition_to_dts_shift: i64,

    /// decode から display への最小差分
    pub least_decode_to_display_delta: i64,

    /// decode から display への最大差分
    pub greatest_decode_to_display_delta: i64,

    /// composition start time
    pub composition_start_time: i64,

    /// composition end time
    pub composition_end_time: i64,
}

impl CslgBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"cslg");
}

impl Encode for CslgBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let version = self.full_box_version();
        if version > 1 {
            return Err(Error::invalid_input(format!(
                "Invalid cslg box version: {version}"
            )));
        }

        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;

        if version == 1 {
            offset += self.composition_to_dts_shift.encode(&mut buf[offset..])?;
            offset += self
                .least_decode_to_display_delta
                .encode(&mut buf[offset..])?;
            offset += self
                .greatest_decode_to_display_delta
                .encode(&mut buf[offset..])?;
            offset += self.composition_start_time.encode(&mut buf[offset..])?;
            offset += self.composition_end_time.encode(&mut buf[offset..])?;
        } else {
            offset += i32::try_from(self.composition_to_dts_shift)
                .map_err(|_| {
                    Error::invalid_input(format!(
                        "cslg version 0 requires composition_to_dts_shift to be in i32 range, got {}",
                        self.composition_to_dts_shift
                    ))
                })?
                .encode(&mut buf[offset..])?;
            offset += i32::try_from(self.least_decode_to_display_delta)
                .map_err(|_| {
                    Error::invalid_input(format!(
                        "cslg version 0 requires least_decode_to_display_delta to be in i32 range, got {}",
                        self.least_decode_to_display_delta
                    ))
                })?
                .encode(&mut buf[offset..])?;
            offset += i32::try_from(self.greatest_decode_to_display_delta)
                .map_err(|_| {
                    Error::invalid_input(format!(
                        "cslg version 0 requires greatest_decode_to_display_delta to be in i32 range, got {}",
                        self.greatest_decode_to_display_delta
                    ))
                })?
                .encode(&mut buf[offset..])?;
            offset += i32::try_from(self.composition_start_time)
                .map_err(|_| {
                    Error::invalid_input(format!(
                        "cslg version 0 requires composition_start_time to be in i32 range, got {}",
                        self.composition_start_time
                    ))
                })?
                .encode(&mut buf[offset..])?;
            offset += i32::try_from(self.composition_end_time)
                .map_err(|_| {
                    Error::invalid_input(format!(
                        "cslg version 0 requires composition_end_time to be in i32 range, got {}",
                        self.composition_end_time
                    ))
                })?
                .encode(&mut buf[offset..])?;
        }

        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for CslgBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            if full_header.version > 1 {
                return Err(Error::invalid_data(format!(
                    "Invalid cslg box version: {}",
                    full_header.version
                )));
            }

            let (
                composition_to_dts_shift,
                least_decode_to_display_delta,
                greatest_decode_to_display_delta,
                composition_start_time,
                composition_end_time,
            ) = if full_header.version == 1 {
                (
                    i64::decode_at(payload, &mut offset)?,
                    i64::decode_at(payload, &mut offset)?,
                    i64::decode_at(payload, &mut offset)?,
                    i64::decode_at(payload, &mut offset)?,
                    i64::decode_at(payload, &mut offset)?,
                )
            } else {
                (
                    i32::decode_at(payload, &mut offset)? as i64,
                    i32::decode_at(payload, &mut offset)? as i64,
                    i32::decode_at(payload, &mut offset)? as i64,
                    i32::decode_at(payload, &mut offset)? as i64,
                    i32::decode_at(payload, &mut offset)? as i64,
                )
            };

            Ok((
                Self {
                    version: full_header.version,
                    composition_to_dts_shift,
                    least_decode_to_display_delta,
                    greatest_decode_to_display_delta,
                    composition_start_time,
                    composition_end_time,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for CslgBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for CslgBox {
    fn full_box_version(&self) -> u8 {
        self.version
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] SampleDependencyTypeBox の 1 サンプル分のフラグ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SdtpSampleFlags(u8);

impl SdtpSampleFlags {
    /// フィールド値を直接指定して新しいフラグを作成する
    pub const fn from_fields(
        is_leading: u8,
        sample_depends_on: u8,
        sample_is_depended_on: u8,
        sample_has_redundancy: u8,
    ) -> Self {
        let value = ((is_leading & 0b11) << 6)
            | ((sample_depends_on & 0b11) << 4)
            | ((sample_is_depended_on & 0b11) << 2)
            | (sample_has_redundancy & 0b11);
        Self(value)
    }

    /// 生の 1 バイト値を返す
    pub const fn get(self) -> u8 {
        self.0
    }

    /// is_leading フィールド（2 bits）を返す
    pub const fn is_leading(self) -> u8 {
        self.0 >> 6
    }

    /// sample_depends_on フィールド（2 bits）を返す
    pub const fn sample_depends_on(self) -> u8 {
        (self.0 >> 4) & 0b11
    }

    /// sample_is_depended_on フィールド（2 bits）を返す
    pub const fn sample_is_depended_on(self) -> u8 {
        (self.0 >> 2) & 0b11
    }

    /// sample_has_redundancy フィールド（2 bits）を返す
    pub const fn sample_has_redundancy(self) -> u8 {
        self.0 & 0b11
    }
}

impl Encode for SdtpSampleFlags {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        self.0.encode(buf)
    }
}

impl Decode for SdtpSampleFlags {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (value, size) = u8::decode(buf)?;
        Ok((Self(value), size))
    }
}

/// [ISO/IEC 14496-12] SampleDependencyTypeBox class (親: [`StblBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SdtpBox {
    /// サンプル単位の依存関係フラグ列
    pub entries: Vec<SdtpSampleFlags>,
}

impl SdtpBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"sdtp");
}

impl Encode for SdtpBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        for entry in &self.entries {
            offset += entry.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for SdtpBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            if full_header.version != 0 {
                return Err(Error::invalid_data(format!(
                    "Invalid sdtp box version: {}",
                    full_header.version
                )));
            }

            let mut entries = Vec::new();
            while offset < payload.len() {
                entries.push(SdtpSampleFlags::decode_at(payload, &mut offset)?);
            }

            Ok((Self { entries }, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for SdtpBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for SdtpBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [`StscBox`] が保持するエントリー
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct StscEntry {
    pub first_chunk: NonZeroU32,
    pub sample_per_chunk: u32,
    pub sample_description_index: NonZeroU32,
}

/// [ISO/IEC 14496-12] SampleToChunkBox class (親: [`StblBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct StscBox {
    pub entries: Vec<StscEntry>,
}

impl StscBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"stsc");
}

impl Encode for StscBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += (self.entries.len() as u32).encode(&mut buf[offset..])?;
        for entry in &self.entries {
            offset += entry.first_chunk.encode(&mut buf[offset..])?;
            offset += entry.sample_per_chunk.encode(&mut buf[offset..])?;
            offset += entry.sample_description_index.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for StscBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let count = u32::decode_at(payload, &mut offset)?;

            let mut entries = Vec::new();
            for _ in 0..count {
                entries.push(StscEntry {
                    first_chunk: NonZeroU32::decode_at(payload, &mut offset)?,
                    sample_per_chunk: u32::decode_at(payload, &mut offset)?,
                    sample_description_index: NonZeroU32::decode_at(payload, &mut offset)?,
                });
            }

            Ok((Self { entries }, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for StscBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for StscBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] SampleSizeBox class (親: [`StblBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum StszBox {
    Fixed {
        sample_size: NonZeroU32,
        sample_count: u32,
    },
    Variable {
        entry_sizes: Vec<u32>,
    },
}

impl StszBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"stsz");
}

impl Encode for StszBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        match self {
            StszBox::Fixed {
                sample_size,
                sample_count,
            } => {
                offset += sample_size.get().encode(&mut buf[offset..])?;
                offset += sample_count.encode(&mut buf[offset..])?;
            }
            StszBox::Variable { entry_sizes } => {
                offset += 0u32.encode(&mut buf[offset..])?;
                offset += (entry_sizes.len() as u32).encode(&mut buf[offset..])?;
                for size in entry_sizes {
                    offset += size.encode(&mut buf[offset..])?;
                }
            }
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for StszBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let sample_size = u32::decode_at(payload, &mut offset)?;
            let sample_count = u32::decode_at(payload, &mut offset)?;

            let stsz_box = if let Some(sample_size) = NonZeroU32::new(sample_size) {
                Self::Fixed {
                    sample_size,
                    sample_count,
                }
            } else {
                let mut entry_sizes = Vec::new();
                for _ in 0..sample_count {
                    entry_sizes.push(u32::decode_at(payload, &mut offset)?);
                }
                Self::Variable { entry_sizes }
            };

            Ok((stsz_box, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for StszBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for StszBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] ChunkOffsetBox class (親: [`StblBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct StcoBox {
    pub chunk_offsets: Vec<u32>,
}

impl StcoBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"stco");
}

impl Encode for StcoBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += (self.chunk_offsets.len() as u32).encode(&mut buf[offset..])?;
        for offset_val in &self.chunk_offsets {
            offset += offset_val.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for StcoBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let count = u32::decode_at(payload, &mut offset)?;

            let mut chunk_offsets = Vec::new();
            for _ in 0..count {
                chunk_offsets.push(u32::decode_at(payload, &mut offset)?);
            }

            Ok((
                Self { chunk_offsets },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for StcoBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for StcoBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] ChunkLargeOffsetBox class (親: [`StblBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct Co64Box {
    pub chunk_offsets: Vec<u64>,
}

impl Co64Box {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"co64");
}

impl Encode for Co64Box {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += (self.chunk_offsets.len() as u32).encode(&mut buf[offset..])?;
        for offset_val in &self.chunk_offsets {
            offset += offset_val.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for Co64Box {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let count = u32::decode_at(payload, &mut offset)?;

            let mut chunk_offsets = Vec::new();
            for _ in 0..count {
                chunk_offsets.push(u64::decode_at(payload, &mut offset)?);
            }

            Ok((
                Self { chunk_offsets },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for Co64Box {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for Co64Box {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] SyncSampleBox class (親: [`StssBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct StssBox {
    pub sample_numbers: Vec<NonZeroU32>,
}

impl StssBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"stss");
}

impl Encode for StssBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += (self.sample_numbers.len() as u32).encode(&mut buf[offset..])?;
        for offset_val in &self.sample_numbers {
            offset += offset_val.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for StssBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let count = u32::decode_at(payload, &mut offset)?;

            let mut sample_numbers = Vec::new();
            for _ in 0..count {
                sample_numbers.push(NonZeroU32::decode_at(payload, &mut offset)?);
            }

            Ok((
                Self { sample_numbers },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for StssBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for StssBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-14] ESDBox class
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct EsdsBox {
    pub es: EsDescriptor,
}

impl EsdsBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"esds");
}

impl Encode for EsdsBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += self.es.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for EsdsBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let es = EsDescriptor::decode_at(payload, &mut offset)?;

            Ok((Self { es }, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for EsdsBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for EsdsBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] MovieExtendsBox class (親: [`MoovBox`])
///
/// Fragmented MP4 で使用するムービー拡張ボックス。
/// このボックスが存在する場合、ファイルは fMP4 フォーマットであることを示す。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct MvexBox {
    pub mehd_box: Option<MehdBox>,
    pub trex_boxes: Vec<TrexBox>,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl MvexBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"mvex");
}

impl Encode for MvexBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        if let Some(b) = &self.mehd_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        for b in &self.trex_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for MvexBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut mehd_box = None;
            let mut trex_boxes = Vec::new();
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    MehdBox::TYPE if mehd_box.is_none() => {
                        mehd_box = Some(MehdBox::decode_at(payload, &mut offset)?);
                    }
                    TrexBox::TYPE => {
                        trex_boxes.push(TrexBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    mehd_box,
                    trex_boxes,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for MvexBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(self.mehd_box.iter().map(as_box_object))
                .chain(self.trex_boxes.iter().map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-12] MovieExtendsHeaderBox class (親: [`MvexBox`])
///
/// フラグメント化されたムービー全体の継続時間を格納する。
/// このボックスはオプションであり、存在しない場合は継続時間が不明であることを意味する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct MehdBox {
    pub fragment_duration: u64,
}

impl MehdBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"mehd");
}

impl Encode for MehdBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        if self.full_box_version() == 1 {
            offset += self.fragment_duration.encode(&mut buf[offset..])?;
        } else {
            offset += (self.fragment_duration as u32).encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for MehdBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;

            let fragment_duration = if full_header.version == 1 {
                u64::decode_at(payload, &mut offset)?
            } else {
                u32::decode_at(payload, &mut offset)? as u64
            };

            Ok((
                Self { fragment_duration },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for MehdBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for MehdBox {
    fn full_box_version(&self) -> u8 {
        if self.fragment_duration > u32::MAX as u64 {
            1
        } else {
            0
        }
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] TrackExtendsBox class (親: [`MvexBox`])
///
/// トラックフラグメントのデフォルト値を定義する。
/// 各トラックに対して 1 つの TrexBox が必要。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct TrexBox {
    pub track_id: u32,
    pub default_sample_description_index: u32,
    pub default_sample_duration: u32,
    pub default_sample_size: u32,
    pub default_sample_flags: SampleFlags,
}

impl TrexBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"trex");
}

impl Encode for TrexBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += self.track_id.encode(&mut buf[offset..])?;
        offset += self
            .default_sample_description_index
            .encode(&mut buf[offset..])?;
        offset += self.default_sample_duration.encode(&mut buf[offset..])?;
        offset += self.default_sample_size.encode(&mut buf[offset..])?;
        offset += self.default_sample_flags.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for TrexBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;

            let track_id = u32::decode_at(payload, &mut offset)?;
            let default_sample_description_index = u32::decode_at(payload, &mut offset)?;
            let default_sample_duration = u32::decode_at(payload, &mut offset)?;
            let default_sample_size = u32::decode_at(payload, &mut offset)?;
            let default_sample_flags = SampleFlags::decode_at(payload, &mut offset)?;

            Ok((
                Self {
                    track_id,
                    default_sample_description_index,
                    default_sample_duration,
                    default_sample_size,
                    default_sample_flags,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for TrexBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for TrexBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}
