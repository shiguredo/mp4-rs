//! CENC (Common Encryption) 関連のボックス定義
//!
//! ISO/IEC 23001-7 の `pssh` / `tenc` / `senc` と、ISO/IEC 14496-12 の `sinf` /
//! `frma` / `schm` / `schi` / `saiz` / `saio` を扱う。
use alloc::{boxed::Box, format, vec::Vec};

use crate::{
    BaseBox, BoxHeader, BoxType, Decode, Encode, Error, FullBox, FullBoxFlags, FullBoxHeader,
    Result, Uint, Utf8String,
    basic_types::as_box_object,
    boxes::{UnknownBox, check_mandatory_box, with_box_type},
};

/// CENC の保護スキーム (`cenc` / `cbc1` / `cens` / `cbcs`) かどうかを返す
pub(crate) fn is_cenc_scheme(scheme_type: [u8; 4]) -> bool {
    matches!(&scheme_type, b"cenc" | b"cbc1" | b"cens" | b"cbcs")
}

/// パターン暗号化を使うスキーム (`cens` / `cbcs`) かどうかを返す
fn uses_pattern_encryption(scheme_type: [u8; 4]) -> bool {
    matches!(&scheme_type, b"cens" | b"cbcs")
}

/// このライブラリが対応する FullBox のバージョン (0 または 1) を検査する
#[track_caller]
fn check_encode_version(box_name: &str, version: u8) -> Result<()> {
    if version > 1 {
        return Err(Error::invalid_input(format!(
            "{box_name} version must be 0 or 1, got {version}"
        )));
    }
    Ok(())
}

/// デコードした FullBox のバージョン (0 または 1) を検査する
#[track_caller]
fn check_decode_version(box_name: &str, version: u8) -> Result<()> {
    if version > 1 {
        return Err(Error::invalid_data(format!(
            "Unsupported {box_name} version: {version}"
        )));
    }
    Ok(())
}

/// 保護スキームと `tenc` のパターン暗号化パラメーターが整合しているかを返す
fn tenc_pattern_matches_scheme(scheme_type: [u8; 4], tenc: &TencBox) -> bool {
    let has_pattern =
        tenc.default_crypt_byte_block.is_some() && tenc.default_skip_byte_block.is_some();
    if uses_pattern_encryption(scheme_type) {
        has_pattern
    } else {
        !has_pattern
    }
}

/// 保護スキームと `tenc` のパターン暗号化パラメーターの整合性を検査する
#[track_caller]
fn check_tenc_for_scheme(scheme_type: [u8; 4], tenc: &TencBox) -> Result<()> {
    if !tenc_pattern_matches_scheme(scheme_type, tenc) {
        if uses_pattern_encryption(scheme_type) {
            return Err(Error::invalid_input(format!(
                "'{}' requires tenc version 1 pattern encryption parameters",
                BoxType::Normal(scheme_type)
            )));
        }
        return Err(Error::invalid_input(format!(
            "'{}' must not have tenc version 1 pattern encryption parameters",
            BoxType::Normal(scheme_type)
        )));
    }
    Ok(())
}

/// [ISO/IEC 23001-7] ProtectionSystemSpecificHeaderBox class
///
/// DRM システム固有の初期化データを保持する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PsshBox {
    /// FullBox バージョン (0 または 1)
    ///
    /// version 0 は KID の一覧を持たず、version 1 は `kids` を持つ。
    pub version: u8,

    /// DRM システムの識別子 (16 バイト)
    pub system_id: [u8; 16],

    /// version 1 の KID 一覧
    ///
    /// version 1 では空の一覧も許容され、その場合はムービー内の全 KID に適用される。
    /// version 0 では空でなければならない。
    pub kids: Vec<[u8; 16]>,

    /// DRM システム固有のデータ
    pub data: Vec<u8>,
}

impl PsshBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"pssh");
}

impl Encode for PsshBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        check_encode_version("PsshBox", self.version)?;
        if self.version == 0 && !self.kids.is_empty() {
            return Err(Error::invalid_input("PsshBox version 0 must not have KIDs"));
        }

        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += self.system_id.encode(&mut buf[offset..])?;
        if self.version > 0 {
            let kid_count = u32::try_from(self.kids.len())
                .map_err(|_| Error::invalid_input("PsshBox KID count exceeds u32::MAX"))?;
            offset += kid_count.encode(&mut buf[offset..])?;
            for kid in &self.kids {
                offset += kid.encode(&mut buf[offset..])?;
            }
        }
        let data_size = u32::try_from(self.data.len())
            .map_err(|_| Error::invalid_input("PsshBox data size exceeds u32::MAX"))?;
        offset += data_size.encode(&mut buf[offset..])?;
        offset += self.data.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for PsshBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            check_decode_version("PsshBox", full_header.version)?;

            let system_id = <[u8; 16]>::decode_at(payload, &mut offset)?;
            let mut kids = Vec::new();
            if full_header.version > 0 {
                let kid_count = u32::decode_at(payload, &mut offset)?;
                for _ in 0..kid_count {
                    kids.push(<[u8; 16]>::decode_at(payload, &mut offset)?);
                }
            }

            let data_size = usize::try_from(u32::decode_at(payload, &mut offset)?)
                .map_err(|_| Error::invalid_data("PsshBox data size exceeds usize::MAX"))?;
            let data_end = offset
                .checked_add(data_size)
                .ok_or_else(|| Error::invalid_data("PsshBox data size overflow"))?;
            if data_end > payload.len() {
                return Err(Error::invalid_data(
                    "PsshBox data size exceeds remaining payload",
                ));
            }
            let data = payload[offset..data_end].to_vec();
            if data_end != payload.len() {
                return Err(Error::invalid_data("PsshBox has trailing bytes"));
            }

            Ok((
                Self {
                    version: full_header.version,
                    system_id,
                    kids,
                    data,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for PsshBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for PsshBox {
    fn full_box_version(&self) -> u8 {
        self.version
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] OriginalFormatBox class
///
/// 保護される前のサンプルエントリーの 4CC を保持する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FrmaBox {
    /// 保護される前のサンプルエントリーの 4CC
    pub data_format: [u8; 4],
}

impl FrmaBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"frma");
}

impl Encode for FrmaBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.data_format.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for FrmaBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;
            if payload.len() != 4 {
                return Err(Error::invalid_data("FrmaBox payload must be 4 bytes"));
            }
            Ok((
                Self {
                    data_format: <[u8; 4]>::decode(payload)?.0,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for FrmaBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

/// [ISO/IEC 14496-12] SchemeTypeBox class
///
/// 保護スキームの種類とバージョンを保持する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SchmBox {
    /// 保護スキームの 4CC (`cenc` / `cbc1` / `cens` / `cbcs` など)
    pub scheme_type: [u8; 4],

    /// 保護スキームのバージョン
    pub scheme_version: u32,

    /// 保護スキームの URI
    ///
    /// flags の `0x000001` が立っている場合だけ存在する。
    pub scheme_uri: Option<Utf8String>,
}

impl SchmBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"schm");

    /// `SchemeTypeBox` の flags のうち、`scheme_uri` の存在を示すビット
    pub const FLAG_SCHEME_URI_PRESENT: u32 = 0x000001;
}

impl Encode for SchmBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += self.scheme_type.encode(&mut buf[offset..])?;
        offset += self.scheme_version.encode(&mut buf[offset..])?;
        if let Some(uri) = &self.scheme_uri {
            offset += uri.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for SchmBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            if full_header.version != 0 {
                return Err(Error::invalid_data(format!(
                    "Unsupported SchmBox version: {}",
                    full_header.version
                )));
            }
            if full_header.flags.get() & !Self::FLAG_SCHEME_URI_PRESENT != 0 {
                return Err(Error::invalid_data(format!(
                    "Unsupported SchmBox flags: {:#x}",
                    full_header.flags.get()
                )));
            }

            let scheme_type = <[u8; 4]>::decode_at(payload, &mut offset)?;
            let scheme_version = u32::decode_at(payload, &mut offset)?;
            let scheme_uri = if full_header.flags.get() & Self::FLAG_SCHEME_URI_PRESENT != 0 {
                Some(Utf8String::decode_at(payload, &mut offset)?)
            } else {
                None
            };
            if offset != payload.len() {
                return Err(Error::invalid_data("SchmBox has trailing bytes"));
            }

            Ok((
                Self {
                    scheme_type,
                    scheme_version,
                    scheme_uri,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for SchmBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for SchmBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        let flags = if self.scheme_uri.is_some() {
            Self::FLAG_SCHEME_URI_PRESENT
        } else {
            0
        };
        FullBoxFlags::new(flags)
    }
}

/// [ISO/IEC 23001-7] TrackEncryptionBox class
///
/// トラック全体の暗号化パラメーターの既定値を保持する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TencBox {
    /// 既定の暗号化の有無 (0 または 1)
    pub default_is_protected: u8,

    /// 既定のサンプルごとの IV サイズ (0 / 8 / 16)
    pub default_per_sample_iv_size: u8,

    /// 既定の KID (16 バイト)
    pub default_kid: [u8; 16],

    /// 既定の constant IV
    ///
    /// `default_is_protected == 1 && default_per_sample_iv_size == 0` の場合だけ存在する。
    pub default_constant_iv: Option<Vec<u8>>,

    /// パターン暗号化の暗号化ブロック数
    ///
    /// version 1 の場合だけ存在する。
    pub default_crypt_byte_block: Option<Uint<u8, 4>>,

    /// パターン暗号化のスキップブロック数
    ///
    /// version 1 の場合だけ存在する。
    pub default_skip_byte_block: Option<Uint<u8, 4>>,
}

impl TencBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"tenc");
}

impl Encode for TencBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        if self.default_is_protected > 1 {
            return Err(Error::invalid_input(
                "TencBox default_is_protected must be 0 or 1",
            ));
        }
        if !matches!(self.default_per_sample_iv_size, 0 | 8 | 16) {
            return Err(Error::invalid_input(
                "TencBox default_per_sample_iv_size must be 0, 8, or 16",
            ));
        }
        let has_crypt = self.default_crypt_byte_block.is_some();
        let has_skip = self.default_skip_byte_block.is_some();
        if has_crypt != has_skip {
            return Err(Error::invalid_input(
                "TencBox pattern encryption parameters must both be present or both be absent",
            ));
        }
        if let Some(v) = self.default_crypt_byte_block
            && v.get() > 0x0F
        {
            return Err(Error::invalid_input(
                "TencBox default_crypt_byte_block exceeds 4 bits",
            ));
        }
        if let Some(v) = self.default_skip_byte_block
            && v.get() > 0x0F
        {
            return Err(Error::invalid_input(
                "TencBox default_skip_byte_block exceeds 4 bits",
            ));
        }
        if self.default_is_protected == 1 && self.default_per_sample_iv_size == 0 {
            let iv = self.default_constant_iv.as_ref().ok_or_else(|| {
                Error::invalid_input(
                    "TencBox default_constant_iv is required when constant IV is used",
                )
            })?;
            if iv.len() != 8 && iv.len() != 16 {
                return Err(Error::invalid_input(
                    "TencBox default_constant_iv must be 8 or 16 bytes",
                ));
            }
        } else if self.default_constant_iv.is_some() {
            return Err(Error::invalid_input(
                "TencBox default_constant_iv must be absent when per-sample IV is used",
            ));
        }

        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += 0u8.encode(&mut buf[offset..])?;
        if self.full_box_version() == 1 {
            let crypt = self.default_crypt_byte_block.map(|v| v.get()).unwrap_or(0);
            let skip = self.default_skip_byte_block.map(|v| v.get()).unwrap_or(0);
            offset += ((crypt << 4) | skip).encode(&mut buf[offset..])?;
        } else {
            offset += 0u8.encode(&mut buf[offset..])?;
        }
        offset += self.default_is_protected.encode(&mut buf[offset..])?;
        offset += self.default_per_sample_iv_size.encode(&mut buf[offset..])?;
        offset += self.default_kid.encode(&mut buf[offset..])?;
        if let Some(iv) = &self.default_constant_iv {
            offset += u8::try_from(iv.len())
                .map_err(|_| Error::invalid_input("TencBox constant IV exceeds u8::MAX"))?
                .encode(&mut buf[offset..])?;
            offset += iv.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for TencBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            check_decode_version("TencBox", full_header.version)?;

            let _reserved = u8::decode_at(payload, &mut offset)?;
            let (default_crypt_byte_block, default_skip_byte_block) = if full_header.version == 1 {
                let pattern = u8::decode_at(payload, &mut offset)?;
                (
                    Some(Uint::new(pattern >> 4)),
                    Some(Uint::new(pattern & 0x0F)),
                )
            } else {
                let _reserved = u8::decode_at(payload, &mut offset)?;
                (None, None)
            };

            let default_is_protected = u8::decode_at(payload, &mut offset)?;
            if default_is_protected > 1 {
                return Err(Error::invalid_data(
                    "TencBox default_is_protected must be 0 or 1",
                ));
            }
            let default_per_sample_iv_size = u8::decode_at(payload, &mut offset)?;
            if !matches!(default_per_sample_iv_size, 0 | 8 | 16) {
                return Err(Error::invalid_data(
                    "TencBox default_per_sample_iv_size must be 0, 8, or 16",
                ));
            }
            let default_kid = <[u8; 16]>::decode_at(payload, &mut offset)?;

            let default_constant_iv =
                if default_is_protected == 1 && default_per_sample_iv_size == 0 {
                    let size = usize::from(u8::decode_at(payload, &mut offset)?);
                    if size != 8 && size != 16 {
                        return Err(Error::invalid_data(
                            "TencBox default_constant_iv must be 8 or 16 bytes",
                        ));
                    }
                    let end = offset
                        .checked_add(size)
                        .ok_or_else(|| Error::invalid_data("TencBox constant IV size overflow"))?;
                    if end > payload.len() {
                        return Err(Error::invalid_data(
                            "TencBox constant IV exceeds remaining payload",
                        ));
                    }
                    let iv = payload[offset..end].to_vec();
                    offset = end;
                    Some(iv)
                } else {
                    None
                };
            if offset != payload.len() {
                return Err(Error::invalid_data("TencBox has trailing bytes"));
            }

            Ok((
                Self {
                    default_is_protected,
                    default_per_sample_iv_size,
                    default_kid,
                    default_constant_iv,
                    default_crypt_byte_block,
                    default_skip_byte_block,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for TencBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for TencBox {
    fn full_box_version(&self) -> u8 {
        if self.default_crypt_byte_block.is_some() && self.default_skip_byte_block.is_some() {
            1
        } else {
            0
        }
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// SAI を 1 バイトも持たないサンプルを `senc` のデコードで受理する最大数
///
/// `senc` は IV サイズ 0 かつサブサンプル暗号化でない場合、サンプルごとのデータを持たないため、
/// `sample_count` が入力サイズから制約されない。入力サイズに比例しないメモリ確保を防ぐための
/// 上限であり、仕様から決まる値ではない。
pub(crate) const MAX_SAMPLES_WITHOUT_AUX_INFO: usize = 1_048_576;

/// ISO/IEC 23001-7 のサブサンプル暗号化で使う、暗号化されない範囲と暗号化される範囲の組
///
/// `senc` (ISO/IEC 23001-7:2023 7.2.1) の 1 サブサンプル分に相当する。
/// 入力データの形式は将来の改訂で変わる可能性がある。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subsample {
    /// 暗号化されないバイト数
    pub bytes_of_clear_data: u16,

    /// 暗号化されるバイト数
    pub bytes_of_protected_data: u32,
}

/// サンプル単位の CENC 暗号化情報
///
/// `senc` (ISO/IEC 23001-7:2023 7.2.1) の 1 サンプル分、および `saiz` が指す
/// Sample Auxiliary Information (SAI) の内容を表す。
/// 入力データの形式は将来の改訂で変わる可能性がある。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SampleEncryptionInfo {
    /// 初期化ベクトル
    ///
    /// constant IV を使うサンプルでは空になる。
    pub iv: Vec<u8>,

    /// 暗号化される範囲
    ///
    /// 空の場合はサンプル全体を暗号化する。
    pub subsamples: Vec<Subsample>,
}

impl SampleEncryptionInfo {
    /// このサンプルの SAI 長を返す
    ///
    /// `subsample_encryption` は `senc` の flags の `0x000002` が立っているかどうかである。
    /// 立っている場合、サブサンプルを持たないサンプルにも `subsample_count` の 2 バイトが入る。
    /// 長さが `usize` で表現できない場合は [`None`] を返す
    pub(crate) fn aux_info_size(&self, subsample_encryption: bool) -> Option<usize> {
        let mut size = self.iv.len();
        if subsample_encryption {
            size = size.checked_add(2)?;
            size = size.checked_add(6usize.checked_mul(self.subsamples.len())?)?;
        }
        Some(size)
    }
}

/// [ISO/IEC 23001-7] SampleEncryptionBox class
///
/// CENC で暗号化されたサンプルごとの SAI (初期化ベクトルとサブサンプル範囲) を保持する。
///
/// # 対応バージョン
///
/// ISO/IEC 23001-7:2023 7.2.1 の version 0 のみ対応する。version 0 以外のデコードは
/// [`crate::ErrorKind::InvalidData`] を返す。
///
/// # IV サイズ
///
/// `senc` 自体はサンプルごとの IV サイズを持たず、対応する `tenc` の
/// `default_per_sample_iv_size` から決まる。そのため [`Decode`] だけではデコードできず、
/// 常に [`crate::ErrorKind::Unsupported`] を返す。正式な API は
/// [`SencBox::decode_with_iv_size()`] である。
///
/// # 対応フラグ
///
/// flags はサブサンプル暗号化を示す `0x000002` のみ対応する。それ以外のビットが立っている
/// 入力のデコードは [`crate::ErrorKind::InvalidData`] を返す。
///
/// なお、ここでの扱いは ISO/IEC 23001-7:2023 に基づくものであり、将来の改訂で
/// 変わる可能性がある。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SencBox {
    /// サンプルごとの暗号化情報
    ///
    /// 要素数が `sample_count` になる。constant IV を使うサンプルは `iv` が空になる。
    pub samples: Vec<SampleEncryptionInfo>,
}

impl SencBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"senc");

    /// サブサンプル暗号化を示す flags のビット
    pub const FLAG_SUBSAMPLE_ENCRYPTION: u32 = 0x000002;

    /// `tenc` から決まるサンプルごとの IV サイズを指定してデコードする
    ///
    /// 第 1 引数はボックスヘッダーを含むバイト列、第 2 引数は `tenc` の
    /// `default_per_sample_iv_size` (0 / 8 / 16) である。
    ///
    /// `per_sample_iv_size` が 0 / 8 / 16 以外の場合は [`crate::ErrorKind::InvalidData`] を返す。
    /// version 0 以外、未知の flags、SAI がペイロードを超える入力、サブサンプル表が
    /// ペイロードの残りに収まらない入力も [`crate::ErrorKind::InvalidData`] を返す。
    /// `per_sample_iv_size` が 0 かつサブサンプル暗号化でない場合、`sample_count` は
    /// 入力サイズから制約されないため、実装上の上限を超えると [`crate::ErrorKind::InvalidData`] を返す。
    ///
    /// なお、ここでの扱いは ISO/IEC 23001-7:2023 7.2.1 に基づくものであり、
    /// 将来の改訂で変わる可能性がある。
    pub fn decode_with_iv_size(buf: &[u8], per_sample_iv_size: u8) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            if !matches!(per_sample_iv_size, 0 | 8 | 16) {
                return Err(Error::invalid_data(format!(
                    "SencBox per-sample IV size must be 0, 8, or 16, got {per_sample_iv_size}"
                )));
            }

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            if full_header.version != 0 {
                return Err(Error::invalid_data(format!(
                    "Unsupported SencBox version: {}",
                    full_header.version
                )));
            }
            if full_header.flags.get() & !Self::FLAG_SUBSAMPLE_ENCRYPTION != 0 {
                return Err(Error::invalid_data(format!(
                    "Unsupported SencBox flags: {:#x}",
                    full_header.flags.get()
                )));
            }
            let subsample_encryption =
                full_header.flags.get() & Self::FLAG_SUBSAMPLE_ENCRYPTION != 0;

            let sample_count = u32::decode_at(payload, &mut offset)?;
            // サンプル 1 つあたりの最小バイト数。IV サイズ 0 かつサブサンプル暗号化でない場合は
            // 各サンプルがペイロードを持たないため、sample_count は入力サイズから制約されない。
            // そのまま確保すると小さな入力で巨大なメモリを確保できてしまうため、上限を設ける
            let bytes_per_sample =
                usize::from(per_sample_iv_size) + if subsample_encryption { 2 } else { 0 };
            let sample_count = usize::try_from(sample_count)
                .map_err(|_| Error::invalid_data("SencBox sample count exceeds usize::MAX"))?;
            if bytes_per_sample == 0 {
                // 入力によらず確保するメモリを抑えるための上限であり、
                // 1 つの traf にこれ以上のサンプルを置く実用的なユースケースは想定していない
                if sample_count > MAX_SAMPLES_WITHOUT_AUX_INFO {
                    return Err(Error::invalid_data(
                        "SencBox sample count exceeds maximum allowed when samples have no auxiliary information",
                    ));
                }
            } else if sample_count > payload.len().saturating_sub(offset) / bytes_per_sample {
                return Err(Error::invalid_data(
                    "SencBox sample count exceeds available payload",
                ));
            }
            let mut samples = Vec::new();
            for _ in 0..sample_count {
                let mut iv = Vec::new();
                if per_sample_iv_size > 0 {
                    let iv_size = usize::from(per_sample_iv_size);
                    let iv_end = offset
                        .checked_add(iv_size)
                        .ok_or_else(|| Error::invalid_data("SencBox sample IV size overflow"))?;
                    if iv_end > payload.len() {
                        return Err(Error::invalid_data(
                            "SencBox sample IV exceeds remaining payload",
                        ));
                    }
                    iv.extend_from_slice(&payload[offset..iv_end]);
                    offset = iv_end;
                }

                let mut subsamples = Vec::new();
                if subsample_encryption {
                    let subsample_count = u16::decode_at(payload, &mut offset)?;
                    // サブサンプル表が残りのペイロードに収まるかを先に確認する。
                    // 個々のフィールドを読みながら判定すると、切り詰められた入力を
                    // InsufficientBuffer として返してしまう (このメソッドは InvalidData を返す)。
                    // 1 サブサンプルは bytes_of_clear_data (2 バイト) + bytes_of_protected_data (4 バイト)
                    let remaining = payload.len().saturating_sub(offset);
                    if usize::from(subsample_count) > remaining / 6 {
                        return Err(Error::invalid_data(
                            "SencBox subsample table exceeds available payload",
                        ));
                    }
                    for _ in 0..subsample_count {
                        subsamples.push(Subsample {
                            bytes_of_clear_data: u16::decode_at(payload, &mut offset)?,
                            bytes_of_protected_data: u32::decode_at(payload, &mut offset)?,
                        });
                    }
                }
                samples.push(SampleEncryptionInfo { iv, subsamples });
            }
            if offset != payload.len() {
                return Err(Error::invalid_data("SencBox has trailing bytes"));
            }

            Ok((Self { samples }, header.external_size() + payload.len()))
        })
    }

    /// サブサンプル暗号化を示す flags を立てる必要があるかを返す
    ///
    /// サブサンプルを 1 つでも持つサンプルがあれば `true` になる。
    fn uses_subsample_encryption(&self) -> bool {
        self.samples
            .iter()
            .any(|sample| !sample.subsamples.is_empty())
    }

    /// エンコードする IV サイズを決める
    ///
    /// `senc` はサンプルごとの IV サイズを持たないため、全サンプルの `iv` の長さが
    /// 同じでなければならない。0 / 8 / 16 以外は仕様上表現できないため
    /// [`crate::ErrorKind::InvalidInput`] を返す
    fn resolve_iv_size(&self) -> Result<u8> {
        let mut iv_size = None;
        for sample in &self.samples {
            match iv_size {
                None => iv_size = Some(sample.iv.len()),
                Some(current) if current == sample.iv.len() => {}
                Some(_) => {
                    return Err(Error::invalid_input(
                        "SencBox samples must all have the same IV size",
                    ));
                }
            }
        }
        let iv_size = iv_size.unwrap_or(0);
        if !matches!(iv_size, 0 | 8 | 16) {
            return Err(Error::invalid_input(format!(
                "SencBox IV size must be 0, 8, or 16, got {iv_size}"
            )));
        }
        Ok(u8::try_from(iv_size).expect("IV size is one of 0, 8, or 16"))
    }
}

impl Encode for SencBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let subsample_encryption = self.uses_subsample_encryption();
        // IV サイズは書き込みには使わないが、全サンプルで同じ長さかつ
        // 0 / 8 / 16 であることを書き込み前に検査する
        self.resolve_iv_size()?;

        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        let sample_count = u32::try_from(self.samples.len())
            .map_err(|_| Error::invalid_input("SencBox sample count exceeds u32::MAX"))?;
        offset += sample_count.encode(&mut buf[offset..])?;
        for sample in &self.samples {
            // `resolve_iv_size()` で全サンプルの長さが同じであることを検査済み
            offset += sample.iv.encode(&mut buf[offset..])?;
            if subsample_encryption {
                let subsample_count = u16::try_from(sample.subsamples.len()).map_err(|_| {
                    Error::invalid_input("SencBox subsample count exceeds u16::MAX")
                })?;
                offset += subsample_count.encode(&mut buf[offset..])?;
                for subsample in &sample.subsamples {
                    offset += subsample.bytes_of_clear_data.encode(&mut buf[offset..])?;
                    offset += subsample
                        .bytes_of_protected_data
                        .encode(&mut buf[offset..])?;
                }
            }
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for SencBox {
    fn decode(_buf: &[u8]) -> Result<(Self, usize)> {
        // `senc` はサンプルごとの IV サイズを持たないため、単独では解釈できない。
        // `tenc` の `default_per_sample_iv_size` を受け取る `decode_with_iv_size()` を使う
        Err(Error::unsupported(
            "SencBox requires the per-sample IV size from the 'tenc' box; use SencBox::decode_with_iv_size()",
        ))
    }
}

impl BaseBox for SencBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for SencBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        let flags = if self.uses_subsample_encryption() {
            Self::FLAG_SUBSAMPLE_ENCRYPTION
        } else {
            0
        };
        FullBoxFlags::new(flags)
    }
}

/// [ISO/IEC 14496-12] SampleAuxiliaryInformationSizesBox class
///
/// SAI のサイズを保持する。`senc` と組み合わせて SAI の位置と長さを決める。
///
/// # 対応バージョンとフラグ
///
/// version 0 のみ対応する。`aux_info_type` / `aux_info_type_parameter` は
/// flags の `0x000001` が立っている場合だけ存在する。`aux_info_type_parameter` は
/// 0 のみ対応し、0 以外のデコードは [`crate::ErrorKind::InvalidData`]、
/// エンコードは [`crate::ErrorKind::InvalidInput`] を返す。
///
/// `aux_info_type` を省略した場合、ISO/IEC 14496-12:2022 8.7.8 では対応するトラックの
/// 保護スキーム (`schm.scheme_type`) の 4CC を既定値として扱う。
///
/// なお、ここでの扱いは ISO/IEC 14496-12:2022 8.7.8 に基づくものであり、
/// 将来の改訂で変わる可能性がある。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SaizBox {
    /// SAI の種類を表す 4CC
    ///
    /// flags の `0x000001` が立っている場合だけ存在する。
    pub aux_info_type: Option<[u8; 4]>,

    /// [`SaizBox::aux_info_type`] のパラメーター
    ///
    /// 0 のみ対応する。`aux_info_type` を持たない場合は 0 でなければならない。
    pub aux_info_type_parameter: u32,

    /// 既定の SAI 長
    ///
    /// 0 の場合は [`SaizBox::sample_info_sizes`] を使う。
    pub default_sample_info_size: u8,

    /// サンプル数
    pub sample_count: u32,

    /// サンプルごとの SAI 長
    ///
    /// [`SaizBox::default_sample_info_size`] が 0 の場合だけ使う。
    pub sample_info_sizes: Vec<u8>,
}

impl SaizBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"saiz");

    /// `aux_info_type` と `aux_info_type_parameter` の存在を示す flags のビット
    pub const FLAG_AUX_INFO_TYPE_PRESENT: u32 = 0x000001;
}

impl Encode for SaizBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        if self.aux_info_type_parameter != 0 {
            return Err(Error::invalid_input(
                "SaizBox aux_info_type_parameter must be 0",
            ));
        }
        if self.default_sample_info_size == 0 {
            let sample_count = usize::try_from(self.sample_count)
                .map_err(|_| Error::invalid_input("SaizBox sample count exceeds usize::MAX"))?;
            if sample_count != self.sample_info_sizes.len() {
                return Err(Error::invalid_input(
                    "SaizBox sample_info_sizes length must equal sample_count when default_sample_info_size is 0",
                ));
            }
        } else if !self.sample_info_sizes.is_empty() {
            return Err(Error::invalid_input(
                "SaizBox sample_info_sizes must be empty when default_sample_info_size is non-zero",
            ));
        }

        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        if let Some(aux_info_type) = self.aux_info_type {
            offset += aux_info_type.encode(&mut buf[offset..])?;
            offset += self.aux_info_type_parameter.encode(&mut buf[offset..])?;
        }
        offset += self.default_sample_info_size.encode(&mut buf[offset..])?;
        offset += self.sample_count.encode(&mut buf[offset..])?;
        offset += self.sample_info_sizes.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for SaizBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            if full_header.version != 0 {
                return Err(Error::invalid_data(format!(
                    "Unsupported SaizBox version: {}",
                    full_header.version
                )));
            }
            if full_header.flags.get() & !Self::FLAG_AUX_INFO_TYPE_PRESENT != 0 {
                return Err(Error::invalid_data(format!(
                    "Unsupported SaizBox flags: {:#x}",
                    full_header.flags.get()
                )));
            }
            let (aux_info_type, aux_info_type_parameter) =
                if full_header.flags.get() & Self::FLAG_AUX_INFO_TYPE_PRESENT != 0 {
                    (
                        Some(<[u8; 4]>::decode_at(payload, &mut offset)?),
                        u32::decode_at(payload, &mut offset)?,
                    )
                } else {
                    (None, 0)
                };
            if aux_info_type_parameter != 0 {
                return Err(Error::invalid_data(
                    "SaizBox aux_info_type_parameter must be 0",
                ));
            }

            let default_sample_info_size = u8::decode_at(payload, &mut offset)?;
            let sample_count = u32::decode_at(payload, &mut offset)?;
            let sample_info_sizes = if default_sample_info_size == 0 {
                let mut sizes = Vec::new();
                for _ in 0..sample_count {
                    sizes.push(u8::decode_at(payload, &mut offset)?);
                }
                sizes
            } else {
                Vec::new()
            };
            if offset != payload.len() {
                return Err(Error::invalid_data("SaizBox has trailing bytes"));
            }

            Ok((
                Self {
                    aux_info_type,
                    aux_info_type_parameter,
                    default_sample_info_size,
                    sample_count,
                    sample_info_sizes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for SaizBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for SaizBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        let flags = if self.aux_info_type.is_some() {
            Self::FLAG_AUX_INFO_TYPE_PRESENT
        } else {
            0
        };
        FullBoxFlags::new(flags)
    }
}

/// [ISO/IEC 14496-12] SampleAuxiliaryInformationOffsetsBox class
///
/// SAI の位置を保持する。fragment 内では、オフセットの基準は `tfhd` の
/// base data offset と同じ基準になる (ISO/IEC 14496-12:2022 8.8.14)。
///
/// # 対応バージョンとフラグ
///
/// version 0 (32 bit offset) と version 1 (64 bit offset) に対応する。
/// `aux_info_type` / `aux_info_type_parameter` は flags の `0x000001` が
/// 立っている場合だけ存在する。`aux_info_type_parameter` は 0 のみ対応し、
/// 0 以外のデコードは [`crate::ErrorKind::InvalidData`]、エンコードは
/// [`crate::ErrorKind::InvalidInput`] を返す。
///
/// なお、ここでの扱いは ISO/IEC 14496-12:2022 8.7.9 / 8.8.14 に基づくものであり、
/// 将来の改訂で変わる可能性がある。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SaioBox {
    /// FullBox バージョン (0: 32 bit offset、1: 64 bit offset)
    pub version: u8,

    /// SAI の種類を表す 4CC
    ///
    /// flags の `0x000001` が立っている場合だけ存在する。
    pub aux_info_type: Option<[u8; 4]>,

    /// [`SaioBox::aux_info_type`] のパラメーター
    ///
    /// 0 のみ対応する。`aux_info_type` を持たない場合は 0 でなければならない。
    pub aux_info_type_parameter: u32,

    /// SAI の位置 (バージョンに応じて 32 bit / 64 bit)
    pub offsets: Vec<u64>,
}

impl SaioBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"saio");

    /// `aux_info_type` と `aux_info_type_parameter` の存在を示す flags のビット
    pub const FLAG_AUX_INFO_TYPE_PRESENT: u32 = 0x000001;
}

impl Encode for SaioBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        if self.version > 1 {
            return Err(Error::invalid_input(format!(
                "SaioBox version must be 0 or 1, got {}",
                self.version
            )));
        }
        if self.aux_info_type_parameter != 0 {
            return Err(Error::invalid_input(
                "SaioBox aux_info_type_parameter must be 0",
            ));
        }

        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        if let Some(aux_info_type) = self.aux_info_type {
            offset += aux_info_type.encode(&mut buf[offset..])?;
            offset += self.aux_info_type_parameter.encode(&mut buf[offset..])?;
        }
        let entry_count = u32::try_from(self.offsets.len())
            .map_err(|_| Error::invalid_input("SaioBox offset count exceeds u32::MAX"))?;
        offset += entry_count.encode(&mut buf[offset..])?;
        if self.version == 0 {
            for value in &self.offsets {
                let value = u32::try_from(*value).map_err(|_| {
                    Error::invalid_input("SaioBox version 0 offset exceeds u32::MAX; use version 1")
                })?;
                offset += value.encode(&mut buf[offset..])?;
            }
        } else {
            for value in &self.offsets {
                offset += value.encode(&mut buf[offset..])?;
            }
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for SaioBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            if full_header.version > 1 {
                return Err(Error::invalid_data(format!(
                    "Unsupported SaioBox version: {}",
                    full_header.version
                )));
            }
            if full_header.flags.get() & !Self::FLAG_AUX_INFO_TYPE_PRESENT != 0 {
                return Err(Error::invalid_data(format!(
                    "Unsupported SaioBox flags: {:#x}",
                    full_header.flags.get()
                )));
            }
            let (aux_info_type, aux_info_type_parameter) =
                if full_header.flags.get() & Self::FLAG_AUX_INFO_TYPE_PRESENT != 0 {
                    (
                        Some(<[u8; 4]>::decode_at(payload, &mut offset)?),
                        u32::decode_at(payload, &mut offset)?,
                    )
                } else {
                    (None, 0)
                };
            if aux_info_type_parameter != 0 {
                return Err(Error::invalid_data(
                    "SaioBox aux_info_type_parameter must be 0",
                ));
            }

            let entry_count = u32::decode_at(payload, &mut offset)?;
            let mut offsets = Vec::new();
            for _ in 0..entry_count {
                if full_header.version == 0 {
                    offsets.push(u64::from(u32::decode_at(payload, &mut offset)?));
                } else {
                    offsets.push(u64::decode_at(payload, &mut offset)?);
                }
            }
            if offset != payload.len() {
                return Err(Error::invalid_data("SaioBox has trailing bytes"));
            }

            Ok((
                Self {
                    version: full_header.version,
                    aux_info_type,
                    aux_info_type_parameter,
                    offsets,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for SaioBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for SaioBox {
    fn full_box_version(&self) -> u8 {
        self.version
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        let flags = if self.aux_info_type.is_some() {
            Self::FLAG_AUX_INFO_TYPE_PRESENT
        } else {
            0
        };
        FullBoxFlags::new(flags)
    }
}

/// [ISO/IEC 14496-12] SchemeInformationBox class
///
/// 保護スキーム固有の情報を保持するコンテナボックス。
/// CENC では `tenc` を子に持つ。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SchiBox {
    /// トラック暗号化の既定値を保持する `tenc` ボックス
    pub tenc_box: Option<TencBox>,

    /// 上記のいずれにも該当しなかった子ボックス群（未知の box_type を含む）
    pub unknown_boxes: Vec<UnknownBox>,
}

impl SchiBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"schi");
}

impl Encode for SchiBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        if let Some(b) = &self.tenc_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for SchiBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut tenc_box = None;
            let mut unknown_boxes = Vec::new();
            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    TencBox::TYPE if tenc_box.is_none() => {
                        tenc_box = Some(TencBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    tenc_box,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for SchiBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(self.tenc_box.iter().map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-12] ProtectionSchemeInfoBox class
///
/// 保護される前の形式と保護スキームの情報を保持するコンテナボックス。
///
/// # エラー条件
///
/// `schm` と `schi` は両方存在するか両方存在しないかでなければならない。
/// CENC 系 scheme では `schi.tenc` が必須で、`cens` / `cbcs` は version 1 のパターン暗号化
/// パラメーター、`cenc` / `cbc1` は version 0 でなければならない。
/// これらに違反する場合、エンコードは [`crate::ErrorKind::InvalidInput`]、
/// デコードは [`crate::ErrorKind::InvalidData`] を返す。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SinfBox {
    /// 保護される前のサンプルエントリーの 4CC を保持する `frma`
    pub frma_box: FrmaBox,

    /// 保護スキームの種類を保持する `schm`
    ///
    /// `schi_box` と同時に存在するか、同時に存在しないかのどちらかでなければならない
    /// (ISO/IEC 14496-12 8.12.1.1 c)。
    pub schm_box: Option<SchmBox>,

    /// 保護スキーム固有の情報を保持する `schi`
    pub schi_box: Option<SchiBox>,

    /// 上記のいずれにも該当しなかった子ボックス群（未知の box_type を含む）
    pub unknown_boxes: Vec<UnknownBox>,
}

impl SinfBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"sinf");

    /// 保護スキームと `schm` / `schi` / `tenc` の整合性を検査する
    fn check_scheme_consistency(&self) -> Result<()> {
        if self.schm_box.is_some() != self.schi_box.is_some() {
            return Err(Error::invalid_input(
                "SinfBox schm and schi must both be present or both be absent",
            ));
        }
        if let Some(schm) = &self.schm_box
            && is_cenc_scheme(schm.scheme_type)
        {
            let schi = self
                .schi_box
                .as_ref()
                .ok_or_else(|| Error::invalid_input("SinfBox CENC scheme requires a schi box"))?;
            let tenc = schi
                .tenc_box
                .as_ref()
                .ok_or_else(|| Error::invalid_input("SinfBox CENC scheme requires a tenc box"))?;
            check_tenc_for_scheme(schm.scheme_type, tenc)?;
        }
        Ok(())
    }

    /// CENC の保護サンプルエントリー用の情報が揃っているかを検査する
    ///
    /// `schm`、`schi`、`tenc` を必須とし、保護スキームが CENC 系であることを要求する。
    pub(crate) fn check_cenc_sample_entry(&self) -> Result<()> {
        let schm = self
            .schm_box
            .as_ref()
            .ok_or_else(|| Error::invalid_input("CENC sample entry requires a schm box"))?;
        if !is_cenc_scheme(schm.scheme_type) {
            return Err(Error::unsupported(format!(
                "CENC sample entry does not support scheme '{}'",
                BoxType::Normal(schm.scheme_type)
            )));
        }
        let schi = self
            .schi_box
            .as_ref()
            .ok_or_else(|| Error::invalid_input("CENC sample entry requires a schi box"))?;
        let tenc = schi
            .tenc_box
            .as_ref()
            .ok_or_else(|| Error::invalid_input("CENC sample entry requires a tenc box"))?;
        check_tenc_for_scheme(schm.scheme_type, tenc)
    }
}

impl Encode for SinfBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        self.check_scheme_consistency()?;

        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.frma_box.encode(&mut buf[offset..])?;
        if let Some(b) = &self.schm_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        if let Some(b) = &self.schi_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for SinfBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut frma_box = None;
            let mut schm_box = None;
            let mut schi_box = None;
            let mut unknown_boxes = Vec::new();
            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    FrmaBox::TYPE if frma_box.is_none() => {
                        frma_box = Some(FrmaBox::decode_at(payload, &mut offset)?);
                    }
                    SchmBox::TYPE if schm_box.is_none() => {
                        schm_box = Some(SchmBox::decode_at(payload, &mut offset)?);
                    }
                    SchiBox::TYPE if schi_box.is_none() => {
                        schi_box = Some(SchiBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            let sinf_box = Self {
                frma_box: check_mandatory_box(frma_box, "frma", "sinf")?,
                schm_box,
                schi_box,
                unknown_boxes,
            };
            if sinf_box.schm_box.is_some() != sinf_box.schi_box.is_some() {
                return Err(Error::invalid_data(
                    "SinfBox schm and schi must both be present or both be absent",
                ));
            }
            if let Some(schm) = &sinf_box.schm_box
                && is_cenc_scheme(schm.scheme_type)
            {
                let schi = sinf_box.schi_box.as_ref().ok_or_else(|| {
                    Error::invalid_data("SinfBox CENC scheme requires a schi box")
                })?;
                let tenc = schi.tenc_box.as_ref().ok_or_else(|| {
                    Error::invalid_data("SinfBox CENC scheme requires a tenc box")
                })?;
                if !tenc_pattern_matches_scheme(schm.scheme_type, tenc) {
                    return Err(Error::invalid_data(format!(
                        "tenc pattern encryption parameters do not match scheme '{}'",
                        BoxType::Normal(schm.scheme_type)
                    )));
                }
            }

            Ok((sinf_box, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for SinfBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.frma_box).map(as_box_object))
                .chain(self.schm_box.iter().map(as_box_object))
                .chain(self.schi_box.iter().map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}
