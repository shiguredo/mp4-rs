//! AV1 ビットストリーム処理ユーティリティ
//!
//! Low Overhead Bitstream Format の OBU 列、Sequence Header OBU、
//! フレームヘッダー先頭部を解析し、`av01` / `av1C` の構築に必要な
//! ストリーム情報を得る API を提供する。
//!
//! 参照仕様は以下のとおり。
//!
//! - AV1 Bitstream & Decoding Process Specification
//!   <https://aomediacodec.github.io/av1-spec/>
//! - AV1 Codec ISO Media File Format Binding v1.3.0
//!   <https://aomediacodec.github.io/av1-isobmff/>

use alloc::vec::Vec;

use crate::{
    Error, Result, Uint,
    boxes::{Av01Box, Av1cBox, VisualSampleEntryFields},
};

/// AV1 spec §6.2.2 の `obu_type` 値
const OBU_TYPE_SEQUENCE_HEADER: u8 = 1;
const OBU_TYPE_TEMPORAL_DELIMITER: u8 = 2;
const OBU_TYPE_FRAME_HEADER: u8 = 3;
const OBU_TYPE_TILE_GROUP: u8 = 4;
const OBU_TYPE_METADATA: u8 = 5;
const OBU_TYPE_FRAME: u8 = 6;
const OBU_TYPE_REDUNDANT_FRAME_HEADER: u8 = 7;
const OBU_TYPE_TILE_LIST: u8 = 8;
const OBU_TYPE_PADDING: u8 = 15;

/// AV1 spec §6.4.2 の `CP_BT_709`
const CP_BT_709: u8 = 1;

/// AV1 spec §6.4.2 の `TC_SRGB`
const TC_SRGB: u8 = 13;

/// AV1 spec §6.4.2 の `MC_IDENTITY`
const MC_IDENTITY: u8 = 0;

/// AV1 spec §5.9.2 の `KEY_FRAME`
const FRAME_TYPE_KEY: u8 = 0;

/// AV1 spec §5.9.2 の `INTER_FRAME`
const FRAME_TYPE_INTER: u8 = 1;

/// AV1 spec §5.9.2 の `INTRA_ONLY_FRAME`
const FRAME_TYPE_INTRA_ONLY: u8 = 2;

/// AV1 spec §5.9.2 の `SWITCH_FRAME`
const FRAME_TYPE_SWITCH: u8 = 3;

/// Visual Sample Entry の幅・高さ欄 (`u16`) に収まる最大値
const VISUAL_SAMPLE_ENTRY_DIM_MAX: u32 = u16::MAX as u32;

/// OBU 列の解析コンテキスト
///
/// AV1 Codec ISO Media File Format Binding v1.3.0 は `av1C` の `configOBUs` と
/// MP4 サンプルで `obu_has_size_field` の規則が異なる。真偽値ではなくこの enum で区別する
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Av1ObuParseContext {
    /// `av1C` の `configOBUs` (Binding §2.3.4)
    ///
    /// すべての OBU で `obu_has_size_field = 1` が必須。空入力は許容する
    ConfigObus,

    /// MP4 サンプル (Binding §2.4)
    ///
    /// 最後以外の OBU は `obu_has_size_field = 1` が必須。最後の OBU だけ省略でき、
    /// 省略時はサンプル末尾までを payload とする。サイズを省略した時点でその OBU が
    /// 列の最後になり、後続バイトはすべて payload に吸収されるため、「最後以外の
    /// OBU が省略した」ことはこのパーサーでは検出できない。空入力は拒否する
    Sample,
}

/// AV1 spec §6.2.2 の `obu_type`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Av1ObuType {
    /// 予約値 (0 および 9..=14)。payload は解釈せずサイズで読み飛ばす
    Reserved(u8),
    /// Sequence Header OBU
    SequenceHeader,
    /// Temporal Delimiter OBU
    TemporalDelimiter,
    /// Frame Header OBU
    FrameHeader,
    /// Tile Group OBU
    TileGroup,
    /// Metadata OBU
    Metadata,
    /// Frame OBU (`frame_header_obu` + `tile_group_obu`)
    Frame,
    /// Redundant Frame Header OBU
    RedundantFrameHeader,
    /// Tile List OBU。Binding はこの版でサポートせず、サンプルでは SHALL NOT
    TileList,
    /// Padding OBU
    Padding,
}

impl Av1ObuType {
    fn from_header_value(value: u8) -> Self {
        match value {
            OBU_TYPE_SEQUENCE_HEADER => Self::SequenceHeader,
            OBU_TYPE_TEMPORAL_DELIMITER => Self::TemporalDelimiter,
            OBU_TYPE_FRAME_HEADER => Self::FrameHeader,
            OBU_TYPE_TILE_GROUP => Self::TileGroup,
            OBU_TYPE_METADATA => Self::Metadata,
            OBU_TYPE_FRAME => Self::Frame,
            OBU_TYPE_REDUNDANT_FRAME_HEADER => Self::RedundantFrameHeader,
            OBU_TYPE_TILE_LIST => Self::TileList,
            OBU_TYPE_PADDING => Self::Padding,
            // `obu_type` は 4 ビットなので残りは 0 と 9..=14
            other => Self::Reserved(other),
        }
    }
}

/// 1 個の OBU の借用範囲
///
/// `header` は OBU header (extension header があればそれを含む) で、`obu_size` の
/// LEB128 は含まない。`payload` は `obu_size` が指すバイト列。`obu` はヘッダーから
/// payload 末尾までのこの OBU 全体
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Av1Obu<'a> {
    /// OBU 種別
    pub obu_type: Av1ObuType,
    /// extension header の `temporal_id`。無いときは 0
    pub temporal_id: u8,
    /// extension header の `spatial_id`。無いときは 0
    pub spatial_id: u8,
    /// OBU header バイト列 (1 または 2 バイト)
    pub header: &'a [u8],
    /// OBU payload
    pub payload: &'a [u8],
    /// この OBU 全体 (header + 任意の size field + payload)
    pub obu: &'a [u8],
}

/// Sequence Header OBU から `av1C` / `av01` とフレーム先頭部に必要な情報
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Av1SequenceHeader {
    /// `seq_profile` (0..=2)
    pub seq_profile: u8,
    /// `seq_level_idx[0]` (0..=31)
    pub seq_level_idx_0: u8,
    /// `seq_tier[0]` (0 または 1)。`seq_level_idx[0] <= 7` のときは構文上 0
    pub seq_tier_0: u8,
    /// `high_bitdepth`
    pub high_bitdepth: bool,
    /// `twelve_bit`。構文に現れないときは 0
    pub twelve_bit: bool,
    /// `mono_chrome`
    pub monochrome: bool,
    /// `subsampling_x` (0 または 1)
    pub chroma_subsampling_x: u8,
    /// `subsampling_y` (0 または 1)
    pub chroma_subsampling_y: u8,
    /// `chroma_sample_position` (0..=3)。構文に現れないときは 0
    pub chroma_sample_position: u8,
    /// `max_frame_width_minus_1 + 1` (1..=65536)
    pub max_frame_width: u32,
    /// `max_frame_height_minus_1 + 1` (1..=65536)
    pub max_frame_height: u32,
    /// `reduced_still_picture_header`。フレーム先頭部の代入経路に使う
    pub reduced_still_picture_header: bool,
}

/// uncompressed header 先頭部の RAP 判定用フィールド
///
/// Binding §2.4 の sync sample は、先頭フレームが Key かつ `show_frame = 1` であることと、
/// Sequence Header が最初の Frame Header より前にあることを要求する
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Av1FrameHeaderPrefix {
    /// `show_existing_frame`。true のときこのヘッダーは RAP にならない
    pub show_existing_frame: bool,
    /// `frame_type`。`show_existing_frame == true` のときはヘッダーに現れないので `None`
    pub frame_type: Option<Av1FrameType>,
    /// `show_frame`。`show_existing_frame == true` のときはヘッダーに現れないので `None`
    pub show_frame: Option<bool>,
}

/// AV1 spec §6.8.2 の `frame_type`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Av1FrameType {
    /// KEY_FRAME
    Key,
    /// INTER_FRAME
    Inter,
    /// INTRA_ONLY_FRAME
    IntraOnly,
    /// SWITCH_FRAME
    Switch,
}

/// [`build_av01_box`] の呼び出し側指定値
///
/// Sequence Header だけでは一意に決まらない `initial_presentation_delay_minus_one` だけを持つ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Av1SampleEntryConfig {
    /// Binding の `initial_presentation_delay_minus_one` (0..=15)
    ///
    /// `None` は `initial_presentation_delay_present = 0` として書き込む
    pub initial_presentation_delay_minus_one: Option<u8>,
}

/// AV1 spec §4.10.5 の `leb128()` をデコードする
///
/// # 戻り値
///
/// `(値, 消費バイト数)`。非最短表現も受理する
///
/// # エラー条件
///
/// - 終端ビットが来る前に入力が尽きた
/// - 8 バイト目の continuation bit が 1
/// - 値が `(1 << 32) - 1` を超える
pub fn decode_leb128(input: &[u8]) -> Result<(u32, usize)> {
    let mut value: u64 = 0;
    for i in 0..8 {
        let Some(&byte) = input.get(i) else {
            return Err(Error::invalid_input("AV1 leb128 is truncated"));
        };
        value |= u64::from(byte & 0x7F) << (i * 7);
        if byte & 0x80 == 0 {
            if value > u64::from(u32::MAX) {
                return Err(Error::invalid_input(
                    "AV1 leb128 value exceeds (1 << 32) - 1",
                ));
            }
            return Ok((value as u32, i + 1));
        }
    }
    // 8 バイトとも continuation bit が立っている (8 バイト目の MSB が 1)
    Err(Error::invalid_input(
        "AV1 leb128 continuation bit must be 0 on the 8th byte",
    ))
}

/// Low Overhead Bitstream Format の OBU 列を解析する
///
/// # 入力
///
/// - `input`: OBU 列のバイト列
/// - `ctx`: [`Av1ObuParseContext`]。サイズフィールド規則がコンテキストで異なる
///
/// # 保持するバイト範囲
///
/// 各 [`Av1Obu`] のスライスはすべて `input` への借用である
///
/// # エラー条件
///
/// - [`Av1ObuParseContext::Sample`] で空入力
/// - [`Av1ObuParseContext::ConfigObus`] で `obu_has_size_field = 0`
/// - `obu_forbidden_bit` / `obu_reserved_1bit` / `extension_header_reserved_3bits` が 0 でない
/// - Sequence Header OBU が `obu_extension_flag = 1`
/// - `OBU_TILE_LIST` (Binding はこの版で未サポート、サンプルでは SHALL NOT)
/// - extension header が入力末尾で欠ける
/// - LEB128 が入力末尾で欠ける、8 バイト目の continuation bit が 1、または値が `(1 << 32) - 1` を超える
/// - 宣言サイズが `usize` を溢れる、または残バイトを超える
///
/// 予約済み `obu_type` (0 および 9..=14) はサイズを使って読み飛ばし、列挙結果に含める。
/// `OBU_TEMPORAL_DELIMITER` / `OBU_PADDING` / `OBU_REDUNDANT_FRAME_HEADER` は SHOULD NOT
/// でも構文としては受理する
pub fn parse_obus(input: &[u8], ctx: Av1ObuParseContext) -> Result<Vec<Av1Obu<'_>>> {
    if input.is_empty() {
        return match ctx {
            Av1ObuParseContext::ConfigObus => Ok(Vec::new()),
            Av1ObuParseContext::Sample => Err(Error::invalid_input(
                "AV1 sample must contain a Temporal Unit (empty input is rejected)",
            )),
        };
    }

    let mut obus = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        let obu = parse_one_obu(&input[pos..], ctx)?;
        let consumed = obu.obu.len();
        // スライスを元の `input` 基準に張り直す
        obus.push(Av1Obu {
            obu_type: obu.obu_type,
            temporal_id: obu.temporal_id,
            spatial_id: obu.spatial_id,
            header: &input[pos..pos + obu.header.len()],
            payload: {
                let payload_start = pos + consumed - obu.payload.len();
                &input[payload_start..pos + consumed]
            },
            obu: &input[pos..pos + consumed],
        });
        pos += consumed;
    }
    Ok(obus)
}

/// `remaining` 先頭から 1 OBU を読む。戻り値のスライスは `remaining` への借用
fn parse_one_obu(remaining: &[u8], ctx: Av1ObuParseContext) -> Result<Av1Obu<'_>> {
    if remaining.is_empty() {
        return Err(Error::invalid_input("AV1 OBU header is truncated"));
    }

    let header0 = remaining[0];
    let forbidden = (header0 >> 7) & 1;
    let obu_type_value = (header0 >> 3) & 0x0F;
    let extension_flag = (header0 >> 2) & 1;
    let has_size_field = (header0 >> 1) & 1;
    let reserved = header0 & 1;

    if forbidden != 0 {
        return Err(Error::invalid_input("AV1 obu_forbidden_bit must be 0"));
    }
    if reserved != 0 {
        return Err(Error::invalid_input("AV1 obu_reserved_1bit must be 0"));
    }

    let mut header_len = 1usize;
    let mut temporal_id = 0u8;
    let mut spatial_id = 0u8;
    if extension_flag == 1 {
        let Some(&ext) = remaining.get(1) else {
            return Err(Error::invalid_input(
                "AV1 obu_extension_header is truncated",
            ));
        };
        temporal_id = ext >> 5;
        spatial_id = (ext >> 3) & 0x03;
        let ext_reserved = ext & 0x07;
        if ext_reserved != 0 {
            return Err(Error::invalid_input(
                "AV1 extension_header_reserved_3bits must be 0",
            ));
        }
        header_len = 2;
    }

    let obu_type = Av1ObuType::from_header_value(obu_type_value);
    if matches!(obu_type, Av1ObuType::TileList) {
        // Binding §1 NOTE: この版は Tile List をサポートしない。
        // Binding §2.4: サンプルでは OBU_TILE_LIST は SHALL NOT。
        // 根拠は Binding であり、RTP / libwebrtc の除外理由ではない
        return Err(Error::invalid_input(
            "AV1 OBU_TILE_LIST is not supported by AV1 Codec ISO Media File Format Binding",
        ));
    }
    if matches!(obu_type, Av1ObuType::SequenceHeader) && extension_flag == 1 {
        return Err(Error::invalid_input(
            "AV1 Sequence Header OBU must have obu_extension_flag equal to 0",
        ));
    }

    let after_header = header_len;
    let (payload, obu_end) = if has_size_field == 1 {
        let (size, leb_len) = decode_leb128(&remaining[after_header..])?;
        let payload_start = after_header + leb_len;
        let payload_end = payload_start
            .checked_add(size as usize)
            .ok_or_else(|| Error::invalid_input("AV1 obu_size overflows usize"))?;
        if payload_end > remaining.len() {
            return Err(Error::invalid_input(
                "AV1 obu_size exceeds remaining input bytes",
            ));
        }
        (&remaining[payload_start..payload_end], payload_end)
    } else {
        match ctx {
            Av1ObuParseContext::ConfigObus => {
                return Err(Error::invalid_input(
                    "AV1 configOBUs requires obu_has_size_field = 1 for every OBU",
                ));
            }
            Av1ObuParseContext::Sample => {
                // Binding §2.4: 最後の OBU だけ size 省略 MAY。省略すると残り全部が
                // payload になるので、この OBU が列の最後になる
                (&remaining[after_header..], remaining.len())
            }
        }
    };

    Ok(Av1Obu {
        obu_type,
        temporal_id,
        spatial_id,
        header: &remaining[..header_len],
        payload,
        obu: &remaining[..obu_end],
    })
}

/// Sequence Header OBU の payload を解析する
///
/// payload は OBU header / size field を除いた Sequence Header 本体。
/// 後続の trailing bits は読み残してよい
///
/// # エラー条件
///
/// - 入力不足
/// - `seq_profile` が 3..=7 (予約)
/// - `reduced_still_picture_header == 1` かつ `still_picture == 0`
pub fn parse_sequence_header(payload: &[u8]) -> Result<Av1SequenceHeader> {
    let mut reader = BitReader::new(payload);

    let seq_profile = reader.read_bits(3)? as u8;
    if seq_profile > 2 {
        return Err(Error::invalid_input(
            "AV1 seq_profile 3..=7 is reserved (must be 0..=2)",
        ));
    }
    let still_picture = reader.read_bit()? != 0;
    let reduced_still_picture_header = reader.read_bit()? != 0;
    if reduced_still_picture_header && !still_picture {
        return Err(Error::invalid_input(
            "AV1 reduced_still_picture_header requires still_picture = 1",
        ));
    }

    let mut seq_level_idx_0 = 0u8;
    let mut seq_tier_0 = 0u8;
    let mut buffer_delay_length_minus_1 = 0u8;

    if reduced_still_picture_header {
        seq_level_idx_0 = reader.read_bits(5)? as u8;
    } else {
        let timing_info_present_flag = reader.read_bit()? != 0;
        let mut decoder_model_info_present_flag = false;
        if timing_info_present_flag {
            skip_timing_info(&mut reader)?;
            decoder_model_info_present_flag = reader.read_bit()? != 0;
            if decoder_model_info_present_flag {
                buffer_delay_length_minus_1 = reader.read_bits(5)? as u8;
                let _num_units_in_decoding_tick = reader.read_bits(32)?;
                let _buffer_removal_time_length_minus_1 = reader.read_bits(5)?;
                let _frame_presentation_time_length_minus_1 = reader.read_bits(5)?;
            }
        }
        let initial_display_delay_present_flag = reader.read_bit()? != 0;
        let operating_points_cnt_minus_1 = reader.read_bits(5)? as u8;
        for i in 0..=operating_points_cnt_minus_1 {
            let _operating_point_idc = reader.read_bits(12)?;
            let seq_level_idx = reader.read_bits(5)? as u8;
            let seq_tier = if seq_level_idx > 7 {
                reader.read_bit()?
            } else {
                0
            };
            if i == 0 {
                seq_level_idx_0 = seq_level_idx;
                seq_tier_0 = seq_tier;
            }
            if decoder_model_info_present_flag {
                let decoder_model_present_for_this_op = reader.read_bit()? != 0;
                if decoder_model_present_for_this_op {
                    let n = u32::from(buffer_delay_length_minus_1) + 1;
                    let _decoder_buffer_delay = reader.read_bits(n)?;
                    let _encoder_buffer_delay = reader.read_bits(n)?;
                    let _low_delay_mode_flag = reader.read_bit()?;
                }
            }
            if initial_display_delay_present_flag {
                let present = reader.read_bit()? != 0;
                if present {
                    let _initial_display_delay_minus_1 = reader.read_bits(4)?;
                }
            }
        }
    }

    let frame_width_bits_minus_1 = reader.read_bits(4)?;
    let frame_height_bits_minus_1 = reader.read_bits(4)?;
    let max_frame_width_minus_1 = reader.read_bits(frame_width_bits_minus_1 + 1)?;
    let max_frame_height_minus_1 = reader.read_bits(frame_height_bits_minus_1 + 1)?;
    let max_frame_width = max_frame_width_minus_1 + 1;
    let max_frame_height = max_frame_height_minus_1 + 1;

    if !reduced_still_picture_header {
        let frame_id_numbers_present_flag = reader.read_bit()? != 0;
        if frame_id_numbers_present_flag {
            let _delta_frame_id_length_minus_2 = reader.read_bits(4)?;
            let _additional_frame_id_length_minus_1 = reader.read_bits(3)?;
        }
    }

    let _use_128x128_superblock = reader.read_bit()?;
    let _enable_filter_intra = reader.read_bit()?;
    let _enable_intra_edge_filter = reader.read_bit()?;

    if !reduced_still_picture_header {
        let _enable_interintra_compound = reader.read_bit()?;
        let _enable_masked_compound = reader.read_bit()?;
        let _enable_warped_motion = reader.read_bit()?;
        let _enable_dual_filter = reader.read_bit()?;
        let enable_order_hint = reader.read_bit()? != 0;
        if enable_order_hint {
            let _enable_jnt_comp = reader.read_bit()?;
            let _enable_ref_frame_mvs = reader.read_bit()?;
        }
        let seq_choose_screen_content_tools = reader.read_bit()? != 0;
        let seq_force_screen_content_tools = if seq_choose_screen_content_tools {
            2
        } else {
            reader.read_bit()?
        };
        if seq_force_screen_content_tools > 0 {
            let seq_choose_integer_mv = reader.read_bit()? != 0;
            if !seq_choose_integer_mv {
                let _seq_force_integer_mv = reader.read_bit()?;
            }
        }
        if enable_order_hint {
            let _order_hint_bits_minus_1 = reader.read_bits(3)?;
        }
    }

    let _enable_superres = reader.read_bit()?;
    let _enable_cdef = reader.read_bit()?;
    let _enable_restoration = reader.read_bit()?;

    let color = read_color_config(&mut reader, seq_profile)?;
    let _film_grain_params_present = reader.read_bit()?;

    Ok(Av1SequenceHeader {
        seq_profile,
        seq_level_idx_0,
        seq_tier_0,
        high_bitdepth: color.high_bitdepth,
        twelve_bit: color.twelve_bit,
        monochrome: color.monochrome,
        chroma_subsampling_x: color.subsampling_x,
        chroma_subsampling_y: color.subsampling_y,
        chroma_sample_position: color.chroma_sample_position,
        max_frame_width,
        max_frame_height,
        reduced_still_picture_header,
    })
}

struct ColorConfig {
    high_bitdepth: bool,
    twelve_bit: bool,
    monochrome: bool,
    subsampling_x: u8,
    subsampling_y: u8,
    chroma_sample_position: u8,
}

fn read_color_config(reader: &mut BitReader<'_>, seq_profile: u8) -> Result<ColorConfig> {
    let high_bitdepth = reader.read_bit()? != 0;
    let mut twelve_bit = false;
    let bit_depth = if seq_profile == 2 && high_bitdepth {
        twelve_bit = reader.read_bit()? != 0;
        if twelve_bit { 12 } else { 10 }
    } else if seq_profile <= 2 {
        if high_bitdepth { 10 } else { 8 }
    } else {
        8
    };

    let monochrome = if seq_profile == 1 {
        false
    } else {
        reader.read_bit()? != 0
    };

    let color_description_present_flag = reader.read_bit()? != 0;
    let (color_primaries, transfer_characteristics, matrix_coefficients) =
        if color_description_present_flag {
            (
                reader.read_bits(8)? as u8,
                reader.read_bits(8)? as u8,
                reader.read_bits(8)? as u8,
            )
        } else {
            (2, 2, 2)
        };

    if monochrome {
        let _color_range = reader.read_bit()?;
        return Ok(ColorConfig {
            high_bitdepth,
            twelve_bit,
            monochrome: true,
            subsampling_x: 1,
            subsampling_y: 1,
            chroma_sample_position: 0,
        });
    }

    let (subsampling_x, subsampling_y) = if color_primaries == CP_BT_709
        && transfer_characteristics == TC_SRGB
        && matrix_coefficients == MC_IDENTITY
    {
        (0, 0)
    } else {
        let _color_range = reader.read_bit()?;
        if seq_profile == 0 {
            (1, 1)
        } else if seq_profile == 1 {
            (0, 0)
        } else if bit_depth == 12 {
            let sx = reader.read_bit()?;
            let sy = if sx == 1 { reader.read_bit()? } else { 0 };
            (sx, sy)
        } else {
            (1, 0)
        }
    };

    let chroma_sample_position = if subsampling_x == 1 && subsampling_y == 1 {
        reader.read_bits(2)? as u8
    } else {
        0
    };
    let _separate_uv_delta_q = reader.read_bit()?;

    Ok(ColorConfig {
        high_bitdepth,
        twelve_bit,
        monochrome: false,
        subsampling_x,
        subsampling_y,
        chroma_sample_position,
    })
}

fn skip_timing_info(reader: &mut BitReader<'_>) -> Result<()> {
    let _num_units_in_display_tick = reader.read_bits(32)?;
    let _time_scale = reader.read_bits(32)?;
    let equal_picture_interval = reader.read_bit()? != 0;
    if equal_picture_interval {
        read_uvlc(reader)?;
    }
    Ok(())
}

/// AV1 spec §4.10.3 の `uvlc()`
fn read_uvlc(reader: &mut BitReader<'_>) -> Result<u32> {
    let mut leading_zeros = 0u32;
    loop {
        let done = reader.read_bit()?;
        if done == 1 {
            break;
        }
        leading_zeros += 1;
        // 仕様は done まで読む。無限 0 を避けるため上限を設ける
        if leading_zeros > 64 {
            return Err(Error::invalid_input("AV1 uvlc leading zeros exceed 64"));
        }
    }
    if leading_zeros >= 32 {
        return Ok(u32::MAX);
    }
    let value = reader.read_bits(leading_zeros)?;
    Ok(value + (1 << leading_zeros) - 1)
}

/// `OBU_FRAME_HEADER` または `OBU_FRAME` の payload 先頭 (uncompressed header) を解析する
///
/// RAP 判定に必要な `show_existing_frame` / `frame_type` / `show_frame` だけを返す。
/// `reduced_still_picture_header == 1` のときは AV1 spec §5.9.2 の代入値を使う。
/// `show_existing_frame == 1` のときは同構文が早期 return するため、
/// `frame_type` / `show_frame` は `None` にする
pub fn parse_frame_header_prefix(
    payload: &[u8],
    seq: &Av1SequenceHeader,
) -> Result<Av1FrameHeaderPrefix> {
    if seq.reduced_still_picture_header {
        return Ok(Av1FrameHeaderPrefix {
            show_existing_frame: false,
            frame_type: Some(Av1FrameType::Key),
            show_frame: Some(true),
        });
    }

    let mut reader = BitReader::new(payload);
    let show_existing_frame = reader.read_bit()? != 0;
    if show_existing_frame {
        return Ok(Av1FrameHeaderPrefix {
            show_existing_frame: true,
            frame_type: None,
            show_frame: None,
        });
    }

    let frame_type = match reader.read_bits(2)? as u8 {
        FRAME_TYPE_KEY => Av1FrameType::Key,
        FRAME_TYPE_INTER => Av1FrameType::Inter,
        FRAME_TYPE_INTRA_ONLY => Av1FrameType::IntraOnly,
        FRAME_TYPE_SWITCH => Av1FrameType::Switch,
        _ => unreachable!("frame_type is 2 bits"),
    };
    let show_frame = reader.read_bit()? != 0;
    Ok(Av1FrameHeaderPrefix {
        show_existing_frame: false,
        frame_type: Some(frame_type),
        show_frame: Some(show_frame),
    })
}

/// [`Av01Box`] を構築する
///
/// # 固定値
///
/// - [`VisualSampleEntryFields`] の `horizresolution` / `vertresolution` /
///   `frame_count` / `compressorname` / `depth`: 同構造体のデフォルト
///   (`NULL_COMPRESSORNAME` を含む。Binding の `"\012AOM Coding"` は RECOMMENDED)
/// - [`VisualSampleEntryFields::data_reference_index`] =
///   [`VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX`]
/// - [`Av01Box::unknown_boxes`] = 空 `Vec`
/// - `av1C` の marker / version は `Av1cBox` のエンコード実装が書く
///
/// # ストリーム導出値 (`seq` から写す)
///
/// - [`Av1cBox`] の profile / level / tier / bitdepth / monochrome / chroma 欄
/// - [`VisualSampleEntryFields::width`] / [`VisualSampleEntryFields::height`] =
///   `max_frame_width` / `max_frame_height` (Binding §2.2.4 の SHALL)
///
/// # 呼び出し側指定値
///
/// - [`Av1SampleEntryConfig::initial_presentation_delay_minus_one`]
/// - `config_obus`: 構築前に [`Av1ObuParseContext::ConfigObus`] で解析する
///
/// # エラー条件
///
/// - `config_obus` が ConfigObus 規則に違反する
/// - Sequence Header OBU が 2 個以上、または先頭以外にある
/// - `config_obus` 内の Sequence Header が `seq` と一致しない
/// - `max_frame_width` / `max_frame_height` が 1..=65535 の範囲外 (0 または 65536 以上。Visual Sample Entry の `u16` に入らない)
/// - `initial_presentation_delay_minus_one` が 16 以上
/// - `seq` の欄が `Av1cBox` のビット幅に収まらない
pub fn build_av01_box(
    seq: &Av1SequenceHeader,
    config_obus: &[u8],
    config: &Av1SampleEntryConfig,
) -> Result<Av01Box> {
    let obus = parse_obus(config_obus, Av1ObuParseContext::ConfigObus)?;
    let mut seen_sequence_header = false;
    for (index, obu) in obus.iter().enumerate() {
        if matches!(obu.obu_type, Av1ObuType::SequenceHeader) {
            if seen_sequence_header {
                return Err(Error::invalid_input(
                    "AV1 configOBUs must contain at most one Sequence Header OBU",
                ));
            }
            if index != 0 {
                return Err(Error::invalid_input(
                    "AV1 configOBUs Sequence Header OBU must be the first OBU when present",
                ));
            }
            seen_sequence_header = true;
            let parsed = parse_sequence_header(obu.payload)?;
            if parsed != *seq {
                return Err(Error::invalid_input(
                    "AV1 configOBUs Sequence Header does not match the provided Av1SequenceHeader",
                ));
            }
        }
    }

    if seq.max_frame_width == 0
        || seq.max_frame_height == 0
        || seq.max_frame_width > VISUAL_SAMPLE_ENTRY_DIM_MAX
        || seq.max_frame_height > VISUAL_SAMPLE_ENTRY_DIM_MAX
    {
        return Err(Error::invalid_input(
            "AV1 max_frame_width / max_frame_height must be 1..=65535 to fit VisualSampleEntry",
        ));
    }

    if seq.seq_profile > 2 {
        return Err(Error::invalid_input("AV1 seq_profile must be 0..=2"));
    }
    if seq.seq_level_idx_0 > 31 {
        return Err(Error::invalid_input("AV1 seq_level_idx_0 must be 0..=31"));
    }
    if seq.seq_tier_0 > 1 {
        return Err(Error::invalid_input("AV1 seq_tier_0 must be 0 or 1"));
    }
    if seq.chroma_subsampling_x > 1 || seq.chroma_subsampling_y > 1 {
        return Err(Error::invalid_input(
            "AV1 chroma_subsampling_x / chroma_subsampling_y must be 0 or 1",
        ));
    }
    if seq.chroma_sample_position > 3 {
        return Err(Error::invalid_input(
            "AV1 chroma_sample_position must be 0..=3",
        ));
    }

    let initial_presentation_delay_minus_one = match config.initial_presentation_delay_minus_one {
        None => None,
        Some(v) if v <= 15 => Some(Uint::new(v)),
        Some(_) => {
            return Err(Error::invalid_input(
                "AV1 initial_presentation_delay_minus_one must be 0..=15",
            ));
        }
    };

    let av1c_box = Av1cBox {
        seq_profile: Uint::new(seq.seq_profile),
        seq_level_idx_0: Uint::new(seq.seq_level_idx_0),
        seq_tier_0: Uint::new(seq.seq_tier_0),
        high_bitdepth: Uint::new(u8::from(seq.high_bitdepth)),
        twelve_bit: Uint::new(u8::from(seq.twelve_bit)),
        monochrome: Uint::new(u8::from(seq.monochrome)),
        chroma_subsampling_x: Uint::new(seq.chroma_subsampling_x),
        chroma_subsampling_y: Uint::new(seq.chroma_subsampling_y),
        chroma_sample_position: Uint::new(seq.chroma_sample_position),
        initial_presentation_delay_minus_one,
        config_obus: config_obus.to_vec(),
    };

    let visual = VisualSampleEntryFields {
        data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
        width: seq.max_frame_width as u16,
        height: seq.max_frame_height as u16,
        horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
        vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
        frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
        compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
        depth: VisualSampleEntryFields::DEFAULT_DEPTH,
    };

    Ok(Av01Box {
        visual,
        av1c_box,
        unknown_boxes: Vec::new(),
    })
}

/// AV1 uncompressed header / Sequence Header の MSB-first ビット読み取り
struct BitReader<'a> {
    input: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bit(&mut self) -> Result<u8> {
        if self.byte_pos >= self.input.len() {
            return Err(Error::invalid_input("AV1 bitstream is truncated"));
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
        let mut value: u32 = 0;
        for _ in 0..n {
            value = (value << 1) | u32::from(self.read_bit()?);
        }
        Ok(value)
    }
}
