//! サンプルエントリー系のボックスをまとめたモジュール
//!
//! このモジュールは内部的なもので、構造体などの外部への提供は boxes モジュールを通して行う
use alloc::{boxed::Box, format, vec::Vec};
use core::num::NonZeroU16;

use crate::{
    BaseBox, BoxHeader, BoxType, Decode, Encode, Error, FixedPointNumber, FullBox, FullBoxFlags,
    FullBoxHeader, Result, Uint,
    basic_types::as_box_object,
    boxes::{EsdsBox, UnknownBox, check_mandatory_box, with_box_type},
};

/// [`StsdBox`][crate::boxes::StsdBox] に含まれるエントリー
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum SampleEntry {
    Avc1(Avc1Box),
    Hev1(Hev1Box),
    Hvc1(Hvc1Box),
    Vp08(Vp08Box),
    Vp09(Vp09Box),
    Av01(Av01Box),
    Opus(OpusBox),
    Mp4a(Mp4aBox),
    Flac(FlacBox),
    Unknown(UnknownBox),
}

impl SampleEntry {
    /// チャンネル数を取得する
    ///
    /// 音声の場合はチャンネル数、映像の場合は None を返す
    pub fn audio_channel_count(&self) -> Option<u8> {
        match self {
            Self::Opus(b) => Some(b.audio.channelcount as u8),
            Self::Mp4a(b) => Some(b.audio.channelcount as u8),
            Self::Flac(b) => Some(b.audio.channelcount as u8),
            _ => None,
        }
    }

    /// サンプリングレートを取得する
    ///
    /// 音声の場合はサンプリングレート、映像の場合は None を返す
    ///
    /// # NOTE
    ///
    /// このメソッドはサンプリングレートの整数部分のみを返し、小数部分は切り捨てられる。
    /// ただし通常は、MP4 ファイルでは音声のサンプリングレートは常に整数値（例: 44100 Hz, 48000 Hz）であり、
    /// 小数部分が 0 以外の値を持つことはないため、問題ないと想定している。
    ///
    /// なお音声コーデックによっては u16 の範囲を超えるサンプリングレートが使用される場合もある。
    /// その可能性がある場合は、このメソッドではなく、コーデック固有の方法で実際のサンプリングレートを取得すること。
    pub fn audio_sample_rate(&self) -> Option<u16> {
        match self {
            Self::Opus(b) => Some(b.audio.samplerate.integer),
            Self::Mp4a(b) => Some(b.audio.samplerate.integer),
            Self::Flac(b) => Some(b.audio.samplerate.integer),
            _ => None,
        }
    }

    /// サンプルサイズ（ビット深度）を取得する
    ///
    /// 音声の場合はサンプルサイズ、映像の場合は None を返す
    pub fn audio_sample_size(&self) -> Option<u16> {
        match self {
            Self::Opus(b) => Some(b.audio.samplesize),
            Self::Mp4a(b) => Some(b.audio.samplesize),
            Self::Flac(b) => Some(b.audio.samplesize),
            _ => None,
        }
    }

    /// 解像度を取得する
    ///
    /// 映像の場合は (幅, 高さ)、音声の場合は None を返す
    pub fn video_resolution(&self) -> Option<(u16, u16)> {
        match self {
            Self::Avc1(b) => Some((b.visual.width, b.visual.height)),
            Self::Hev1(b) => Some((b.visual.width, b.visual.height)),
            Self::Hvc1(b) => Some((b.visual.width, b.visual.height)),
            Self::Vp08(b) => Some((b.visual.width, b.visual.height)),
            Self::Vp09(b) => Some((b.visual.width, b.visual.height)),
            Self::Av01(b) => Some((b.visual.width, b.visual.height)),
            _ => None,
        }
    }

    fn inner_box(&self) -> &dyn BaseBox {
        match self {
            Self::Avc1(b) => b,
            Self::Hev1(b) => b,
            Self::Hvc1(b) => b,
            Self::Vp08(b) => b,
            Self::Vp09(b) => b,
            Self::Av01(b) => b,
            Self::Opus(b) => b,
            Self::Mp4a(b) => b,
            Self::Flac(b) => b,
            Self::Unknown(b) => b,
        }
    }
}

impl Encode for SampleEntry {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        match self {
            Self::Avc1(b) => b.encode(buf),
            Self::Hev1(b) => b.encode(buf),
            Self::Hvc1(b) => b.encode(buf),
            Self::Vp08(b) => b.encode(buf),
            Self::Vp09(b) => b.encode(buf),
            Self::Av01(b) => b.encode(buf),
            Self::Opus(b) => b.encode(buf),
            Self::Mp4a(b) => b.encode(buf),
            Self::Flac(b) => b.encode(buf),
            Self::Unknown(b) => b.encode(buf),
        }
    }
}

impl Decode for SampleEntry {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (header, _) = BoxHeader::decode(buf)?;
        match header.box_type {
            Avc1Box::TYPE => Avc1Box::decode(buf).map(|(b, n)| (Self::Avc1(b), n)),
            Hev1Box::TYPE => Hev1Box::decode(buf).map(|(b, n)| (Self::Hev1(b), n)),
            Hvc1Box::TYPE => Hvc1Box::decode(buf).map(|(b, n)| (Self::Hvc1(b), n)),
            Vp08Box::TYPE => Vp08Box::decode(buf).map(|(b, n)| (Self::Vp08(b), n)),
            Vp09Box::TYPE => Vp09Box::decode(buf).map(|(b, n)| (Self::Vp09(b), n)),
            Av01Box::TYPE => Av01Box::decode(buf).map(|(b, n)| (Self::Av01(b), n)),
            OpusBox::TYPE => OpusBox::decode(buf).map(|(b, n)| (Self::Opus(b), n)),
            Mp4aBox::TYPE => Mp4aBox::decode(buf).map(|(b, n)| (Self::Mp4a(b), n)),
            FlacBox::TYPE => FlacBox::decode(buf).map(|(b, n)| (Self::Flac(b), n)),
            _ => UnknownBox::decode(buf).map(|(b, n)| (Self::Unknown(b), n)),
        }
    }
}

impl BaseBox for SampleEntry {
    fn box_type(&self) -> BoxType {
        self.inner_box().box_type()
    }

    fn is_unknown_box(&self) -> bool {
        self.inner_box().is_unknown_box()
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        self.inner_box().children()
    }
}

/// 映像系の [`SampleEntry`] に共通のフィールドをまとめた構造体
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct VisualSampleEntryFields {
    pub data_reference_index: NonZeroU16,
    pub width: u16,
    pub height: u16,
    pub horizresolution: FixedPointNumber<u16, u16>,
    pub vertresolution: FixedPointNumber<u16, u16>,
    pub frame_count: u16,
    pub compressorname: [u8; 32],
    pub depth: u16,
}

impl VisualSampleEntryFields {
    /// [`VisualSampleEntryFields::data_reference_index`] のデフォルト値
    pub const DEFAULT_DATA_REFERENCE_INDEX: NonZeroU16 = NonZeroU16::MIN;

    /// [`VisualSampleEntryFields::horizresolution`] のデフォルト値 (72 dpi)
    pub const DEFAULT_HORIZRESOLUTION: FixedPointNumber<u16, u16> = FixedPointNumber::new(0x48, 0);

    /// [`VisualSampleEntryFields::vertresolution`] のデフォルト値 (72 dpi)
    pub const DEFAULT_VERTRESOLUTION: FixedPointNumber<u16, u16> = FixedPointNumber::new(0x48, 0);

    /// [`VisualSampleEntryFields::frame_count`] のデフォルト値 (1)
    pub const DEFAULT_FRAME_COUNT: u16 = 1;

    /// [`VisualSampleEntryFields::depth`] のデフォルト値 (images are in colour with no alpha)
    pub const DEFAULT_DEPTH: u16 = 0x0018;

    /// 名前なしを表す [`VisualSampleEntryFields::compressorname`] の値
    pub const NULL_COMPRESSORNAME: [u8; 32] = [0; 32];
}

impl Encode for VisualSampleEntryFields {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = 0;
        offset += [0u8; 6].encode(&mut buf[offset..])?;
        offset += self.data_reference_index.encode(&mut buf[offset..])?;
        offset += [0u8; 2 + 2 + 4 * 3].encode(&mut buf[offset..])?;
        offset += self.width.encode(&mut buf[offset..])?;
        offset += self.height.encode(&mut buf[offset..])?;
        offset += self.horizresolution.encode(&mut buf[offset..])?;
        offset += self.vertresolution.encode(&mut buf[offset..])?;
        offset += [0u8; 4].encode(&mut buf[offset..])?;
        offset += self.frame_count.encode(&mut buf[offset..])?;
        offset += self.compressorname.encode(&mut buf[offset..])?;
        offset += self.depth.encode(&mut buf[offset..])?;
        offset += (-1i16).encode(&mut buf[offset..])?;
        Ok(offset)
    }
}

impl Decode for VisualSampleEntryFields {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let mut offset = 0;
        let _ = <[u8; 6]>::decode_at(buf, &mut offset)?;
        let data_reference_index = NonZeroU16::decode_at(buf, &mut offset)?;
        let _ = <[u8; 2 + 2 + 4 * 3]>::decode_at(buf, &mut offset)?;
        let width = u16::decode_at(buf, &mut offset)?;
        let height = u16::decode_at(buf, &mut offset)?;
        let horizresolution = FixedPointNumber::decode_at(buf, &mut offset)?;
        let vertresolution = FixedPointNumber::decode_at(buf, &mut offset)?;
        let _ = <[u8; 4]>::decode_at(buf, &mut offset)?;
        let frame_count = u16::decode_at(buf, &mut offset)?;
        let compressorname = <[u8; 32]>::decode_at(buf, &mut offset)?;
        let depth = u16::decode_at(buf, &mut offset)?;
        let _ = <[u8; 2]>::decode_at(buf, &mut offset)?;
        Ok((
            Self {
                data_reference_index,
                width,
                height,
                horizresolution,
                vertresolution,
                frame_count,
                compressorname,
                depth,
            },
            offset,
        ))
    }
}

/// [ISO/IEC 14496-15] AVCSampleEntry class (親: [`StsdBox`][crate::boxes::StsdBox])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct Avc1Box {
    pub visual: VisualSampleEntryFields,
    pub avcc_box: AvccBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl Avc1Box {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"avc1");
}

impl Encode for Avc1Box {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.visual.encode(&mut buf[offset..])?;
        offset += self.avcc_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for Avc1Box {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let visual = VisualSampleEntryFields::decode_at(payload, &mut offset)?;

            let mut avcc_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    AvccBox::TYPE if avcc_box.is_none() => {
                        avcc_box = Some(AvccBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    visual,
                    avcc_box: check_mandatory_box(avcc_box, "avcc", "avc1")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for Avc1Box {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.avcc_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-15] AVCConfigurationBox class (親: [`Avc1Box`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct AvccBox {
    pub avc_profile_indication: u8,
    pub profile_compatibility: u8,
    pub avc_level_indication: u8,
    pub length_size_minus_one: Uint<u8, 2>,
    pub sps_list: Vec<Vec<u8>>,
    pub pps_list: Vec<Vec<u8>>,
    pub chroma_format: Option<Uint<u8, 2>>,
    pub bit_depth_luma_minus8: Option<Uint<u8, 3>>,
    pub bit_depth_chroma_minus8: Option<Uint<u8, 3>>,
    pub sps_ext_list: Vec<Vec<u8>>,
}

impl AvccBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"avcC");

    const CONFIGURATION_VERSION: u8 = 1;
}

impl Encode for AvccBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;

        offset += Self::CONFIGURATION_VERSION.encode(&mut buf[offset..])?;
        offset += self.avc_profile_indication.encode(&mut buf[offset..])?;
        offset += self.profile_compatibility.encode(&mut buf[offset..])?;
        offset += self.avc_level_indication.encode(&mut buf[offset..])?;
        offset += (0b1111_1100 | self.length_size_minus_one.get()).encode(&mut buf[offset..])?;

        if self.sps_list.len() > 31 {
            return Err(Error::invalid_input("Too many SPSs (max 31)"));
        }
        let sps_count = self.sps_list.len() as u8;
        offset += (0b1110_0000 | sps_count).encode(&mut buf[offset..])?;
        for sps in &self.sps_list {
            let size =
                u16::try_from(sps.len()).map_err(|_| Error::invalid_input("Too long SPS"))?;
            offset += size.encode(&mut buf[offset..])?;
            offset += sps.encode(&mut buf[offset..])?;
        }

        if self.pps_list.len() > 31 {
            return Err(Error::invalid_input("Too many PPSs (max 31)"));
        }
        let pps_count = self.pps_list.len() as u8;
        offset += pps_count.encode(&mut buf[offset..])?;
        for pps in &self.pps_list {
            let size =
                u16::try_from(pps.len()).map_err(|_| Error::invalid_input("Too long PPS"))?;
            offset += size.encode(&mut buf[offset..])?;
            offset += pps.encode(&mut buf[offset..])?;
        }

        if !matches!(self.avc_profile_indication, 66 | 77 | 88) {
            let chroma_format = self.chroma_format.ok_or_else(|| {
                Error::invalid_input("Missing 'chroma_format' field in 'avcC' box")
            })?;
            let bit_depth_luma_minus8 = self.bit_depth_luma_minus8.ok_or_else(|| {
                Error::invalid_input("Missing 'bit_depth_luma_minus8' field in 'avcC' box")
            })?;
            let bit_depth_chroma_minus8 = self.bit_depth_chroma_minus8.ok_or_else(|| {
                Error::invalid_input("Missing 'bit_depth_chroma_minus8' field in 'avcC' box")
            })?;
            offset += (0b1111_1100 | chroma_format.get()).encode(&mut buf[offset..])?;
            offset += (0b1111_1000 | bit_depth_luma_minus8.get()).encode(&mut buf[offset..])?;
            offset += (0b1111_1000 | bit_depth_chroma_minus8.get()).encode(&mut buf[offset..])?;

            let sps_ext_count = u8::try_from(self.sps_ext_list.len())
                .map_err(|_| Error::invalid_input("Too many SPS EXTs"))?;
            offset += sps_ext_count.encode(&mut buf[offset..])?;
            for sps_ext in &self.sps_ext_list {
                let size = u16::try_from(sps_ext.len())
                    .map_err(|_| Error::invalid_input("Too long SPS EXT"))?;
                offset += size.encode(&mut buf[offset..])?;
                offset += sps_ext.encode(&mut buf[offset..])?;
            }
        }

        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for AvccBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let configuration_version = u8::decode_at(payload, &mut offset)?;
            if configuration_version != Self::CONFIGURATION_VERSION {
                return Err(Error::invalid_data(format!(
                    "Unsupported avcC configuration version: {configuration_version}"
                )));
            }

            let avc_profile_indication = u8::decode_at(payload, &mut offset)?;
            let profile_compatibility = u8::decode_at(payload, &mut offset)?;
            let avc_level_indication = u8::decode_at(payload, &mut offset)?;
            let length_size_minus_one = Uint::from_bits(u8::decode_at(payload, &mut offset)?);

            let sps_count =
                Uint::<u8, 5>::from_bits(u8::decode_at(payload, &mut offset)?).get() as usize;
            let mut sps_list = Vec::new();
            for _ in 0..sps_count {
                let size = u16::decode_at(payload, &mut offset)? as usize;
                if offset + size > payload.len() {
                    return Err(Error::invalid_data("SPS data exceeds payload boundary"));
                }
                let sps = payload[offset..offset + size].to_vec();
                offset += size;
                sps_list.push(sps);
            }

            let pps_count = u8::decode_at(payload, &mut offset)? as usize;
            let mut pps_list = Vec::new();
            for _ in 0..pps_count {
                let size = u16::decode_at(payload, &mut offset)? as usize;
                if offset + size > payload.len() {
                    return Err(Error::invalid_data("PPS data exceeds payload boundary"));
                }
                let pps = payload[offset..offset + size].to_vec();
                offset += size;
                pps_list.push(pps);
            }

            let mut chroma_format = None;
            let mut bit_depth_luma_minus8 = None;
            let mut bit_depth_chroma_minus8 = None;
            let mut sps_ext_list = Vec::new();

            // [NOTE]
            // ISO/IEC 14496-15 の仕様としては、プロファイルが 66 | 77 | 88 以外の場合には、
            // 以降のフィールドが必須扱いとなっている。
            // ただし、現実的にはその仕様を守っていないファイルが存在するため、
            // 「残りのペイロードのサイズが空の場合には、以降の処理をスキップする」というチェックを追加している。
            if !matches!(avc_profile_indication, 66 | 77 | 88) && offset < payload.len() {
                chroma_format = Some(Uint::from_bits(u8::decode_at(payload, &mut offset)?));
                bit_depth_luma_minus8 = Some(Uint::from_bits(u8::decode_at(payload, &mut offset)?));
                bit_depth_chroma_minus8 =
                    Some(Uint::from_bits(u8::decode_at(payload, &mut offset)?));

                let sps_ext_count = u8::decode_at(payload, &mut offset)? as usize;
                for _ in 0..sps_ext_count {
                    let size = u16::decode_at(payload, &mut offset)? as usize;
                    if offset + size > payload.len() {
                        return Err(Error::invalid_data("SPS EXT data exceeds payload boundary"));
                    }
                    let sps_ext = payload[offset..offset + size].to_vec();
                    offset += size;
                    sps_ext_list.push(sps_ext);
                }
            }

            Ok((
                Self {
                    avc_profile_indication,
                    profile_compatibility,
                    avc_level_indication,
                    length_size_minus_one,
                    sps_list,
                    pps_list,
                    chroma_format,
                    bit_depth_luma_minus8,
                    bit_depth_chroma_minus8,
                    sps_ext_list,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for AvccBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

/// [ISO/IEC 14496-15] HEVCSampleEntry class (親: [`StsdBox`][crate::boxes::StsdBox])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct Hev1Box {
    pub visual: VisualSampleEntryFields,
    pub hvcc_box: HvccBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl Hev1Box {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"hev1");
}

impl Encode for Hev1Box {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.visual.encode(&mut buf[offset..])?;
        offset += self.hvcc_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for Hev1Box {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let visual = VisualSampleEntryFields::decode_at(payload, &mut offset)?;

            let mut hvcc_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    HvccBox::TYPE if hvcc_box.is_none() => {
                        hvcc_box = Some(HvccBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    visual,
                    hvcc_box: check_mandatory_box(hvcc_box, "hvcc", "hev1")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for Hev1Box {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.hvcc_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-15] HEVCSampleEntry class (親: [`StsdBox`][crate::boxes::StsdBox])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct Hvc1Box {
    pub visual: VisualSampleEntryFields,
    pub hvcc_box: HvccBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl Hvc1Box {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"hvc1");
}

impl Encode for Hvc1Box {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.visual.encode(&mut buf[offset..])?;
        offset += self.hvcc_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for Hvc1Box {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let visual = VisualSampleEntryFields::decode_at(payload, &mut offset)?;

            let mut hvcc_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    HvccBox::TYPE if hvcc_box.is_none() => {
                        hvcc_box = Some(HvccBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    visual,
                    hvcc_box: check_mandatory_box(hvcc_box, "hvcc", "hvc1")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for Hvc1Box {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.hvcc_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [`HvccBox`] 内の NAL ユニット配列を保持する構造体
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct HvccNalUintArray {
    pub array_completeness: Uint<u8, 1, 7>,
    pub nal_unit_type: Uint<u8, 6, 0>,
    pub nalus: Vec<Vec<u8>>,
}

/// [ISO/IEC 14496-15] HVCConfigurationBox class (親: [`Hev1Box`], [`Hvc1Box`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct HvccBox {
    pub general_profile_space: Uint<u8, 2, 6>,
    pub general_tier_flag: Uint<u8, 1, 5>,
    pub general_profile_idc: Uint<u8, 5, 0>,
    pub general_profile_compatibility_flags: u32,
    pub general_constraint_indicator_flags: Uint<u64, 48>,
    pub general_level_idc: u8,
    pub min_spatial_segmentation_idc: Uint<u16, 12>,
    pub parallelism_type: Uint<u8, 2>,
    pub chroma_format_idc: Uint<u8, 2>,
    pub bit_depth_luma_minus8: Uint<u8, 3>,
    pub bit_depth_chroma_minus8: Uint<u8, 3>,
    pub avg_frame_rate: u16,
    pub constant_frame_rate: Uint<u8, 2, 6>,
    pub num_temporal_layers: Uint<u8, 3, 3>,
    pub temporal_id_nested: Uint<u8, 1, 2>,
    pub length_size_minus_one: Uint<u8, 2, 0>,
    pub nalu_arrays: Vec<HvccNalUintArray>,
}

impl HvccBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"hvcC");

    const CONFIGURATION_VERSION: u8 = 1;
}

impl Encode for HvccBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += Self::CONFIGURATION_VERSION.encode(&mut buf[offset..])?;
        offset += (self.general_profile_space.to_bits()
            | self.general_tier_flag.to_bits()
            | self.general_profile_idc.to_bits())
        .encode(&mut buf[offset..])?;
        offset += self
            .general_profile_compatibility_flags
            .encode(&mut buf[offset..])?;
        offset += self.general_constraint_indicator_flags.get().to_be_bytes()[2..]
            .encode(&mut buf[offset..])?;
        offset += self.general_level_idc.encode(&mut buf[offset..])?;
        offset += (0b1111_0000_0000_0000 | self.min_spatial_segmentation_idc.to_bits())
            .encode(&mut buf[offset..])?;
        offset += (0b1111_1100 | self.parallelism_type.to_bits()).encode(&mut buf[offset..])?;
        offset += (0b1111_1100 | self.chroma_format_idc.to_bits()).encode(&mut buf[offset..])?;
        offset +=
            (0b1111_1000 | self.bit_depth_luma_minus8.to_bits()).encode(&mut buf[offset..])?;
        offset +=
            (0b1111_1000 | self.bit_depth_chroma_minus8.to_bits()).encode(&mut buf[offset..])?;
        offset += self.avg_frame_rate.encode(&mut buf[offset..])?;
        offset += (self.constant_frame_rate.to_bits()
            | self.num_temporal_layers.to_bits()
            | self.temporal_id_nested.to_bits()
            | self.length_size_minus_one.to_bits())
        .encode(&mut buf[offset..])?;
        offset += u8::try_from(self.nalu_arrays.len())
            .map_err(|_| {
                Error::invalid_input(format!("Too many NALU arrays: {}", self.nalu_arrays.len()))
            })?
            .encode(&mut buf[offset..])?;
        for nalu_array in &self.nalu_arrays {
            offset += (nalu_array.array_completeness.to_bits()
                | nalu_array.nal_unit_type.to_bits())
            .encode(&mut buf[offset..])?;
            offset += u16::try_from(nalu_array.nalus.len())
                .map_err(|_| {
                    Error::invalid_input(format!("Too many NALUs: {}", nalu_array.nalus.len()))
                })?
                .encode(&mut buf[offset..])?;
            for nalu in &nalu_array.nalus {
                offset += u16::try_from(nalu.len())
                    .map_err(|_| Error::invalid_input(format!("Too large NALU: {}", nalu.len())))?
                    .encode(&mut buf[offset..])?;
                offset += nalu.encode(&mut buf[offset..])?;
            }
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for HvccBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let configuration_version = u8::decode_at(payload, &mut offset)?;
            if configuration_version != Self::CONFIGURATION_VERSION {
                return Err(Error::invalid_data(format!(
                    "Unsupported hvcC version: {configuration_version}"
                )));
            }

            let b = u8::decode_at(payload, &mut offset)?;
            let general_profile_space = Uint::from_bits(b);
            let general_tier_flag = Uint::from_bits(b);
            let general_profile_idc = Uint::from_bits(b);

            let general_profile_compatibility_flags = u32::decode_at(payload, &mut offset)?;

            let mut buf_constraint = [0; 8];
            if offset + 6 > payload.len() {
                return Err(Error::invalid_data(
                    "general_constraint_indicator_flags exceeds payload boundary",
                ));
            }
            buf_constraint[2..].copy_from_slice(&payload[offset..offset + 6]);
            offset += 6;
            let general_constraint_indicator_flags =
                Uint::from_bits(u64::from_be_bytes(buf_constraint));

            let general_level_idc = u8::decode_at(payload, &mut offset)?;
            let min_spatial_segmentation_idc =
                Uint::from_bits(u16::decode_at(payload, &mut offset)?);
            let parallelism_type = Uint::from_bits(u8::decode_at(payload, &mut offset)?);
            let chroma_format_idc = Uint::from_bits(u8::decode_at(payload, &mut offset)?);
            let bit_depth_luma_minus8 = Uint::from_bits(u8::decode_at(payload, &mut offset)?);
            let bit_depth_chroma_minus8 = Uint::from_bits(u8::decode_at(payload, &mut offset)?);
            let avg_frame_rate = u16::decode_at(payload, &mut offset)?;

            let b = u8::decode_at(payload, &mut offset)?;
            let constant_frame_rate = Uint::from_bits(b);
            let num_temporal_layers = Uint::from_bits(b);
            let temporal_id_nested = Uint::from_bits(b);
            let length_size_minus_one = Uint::from_bits(b);

            let num_of_arrays = u8::decode_at(payload, &mut offset)?;
            let mut nalu_arrays = Vec::new();
            for _ in 0..num_of_arrays {
                let b = u8::decode_at(payload, &mut offset)?;
                let array_completeness = Uint::from_bits(b);
                let nal_unit_type = Uint::from_bits(b);

                let num_nalus = u16::decode_at(payload, &mut offset)?;
                let mut nalus = Vec::new();
                for _ in 0..num_nalus {
                    let nal_unit_length = u16::decode_at(payload, &mut offset)? as usize;
                    if offset + nal_unit_length > payload.len() {
                        return Err(Error::invalid_data(
                            "NAL unit data exceeds payload boundary",
                        ));
                    }
                    let nal_unit = payload[offset..offset + nal_unit_length].to_vec();
                    offset += nal_unit_length;
                    nalus.push(nal_unit);
                }
                nalu_arrays.push(HvccNalUintArray {
                    array_completeness,
                    nal_unit_type,
                    nalus,
                });
            }

            Ok((
                Self {
                    general_profile_space,
                    general_tier_flag,
                    general_profile_idc,
                    general_profile_compatibility_flags,
                    general_constraint_indicator_flags,
                    general_level_idc,
                    min_spatial_segmentation_idc,
                    parallelism_type,
                    chroma_format_idc,
                    bit_depth_luma_minus8,
                    bit_depth_chroma_minus8,
                    avg_frame_rate,
                    constant_frame_rate,
                    num_temporal_layers,
                    temporal_id_nested,
                    length_size_minus_one,
                    nalu_arrays,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for HvccBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

/// [<https://www.webmproject.org/vp9/mp4/>] VP8SampleEntry class (親: [`StsdBox`][crate::boxes::StsdBox])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct Vp08Box {
    pub visual: VisualSampleEntryFields,
    pub vpcc_box: VpccBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl Vp08Box {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"vp08");
}

impl Encode for Vp08Box {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.visual.encode(&mut buf[offset..])?;
        offset += self.vpcc_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for Vp08Box {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let visual = VisualSampleEntryFields::decode_at(payload, &mut offset)?;

            let mut vpcc_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    VpccBox::TYPE if vpcc_box.is_none() => {
                        vpcc_box = Some(VpccBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    visual,
                    vpcc_box: check_mandatory_box(vpcc_box, "vpcC", "vp08")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for Vp08Box {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.vpcc_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [<https://www.webmproject.org/vp9/mp4/>] VP9SampleEntry class (親: [`StsdBox`][crate::boxes::StsdBox])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct Vp09Box {
    pub visual: VisualSampleEntryFields,
    pub vpcc_box: VpccBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl Vp09Box {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"vp09");
}

impl Encode for Vp09Box {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.visual.encode(&mut buf[offset..])?;
        offset += self.vpcc_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for Vp09Box {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let visual = VisualSampleEntryFields::decode_at(payload, &mut offset)?;

            let mut vpcc_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    VpccBox::TYPE if vpcc_box.is_none() => {
                        vpcc_box = Some(VpccBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    visual,
                    vpcc_box: check_mandatory_box(vpcc_box, "vpcC", "vp09")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for Vp09Box {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.vpcc_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [<https://www.webmproject.org/vp9/mp4/>] VPCodecConfigurationBox class (親: [`Vp08Box`], [`Vp09Box`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct VpccBox {
    pub profile: u8,
    pub level: u8,
    pub bit_depth: Uint<u8, 4, 4>,
    pub chroma_subsampling: Uint<u8, 3, 1>,
    pub video_full_range_flag: Uint<u8, 1>,
    pub colour_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub codec_initialization_data: Vec<u8>,
}

impl VpccBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"vpcC");
}

impl Encode for VpccBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += self.profile.encode(&mut buf[offset..])?;
        offset += self.level.encode(&mut buf[offset..])?;
        offset += (self.bit_depth.to_bits()
            | self.chroma_subsampling.to_bits()
            | self.video_full_range_flag.to_bits())
        .encode(&mut buf[offset..])?;
        offset += self.colour_primaries.encode(&mut buf[offset..])?;
        offset += self.transfer_characteristics.encode(&mut buf[offset..])?;
        offset += self.matrix_coefficients.encode(&mut buf[offset..])?;
        offset += (self.codec_initialization_data.len() as u16).encode(&mut buf[offset..])?;
        offset += self.codec_initialization_data.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for VpccBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let header_obj = FullBoxHeader::decode_at(payload, &mut offset)?;
            if header_obj.version != 1 {
                return Err(Error::invalid_data(format!(
                    "Unexpected full box header version: box=vpcC, version={}",
                    header_obj.version
                )));
            }

            let profile = u8::decode_at(payload, &mut offset)?;
            let level = u8::decode_at(payload, &mut offset)?;

            let b = u8::decode_at(payload, &mut offset)?;
            let bit_depth = Uint::from_bits(b);
            let chroma_subsampling = Uint::from_bits(b);
            let video_full_range_flag = Uint::from_bits(b);
            let colour_primaries = u8::decode_at(payload, &mut offset)?;
            let transfer_characteristics = u8::decode_at(payload, &mut offset)?;
            let matrix_coefficients = u8::decode_at(payload, &mut offset)?;
            let codec_init_size = u16::decode_at(payload, &mut offset)? as usize;
            if offset + codec_init_size > payload.len() {
                return Err(Error::invalid_data(
                    "codec initialization data exceeds payload boundary",
                ));
            }
            let codec_initialization_data = payload[offset..offset + codec_init_size].to_vec();

            Ok((
                Self {
                    profile,
                    level,
                    bit_depth,
                    chroma_subsampling,
                    video_full_range_flag,
                    colour_primaries,
                    transfer_characteristics,
                    matrix_coefficients,
                    codec_initialization_data,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for VpccBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for VpccBox {
    fn full_box_version(&self) -> u8 {
        1
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [<https://aomediacodec.github.io/av1-isobmff/>] AV1SampleEntry class (親: [`StsdBox`][crate::boxes::StsdBox])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct Av01Box {
    pub visual: VisualSampleEntryFields,
    pub av1c_box: Av1cBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl Av01Box {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"av01");
}

impl Encode for Av01Box {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.visual.encode(&mut buf[offset..])?;
        offset += self.av1c_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for Av01Box {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let visual = VisualSampleEntryFields::decode_at(payload, &mut offset)?;

            let mut av1c_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    Av1cBox::TYPE if av1c_box.is_none() => {
                        av1c_box = Some(Av1cBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    visual,
                    av1c_box: check_mandatory_box(av1c_box, "av1c", "av01")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for Av01Box {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.av1c_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [<https://aomediacodec.github.io/av1-isobmff/>] AV1CodecConfigurationBox class (親: [`StsdBox`][crate::boxes::StsdBox])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct Av1cBox {
    pub seq_profile: Uint<u8, 3, 5>,
    pub seq_level_idx_0: Uint<u8, 5, 0>,
    pub seq_tier_0: Uint<u8, 1, 7>,
    pub high_bitdepth: Uint<u8, 1, 6>,
    pub twelve_bit: Uint<u8, 1, 5>,
    pub monochrome: Uint<u8, 1, 4>,
    pub chroma_subsampling_x: Uint<u8, 1, 3>,
    pub chroma_subsampling_y: Uint<u8, 1, 2>,
    pub chroma_sample_position: Uint<u8, 2, 0>,
    pub initial_presentation_delay_minus_one: Option<Uint<u8, 4, 0>>,
    pub config_obus: Vec<u8>,
}

impl Av1cBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"av1C");

    const MARKER: Uint<u8, 1, 7> = Uint::new(1);
    const VERSION: Uint<u8, 7, 0> = Uint::new(1);
}

impl Encode for Av1cBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += (Self::MARKER.to_bits() | Self::VERSION.to_bits()).encode(&mut buf[offset..])?;
        offset += (self.seq_profile.to_bits() | self.seq_level_idx_0.to_bits())
            .encode(&mut buf[offset..])?;
        offset += (self.seq_tier_0.to_bits()
            | self.high_bitdepth.to_bits()
            | self.twelve_bit.to_bits()
            | self.monochrome.to_bits()
            | self.chroma_subsampling_x.to_bits()
            | self.chroma_subsampling_y.to_bits()
            | self.chroma_sample_position.to_bits())
        .encode(&mut buf[offset..])?;
        if let Some(v) = self.initial_presentation_delay_minus_one {
            offset += (0b1_0000 | v.to_bits()).encode(&mut buf[offset..])?;
        } else {
            offset += 0u8.encode(&mut buf[offset..])?;
        }
        offset += self.config_obus.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for Av1cBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let b = u8::decode_at(payload, &mut offset)?;
            let marker = Uint::from_bits(b);
            let version = Uint::from_bits(b);
            if marker != Self::MARKER {
                return Err(Error::invalid_data("Unexpected av1C marker"));
            }
            if version != Self::VERSION {
                return Err(Error::invalid_data(format!(
                    "Unsupported av1C version: {}",
                    version.get()
                )));
            }

            let b = u8::decode_at(payload, &mut offset)?;
            let seq_profile = Uint::from_bits(b);
            let seq_level_idx_0 = Uint::from_bits(b);

            let b = u8::decode_at(payload, &mut offset)?;
            let seq_tier_0 = Uint::from_bits(b);
            let high_bitdepth = Uint::from_bits(b);
            let twelve_bit = Uint::from_bits(b);
            let monochrome = Uint::from_bits(b);
            let chroma_subsampling_x = Uint::from_bits(b);
            let chroma_subsampling_y = Uint::from_bits(b);
            let chroma_sample_position = Uint::from_bits(b);

            let b = u8::decode_at(payload, &mut offset)?;
            let initial_presentation_delay_minus_one = if Uint::<u8, 1, 4>::from_bits(b).get() == 1
            {
                Some(Uint::from_bits(b))
            } else {
                None
            };

            let config_obus = payload[offset..].to_vec();

            Ok((
                Self {
                    seq_profile,
                    seq_level_idx_0,
                    seq_tier_0,
                    high_bitdepth,
                    twelve_bit,
                    monochrome,
                    chroma_subsampling_x,
                    chroma_subsampling_y,
                    chroma_sample_position,
                    initial_presentation_delay_minus_one,
                    config_obus,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for Av1cBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

/// [<https://gitlab.xiph.org/xiph/opus/-/blob/main/doc/opus_in_isobmff.html>] OpusSampleEntry class (親: [`StsdBox`][crate::boxes::StsdBox])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct OpusBox {
    pub audio: AudioSampleEntryFields,
    pub dops_box: DopsBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl OpusBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"Opus");
}

impl Encode for OpusBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.audio.encode(&mut buf[offset..])?;
        offset += self.dops_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for OpusBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let audio = AudioSampleEntryFields::decode_at(payload, &mut offset)?;

            let mut dops_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    DopsBox::TYPE if dops_box.is_none() => {
                        dops_box = Some(DopsBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    audio,
                    dops_box: check_mandatory_box(dops_box, "dops", "Opus")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for OpusBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.dops_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-14] MP4AudioSampleEntry class
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct Mp4aBox {
    pub audio: AudioSampleEntryFields,
    pub esds_box: EsdsBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl Mp4aBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"mp4a");
}

impl Encode for Mp4aBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.audio.encode(&mut buf[offset..])?;
        offset += self.esds_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for Mp4aBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let audio = AudioSampleEntryFields::decode_at(payload, &mut offset)?;

            let mut esds_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    EsdsBox::TYPE if esds_box.is_none() => {
                        esds_box = Some(EsdsBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    audio,
                    esds_box: check_mandatory_box(esds_box, "esds", "mp4a")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for Mp4aBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.esds_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [Encapsulation of FLAC in ISO Base Media File Format] FLACSampleEntry class (親: [`StsdBox`][crate::boxes::StsdBox])
///
/// <https://github.com/xiph/flac/blob/master/doc/isoflac.txt>
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct FlacBox {
    pub audio: AudioSampleEntryFields,
    pub dfla_box: DflaBox,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl FlacBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"fLaC");
}

impl Encode for FlacBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.audio.encode(&mut buf[offset..])?;
        offset += self.dfla_box.encode(&mut buf[offset..])?;
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for FlacBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let audio = AudioSampleEntryFields::decode_at(payload, &mut offset)?;

            let mut dfla_box = None;
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    DflaBox::TYPE if dfla_box.is_none() => {
                        dfla_box = Some(DflaBox::decode_at(payload, &mut offset)?);
                    }

                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    audio,
                    dfla_box: check_mandatory_box(dfla_box, "dfLa", "fLaC")?,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for FlacBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.dfla_box).map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [Encapsulation of FLAC in ISO Base Media File Format] FLACSpecificBox class (親: [`FlacBox`])
///
/// <https://github.com/xiph/flac/blob/master/doc/isoflac.txt>
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DflaBox {
    /// FLAC メタデータブロックのリスト
    /// 最初のブロックは必ず STREAMINFO (block_type=0) でなければならない
    pub metadata_blocks: Vec<FlacMetadataBlock>,
}

impl DflaBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"dfLa");

    const VERSION: u8 = 0;
}

impl Encode for DflaBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;

        for block in &self.metadata_blocks {
            offset += block.encode(&mut buf[offset..])?;
        }

        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for DflaBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            if full_header.version != Self::VERSION {
                return Err(Error::invalid_data(format!(
                    "Unsupported dfLa version: {}",
                    full_header.version
                )));
            }

            let mut metadata_blocks = Vec::new();
            while offset < payload.len() {
                let block = FlacMetadataBlock::decode_at(payload, &mut offset)?;
                let is_last = block.last_metadata_block_flag.as_bool();
                metadata_blocks.push(block);
                if is_last {
                    break;
                }
            }

            if offset < payload.len() {
                return Err(Error::invalid_data(format!(
                    "Unexpected data after last metadata block ({} bytes remaining)",
                    payload.len() - offset
                )));
            }

            if metadata_blocks.is_empty() {
                return Err(Error::invalid_data(
                    "dfLa box must contain at least one metadata block (STREAMINFO)",
                ));
            }

            if metadata_blocks[0].block_type != FlacMetadataBlock::BLOCK_TYPE_STREAMINFO {
                return Err(Error::invalid_data(
                    "First metadata block in dfLa must be STREAMINFO (block_type=0)",
                ));
            }

            Ok((
                Self { metadata_blocks },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for DflaBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for DflaBox {
    fn full_box_version(&self) -> u8 {
        Self::VERSION
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// FLAC メタデータブロック
///
/// FLAC 仕様の METADATA_BLOCK 構造を表現する
///
/// <https://xiph.org/flac/format.html>
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlacMetadataBlock {
    /// 最後のメタデータブロックかどうかを示すフラグ
    pub last_metadata_block_flag: Uint<u8, 1, 7>,

    /// ブロックタイプ
    /// 0: STREAMINFO, 1: PADDING, 2: APPLICATION, 3: SEEKTABLE,
    /// 4: VORBIS_COMMENT, 5: CUESHEET, 6: PICTURE
    pub block_type: Uint<u8, 7>,

    /// ブロックデータ
    pub block_data: Vec<u8>,
}

impl FlacMetadataBlock {
    /// ブロックタイプ: STREAMINFO
    pub const BLOCK_TYPE_STREAMINFO: Uint<u8, 7> = Uint::new(0);

    /// ブロックタイプ: PADDING
    pub const BLOCK_TYPE_PADDING: Uint<u8, 7> = Uint::new(1);

    /// ブロックタイプ: APPLICATION
    pub const BLOCK_TYPE_APPLICATION: Uint<u8, 7> = Uint::new(2);

    /// ブロックタイプ: SEEK TABLE
    pub const BLOCK_TYPE_SEEKTABLE: Uint<u8, 7> = Uint::new(3);

    /// ブロックタイプ: VORBIS COMMENT
    pub const BLOCK_TYPE_VORBIS_COMMENT: Uint<u8, 7> = Uint::new(4);

    /// ブロックタイプ: CUESHEET
    pub const BLOCK_TYPE_CUESHEET: Uint<u8, 7> = Uint::new(5);

    /// ブロックタイプ: PICTURE
    pub const BLOCK_TYPE_PICTURE: Uint<u8, 7> = Uint::new(6);
}

impl Encode for FlacMetadataBlock {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let length = self.block_data.len();
        if length > 0xFF_FFFF {
            return Err(Error::invalid_input(
                "FLAC metadata block data is too large (max 16777215 bytes)",
            ));
        }

        let mut offset = 0;
        let first_byte = self.last_metadata_block_flag.to_bits() | self.block_type.to_bits();
        offset += first_byte.encode(&mut buf[offset..])?;

        let length_bytes = [
            ((length >> 16) & 0xFF) as u8,
            ((length >> 8) & 0xFF) as u8,
            (length & 0xFF) as u8,
        ];
        offset += length_bytes.encode(&mut buf[offset..])?;
        offset += self.block_data.as_slice().encode(&mut buf[offset..])?;

        Ok(offset)
    }
}

impl Decode for FlacMetadataBlock {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let mut offset = 0;

        let first_byte = u8::decode_at(buf, &mut offset)?;
        let last_metadata_block_flag = Uint::from_bits(first_byte);
        let block_type = Uint::from_bits(first_byte);

        let length_bytes = <[u8; 3]>::decode_at(buf, &mut offset)?;
        let length = ((length_bytes[0] as usize) << 16)
            | ((length_bytes[1] as usize) << 8)
            | (length_bytes[2] as usize);

        Error::check_buffer_size(offset + length, buf)?;
        let block_data = buf[offset..offset + length].to_vec();
        offset += length;

        Ok((
            Self {
                last_metadata_block_flag,
                block_type,
                block_data,
            },
            offset,
        ))
    }
}

/// 音声系の [`SampleEntry`] に共通のフィールドをまとめた構造体
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct AudioSampleEntryFields {
    pub data_reference_index: NonZeroU16,
    pub channelcount: u16,
    pub samplesize: u16,
    pub samplerate: FixedPointNumber<u16, u16>,
}

impl AudioSampleEntryFields {
    /// [`AudioSampleEntryFields::data_reference_index`] のデフォルト値
    pub const DEFAULT_DATA_REFERENCE_INDEX: NonZeroU16 = NonZeroU16::MIN;

    /// [`AudioSampleEntryFields::samplesize`] のデフォルト値 (16)
    pub const DEFAULT_SAMPLESIZE: u16 = 16;
}

impl Encode for AudioSampleEntryFields {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = 0;
        offset += [0u8; 6].encode(&mut buf[offset..])?;
        offset += self.data_reference_index.encode(&mut buf[offset..])?;
        offset += [0u8; 4 * 2].encode(&mut buf[offset..])?;
        offset += self.channelcount.encode(&mut buf[offset..])?;
        offset += self.samplesize.encode(&mut buf[offset..])?;
        offset += [0u8; 2].encode(&mut buf[offset..])?;
        offset += [0u8; 2].encode(&mut buf[offset..])?;
        offset += self.samplerate.encode(&mut buf[offset..])?;
        Ok(offset)
    }
}

impl Decode for AudioSampleEntryFields {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let mut offset = 0;
        let _ = <[u8; 6]>::decode_at(buf, &mut offset)?;
        let data_reference_index = NonZeroU16::decode_at(buf, &mut offset)?;
        let _ = <[u8; 4 * 2]>::decode_at(buf, &mut offset)?;
        let channelcount = u16::decode_at(buf, &mut offset)?;
        let samplesize = u16::decode_at(buf, &mut offset)?;
        let _ = <[u8; 2]>::decode_at(buf, &mut offset)?;
        let _ = <[u8; 2]>::decode_at(buf, &mut offset)?;
        let samplerate = FixedPointNumber::decode_at(buf, &mut offset)?;
        Ok((
            Self {
                data_reference_index,
                channelcount,
                samplesize,
                samplerate,
            },
            offset,
        ))
    }
}

/// [<https://gitlab.xiph.org/xiph/opus/-/blob/main/doc/opus_in_isobmff.html>] OpusSpecificBox class (親: [`OpusBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct DopsBox {
    pub output_channel_count: u8,
    pub pre_skip: u16,
    pub input_sample_rate: u32,
    pub output_gain: i16,
}

impl DopsBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"dOps");

    const VERSION: u8 = 0;
}

impl Encode for DopsBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += Self::VERSION.encode(&mut buf[offset..])?;
        offset += self.output_channel_count.encode(&mut buf[offset..])?;
        offset += self.pre_skip.encode(&mut buf[offset..])?;
        offset += self.input_sample_rate.encode(&mut buf[offset..])?;
        offset += self.output_gain.encode(&mut buf[offset..])?;
        offset += 0u8.encode(&mut buf[offset..])?; // ChannelMappingFamily
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for DopsBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let version = u8::decode_at(payload, &mut offset)?;
            if version != Self::VERSION {
                return Err(Error::invalid_data(format!(
                    "Unsupported dOps version: {version}"
                )));
            }

            let output_channel_count = u8::decode_at(payload, &mut offset)?;
            let pre_skip = u16::decode_at(payload, &mut offset)?;
            let input_sample_rate = u32::decode_at(payload, &mut offset)?;
            let output_gain = i16::decode_at(payload, &mut offset)?;
            let channel_mapping_family = u8::decode_at(payload, &mut offset)?;
            if channel_mapping_family != 0 {
                return Err(Error::unsupported(
                    "`ChannelMappingFamily != 0` in 'dOps' box is not supported",
                ));
            }

            Ok((
                Self {
                    output_channel_count,
                    pre_skip,
                    input_sample_rate,
                    output_gain,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for DopsBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}
