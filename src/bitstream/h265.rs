//! H.265 ビットストリーム処理ユーティリティ
//!
//! Annex B / length-prefixed の NAL ユニット列の解析、SPS の解析、
//! VPS / SPS / PPS の抽出、`hev1` / `hvc1` / `hvcC` の構築を提供する。
//!
//! 参照仕様は以下のとおり。
//!
//! - ITU-T H.265 (V11) (ISO/IEC 23008-2 と技術的に整合): NAL ユニット
//!   (7.3.1 / 7.4.2)、SPS (7.3.2.2.1 / 7.4.3.2.1)、`profile_tier_level`
//!   (7.3.3)、Annex B の開始コード
//! - ISO/IEC 14496-15:2022: `HEVCDecoderConfigurationRecord` (`hvcC`) の
//!   `lengthSizeMinusOne` (0 / 1 / 3 が正当、2 は reserved)、`'hvc1'` / `'hev1'`
//!   の `array_completeness` の扱い
//!
//! # NAL バイト列の契約
//!
//! このモジュールが返す NAL バイト列は、2 バイトの NAL ヘッダーを含み、
//! 開始コードと長さプレフィックスを含まない EBSP (emulation prevention byte を
//! 残したまま) である。RBSP 化は [`parse_sps`] の内部だけで行う。
//!
//! # 長さフィールド幅の契約
//!
//! length-prefixed 形式を扱う API の長さフィールド幅は [`LengthSize`] 型で
//! 表現する。幅 3 (ISO/IEC 14496-15:2022 の `lengthSizeMinusOne == 2`、reserved)
//! はこの型で表現できない。

pub use crate::bitstream::nal::LengthSize;

use alloc::{vec, vec::Vec};

use crate::{
    Error, Result, Uint,
    bitstream::nal,
    boxes::{Hev1Box, Hvc1Box, HvccBox, HvccNalUintArray, VisualSampleEntryFields},
};

/// NAL ユニット種別 (ITU-T H.265 Table 7-1 の `nal_unit_type`)
///
/// この列挙は Table 7-1 の主要種別だけを名前付きで持ち、予約・未指定や
/// 実ストリームで使われる定義値以外の種別は [`Other`](H265NalUnitType::Other)
/// として不透明に通す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum H265NalUnitType {
    /// VPS (Video parameter set) (32)
    Vps,
    /// SPS (Sequence parameter set) (33)
    Sps,
    /// PPS (Picture parameter set) (34)
    Pps,
    /// アクセスユニット区切り (Access unit delimiter) (35)
    Aud,
    /// prefix SEI (Supplemental enhancement information) (39)
    PrefixSei,
    /// suffix SEI (Supplemental enhancement information) (40)
    SuffixSei,
    /// Table 7-1 の定義値以外 (0..=31、36..=38、41..=63 など)。
    /// 実ストリームで使われる範囲外の値もここで不透明に通す
    Other(u8),
}

impl H265NalUnitType {
    /// ヘッダーの `nal_unit_type` の値 (6 ビット) を [`H265NalUnitType`] に写す
    fn from_header_value(value: u8) -> Self {
        match value {
            32 => Self::Vps,
            33 => Self::Sps,
            34 => Self::Pps,
            35 => Self::Aud,
            39 => Self::PrefixSei,
            40 => Self::SuffixSei,
            other => Self::Other(other),
        }
    }
}

/// [`HvccBox`] の各 NAL 配列に格納できる最大個数
///
/// ISO/IEC 14496-15:2022 8.3.2.1.3 の `numNalus` が `unsigned int(16)` のため
const MAX_NALUS_PER_ARRAY: usize = u16::MAX as usize;

/// [`HvccBox`] の各 NAL 配列の `nal_unit_type` (ITU-T H.265 Table 7-1)
///
/// 本モジュールの構築 API は VPS / SPS / PPS の 3 配列だけを載せる
const NAL_UNIT_TYPE_VPS: u8 = 32;
const NAL_UNIT_TYPE_SPS: u8 = 33;
const NAL_UNIT_TYPE_PPS: u8 = 34;

/// H.265 の 1 個の NAL ユニット
///
/// [`parse_annexb_nal_units`] / [`parse_length_prefixed_nal_units`] が返す要素で、
/// 入力バイト列を借用する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct H265NalUnit<'a> {
    /// `nal_unit_type` (NAL ヘッダーの 6 ビット、ITU-T H.265 Table 7-1)
    pub nal_unit_type: H265NalUnitType,

    /// `nuh_layer_id` (0..=62)
    ///
    /// 63 は ITU-T H.265 7.4.2.2 で将来拡張用に予約されており、拒否する
    pub nuh_layer_id: u8,

    /// `nuh_temporal_id_plus1` (1..=7)
    ///
    /// `TemporalId = nuh_temporal_id_plus1 - 1`
    pub nuh_temporal_id_plus1: u8,

    /// NAL 本体
    ///
    /// 2 バイトの NAL ヘッダーを含み、開始コードと長さプレフィックスを含まない
    /// EBSP (emulation prevention byte を残したまま)
    pub data: &'a [u8],
}

/// H.265 の NAL ヘッダー (ITU-T H.265 7.3.1.2 / 7.4.2.2) を検証して
/// `nal_unit_type` を返す
///
/// - ヘッダー 2 バイトに満たない NAL は [`crate::Error`]
/// - `forbidden_zero_bit` が 1 なら [`crate::Error`]
/// - `nuh_temporal_id_plus1` が 0 なら [`crate::Error`]
///   (shall not be equal to 0。`TemporalId = nuh_temporal_id_plus1 - 1`)
/// - `nuh_layer_id` が 63 なら [`crate::Error`] (0..=62 だけを受理し、
///   63 は将来拡張用)
/// - 予約・未指定の `nal_unit_type` はエラーにせずそのまま返す
///   (フレーミングでは不透明な NAL として通す)
fn validate_h265_nal_header(data: &[u8]) -> Result<H265NalUnitType> {
    if data.len() < 2 {
        return Err(Error::invalid_input("NAL unit is shorter than 2 bytes"));
    }
    if data[0] & 0b1000_0000 != 0 {
        return Err(Error::invalid_input("forbidden_zero_bit must be 0"));
    }
    if data[1] & 0b0000_0111 == 0 {
        return Err(Error::invalid_input("nuh_temporal_id_plus1 must not be 0"));
    }
    if nuh_layer_id(data) == 63 {
        return Err(Error::invalid_input("nuh_layer_id must be 0..=62"));
    }
    Ok(H265NalUnitType::from_header_value(nal_unit_type(data)))
}

/// 2 バイト NAL ヘッダーの `nal_unit_type` (6 ビット) を返す
///
/// 16 ビットの NAL ヘッダー (ITU-T H.265 7.3.1.2) では `nal_unit_type` は
/// 上位バイトの bit6..bit1 に位置する
fn nal_unit_type(data: &[u8]) -> u8 {
    (data[0] >> 1) & 0b0011_1111
}

/// 2 バイト NAL ヘッダーの `nuh_layer_id` (6 ビット) を返す
///
/// 上位バイトの bit0 と下位バイトの bit7..bit3 に跨る
fn nuh_layer_id(data: &[u8]) -> u8 {
    ((data[0] & 0x01) << 5) | (data[1] >> 3)
}

/// 2 バイト NAL ヘッダーの `nuh_temporal_id_plus1` (3 ビット) を返す
fn nuh_temporal_id_plus1(data: &[u8]) -> u8 {
    data[1] & 0b0000_0111
}

/// 走査済みの NAL 本体から [`H265NalUnit`] を作る
///
/// H.265 のヘッダー検証を適用する (NAL 本体が 2 バイト未満、
/// `forbidden_zero_bit == 1`、`nuh_temporal_id_plus1 == 0`、
/// `nuh_layer_id == 63` の場合は [`crate::Error`])
fn to_h265_nal_unit(body: &[u8]) -> Result<H265NalUnit<'_>> {
    let nal_unit_type = validate_h265_nal_header(body)?;
    Ok(H265NalUnit {
        nal_unit_type,
        nuh_layer_id: nuh_layer_id(body),
        nuh_temporal_id_plus1: nuh_temporal_id_plus1(body),
        data: body,
    })
}

/// Annex B (ITU-T H.265 Annex B) の NAL ユニット列を解析する
///
/// # 入力
///
/// - `input`: 3 バイト (`0x000001`) / 4 バイト (`0x00000001`) の開始コードで
///   区切られた NAL ユニット列
///
/// # 返り値
///
/// NAL ユニットの列。各 [`H265NalUnit::data`] は 2 バイトの NAL ヘッダーを
/// 含み、開始コードを含まない EBSP である。
///
/// # エラー条件
///
/// - 非空入力に開始コードが 1 つも無い
/// - 最初の開始コードより前に非ゼロバイトがある
/// - 開始コードの直後に次の開始コードまたは入力終端が来る空 NAL
/// - NAL ヘッダー 2 バイトに満たない NAL
/// - `forbidden_zero_bit == 1` の NAL
/// - `nuh_temporal_id_plus1 == 0` の NAL
/// - `nuh_layer_id == 63` の NAL
///
/// 空入力は NAL ユニット 0 個の成功として扱う (開始コード欠落とは区別する)。
/// 先頭の開始コードより前の `leading_zero_8bits`、NAL 間のゼロ詰め、および
/// 最後の NAL より後の `trailing_zero_8bits` は境界の詰め物として NAL 本体に
/// 含めない。
pub fn parse_annexb_nal_units(input: &[u8]) -> Result<Vec<H265NalUnit<'_>>> {
    let bodies = nal::scan_annexb_nals(input)?;
    bodies.iter().map(|body| to_h265_nal_unit(body)).collect()
}

/// length-prefixed 形式 (ISO/IEC 14496-15:2022) の NAL ユニット列を解析する
///
/// # 入力
///
/// - `input`: 大端序の長さフィールド + NAL 本体を繰り返したバイト列
/// - `length_size`: 長さフィールド幅
///
/// # 返り値
///
/// NAL ユニットの列。各 [`H265NalUnit::data`] は 2 バイトの NAL ヘッダーを
/// 含み、長さプレフィックスを含まない EBSP である。
///
/// # エラー条件
///
/// - 長さフィールドが入力末尾を超える
/// - 宣言長が残バイトを超える (切り詰め)
/// - 宣言長が 0 の NAL
/// - NAL ヘッダー 2 バイトに満たない NAL
/// - `forbidden_zero_bit == 1` の NAL
/// - `nuh_temporal_id_plus1 == 0` の NAL
/// - `nuh_layer_id == 63` の NAL
///
/// 空入力は NAL ユニット 0 個の成功として扱う。
pub fn parse_length_prefixed_nal_units(
    input: &[u8],
    length_size: LengthSize,
) -> Result<Vec<H265NalUnit<'_>>> {
    let bodies = nal::scan_length_prefixed_nals(input, length_size)?;
    bodies.iter().map(|body| to_h265_nal_unit(body)).collect()
}

/// Annex B の NAL ユニット列を length-prefixed 形式へ変換する
///
/// # 入力
///
/// - `input`: 開始コードで区切られた NAL ユニット列
/// - `length_size`: 出力の長さフィールド幅
///
/// # 返り値
///
/// 各 NAL 本体 (開始コード除く) の前に大端序の長さフィールドを付けたバイト列。
/// NAL 本体はそのまま (EBSP) で、ヘッダー検証は行わない。
///
/// # エラー条件
///
/// - Annex B の境界走査エラー ([`parse_annexb_nal_units`] の走査分。
///   NAL ヘッダーは検証せず、`forbidden_zero_bit == 1` でも変換は成功する)
/// - NAL 本体が `length_size` バイトの長さフィールドに収まらない
///   (黙った切り詰めはしない)
pub fn annexb_to_length_prefixed(input: &[u8], length_size: LengthSize) -> Result<Vec<u8>> {
    nal::annexb_to_length_prefixed(input, length_size)
}

/// length-prefixed 形式の NAL ユニット列を Annex B へ変換する
///
/// # 入力
///
/// - `input`: 大端序の長さフィールド + NAL 本体を繰り返したバイト列
/// - `length_size`: 長さフィールド幅
///
/// # 返り値
///
/// 各 NAL 本体の前に 4 バイト開始コード (`0x00000001`) を付けたバイト列。
/// NAL 本体はそのまま (EBSP) で、ヘッダー検証は行わない。
///
/// # エラー条件
///
/// - length-prefixed の境界走査エラー ([`parse_length_prefixed_nal_units`] の
///   走査分。NAL ヘッダーは検証せず、`forbidden_zero_bit == 1` でも変換は成功する)
///
/// NAL type に応じた `zero_byte` の shall (VPS / SPS / PPS、AU 先頭) は
/// アクセスユニット検出が対象外のため実装しない。
pub fn length_prefixed_to_annexb(input: &[u8], length_size: LengthSize) -> Result<Vec<u8>> {
    nal::length_prefixed_to_annexb(input, length_size)
}

/// NAL ユニット列から指定した [`H265NalUnitType`] の NAL 本体を集める
///
/// 入力順を保ったまま、一致した NAL の [`H265NalUnit::data`] だけを返す。
/// エラーにはならない (一致が無ければ空 `Vec`)。
pub fn collect_nal_units<'a, I>(nals: I, nal_unit_type: H265NalUnitType) -> Vec<&'a [u8]>
where
    I: IntoIterator<Item = H265NalUnit<'a>>,
{
    nals.into_iter()
        .filter(|nal| nal.nal_unit_type == nal_unit_type)
        .map(|nal| nal.data)
        .collect()
}

/// SPS (Sequence Parameter Set) の解析結果
///
/// ITU-T H.265 7.4.3.2.1 の `seq_parameter_set_data` から導出した値。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct H265Sps {
    /// `general_profile_space` (2 ビット)
    pub general_profile_space: u8,

    /// `general_tier_flag`
    pub general_tier_flag: u8,

    /// `general_profile_idc` (5 ビット)
    ///
    /// 5 ビット値のまま受理する (プロファイル許可リストで制限しない)
    pub general_profile_idc: u8,

    /// `general_profile_compatibility_flags` (32 ビット)
    pub general_profile_compatibility_flags: u32,

    /// `general_constraint_indicator_flags` (下位 48 ビット)
    pub general_constraint_indicator_flags: u64,

    /// `general_level_idc`
    pub general_level_idc: u8,

    /// `sps_max_sub_layers_minus1` (0..=6)
    pub sps_max_sub_layers_minus1: u8,

    /// `sps_temporal_id_nesting_flag`
    pub sps_temporal_id_nesting_flag: u8,

    /// `chroma_format_idc` (0..=3)
    pub chroma_format_idc: u8,

    /// `bit_depth_luma_minus8` (0..=7)
    ///
    /// ISO/IEC 14496-15:2022 8.3.2.1.2 の `bitDepthLumaMinus8` が
    /// `unsigned int(3)` のため、仕様上あり得る 8 (16-bit) は拒否する
    pub bit_depth_luma_minus8: u8,

    /// `bit_depth_chroma_minus8` (0..=7)
    ///
    /// ISO/IEC 14496-15:2022 8.3.2.1.2 の `bitDepthChromaMinus8` が
    /// `unsigned int(3)` のため、仕様上あり得る 8 (16-bit) は拒否する
    pub bit_depth_chroma_minus8: u8,

    /// クロップ適用後の幅 (ピクセル)
    ///
    /// `VisualSampleEntryFields::width` に写せるよう `u16` に収まらない値は
    /// [`parse_sps`] が拒否する
    pub width: u16,

    /// クロップ適用後の高さ (ピクセル)
    ///
    /// `VisualSampleEntryFields::height` に写せるよう `u16` に収まらない値は
    /// [`parse_sps`] が拒否する
    pub height: u16,
}

/// NAL ヘッダー付き EBSP の SPS を解析する
///
/// # 入力
///
/// - `nal_unit`: NAL ヘッダー 2 バイト + EBSP の SPS
///
/// 内部でヘッダーを検証し、残バイトから emulation prevention byte を除いて
/// RBSP を得て、ITU-T H.265 7.3.2.2.1 / 7.3.3 / 7.4.3.2.1 の `u(n)` と
/// Exp-Golomb (`ue(v)`) で読む。VPS 構文と PPS 構文と VUI は読まない。
///
/// # エラー条件
///
/// - NAL ヘッダー 2 バイトに満たない NAL
/// - NAL ヘッダーの `forbidden_zero_bit == 1`、`nuh_temporal_id_plus1 == 0`、
///   `nuh_layer_id == 63`
/// - `nal_unit_type` が 33 (SPS) 以外
/// - SPS の `TemporalId` が 0 以外 (ITU-T H.265 7.4.2.2)
/// - `sps_max_sub_layers_minus1 > 6`、`chroma_format_idc > 3`、
///   `bit_depth_luma_minus8 > 7`、`bit_depth_chroma_minus8 > 7`
///   (7.4.3.2.1 の値域。bit depth は hvcC の 3 ビット欄に載せられる 0..=7 に限る)
/// - `sps_max_sub_layers_minus1 == 0` のとき `sps_temporal_id_nesting_flag == 0`
///   (7.4.3.2.1 は 1 と定める)
/// - 寸法の導出に必要な構文 (bit depth まで) が途中で終わる SPS、
///   Exp-Golomb の途中終端。`log2_max_pic_order_cnt_lsb_minus4` 以降の欠落は
///   成功とする
/// - クロップが符号化サイズ以上 (7.4.3.2.1 は未満を要求)
/// - クロップ後の幅または高さが 0
/// - クロップ後の幅または高さが `u16::MAX` を超える
///
/// # 対象外
///
/// VUI と `log2_max_pic_order_cnt_lsb_minus4` 以降は読まない。sub-layer の
/// profile / level はビット位置を進めるためだけに読み飛ばし、公開結果には
/// 載せない。
pub fn parse_sps(nal_unit: &[u8]) -> Result<H265Sps> {
    let nal_unit_type = validate_h265_nal_header(nal_unit)?;
    if nal_unit_type != H265NalUnitType::Sps {
        return Err(Error::invalid_input("SPS NAL unit type must be 33"));
    }
    // VPS / SPS の TemporalId は 0 でなければならない (ITU-T H.265 7.4.2.2)
    if nuh_temporal_id_plus1(nal_unit) != 1 {
        return Err(Error::invalid_input("SPS TemporalId must be 0"));
    }

    // ヘッダー 2 バイトのあと、i = 2 から 0x000003 を捨てる
    // (ITU-T H.265 7.3.1.1 の nal_unit() 構文ループと 7.4.2.1 の規定)
    let rbsp = remove_emulation_prevention_bytes(&nal_unit[2..]);
    let mut reader = SpsBitReader::new(&rbsp);

    // sps_video_parameter_set_id (u(4)) は読み飛ばす
    let _sps_video_parameter_set_id = reader.read_bits(4)?;
    let sps_max_sub_layers_minus1 = reader.read_bits(3)? as u8;
    if sps_max_sub_layers_minus1 > 6 {
        return Err(Error::invalid_input(
            "sps_max_sub_layers_minus1 must be 0..=6",
        ));
    }
    let sps_temporal_id_nesting_flag = reader.read_bit()?;
    // 7.4.3.2.1 は sps_max_sub_layers_minus1 == 0 のとき sps_temporal_id_nesting_flag
    // を 1 と定める
    if sps_max_sub_layers_minus1 == 0 && sps_temporal_id_nesting_flag == 0 {
        return Err(Error::invalid_input(
            "sps_temporal_id_nesting_flag must be 1 when sps_max_sub_layers_minus1 is 0",
        ));
    }

    // profile_tier_level(1, sps_max_sub_layers_minus1) (ITU-T H.265 7.3.3)
    let general_profile_space = reader.read_bits(2)? as u8;
    let general_tier_flag = reader.read_bit()?;
    let general_profile_idc = reader.read_bits(5)? as u8;
    let general_profile_compatibility_flags = reader.read_bits(32)? as u32;
    let general_constraint_indicator_flags = reader.read_bits(48)?;
    let general_level_idc = reader.read_bits(8)? as u8;

    // sub-layer の present flag はビット位置を進めるためだけに読む
    let mut sub_layer_profile_present_flag = [0u8; 7];
    let mut sub_layer_level_present_flag = [0u8; 7];
    for i in 0..sps_max_sub_layers_minus1 as usize {
        sub_layer_profile_present_flag[i] = reader.read_bit()?;
        sub_layer_level_present_flag[i] = reader.read_bit()?;
    }
    // sps_max_sub_layers_minus1 > 0 のとき reserved_zero_2bits (7.3.3)
    if sps_max_sub_layers_minus1 > 0 {
        for _ in sps_max_sub_layers_minus1..8 {
            reader.skip_bits(2)?;
        }
    }
    for i in 0..sps_max_sub_layers_minus1 as usize {
        // sub-layer profile (2 + 1 + 5 + 32 + 4 + 44 = 88 bits) は読み飛ばす
        if sub_layer_profile_present_flag[i] != 0 {
            reader.skip_bits(88)?;
        }
        if sub_layer_level_present_flag[i] != 0 {
            let _sub_layer_level_idc = reader.read_bits(8)?;
        }
    }

    // sps_seq_parameter_set_id (ue(v)) は読み飛ばす
    let _sps_seq_parameter_set_id = reader.read_ue()?;
    let chroma_format_idc = reader.read_ue()?;
    if chroma_format_idc > 3 {
        return Err(Error::invalid_input("chroma_format_idc must be 0..=3"));
    }
    // chroma_format_idc == 3 のときだけ separate_colour_plane_flag が存在する。
    // 不在時は 0
    let separate_colour_plane_flag = if chroma_format_idc == 3 {
        reader.read_bit()?
    } else {
        0
    };
    let pic_width_in_luma_samples = reader.read_ue()?;
    let pic_height_in_luma_samples = reader.read_ue()?;
    let conformance_window_flag = reader.read_bit()?;
    let (conf_win_left_offset, conf_win_right_offset, conf_win_top_offset, conf_win_bottom_offset) =
        if conformance_window_flag != 0 {
            (
                reader.read_ue()?,
                reader.read_ue()?,
                reader.read_ue()?,
                reader.read_ue()?,
            )
        } else {
            (0, 0, 0, 0)
        };
    let bit_depth_luma_minus8 = reader.read_ue()?;
    // 7.4.3.2.1 は 0..=8 だが、ISO/IEC 14496-15:2022 8.3.2.1.2 の
    // bitDepthLumaMinus8 は unsigned int(3) (0..=7) であり hvcC に載せられない。
    // Uint::new は範囲を検証しないため、8 を渡すと encode が下位 3 ビット 0
    // (8-bit) として黙って書き出す。したがって 0..=7 以外は Error にする
    if bit_depth_luma_minus8 > 7 {
        return Err(Error::invalid_input("bit_depth_luma_minus8 must be 0..=7"));
    }
    let bit_depth_chroma_minus8 = reader.read_ue()?;
    if bit_depth_chroma_minus8 > 7 {
        return Err(Error::invalid_input(
            "bit_depth_chroma_minus8 must be 0..=7",
        ));
    }

    // ChromaArrayType は separate_colour_plane_flag == 1 なら 0、さもなくば
    // chroma_format_idc (7.4.3.2.1)
    let _chroma_array_type = if separate_colour_plane_flag == 1 {
        0
    } else {
        chroma_format_idc
    };

    // SubWidthC / SubHeightC は ITU-T H.265 Table 6-1 による
    // (chroma_format_idc 0: 1/1、1: 2/2、2: 2/1、3: 1/1。
    //  separate_colour_plane_flag == 1 の行も 1/1)
    let (sub_width_c, sub_height_c) = match chroma_format_idc {
        0 => (1u64, 1u64),
        1 => (2, 2),
        2 => (2, 1),
        3 => (1, 1),
        _ => unreachable!("chroma_format_idc > 3 は上で拒否済み"),
    };

    // クロップ適用。7.4.3.2.1 は SubWidthC * (left + right) が
    // pic_width_in_luma_samples 未満、高さ側も同様であることを要求する。
    // 食いつぶす (以上) 場合は飽和せず Error にする
    let cropped_width = if conformance_window_flag != 0 {
        let crop =
            sub_width_c * (u64::from(conf_win_left_offset) + u64::from(conf_win_right_offset));
        if crop >= u64::from(pic_width_in_luma_samples) {
            return Err(Error::invalid_input(
                "conformance window exceeds the coded width",
            ));
        }
        u64::from(pic_width_in_luma_samples) - crop
    } else {
        u64::from(pic_width_in_luma_samples)
    };
    let cropped_height = if conformance_window_flag != 0 {
        let crop =
            sub_height_c * (u64::from(conf_win_top_offset) + u64::from(conf_win_bottom_offset));
        if crop >= u64::from(pic_height_in_luma_samples) {
            return Err(Error::invalid_input(
                "conformance window exceeds the coded height",
            ));
        }
        u64::from(pic_height_in_luma_samples) - crop
    } else {
        u64::from(pic_height_in_luma_samples)
    };

    // クロップ後の幅または高さが 0、u16::MAX 超過は飽和せず拒否する
    if cropped_width == 0 || cropped_height == 0 {
        return Err(Error::invalid_input(
            "cropped width and height must be non-zero",
        ));
    }
    let width = u16::try_from(cropped_width)
        .map_err(|_| Error::invalid_input("frame width exceeds u16::MAX"))?;
    let height = u16::try_from(cropped_height)
        .map_err(|_| Error::invalid_input("frame height exceeds u16::MAX"))?;

    Ok(H265Sps {
        general_profile_space,
        general_tier_flag,
        general_profile_idc,
        general_profile_compatibility_flags,
        general_constraint_indicator_flags,
        general_level_idc,
        sps_max_sub_layers_minus1,
        sps_temporal_id_nesting_flag,
        chroma_format_idc: chroma_format_idc as u8,
        bit_depth_luma_minus8: bit_depth_luma_minus8 as u8,
        bit_depth_chroma_minus8: bit_depth_chroma_minus8 as u8,
        width,
        height,
    })
}

/// [`Hev1Box`] / [`Hvc1Box`] の構築に必要な、ストリームから一意に決まらない
/// 設定値
///
/// profile / level / width / height / chroma / bit depth / VPS / SPS / PPS は
/// [`build_hev1_box`] / [`build_hvc1_box`] 側で EBSP から導出するため、
/// 本構造体には含めない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct H265SampleEntryConfig {
    /// NAL 長フィールド幅
    pub length_size: LengthSize,
}

/// VPS / SPS / PPS の EBSP リストと設定値から [`Hev1Box`] を 1 つ構築する
///
/// [`SampleEntry`][crate::boxes::SampleEntry] には包まず [`Hev1Box`] をそのまま返す。
///
/// `'hev1'` はパラメータセットがサンプル中にも in-band で現れうる場合の
/// fourcc であり、ISO/IEC 14496-15:2022 8.4.1.1.1 は全配列の
/// `array_completeness` を 0 とする。構築 API は代表 SPS から `hvcC` 欄と
/// 寸法を埋めるため、VPS / SPS / PPS の 3 種を 1 個以上要求する。
///
/// # 固定値 (関数側で埋める)
///
/// - [`VisualSampleEntryFields`] の `horizresolution` / `vertresolution` /
///   `frame_count` / `compressorname` / `depth`: 同構造体のデフォルト
/// - [`VisualSampleEntryFields::data_reference_index`] =
///   [`VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX`]
/// - [`Hev1Box::unknown_boxes`] = 空 `Vec`
/// - [`HvccBox::min_spatial_segmentation_idc`] = 0 / [`HvccBox::parallelism_type`] = 0 /
///   [`HvccBox::avg_frame_rate`] = 0 / [`HvccBox::constant_frame_rate`] = 0
/// - [`HvccBox`] の configurationVersion は 1 (encode 側が書く)
///
/// # ストリーム導出値 (先頭 SPS から写す)
///
/// - [`HvccBox`] の profile / level / chroma / bit depth / temporal の各欄
/// - [`VisualSampleEntryFields::width`] / [`VisualSampleEntryFields::height`]:
///   先頭 SPS のクロップ適用後の値
/// - [`HvccBox::nalu_arrays`]: VPS / SPS / PPS の EBSP を入力順で 3 配列に格納
///
/// # 呼び出し側指定値
///
/// - [`H265SampleEntryConfig::length_size`]: NAL 長フィールド幅 ([`LengthSize`])
///
/// # エラー条件
///
/// - VPS / SPS / PPS のいずれかが 0 個
/// - VPS / SPS / PPS のいずれかの配列が `u16::MAX` 個超、または NAL が
///   `u16::MAX` バイト超 (hvcC の個数 / 長さ欄が 16 ビット)
/// - VPS / SPS / PPS が非空・NAL type 32 / 33 / 34 以外。VPS / SPS は
///   `TemporalId == 0` でないものを拒否 (PPS は 0 でなくてよい)
/// - 先頭 SPS の解析失敗 ([`parse_sps`] のエラー条件)
pub fn build_hev1_box(
    vps_list: &[Vec<u8>],
    sps_list: &[Vec<u8>],
    pps_list: &[Vec<u8>],
    config: &H265SampleEntryConfig,
) -> Result<Hev1Box> {
    let (hvcc_box, visual) = build_hvcc_box_and_visual(vps_list, sps_list, pps_list, config, 0)?;
    Ok(Hev1Box {
        visual,
        hvcc_box,
        unknown_boxes: Vec::new(),
    })
}

/// VPS / SPS / PPS の EBSP リストと設定値から [`Hvc1Box`] を 1 つ構築する
///
/// [`SampleEntry`][crate::boxes::SampleEntry] には包まず [`Hvc1Box`] をそのまま返す。
///
/// `'hvc1'` はパラメータセットが全て `hvcC` の out-of-band に格納される場合の
/// fourcc であり、ISO/IEC 14496-15:2022 8.4.1.1.1 はパラメータセット配列の
/// `array_completeness` を 1 に必須 (default and mandatory) とする。構築 API は
/// 代表 SPS から `hvcC` 欄と寸法を埋めるため、VPS / SPS / PPS の 3 種を
/// 1 個以上要求する。
///
/// 固定値 / ストリーム導出値 / 呼び出し側指定値とエラー条件は
/// [`build_hev1_box`] と同じ。`array_completeness` が 1 であることだけが
/// 異なる。
pub fn build_hvc1_box(
    vps_list: &[Vec<u8>],
    sps_list: &[Vec<u8>],
    pps_list: &[Vec<u8>],
    config: &H265SampleEntryConfig,
) -> Result<Hvc1Box> {
    let (hvcc_box, visual) = build_hvcc_box_and_visual(vps_list, sps_list, pps_list, config, 1)?;
    Ok(Hvc1Box {
        visual,
        hvcc_box,
        unknown_boxes: Vec::new(),
    })
}

/// Annex B の入力から [`Hev1Box`] を構築する
///
/// [`parse_annexb_nal_units`] で列挙した NAL から type 32 (VPS) / type 33 (SPS) /
/// type 34 (PPS) を入力順で集め、[`build_hev1_box`] に渡す薄いラッパー。
/// VCL / SEI 等の他種別の NAL は無視する。
///
/// # エラー条件
///
/// - [`parse_annexb_nal_units`] のエラー条件
/// - VPS / SPS / PPS のいずれかが 0 個 (Annex B に入っていない場合)
/// - [`build_hev1_box`] のエラー条件
pub fn build_hev1_box_from_annexb(input: &[u8], config: &H265SampleEntryConfig) -> Result<Hev1Box> {
    let nals = parse_annexb_nal_units(input)?;
    build_hev1_box(
        &collect_parameter_sets(nals.iter().copied(), H265NalUnitType::Vps),
        &collect_parameter_sets(nals.iter().copied(), H265NalUnitType::Sps),
        &collect_parameter_sets(nals.iter().copied(), H265NalUnitType::Pps),
        config,
    )
}

/// Annex B の入力から [`Hvc1Box`] を構築する
///
/// [`parse_annexb_nal_units`] で列挙した NAL から type 32 (VPS) / type 33 (SPS) /
/// type 34 (PPS) を入力順で集め、[`build_hvc1_box`] に渡す薄いラッパー。
/// VCL / SEI 等の他種別の NAL は無視する。
///
/// # エラー条件
///
/// - [`parse_annexb_nal_units`] のエラー条件
/// - VPS / SPS / PPS のいずれかが 0 個 (Annex B に入っていない場合)
/// - [`build_hvc1_box`] のエラー条件
pub fn build_hvc1_box_from_annexb(input: &[u8], config: &H265SampleEntryConfig) -> Result<Hvc1Box> {
    let nals = parse_annexb_nal_units(input)?;
    build_hvc1_box(
        &collect_parameter_sets(nals.iter().copied(), H265NalUnitType::Vps),
        &collect_parameter_sets(nals.iter().copied(), H265NalUnitType::Sps),
        &collect_parameter_sets(nals.iter().copied(), H265NalUnitType::Pps),
        config,
    )
}

/// NAL ユニット列から指定した種別の NAL 本体を `Vec<Vec<u8>>` に写す
fn collect_parameter_sets<'a, I>(nals: I, nal_unit_type: H265NalUnitType) -> Vec<Vec<u8>>
where
    I: IntoIterator<Item = H265NalUnit<'a>>,
{
    collect_nal_units(nals, nal_unit_type)
        .into_iter()
        .map(|nal| nal.to_vec())
        .collect()
}

/// VPS / SPS / PPS の EBSP リストから `hvcC` ボックスと映像共通フィールドを
/// 構築する
///
/// `array_completeness` は `'hvc1'` で 1、`'hev1'` で 0 になる
/// (ISO/IEC 14496-15:2022 8.4.1.1.1)。
fn build_hvcc_box_and_visual(
    vps_list: &[Vec<u8>],
    sps_list: &[Vec<u8>],
    pps_list: &[Vec<u8>],
    config: &H265SampleEntryConfig,
    array_completeness: u8,
) -> Result<(HvccBox, VisualSampleEntryFields)> {
    // いずれかが 0 個なら Error。構築 API は代表 SPS から hvcC 欄と寸法を
    // 埋めるため、hev1 でも 3 種を 1 個以上要求する
    let first_sps = sps_list
        .first()
        .ok_or_else(|| Error::invalid_input("SPS list must not be empty"))?;
    if vps_list.is_empty() {
        return Err(Error::invalid_input("VPS list must not be empty"));
    }
    if pps_list.is_empty() {
        return Err(Error::invalid_input("PPS list must not be empty"));
    }

    // HvccBox::encode が配列個数と NAL 個数 / 長さを u8 / u16 で書くため、
    // ボックス encode に渡す前に上限を検証する
    if vps_list.len() > MAX_NALUS_PER_ARRAY
        || sps_list.len() > MAX_NALUS_PER_ARRAY
        || pps_list.len() > MAX_NALUS_PER_ARRAY
    {
        return Err(Error::invalid_input(
            "too many parameter sets (max u16::MAX)",
        ));
    }
    for nal in vps_list.iter().chain(sps_list).chain(pps_list) {
        if nal.len() > u16::MAX as usize {
            return Err(Error::invalid_input(
                "parameter set is too long (max u16::MAX)",
            ));
        }
    }

    // 全ての VPS / SPS / PPS を非空・NAL type 32 / 33 / 34 として検証する。
    // 構文解析して代表値にするのは先頭 SPS だけでよい
    validate_parameter_sets(vps_list, H265NalUnitType::Vps, true)?;
    validate_parameter_sets(sps_list, H265NalUnitType::Sps, true)?;
    validate_parameter_sets(pps_list, H265NalUnitType::Pps, false)?;

    // 先頭 SPS を解析して代表値にする
    let sps = parse_sps(first_sps)?;

    // 呼び出し側指定の長さフィールド幅を length_size_minus_one (0 / 1 / 3) へ
    // 写す。幅 3 (length_size_minus_one == 2) は ISO/IEC 14496-15:2022
    // 8.3.2.1.3 で reserved のため LengthSize 型では表現できない
    let length_size_minus_one = Uint::new(config.length_size.length_size_minus_one());

    let hvcc_box = HvccBox {
        general_profile_space: Uint::new(sps.general_profile_space),
        general_tier_flag: Uint::new(sps.general_tier_flag),
        general_profile_idc: Uint::new(sps.general_profile_idc),
        general_profile_compatibility_flags: sps.general_profile_compatibility_flags,
        general_constraint_indicator_flags: Uint::new(sps.general_constraint_indicator_flags),
        general_level_idc: sps.general_level_idc,
        // VUI を読まないため追加制限を付けない。0 は空間分割の下限であり、
        // 活性化される全パラメータセットの最低以下であることを shall とする
        // 8.3.2.1.1 の制約を常に満たす
        min_spatial_segmentation_idc: Uint::new(0),
        // PPS 構文は対象外のため 0 (混合または不明なら 0 に should、8.3.2.1.3)
        parallelism_type: Uint::new(0),
        chroma_format_idc: Uint::new(sps.chroma_format_idc),
        bit_depth_luma_minus8: Uint::new(sps.bit_depth_luma_minus8),
        bit_depth_chroma_minus8: Uint::new(sps.bit_depth_chroma_minus8),
        // 0 は unspecified average frame rate (8.3.2.1.3)
        avg_frame_rate: 0,
        // 0 は「定フレームレートであるとは限らない」(8.3.2.1.3)
        constant_frame_rate: Uint::new(0),
        // 1..=7 (8.3.2.1.3 は 1 を非 temporal scalable とする)
        num_temporal_layers: Uint::new(sps.sps_max_sub_layers_minus1 + 1),
        temporal_id_nested: Uint::new(sps.sps_temporal_id_nesting_flag),
        length_size_minus_one,
        nalu_arrays: vec![
            HvccNalUintArray {
                array_completeness: Uint::new(array_completeness),
                nal_unit_type: Uint::new(NAL_UNIT_TYPE_VPS),
                nalus: vps_list.to_vec(),
            },
            HvccNalUintArray {
                array_completeness: Uint::new(array_completeness),
                nal_unit_type: Uint::new(NAL_UNIT_TYPE_SPS),
                nalus: sps_list.to_vec(),
            },
            HvccNalUintArray {
                array_completeness: Uint::new(array_completeness),
                nal_unit_type: Uint::new(NAL_UNIT_TYPE_PPS),
                nalus: pps_list.to_vec(),
            },
        ],
    };

    let visual = VisualSampleEntryFields {
        data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
        width: sps.width,
        height: sps.height,
        horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
        vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
        frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
        compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
        depth: VisualSampleEntryFields::DEFAULT_DEPTH,
    };

    Ok((hvcc_box, visual))
}

/// パラメータセットの NAL ヘッダーを検証する
///
/// `expected` と異なる NAL type を拒否する。`require_temporal_id_zero` が
/// 真のとき `TemporalId == 0` を要求する (ITU-T H.265 7.4.2.2 は VPS / SPS に
/// 課す。PPS は NOTE 9 どおり 0 でなくてよい)
fn validate_parameter_sets(
    list: &[Vec<u8>],
    expected: H265NalUnitType,
    require_temporal_id_zero: bool,
) -> Result<()> {
    for nal in list {
        if validate_h265_nal_header(nal)? != expected {
            return Err(Error::invalid_input(
                "parameter set NAL unit type does not match the expected type",
            ));
        }
        if require_temporal_id_zero && nuh_temporal_id_plus1(nal) != 1 {
            return Err(Error::invalid_input("parameter set TemporalId must be 0"));
        }
    }
    Ok(())
}

/// EBSP から emulation prevention byte (0x03) を除いて RBSP を得る
///
/// ITU-T H.265 7.3.1.1 の構文ループと 7.4.2.1 の規定により、直前に 0x00 が
/// 2 バイト続く 0x03 は `emulation_prevention_three_byte` としてデコード処理が
/// 破棄する。入力側の位置で判定する (RBSP の `00 00 03` は EBSP では
/// `00 00 03 03` になり、出力側の連続ゼロ数では正しく戻せない)
fn remove_emulation_prevention_bytes(ebsp: &[u8]) -> Vec<u8> {
    let mut rbsp = Vec::new();
    let mut i = 0;
    while i < ebsp.len() {
        if i >= 2 && ebsp[i - 2] == 0 && ebsp[i - 1] == 0 && ebsp[i] == 0x03 {
            i += 1;
            continue;
        }
        rbsp.push(ebsp[i]);
        i += 1;
    }
    rbsp
}

/// SPS の RBSP を読む MSB-first ビットリーダー
///
/// `u(n)` の固定長フィールドと、Exp-Golomb の `ue(v)` を読む。
/// 入力が尽きた場合は [`crate::Error`] を返す
struct SpsBitReader<'a> {
    input: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> SpsBitReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bit(&mut self) -> Result<u8> {
        if self.byte_pos >= self.input.len() {
            return Err(Error::invalid_input("SPS is truncated"));
        }
        let bit = (self.input[self.byte_pos] >> (7 - self.bit_pos)) & 1;
        self.bit_pos += 1;
        if self.bit_pos == 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
        Ok(bit)
    }

    fn read_bits(&mut self, n: u32) -> Result<u64> {
        // 64 ビット超のフィールドは SPS の構文要素にない (最大 48 ビット)。
        // 読み飛ばしは skip_bits を使う
        let mut value: u64 = 0;
        for _ in 0..n {
            value = (value << 1) | u64::from(self.read_bit()?);
        }
        Ok(value)
    }

    /// ビット位置を進めるためだけに `n` ビット読み飛ばす
    fn skip_bits(&mut self, n: u32) -> Result<()> {
        for _ in 0..n {
            self.read_bit()?;
        }
        Ok(())
    }

    /// 符号なし Exp-Golomb (`ue(v)`) を読む
    ///
    /// 先頭の 0 の連続数 `leadingZeroBits` から
    /// `codeNum = 2^leadingZeroBits - 1 + 後続ビット` を復元する
    fn read_ue(&mut self) -> Result<u32> {
        let mut zeros: u32 = 0;
        while self.read_bit()? == 0 {
            zeros += 1;
            // 32 ビットを超える Exp-Golomb コードは SPS の要素として不正
            if zeros >= 32 {
                return Err(Error::invalid_input("Exp-Golomb code exceeds 32 bits"));
            }
        }
        let suffix = if zeros > 0 { self.read_bits(zeros)? } else { 0 };
        let code_num = (1u64 << zeros) - 1 + suffix;
        u32::try_from(code_num)
            .map_err(|_| Error::invalid_input("Exp-Golomb code value is too large"))
    }
}
