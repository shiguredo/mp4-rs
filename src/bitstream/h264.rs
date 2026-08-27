//! H.264 ビットストリーム処理ユーティリティ
//!
//! Annex B / length-prefixed の NAL ユニット列の解析、SPS の解析、
//! SPS / PPS の抽出、`avc1` / `avcC` の構築、profile-level-id の
//! パース・正規化を提供する。
//!
//! 参照仕様は以下のとおり。
//!
//! - ITU-T H.264 (06/2026): NAL ユニット (7.3.1 / 7.4.1)、SPS (7.3.2.1.1 /
//!   7.4.2.1.1)、Annex B の開始コード、profile / level (Annex A Table A-1)
//! - ISO/IEC 14496-15: `AVCDecoderConfigurationRecord` (`avcC`) の
//!   `lengthSizeMinusOne` (0 / 1 / 3 が正当、2 は reserved)
//! - RFC 6184: profile-level-id (Section 8.1 Table 5 と Level 1b の記述)
//!
//! # NAL バイト列の契約
//!
//! このモジュールが返す NAL バイト列は、1 バイトの NAL ヘッダーを含み、
//! 開始コードと長さプレフィックスを含まない EBSP (emulation prevention byte を
//! 残したまま) である。RBSP 化は [`parse_sps`] の内部だけで行う。
//!
//! # 長さフィールド幅の契約
//!
//! length-prefixed 形式を扱う API の長さフィールド幅は [`LengthSize`] 型で
//! 表現する。幅 3 (ISO/IEC 14496-15 の `lengthSizeMinusOne == 2`、reserved) は
//! この型で表現できない。

pub use crate::bitstream::nal::LengthSize;

use alloc::vec::Vec;

use crate::{
    Error, Result, Uint,
    bitstream::nal,
    boxes::{Avc1Box, AvccBox, VisualSampleEntryFields},
};

/// NAL ユニット種別 (ITU-T H.264 Table 7-1 の `nal_unit_type`)
///
/// この列挙は Table 7-1 の主要種別だけを名前付きで持ち、予約・未指定や
/// 実ストリームで使われる定義値以外の種別は [`Other`](H264NalUnitType::Other)
/// として不透明に通す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum H264NalUnitType {
    /// 非 IDR ピクチャの符号化スライス (1)
    NonIdrSlice,
    /// IDR ピクチャの符号化スライス (5)
    IdrSlice,
    /// SEI (Supplemental enhancement information) (6)
    Sei,
    /// SPS (Sequence parameter set) (7)
    Sps,
    /// PPS (Picture parameter set) (8)
    Pps,
    /// アクセスユニット区切り (Access unit delimiter) (9)
    Aud,
    /// Table 7-1 の定義値以外 (0、2..=4、10..=31 など)。実ストリームで
    /// 使われる範囲外の値もここで不透明に通す
    Other(u8),
}

impl H264NalUnitType {
    /// ヘッダーの下位 5 ビットを [`H264NalUnitType`] に写す
    fn from_header_value(value: u8) -> Self {
        match value {
            1 => Self::NonIdrSlice,
            5 => Self::IdrSlice,
            6 => Self::Sei,
            7 => Self::Sps,
            8 => Self::Pps,
            9 => Self::Aud,
            other => Self::Other(other),
        }
    }
}

/// [`AvccBox`] に格納できる SPS の最大個数 (ISO/IEC 14496-15 の `unsigned int(5)`)
const MAX_SPS_COUNT: usize = 31;

/// [`AvccBox`] に格納できる PPS の最大個数 (ISO/IEC 14496-15 の `unsigned int(8)`)
const MAX_PPS_COUNT: usize = 255;

/// H.264 の 1 個の NAL ユニット
///
/// [`parse_annexb_nal_units`] / [`parse_length_prefixed_nal_units`] が返す要素で、
/// 入力バイト列を借用する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct H264NalUnit<'a> {
    /// `nal_unit_type` (NAL ヘッダーの下位 5 ビット、ITU-T H.264 7.4.1)
    pub nal_unit_type: H264NalUnitType,

    /// NAL 本体
    ///
    /// 1 バイトの NAL ヘッダーを含み、開始コードと長さプレフィックスを含まない
    /// EBSP (emulation prevention byte を残したまま)
    pub data: &'a [u8],
}

/// H.264 の NAL ヘッダー (ITU-T H.264 7.3.1 / 7.4.1) を検証して `nal_unit_type` を返す
///
/// - ヘッダー 1 バイトに満たない NAL は [`crate::Error`]
/// - `forbidden_zero_bit` が 1 なら [`crate::Error`]
/// - 予約・未指定の `nal_unit_type` はエラーにせずそのまま返す
///   (フレーミングでは不透明な NAL として通す)
fn validate_h264_nal_header(data: &[u8]) -> Result<H264NalUnitType> {
    let Some(&header) = data.first() else {
        return Err(Error::invalid_input("NAL unit is shorter than 1 byte"));
    };
    if header & 0b1000_0000 != 0 {
        return Err(Error::invalid_input("forbidden_zero_bit must be 0"));
    }
    Ok(H264NalUnitType::from_header_value(header & 0b0001_1111))
}

/// 走査済みの NAL 本体から [`H264NalUnit`] を作る
///
/// H.264 のヘッダー検証を適用する (NAL 本体が 1 バイト未満や
/// `forbidden_zero_bit == 1` の場合は [`crate::Error`])
fn to_h264_nal_unit(body: &[u8]) -> Result<H264NalUnit<'_>> {
    let nal_unit_type = validate_h264_nal_header(body)?;
    Ok(H264NalUnit {
        nal_unit_type,
        data: body,
    })
}

/// Annex B (ITU-T H.264 Annex B) の NAL ユニット列を解析する
///
/// # 入力
///
/// - `input`: 3 バイト (`0x000001`) / 4 バイト (`0x00000001`) の開始コードで
///   区切られた NAL ユニット列
///
/// # 返り値
///
/// NAL ユニットの列。各 [`H264NalUnit::data`] は 1 バイトの NAL ヘッダーを
/// 含み、開始コードを含まない EBSP である。
///
/// # エラー条件
///
/// - 非空入力に開始コードが 1 つも無い
/// - 最初の開始コードより前に非ゼロバイトがある
/// - 開始コードの直後に次の開始コードまたは入力終端が来る空 NAL
/// - `forbidden_zero_bit == 1` の NAL
/// - ヘッダー 1 バイトに満たない NAL
///
/// 空入力は NAL ユニット 0 個の成功として扱う (開始コード欠落とは区別する)。
/// 先頭の開始コードより前の `leading_zero_8bits`、NAL 間のゼロ詰め、および
/// 最後の NAL より後の `trailing_zero_8bits` は境界の詰め物として NAL 本体に
/// 含めない。
pub fn parse_annexb_nal_units(input: &[u8]) -> Result<Vec<H264NalUnit<'_>>> {
    let bodies = nal::scan_annexb_nals(input)?;
    bodies.iter().map(|body| to_h264_nal_unit(body)).collect()
}

/// length-prefixed 形式 (ISO/IEC 14496-15) の NAL ユニット列を解析する
///
/// # 入力
///
/// - `input`: 大端序の長さフィールド + NAL 本体を繰り返したバイト列
/// - `length_size`: 長さフィールド幅
///
/// # 返り値
///
/// NAL ユニットの列。各 [`H264NalUnit::data`] は 1 バイトの NAL ヘッダーを
/// 含み、長さプレフィックスを含まない EBSP である。
///
/// # エラー条件
///
/// - 長さフィールドが入力末尾を超える
/// - 宣言長が残バイトを超える (切り詰め)
/// - 宣言長が 0 の NAL
/// - `forbidden_zero_bit == 1` の NAL
/// - ヘッダー 1 バイトに満たない NAL
///
/// 空入力は NAL ユニット 0 個の成功として扱う。
pub fn parse_length_prefixed_nal_units(
    input: &[u8],
    length_size: LengthSize,
) -> Result<Vec<H264NalUnit<'_>>> {
    let bodies = nal::scan_length_prefixed_nals(input, length_size)?;
    bodies.iter().map(|body| to_h264_nal_unit(body)).collect()
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
pub fn length_prefixed_to_annexb(input: &[u8], length_size: LengthSize) -> Result<Vec<u8>> {
    nal::length_prefixed_to_annexb(input, length_size)
}

/// NAL ユニット列から指定した [`H264NalUnitType`] の NAL 本体を集める
///
/// 入力順を保ったまま、一致した NAL の [`H264NalUnit::data`] だけを返す。
/// エラーにはならない (一致が無ければ空 `Vec`)。
pub fn collect_nal_units<'a, I>(nals: I, nal_unit_type: H264NalUnitType) -> Vec<&'a [u8]>
where
    I: IntoIterator<Item = H264NalUnit<'a>>,
{
    nals.into_iter()
        .filter(|nal| nal.nal_unit_type == nal_unit_type)
        .map(|nal| nal.data)
        .collect()
}

/// SPS (Sequence Parameter Set) の解析結果
///
/// ITU-T H.264 7.4.2.1.1 の `seq_parameter_set_data` から導出した値。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct H264Sps {
    /// `profile_idc`
    pub profile_idc: u8,

    /// constraint フラグ 1 バイト全体
    ///
    /// `constraint_set0_flag` から `constraint_set5_flag` までの 6 ビットと
    /// `reserved_zero_2bits` 2 ビットをそのまま保持する
    pub constraint_set_flags: u8,

    /// `level_idc`
    pub level_idc: u8,

    /// `chroma_format_idc`
    ///
    /// SPS 追加構文を読まない `profile_idc` (66 / 77 / 88 を含む) では、
    /// ITU-T H.264 7.4.2.1.1 の不在時推論どおり 1 (4:2:0)
    pub chroma_format_idc: u8,

    /// `bit_depth_luma_minus8`
    ///
    /// SPS 追加構文を読まない `profile_idc` では不在時推論どおり 0
    pub bit_depth_luma_minus8: u8,

    /// `bit_depth_chroma_minus8`
    ///
    /// SPS 追加構文を読まない `profile_idc` では不在時推論どおり 0
    pub bit_depth_chroma_minus8: u8,

    /// クロップ適用後の幅 (ピクセル)
    ///
    /// `VisualSampleEntryFields::width` に写せるよう `u16` に収まらない値は
    /// [`parse_sps`] が拒否する
    pub width: u16,

    /// クロップ適用後の高さ (ピクセル)
    ///
    /// フレームの輝度高さ `16 * FrameHeightInMbs` からクロップを引いた値。
    /// [`VisualSampleEntryFields::height`] に写せるよう `u16` に収まらない値は
    /// [`parse_sps`] が拒否する
    pub height: u16,
}

/// NAL ヘッダー付き EBSP の SPS を解析する
///
/// # 入力
///
/// - `nal_unit`: NAL ヘッダー 1 バイト + EBSP の SPS
///
/// 内部でヘッダーを検証し、残バイトから emulation prevention byte を除いて
/// RBSP を得て、ITU-T H.264 7.3.2.1.1 / 7.4.2.1.1 の Exp-Golomb (`ue(v)` /
/// `se(v)`) で読む。
///
/// # エラー条件
///
/// - NAL ヘッダーの `forbidden_zero_bit == 1`、または NAL 本体が 1 バイト未満
/// - `nal_unit_type` が 7 (SPS) 以外
/// - `chroma_format_idc > 3`、`bit_depth_luma_minus8 > 6`、
///   `bit_depth_chroma_minus8 > 6`、`pic_order_cnt_type > 2` (7.4.2.1.1 の値域外)
/// - 寸法の導出に必要な構文 (frame cropping まで) が途中で終わる SPS、
///   Exp-Golomb の途中終端。`vui_parameters_present_flag` 以降の欠落は成功とする
/// - クロップが符号化サイズを食いつぶす
/// - クロップ後の幅または高さが 0
/// - クロップ後の幅または高さが `u16::MAX` を超える
///
/// # 対象外
///
/// scaling list と VUI は公開結果に載せない。`vui_parameters_present_flag`
/// 以降は読まない。
pub fn parse_sps(nal_unit: &[u8]) -> Result<H264Sps> {
    let nal_unit_type = validate_h264_nal_header(nal_unit)?;
    if nal_unit_type != H264NalUnitType::Sps {
        return Err(Error::invalid_input("SPS NAL unit type must be 7"));
    }

    let rbsp = remove_emulation_prevention_bytes(&nal_unit[1..]);
    let mut reader = SpsBitReader::new(&rbsp);

    // profile_idc / constraint フラグ 1 バイト全体 / level_idc / seq_parameter_set_id
    let profile_idc = reader.read_bits(8)? as u8;
    let constraint_set_flags = reader.read_bits(8)? as u8;
    let level_idc = reader.read_bits(8)? as u8;
    let _seq_parameter_set_id = reader.read_ue()?;

    // SPS 追加構文 (chroma_format_idc 以降) を読む profile_idc は
    // ITU-T H.264 7.3.2.1.1 の条件節どおり次に限る
    let has_extended_syntax = matches!(
        profile_idc,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
    );

    // 不在時推論 (ITU-T H.264 7.4.2.1.1): chroma_format_idc = 1 (4:2:0)、
    // bit_depth_luma_minus8 = 0、bit_depth_chroma_minus8 = 0
    let mut chroma_format_idc: u32 = 1;
    let mut bit_depth_luma_minus8: u32 = 0;
    let mut bit_depth_chroma_minus8: u32 = 0;
    let mut separate_colour_plane_flag: u8 = 0;

    if has_extended_syntax {
        chroma_format_idc = reader.read_ue()?;
        if chroma_format_idc > 3 {
            return Err(Error::invalid_input("chroma_format_idc must be 0..=3"));
        }
        if chroma_format_idc == 3 {
            separate_colour_plane_flag = reader.read_bit()?;
        }
        bit_depth_luma_minus8 = reader.read_ue()?;
        if bit_depth_luma_minus8 > 6 {
            return Err(Error::invalid_input("bit_depth_luma_minus8 must be 0..=6"));
        }
        bit_depth_chroma_minus8 = reader.read_ue()?;
        if bit_depth_chroma_minus8 > 6 {
            return Err(Error::invalid_input(
                "bit_depth_chroma_minus8 must be 0..=6",
            ));
        }

        // qpprime_y_zero_transform_bypass_flag (u(1)) と seq_scaling_matrix_present_flag
        // 配下の scaling list は寸法に到達するための読み飛ばし
        let _qpprime_y_zero_transform_bypass_flag = reader.read_bit()?;
        if reader.read_bit()? != 0 {
            let list_count = if chroma_format_idc != 3 { 8 } else { 12 };
            for i in 0..list_count {
                if reader.read_bit()? != 0 {
                    // i < 6 は 16 要素、それ以外は 64 要素の scaling_list
                    let size = if i < 6 { 16 } else { 64 };
                    skip_scaling_list(&mut reader, size)?;
                }
            }
        }
    }

    // pic_order_cnt_type 0 / 1 の追加構文はビット位置を進めるためだけに読み飛ばす
    let _log2_max_frame_num_minus4 = reader.read_ue()?;
    let pic_order_cnt_type = reader.read_ue()?;
    if pic_order_cnt_type > 2 {
        return Err(Error::invalid_input("pic_order_cnt_type must be 0..=2"));
    }
    if pic_order_cnt_type == 0 {
        let _log2_max_pic_order_cnt_lsb_minus4 = reader.read_ue()?;
    } else if pic_order_cnt_type == 1 {
        let _delta_pic_order_always_zero_flag = reader.read_bit()?;
        let _offset_for_non_ref_pic = reader.read_se()?;
        let _offset_for_top_to_bottom_field = reader.read_se()?;
        let num_ref_frames_in_pic_order_cnt_cycle = reader.read_ue()?;
        for _ in 0..num_ref_frames_in_pic_order_cnt_cycle {
            let _offset_for_ref_frame = reader.read_se()?;
        }
    }
    let _max_num_ref_frames = reader.read_ue()?;
    let _gaps_in_frame_num_value_allowed_flag = reader.read_bit()?;

    let pic_width_in_mbs_minus1 = reader.read_ue()?;
    let pic_height_in_map_units_minus1 = reader.read_ue()?;
    let frame_mbs_only_flag = reader.read_bit()?;
    if frame_mbs_only_flag == 0 {
        let _mb_adaptive_frame_field_flag = reader.read_bit()?;
    }
    let _direct_8x8_inference_flag = reader.read_bit()?;
    let frame_cropping_flag = reader.read_bit()?;
    let (
        frame_crop_left_offset,
        frame_crop_right_offset,
        frame_crop_top_offset,
        frame_crop_bottom_offset,
    ) = if frame_cropping_flag != 0 {
        (
            reader.read_ue()?,
            reader.read_ue()?,
            reader.read_ue()?,
            reader.read_ue()?,
        )
    } else {
        (0, 0, 0, 0)
    };

    // 寸法の導出は ITU-T H.264 7.4.2.1.1 に従う
    // PicWidthInSamplesL = (pic_width_in_mbs_minus1 + 1) * 16
    let pic_width_in_samples_l = (u64::from(pic_width_in_mbs_minus1) + 1) * 16;
    // PicHeightInMapUnits = pic_height_in_map_units_minus1 + 1
    // FrameHeightInMbs = (2 - frame_mbs_only_flag) * PicHeightInMapUnits
    // サンプルエントリーの高さはフレームの輝度高さ 16 * FrameHeightInMbs
    let pic_height_in_map_units = u64::from(pic_height_in_map_units_minus1) + 1;
    let coded_height = (2 - u64::from(frame_mbs_only_flag)) * pic_height_in_map_units * 16;

    // ChromaArrayType は separate_colour_plane_flag == 1 なら 0、さもなくば chroma_format_idc
    let chroma_array_type = if separate_colour_plane_flag == 1 {
        0
    } else {
        chroma_format_idc
    };

    // CropUnitX / CropUnitY の導出 (7.4.2.1.1)
    let (crop_unit_x, crop_unit_y) = if chroma_array_type == 0 {
        (1, 2 - u64::from(frame_mbs_only_flag))
    } else {
        // SubWidthC / SubHeightC は ITU-T H.264 Table 6-1 による
        // (chroma_format_idc 0 は chroma_array_type 0 になりこの分岐に来ない)
        let (sub_width_c, sub_height_c) = match chroma_format_idc {
            1 => (2, 2),
            2 => (2, 1),
            3 => (1, 1),
            _ => unreachable!("chroma_format_idc 0 は chroma_array_type 0 の分岐で処理される"),
        };
        (
            sub_width_c,
            sub_height_c * (2 - u64::from(frame_mbs_only_flag)),
        )
    };

    // クロップ適用。食いつぶす場合は飽和せず Error にする
    let cropped_width = if frame_cropping_flag != 0 {
        let crop =
            crop_unit_x * (u64::from(frame_crop_left_offset) + u64::from(frame_crop_right_offset));
        if crop > pic_width_in_samples_l {
            return Err(Error::invalid_input(
                "frame cropping exceeds the coded width",
            ));
        }
        pic_width_in_samples_l - crop
    } else {
        pic_width_in_samples_l
    };
    let cropped_height = if frame_cropping_flag != 0 {
        let crop =
            crop_unit_y * (u64::from(frame_crop_top_offset) + u64::from(frame_crop_bottom_offset));
        if crop > coded_height {
            return Err(Error::invalid_input(
                "frame cropping exceeds the coded height",
            ));
        }
        coded_height - crop
    } else {
        coded_height
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

    Ok(H264Sps {
        profile_idc,
        constraint_set_flags,
        level_idc,
        chroma_format_idc: chroma_format_idc as u8,
        bit_depth_luma_minus8: bit_depth_luma_minus8 as u8,
        bit_depth_chroma_minus8: bit_depth_chroma_minus8 as u8,
        width,
        height,
    })
}

/// H.264 の sub-profile (RFC 6184 Section 8.1 Table 5 の 12 profile)
///
/// 同じ sub-profile を表す複数の (profile_idc, profile-iop) 組み合わせは
/// [`parse_profile_level_id`] でこの enum の 1 値へ正規化される。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum H264Profile {
    /// Constrained Baseline (CB)
    ConstrainedBaseline,
    /// Baseline (B)
    Baseline,
    /// Main (M)
    Main,
    /// Extended (E)
    Extended,
    /// High (H)
    High,
    /// High 10 (H10)
    High10,
    /// High 4:2:2 (H42)
    High42,
    /// High 4:4:4 Predictive (H44)
    High44,
    /// High 10 Intra (H10I)
    High10Intra,
    /// High 4:2:2 Intra (H42I)
    High42Intra,
    /// High 4:4:4 Intra (H44I)
    High44Intra,
    /// CAVLC 4:4:4 Intra (C44I)
    Cavlc444Intra,
}

/// H.264 の level (ITU-T H.264 (06/2026) Annex A Table A-1)
///
/// [`H264Level::Level1b`] は `level_idc` だけでは決まらず、profile と
/// profile-iop の組み合わせで決まるため、独立した値として持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum H264Level {
    /// Level 1 (`level_idc` 10)
    Level1,
    /// Level 1b (ad hoc な表現。profile により `level_idc` 11 +
    /// `constraint_set3_flag` 1、または `level_idc` 9)
    Level1b,
    /// Level 1.1 (`level_idc` 11)
    Level1_1,
    /// Level 1.2 (`level_idc` 12)
    Level1_2,
    /// Level 1.3 (`level_idc` 13)
    Level1_3,
    /// Level 2 (`level_idc` 20)
    Level2,
    /// Level 2.1 (`level_idc` 21)
    Level2_1,
    /// Level 2.2 (`level_idc` 22)
    Level2_2,
    /// Level 3 (`level_idc` 30)
    Level3,
    /// Level 3.1 (`level_idc` 31)
    Level3_1,
    /// Level 3.2 (`level_idc` 32)
    Level3_2,
    /// Level 4 (`level_idc` 40)
    Level4,
    /// Level 4.1 (`level_idc` 41)
    Level4_1,
    /// Level 4.2 (`level_idc` 42)
    Level4_2,
    /// Level 5 (`level_idc` 50)
    Level5,
    /// Level 5.1 (`level_idc` 51)
    Level5_1,
    /// Level 5.2 (`level_idc` 52)
    Level5_2,
    /// Level 6 (`level_idc` 60)
    Level6,
    /// Level 6.1 (`level_idc` 61)
    Level6_1,
    /// Level 6.2 (`level_idc` 62)
    Level6_2,
}

/// H.264 の sub-profile と level の正規化結果
///
/// RFC 6184 の SDP パラメータ `profile-level-id` (3 バイト / 6 桁 hex)
/// そのものではなく、それが示す default sub-profile と default level である。
/// [`parse_profile_level_id`] / [`parse_profile_level_id_hex`] が返す。
/// 元の 3 バイト (profile_idc / profile-iop / level_idc) は保持せず、
/// 正規化した enum だけを持つ。正規化は非可逆であるため、
/// 3 バイトや hex への逆変換メソッドは提供しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct H264ProfileLevel {
    /// sub-profile (RFC 6184 Section 8.1 Table 5)
    pub profile: H264Profile,

    /// level (ITU-T H.264 (06/2026) Annex A Table A-1)
    pub level: H264Level,
}

/// RFC 6184 Section 8.1 Table 5 の 1 行
///
/// `mask` は profile-iop の固定ビット位置、`value` はその固定ビットの
/// 期待値。Table 5 の `x` (どちらでもよい) は mask から外れる。
#[derive(Debug, Clone, Copy)]
struct H264ProfilePattern {
    profile: H264Profile,
    profile_idc: u8,
    mask: u8,
    value: u8,
}

/// RFC 6184 Section 8.1 Table 5 の (profile_idc, profile-iop) パターン
///
/// profile-iop は MSB から constraint_set0_flag から constraint_set5_flag
/// と reserved_zero_2bits の 1 バイトで、全パターンが reserved_zero_2bits
/// (下位 2 bit) に 0 を要求する。Table 5 の `x` はどちらでもよい。
/// Annex A の profile 追加や `reserved_zero_2bits` の非 0 定義が
/// 将来変更される可能性があっても、本テーブルは Table 5 の 12 sub-profile
/// に固定する。
const H264_PROFILE_PATTERNS: &[H264ProfilePattern] = &[
    // CB: profile_idc 42 (66) + x1xx0000
    H264ProfilePattern {
        profile: H264Profile::ConstrainedBaseline,
        profile_idc: 0x42,
        mask: 0b0100_1111,
        value: 0b0100_0000,
    },
    // CB: profile_idc 4D (77) + 1xxx0000
    H264ProfilePattern {
        profile: H264Profile::ConstrainedBaseline,
        profile_idc: 0x4D,
        mask: 0b1000_1111,
        value: 0b1000_0000,
    },
    // CB: profile_idc 58 (88) + 11xx0000
    H264ProfilePattern {
        profile: H264Profile::ConstrainedBaseline,
        profile_idc: 0x58,
        mask: 0b1100_1111,
        value: 0b1100_0000,
    },
    // B: profile_idc 42 (66) + x0xx0000
    H264ProfilePattern {
        profile: H264Profile::Baseline,
        profile_idc: 0x42,
        mask: 0b0100_1111,
        value: 0,
    },
    // B: profile_idc 58 (88) + 10xx0000
    H264ProfilePattern {
        profile: H264Profile::Baseline,
        profile_idc: 0x58,
        mask: 0b1100_1111,
        value: 0b1000_0000,
    },
    // M: profile_idc 4D (77) + 0x0x0000
    H264ProfilePattern {
        profile: H264Profile::Main,
        profile_idc: 0x4D,
        mask: 0b1010_1111,
        value: 0,
    },
    // E: profile_idc 58 (88) + 00xx0000
    H264ProfilePattern {
        profile: H264Profile::Extended,
        profile_idc: 0x58,
        mask: 0b1100_1111,
        value: 0,
    },
    // H: profile_idc 64 (100) + 00000000
    H264ProfilePattern {
        profile: H264Profile::High,
        profile_idc: 0x64,
        mask: 0xFF,
        value: 0,
    },
    // H10: profile_idc 6E (110) + 00000000
    H264ProfilePattern {
        profile: H264Profile::High10,
        profile_idc: 0x6E,
        mask: 0xFF,
        value: 0,
    },
    // H42: profile_idc 7A (122) + 00000000
    H264ProfilePattern {
        profile: H264Profile::High42,
        profile_idc: 0x7A,
        mask: 0xFF,
        value: 0,
    },
    // H44: profile_idc F4 (244) + 00000000
    H264ProfilePattern {
        profile: H264Profile::High44,
        profile_idc: 0xF4,
        mask: 0xFF,
        value: 0,
    },
    // H10I: profile_idc 6E (110) + 00010000
    H264ProfilePattern {
        profile: H264Profile::High10Intra,
        profile_idc: 0x6E,
        mask: 0xFF,
        value: 0b0001_0000,
    },
    // H42I: profile_idc 7A (122) + 00010000
    H264ProfilePattern {
        profile: H264Profile::High42Intra,
        profile_idc: 0x7A,
        mask: 0xFF,
        value: 0b0001_0000,
    },
    // H44I: profile_idc F4 (244) + 00010000
    H264ProfilePattern {
        profile: H264Profile::High44Intra,
        profile_idc: 0xF4,
        mask: 0xFF,
        value: 0b0001_0000,
    },
    // C44I: profile_idc 2C (44) + 00010000
    H264ProfilePattern {
        profile: H264Profile::Cavlc444Intra,
        profile_idc: 0x2C,
        mask: 0xFF,
        value: 0b0001_0000,
    },
];

/// RFC 6184 Section 8.1 Table 5 の sub-profile を判定する
///
/// Table 5 に載っていない (profile_idc, profile-iop) の組み合わせは
/// [`crate::Error`] とする。全パターンが reserved_zero_2bits (下位 2 bit)
/// に 0 を要求するため、非 0 の組み合わせもここで自然に拒否される。
fn parse_h264_profile(profile_idc: u8, profile_iop: u8) -> Result<H264Profile> {
    for pattern in H264_PROFILE_PATTERNS {
        if pattern.profile_idc == profile_idc && (profile_iop & pattern.mask) == pattern.value {
            return Ok(pattern.profile);
        }
    }
    Err(Error::invalid_input(
        "profile_idc / profile-iop combination is not listed in RFC 6184 Section 8.1 Table 5",
    ))
}

/// ITU-T H.264 (06/2026) Annex A Table A-1 の `level_idc` から [`H264Level`] へ変換する
///
/// Level 1b は `level_idc` だけでは決まらないため、ここでは扱わない
/// ([`parse_h264_level`] が Level 1b を先に判定する)
fn level_from_idc(level_idc: u8) -> Result<H264Level> {
    match level_idc {
        10 => Ok(H264Level::Level1),
        11 => Ok(H264Level::Level1_1),
        12 => Ok(H264Level::Level1_2),
        13 => Ok(H264Level::Level1_3),
        20 => Ok(H264Level::Level2),
        21 => Ok(H264Level::Level2_1),
        22 => Ok(H264Level::Level2_2),
        30 => Ok(H264Level::Level3),
        31 => Ok(H264Level::Level3_1),
        32 => Ok(H264Level::Level3_2),
        40 => Ok(H264Level::Level4),
        41 => Ok(H264Level::Level4_1),
        42 => Ok(H264Level::Level4_2),
        50 => Ok(H264Level::Level5),
        51 => Ok(H264Level::Level5_1),
        52 => Ok(H264Level::Level5_2),
        60 => Ok(H264Level::Level6),
        61 => Ok(H264Level::Level6_1),
        62 => Ok(H264Level::Level6_2),
        _ => Err(Error::invalid_input(
            "level_idc is not defined in ITU-T H.264 Annex A Table A-1",
        )),
    }
}

/// profile_idc / profile-iop / level_idc から [`H264Level`] を決める
///
/// Level 1b の ad hoc 表現 (RFC 6184 Section 8.1 と Section 8.2.2 の
/// informative note、ITU-T H.264 7.4.2.1.1) を先に判定する。
///
/// - profile_idc が 66 / 77 / 88: `level_idc == 11` かつ
///   `constraint_set3_flag == 1` なら Level 1b、`level_idc == 11` かつ
///   `constraint_set3_flag == 0` なら Level 1.1。`level_idc == 9` は
///   Level 1b の合図ではないためエラー
/// - それ以外の Table 5 profile: `level_idc == 9` なら Level 1b、
///   `level_idc == 11` なら Level 1.1。`constraint_set3_flag` は
///   sub-profile 判定に使い、Level 1b 判定には使わない
fn parse_h264_level(profile_idc: u8, profile_iop: u8, level_idc: u8) -> Result<H264Level> {
    // constraint_set3_flag は profile-iop の bit 4
    // (RFC 6184 Section 8.1 の "bit 4 (constraint_set3_flag)" の表記どおり)
    let constraint_set3_flag = profile_iop & 0b0001_0000 != 0;
    if matches!(profile_idc, 66 | 77 | 88) {
        match level_idc {
            9 => Err(Error::invalid_input(
                "level_idc 9 does not indicate Level 1b for profile_idc 66 / 77 / 88",
            )),
            11 if constraint_set3_flag => Ok(H264Level::Level1b),
            _ => level_from_idc(level_idc),
        }
    } else {
        match level_idc {
            9 => Ok(H264Level::Level1b),
            _ => level_from_idc(level_idc),
        }
    }
}

/// H.264 の profile-level-id を 3 バイト (profile_idc / profile-iop / level_idc)
/// から [`H264ProfileLevel`] へ正規化する
///
/// # 入力の解釈
///
/// - `profile_idc`: SPS の `profile_idc` (ITU-T H.264 7.4.2.1.1)
/// - `profile_iop`: RFC 6184 Section 8.1 の profile-iop 1 バイト全体。
///   [`H264Sps::constraint_set_flags`] と同一レイアウト
///   (constraint_set0_flag から constraint_set5_flag と reserved_zero_2bits)
///   なので、[`parse_sps`] の 3 フィールドは変換せずに渡せる。
///   [`parse_sps`] の成功は本関数の成功を意味しない。`parse_sps` は Table 5
///   外の `profile_idc` も生値で返す
/// - `level_idc`: SPS の `level_idc`
///
/// sub-profile は RFC 6184 Section 8.1 Table 5 の 12 profile へ正規化する。
/// Table 5 の複数表現は同一の [`H264Profile`] になり、元の 3 バイトは
/// 保持しない (非可逆)。Level 1b は profile と profile-iop / level_idc の
/// 組み合わせで判定する。Annex A の profile 追加や `reserved_zero_2bits` の
/// 意味が将来変更される可能性があっても、本 API は Table 5 の 12 sub-profile
/// に固定する。
///
/// # エラー条件
///
/// - Table 5 に載っていない (profile_idc, profile-iop) の組み合わせ。
///   Constrained High や、RFC 6184 Section 8.1 が Note する multiple
///   profiles の common subset もここに含む
/// - Annex A Table A-1 の既知集合外の `level_idc`
/// - profile_idc が 66 / 77 / 88 のときの `level_idc == 9`
///
/// # 対象外
///
/// - SDP の level 大小比較、互換判定、`max-recv-level` の解釈
/// - C API / WASM バインディング
pub fn parse_profile_level_id(
    profile_idc: u8,
    profile_iop: u8,
    level_idc: u8,
) -> Result<H264ProfileLevel> {
    let profile = parse_h264_profile(profile_idc, profile_iop)?;
    let level = parse_h264_level(profile_idc, profile_iop, level_idc)?;
    Ok(H264ProfileLevel { profile, level })
}

/// RFC 4648 base16 の 1 文字を 4 bit へ変換する
///
/// `A-F` と `a-f` を受理し、それ以外は `None`
fn hex_char_to_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'F' => Some(c - b'A' + 10),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// SDP の profile-level-id 文字列を 3 バイトへデコードする
///
/// ちょうど 6 桁の RFC 4648 base16 だけを受理する (RFC 6184 Section 8.1)
fn decode_profile_level_id_hex(hex: &str) -> Result<[u8; 3]> {
    let bytes = hex.as_bytes();
    if bytes.len() != 6 {
        return Err(Error::invalid_input(
            "profile-level-id must be exactly 6 hexadecimal digits",
        ));
    }
    let mut out = [0u8; 3];
    for i in 0..3 {
        let hi = hex_char_to_nibble(bytes[i * 2])
            .ok_or_else(|| Error::invalid_input("profile-level-id contains a non-base16 digit"))?;
        let lo = hex_char_to_nibble(bytes[i * 2 + 1])
            .ok_or_else(|| Error::invalid_input("profile-level-id contains a non-base16 digit"))?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

/// SDP の profile-level-id 文字列を [`H264ProfileLevel`] へ正規化する
///
/// # 入力の解釈
///
/// ちょうど 6 桁の RFC 4648 base16 文字列 (RFC 6184 Section 8.1)。
/// `A-F` と `a-f` の両方を受理する。3 バイト (profile_idc / profile-iop /
/// level_idc) へデコードして [`parse_profile_level_id`] に渡す薄いラッパー。
///
/// # エラー条件
///
/// - 6 桁でない (桁数エラー)
/// - base16 でない文字を含む
/// - [`parse_profile_level_id`] のエラー条件
pub fn parse_profile_level_id_hex(hex: &str) -> Result<H264ProfileLevel> {
    let [profile_idc, profile_iop, level_idc] = decode_profile_level_id_hex(hex)?;
    parse_profile_level_id(profile_idc, profile_iop, level_idc)
}

/// [`Avc1Box`] の構築に必要な、ストリームから一意に決まらない設定値
///
/// profile / level / width / height / chroma / bit depth / SPS / PPS は
/// [`build_avc1_box`] 側で SPS / PPS の EBSP から導出するため、
/// 本構造体には含めない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct H264SampleEntryConfig {
    /// NAL 長フィールド幅
    pub length_size: LengthSize,
}

/// SPS / PPS の EBSP リストと設定値から [`Avc1Box`] を 1 つ構築する
///
/// [`SampleEntry`][crate::boxes::SampleEntry] には包まず [`Avc1Box`] をそのまま返す。
///
/// # 固定値 (関数側で埋める)
///
/// - [`VisualSampleEntryFields`] の `horizresolution` / `vertresolution` /
///   `frame_count` / `compressorname` / `depth`: 同構造体のデフォルト
/// - [`VisualSampleEntryFields::data_reference_index`] =
///   [`VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX`]
/// - [`Avc1Box::unknown_boxes`] = 空 `Vec`
/// - [`AvccBox::sps_ext_list`] = 空 `Vec`
/// - [`AvccBox`] の configurationVersion は 1 (encode 側が書く)
///
/// # ストリーム導出値 (先頭 SPS から写す)
///
/// - [`AvccBox::avc_profile_indication`] / [`AvccBox::profile_compatibility`] /
///   [`AvccBox::avc_level_indication`]: SPS の `profile_idc` / constraint フラグ
///   1 バイト全体 / `level_idc`
/// - [`AvccBox::sps_list`] / [`AvccBox::pps_list`]: 呼び出し側が渡した EBSP を、
///   開始コードを付けず、emulation prevention byte を残したまま格納する
/// - [`AvccBox::chroma_format`] / [`AvccBox::bit_depth_luma_minus8`] /
///   [`AvccBox::bit_depth_chroma_minus8`]: `profile_idc` が 66 / 77 / 88 なら
///   `None`、それ以外は SPS の値 (追加構文が無い `profile_idc` では推論値) の
///   `Some`。66 / 77 / 88 以外では [`Encode::encode`](crate::Encode::encode) がこの欄を
///   必須とするため、必ず `Some` にして encode が失敗しないようにする
/// - [`VisualSampleEntryFields::width`] / [`VisualSampleEntryFields::height`]:
///   先頭 SPS のクロップ適用後の値
///
/// # 呼び出し側指定値
///
/// - [`H264SampleEntryConfig::length_size`]: NAL 長フィールド幅 ([`LengthSize`])
///
/// # エラー条件
///
/// - SPS または PPS が 0 個
/// - SPS が 31 個超、PPS が 255 個超 (現行 [`Encode::encode`](crate::Encode::encode) の上限)
/// - SPS / PPS の NAL が `u16::MAX` バイト超 (avcC の長さ欄が 16 ビット)
/// - SPS が非空・NAL type 7 以外。PPS が非空・NAL type 8 以外
///   (構文解析は先頭 SPS だけ。PPS 構文は解析しない)
/// - 先頭 SPS の解析失敗 ([`parse_sps`] のエラー条件)
pub fn build_avc1_box(
    sps_list: &[Vec<u8>],
    pps_list: &[Vec<u8>],
    config: &H264SampleEntryConfig,
) -> Result<Avc1Box> {
    // SPS または PPS が 0 個なら Error
    let first_sps = sps_list
        .first()
        .ok_or_else(|| Error::invalid_input("SPS list must not be empty"))?;
    if pps_list.is_empty() {
        return Err(Error::invalid_input("PPS list must not be empty"));
    }

    // AvccBox::encode の上限をボックス encode に渡す前に検証する
    if sps_list.len() > MAX_SPS_COUNT {
        return Err(Error::invalid_input("too many SPSs (max 31)"));
    }
    if pps_list.len() > MAX_PPS_COUNT {
        return Err(Error::invalid_input("too many PPSs (max 255)"));
    }
    for sps in sps_list {
        if sps.len() > u16::MAX as usize {
            return Err(Error::invalid_input("SPS is too long (max u16::MAX)"));
        }
        // 全ての SPS を非空・NAL type 7 として検証する (PPS と同じ方針)。
        // 解析して代表値にするのは先頭 SPS だけでよい
        let Some(&header) = sps.first() else {
            return Err(Error::invalid_input("SPS must be a non-empty NAL unit"));
        };
        if header & 0b1000_0000 != 0 {
            return Err(Error::invalid_input("SPS forbidden_zero_bit must be 0"));
        }
        if H264NalUnitType::from_header_value(header & 0b0001_1111) != H264NalUnitType::Sps {
            return Err(Error::invalid_input("SPS NAL unit type must be 7"));
        }
    }
    for pps in pps_list {
        if pps.len() > u16::MAX as usize {
            return Err(Error::invalid_input("PPS is too long (max u16::MAX)"));
        }
    }

    // PPS は NAL type 8 であることと非空であることだけを検証する (PPS 構文は解析しない)
    for pps in pps_list {
        let Some(&header) = pps.first() else {
            return Err(Error::invalid_input("PPS must be a non-empty NAL unit"));
        };
        if header & 0b1000_0000 != 0 {
            return Err(Error::invalid_input("PPS forbidden_zero_bit must be 0"));
        }
        if H264NalUnitType::from_header_value(header & 0b0001_1111) != H264NalUnitType::Pps {
            return Err(Error::invalid_input("PPS NAL unit type must be 8"));
        }
    }

    // 先頭 SPS を解析して代表値にする
    let sps = parse_sps(first_sps)?;

    // 呼び出し側指定の長さフィールド幅を length_size_minus_one (0 / 1 / 3) へ写す。
    // 幅 3 (length_size_minus_one == 2) は ISO/IEC 14496-15 で reserved のため
    // LengthSize 型では表現できない
    let length_size_minus_one = Uint::new(config.length_size.length_size_minus_one());

    // AvccBox::encode は avc_profile_indication が 66 / 77 / 88 以外のとき
    // 追加欄 (chroma_format 等) を必須とする。SPS の値は 66 / 77 / 88 以外では
    // 追加構文の値か推論値が常に定まるため、必ず Some にして encode が
    // 必須欄欠落で失敗しないようにする
    let (chroma_format, bit_depth_luma_minus8, bit_depth_chroma_minus8) =
        if matches!(sps.profile_idc, 66 | 77 | 88) {
            (None, None, None)
        } else {
            (
                Some(Uint::new(sps.chroma_format_idc)),
                Some(Uint::new(sps.bit_depth_luma_minus8)),
                Some(Uint::new(sps.bit_depth_chroma_minus8)),
            )
        };

    let avcc_box = AvccBox {
        avc_profile_indication: sps.profile_idc,
        profile_compatibility: sps.constraint_set_flags,
        avc_level_indication: sps.level_idc,
        length_size_minus_one,
        sps_list: sps_list.to_vec(),
        pps_list: pps_list.to_vec(),
        chroma_format,
        bit_depth_luma_minus8,
        bit_depth_chroma_minus8,
        // SPS extension (type 13) の抽出は対象外のため空
        sps_ext_list: Vec::new(),
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

    Ok(Avc1Box {
        visual,
        avcc_box,
        unknown_boxes: Vec::new(),
    })
}

/// Annex B の入力から [`Avc1Box`] を構築する
///
/// [`parse_annexb_nal_units`] で列挙した NAL から type 7 (SPS) / type 8 (PPS)
/// を入力順で集め、[`build_avc1_box`] に渡す薄いラッパー。
/// SEI / IDR / AUD 等の他種別の NAL は無視する。
///
/// # エラー条件
///
/// - [`parse_annexb_nal_units`] のエラー条件
/// - SPS または PPS が 0 個 (Annex B に入っていない場合)
/// - [`build_avc1_box`] のエラー条件
pub fn build_avc1_box_from_annexb(input: &[u8], config: &H264SampleEntryConfig) -> Result<Avc1Box> {
    let nals = parse_annexb_nal_units(input)?;
    let sps_list: Vec<Vec<u8>> = collect_nal_units(nals.iter().copied(), H264NalUnitType::Sps)
        .into_iter()
        .map(|s| s.to_vec())
        .collect();
    let pps_list: Vec<Vec<u8>> = collect_nal_units(nals.iter().copied(), H264NalUnitType::Pps)
        .into_iter()
        .map(|s| s.to_vec())
        .collect();
    build_avc1_box(&sps_list, &pps_list, config)
}

/// EBSP から emulation prevention byte (0x03) を除いて RBSP を得る
///
/// ITU-T H.264 7.3.1 の構文ループと 7.4.1 の規定により、直前に 0x00 が
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

/// ITU-T H.264 7.3.2.1.1 の `scaling_list` 構文を読み飛ばす
///
/// `nextScale == 0` になると以降 `delta_scale` は読まれないため、
/// 状態を追跡しながらビット位置を進める
fn skip_scaling_list(reader: &mut SpsBitReader<'_>, size_of_scaling_list: u32) -> Result<()> {
    let mut last_scale: i64 = 8;
    let mut next_scale: i64 = 8;
    for _ in 0..size_of_scaling_list {
        if next_scale != 0 {
            let delta_scale = reader.read_se()?;
            next_scale = (last_scale + delta_scale + 256) % 256;
        }
        last_scale = if next_scale == 0 {
            last_scale
        } else {
            next_scale
        };
    }
    Ok(())
}

/// SPS の RBSP を読む MSB-first ビットリーダー
///
/// `u(n)` の固定長フィールドと、Exp-Golomb の `ue(v)` / `se(v)` を読む。
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

    fn read_bits(&mut self, n: u32) -> Result<u32> {
        // 32 ビット超は SPS の構文要素にない。u64 で累積して 32 ビットへ丸める
        let mut value: u64 = 0;
        for _ in 0..n {
            value = (value << 1) | u64::from(self.read_bit()?);
        }
        Ok(value as u32)
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
        let code_num = (1u64 << zeros) - 1 + u64::from(suffix);
        u32::try_from(code_num)
            .map_err(|_| Error::invalid_input("Exp-Golomb code value is too large"))
    }

    /// 符号付き Exp-Golomb (`se(v)`) を読む
    ///
    /// `codeNum` が偶数なら `-codeNum / 2`、奇数なら `(codeNum + 1) / 2`
    fn read_se(&mut self) -> Result<i64> {
        let code_num = self.read_ue()?;
        let magnitude = u64::from(code_num).div_ceil(2);
        let signed = if code_num % 2 == 0 {
            -(magnitude as i64)
        } else {
            magnitude as i64
        };
        Ok(signed)
    }
}
