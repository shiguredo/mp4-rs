//! ボックス群
use alloc::{boxed::Box, format, vec::Vec};

use crate::{BaseBox, BoxHeader, BoxSize, BoxType, Decode, Encode, Error, Result};

pub use crate::boxes_fmp4::{
    MfhdBox, MfraBox, MfroBox, MoofBox, SidxBox, SidxReference, TfdtBox, TfhdBox, TfraBox,
    TfraEntry, TrafBox, TrunBox, TrunSample,
};
pub use crate::boxes_moov_tree::{
    Co64Box, CslgBox, CttsBox, CttsEntry, DinfBox, DrefBox, EdtsBox, ElstBox, ElstEntry, EsdsBox,
    HdlrBox, MdhdBox, MdiaBox, MehdBox, MinfBox, MoovBox, MvexBox, MvhdBox, SdtpBox,
    SdtpSampleFlags, SmhdBox, StblBox, StcoBox, StscBox, StscEntry, StsdBox, StssBox, StszBox,
    SttsBox, SttsEntry, TkhdBox, TrakBox, TrexBox, UrlBox, VmhdBox,
};
pub use crate::boxes_sample_entry::{
    AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, DflaBox, DopsBox, FlacBox,
    FlacMetadataBlock, Hev1Box, Hvc1Box, HvccBox, HvccNalUintArray, Mp4aBox, OpusBox, SampleEntry,
    VisualSampleEntryFields, Vp08Box, Vp09Box, VpccBox,
};

pub(crate) fn with_box_type<F, T>(ty: BoxType, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    f().map_err(|mut e| {
        if e.box_type.is_none() {
            // エラー発生時には、エラーの原因となった（最初の）ボックスの種別の情報をセットする
            e.box_type = Some(ty);
        }
        e
    })
}

#[track_caller]
pub(crate) fn check_mandatory_box<T>(
    maybe_box: Option<T>,
    expected: &str,
    parent: &str,
) -> Result<T> {
    // [NOTE]
    // ok_or_else() でも同じことができるが `Error::invalid_data()` をクロージャーで囲ってしまうと、
    // `check_mandatory_box()` 自体の `track_caller` 指定の意味がなくなってしまうので、あえて if-else で実装している
    if let Some(b) = maybe_box {
        Ok(b)
    } else {
        Err(Error::invalid_data(format!(
            "Missing mandatory '{expected}' box in '{parent}' box"
        )))
    }
}

/// ペイロードの解釈方法が不明なボックスを保持するための構造体
///
/// ペイロードは単なるバイト列として扱われる
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnknownBox {
    /// ボックス種別
    pub box_type: BoxType,

    /// ボックスサイズ
    pub box_size: BoxSize,

    /// ペイロード
    pub payload: Vec<u8>,
}

impl Encode for UnknownBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = BoxHeader::new(self.box_type, self.box_size).encode(buf)?;
        offset += self.payload.encode(&mut buf[offset..])?;
        Ok(offset)
    }
}

impl Decode for UnknownBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
        Ok((
            Self {
                box_type: header.box_type,
                box_size: header.box_size,
                payload: payload.to_vec(),
            },
            header.external_size() + payload.len(),
        ))
    }
}

impl BaseBox for UnknownBox {
    fn box_type(&self) -> BoxType {
        self.box_type
    }

    fn is_unknown_box(&self) -> bool {
        true
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

/// [`FtypBox`] で使われるブランド定義
///
/// ブランドは、対象の MP4 ファイルを読み込んで処理する際に必要となる要件（登場する可能性があるボックス群やハンドリングすべきフラグなど）を指定する
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Brand([u8; 4]);

impl Brand {
    /// [ISO/IEC 14496-12] `isom` ブランド
    pub const ISOM: Self = Self::new(*b"isom");

    /// [ISO/IEC 14496-12] `avc1` ブランド
    pub const AVC1: Self = Self::new(*b"avc1");

    /// [ISO/IEC 14496-15] `hev1` ブランド
    pub const HEV1: Self = Self::new(*b"hev1");

    /// [ISO/IEC 14496-15] `hvc1` ブランド
    pub const HVC1: Self = Self::new(*b"hvc1");

    /// [ISO/IEC 14496-12] `iso2` ブランド
    pub const ISO2: Self = Self::new(*b"iso2");

    /// [ISO/IEC 14496-12] `mp71` ブランド
    pub const MP71: Self = Self::new(*b"mp71");

    /// [ISO/IEC 14496-12] `iso3` ブランド
    pub const ISO3: Self = Self::new(*b"iso3");

    /// [ISO/IEC 14496-12] `iso4` ブランド
    pub const ISO4: Self = Self::new(*b"iso4");

    /// [ISO/IEC 14496-12] `iso5` ブランド
    pub const ISO5: Self = Self::new(*b"iso5");

    /// [ISO/IEC 14496-12] `iso6` ブランド
    pub const ISO6: Self = Self::new(*b"iso6");

    /// [ISO/IEC 14496-12] `iso7` ブランド
    pub const ISO7: Self = Self::new(*b"iso7");

    /// [ISO/IEC 14496-12] `iso8` ブランド
    pub const ISO8: Self = Self::new(*b"iso8");

    /// [ISO/IEC 14496-12] `iso9` ブランド
    pub const ISO9: Self = Self::new(*b"iso9");

    /// [ISO/IEC 14496-12] `isoa` ブランド
    pub const ISOA: Self = Self::new(*b"isoa");

    /// [ISO/IEC 14496-12] `isob` ブランド
    pub const ISOB: Self = Self::new(*b"isob");

    /// [ISO/IEC 14496-12] `relo` ブランド
    pub const RELO: Self = Self::new(*b"relo");

    /// [ISO/IEC 14496-14] `mp41` ブランド
    pub const MP41: Self = Self::new(*b"mp41");

    /// [<https://aomediacodec.github.io/av1-isobmff/>] `av01` ブランド
    pub const AV01: Self = Self::new(*b"av01");

    /// バイト列を渡して、対応するブランドを作成する
    pub const fn new(brand: [u8; 4]) -> Self {
        Self(brand)
    }

    /// このブランドを表すバイト列を取得する
    pub const fn get(self) -> [u8; 4] {
        self.0
    }
}

impl core::fmt::Debug for Brand {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Ok(s) = core::str::from_utf8(&self.0) {
            f.debug_tuple("Brand").field(&s).finish()
        } else {
            f.debug_tuple("Brand").field(&self.0).finish()
        }
    }
}

impl Encode for Brand {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        self.0.encode(buf)
    }
}

impl Decode for Brand {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (bytes, offset) = <[u8; 4]>::decode(buf)?;
        Ok((Self(bytes), offset))
    }
}

/// [ISO/IEC 14496-12] FileTypeBox class
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct FtypBox {
    pub major_brand: Brand,
    pub minor_version: u32,
    pub compatible_brands: Vec<Brand>,
}

impl FtypBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"ftyp");
}

impl Encode for FtypBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.major_brand.encode(&mut buf[offset..])?;
        offset += self.minor_version.encode(&mut buf[offset..])?;
        for brand in &self.compatible_brands {
            offset += brand.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for FtypBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let major_brand = Brand::decode_at(payload, &mut offset)?;
            let minor_version = u32::decode_at(payload, &mut offset)?;

            let mut compatible_brands = Vec::new();
            while offset < payload.len() {
                compatible_brands.push(Brand::decode_at(payload, &mut offset)?);
            }

            Ok((
                Self {
                    major_brand,
                    minor_version,
                    compatible_brands,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for FtypBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

/// [`Mp4File`](crate::Mp4File) のトップレベルに位置するボックス群のデフォルト実装
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum RootBox {
    Free(FreeBox),
    Mdat(MdatBox),
    Moov(MoovBox),
    Moof(MoofBox),
    Mfra(MfraBox),
    Sidx(SidxBox),
    Unknown(UnknownBox),
}

impl RootBox {
    fn inner_box(&self) -> &dyn BaseBox {
        match self {
            RootBox::Free(b) => b,
            RootBox::Mdat(b) => b,
            RootBox::Moov(b) => b,
            RootBox::Moof(b) => b,
            RootBox::Mfra(b) => b,
            RootBox::Sidx(b) => b,
            RootBox::Unknown(b) => b,
        }
    }
}

impl Encode for RootBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        match self {
            RootBox::Free(b) => b.encode(buf),
            RootBox::Mdat(b) => b.encode(buf),
            RootBox::Moov(b) => b.encode(buf),
            RootBox::Moof(b) => b.encode(buf),
            RootBox::Mfra(b) => b.encode(buf),
            RootBox::Sidx(b) => b.encode(buf),
            RootBox::Unknown(b) => b.encode(buf),
        }
    }
}

impl Decode for RootBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (header, _header_size) = BoxHeader::decode(buf)?;
        match header.box_type {
            FreeBox::TYPE => FreeBox::decode(buf).map(|(b, n)| (RootBox::Free(b), n)),
            MdatBox::TYPE => MdatBox::decode(buf).map(|(b, n)| (RootBox::Mdat(b), n)),
            MoovBox::TYPE => MoovBox::decode(buf).map(|(b, n)| (RootBox::Moov(b), n)),
            MoofBox::TYPE => MoofBox::decode(buf).map(|(b, n)| (RootBox::Moof(b), n)),
            MfraBox::TYPE => MfraBox::decode(buf).map(|(b, n)| (RootBox::Mfra(b), n)),
            SidxBox::TYPE => SidxBox::decode(buf).map(|(b, n)| (RootBox::Sidx(b), n)),
            _ => UnknownBox::decode(buf).map(|(b, n)| (RootBox::Unknown(b), n)),
        }
    }
}

impl BaseBox for RootBox {
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

/// [ISO/IEC 14496-12] FreeSpaceBox class
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct FreeBox {
    pub payload: Vec<u8>,
}

impl FreeBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"free");
}

impl Encode for FreeBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let box_size = BoxSize::with_payload_size(Self::TYPE, self.payload.len() as u64);
        let mut offset = BoxHeader::new(Self::TYPE, box_size).encode(buf)?;
        offset += self.payload.encode(&mut buf[offset..])?;
        Ok(offset)
    }
}

impl Decode for FreeBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            Ok((
                Self {
                    payload: payload.to_vec(),
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for FreeBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

/// [ISO/IEC 14496-12] MediaDataBox class
///
/// # NOTE
///
/// 可変長ペイロードを表現したい場合には、この構造体ではなく [`BoxHeader`] を直接使用する必要がある
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MdatBox {
    /// ペイロード
    pub payload: Vec<u8>,
}

impl MdatBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"mdat");
}

impl Encode for MdatBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let box_size = BoxSize::with_payload_size(Self::TYPE, self.payload.len() as u64);
        let mut offset = BoxHeader::new(Self::TYPE, box_size).encode(buf)?;
        offset += self.payload.encode(&mut buf[offset..])?;
        Ok(offset)
    }
}

impl Decode for MdatBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            Ok((
                Self {
                    payload: payload.to_vec(),
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for MdatBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}
