//! VP9 ビットストリーム処理ユーティリティ
//!
//! VP9 フレームの uncompressed header 部分を解析し、profile / bit_depth /
//! 色・クロマ情報 / フレーム種別 / 解像度など `vp09` / `vpcC` の構築に必要な
//! ストリーム情報を得る API を提供する。
//!
//! 参照仕様は以下のとおり。
//!
//! - WebM Project 「VP9 Bitstream and Decoding Process Specification」
//!   <https://www.webmproject.org/vp9/> (uncompressed_header syntax は Section 6.2)
//! - VP Codec ISO Media File Format Binding <https://www.webmproject.org/vp9/mp4/>

use alloc::vec::Vec;

use crate::{
    Error, Result, Uint,
    boxes::{VisualSampleEntryFields, Vp09Box, VpccBox},
};

/// VP9 の `frame_marker` (VP9 spec Section 6.2 で常に 2)
const FRAME_MARKER: u32 = 2;

/// VP9 のキーフレーム / intra-only フレームで frame tag 直後に現れる sync code
///
/// VP9 spec Section 6.2 の `frame_sync_code` フィールドで、`0x49 0x83 0x42` の
/// 24 ビット固定バイト列でなければならない
const FRAME_SYNC_CODE: [u8; 3] = [0x49, 0x83, 0x42];

/// [`Vp9FrameHeader::color_space`] の sRGB 値
///
/// VP9 spec Section 7.2.2 の `CS_RGB = 7`。色空間が sRGB のときは color_range が
/// 常に full range 扱いになり、chroma subsampling が 4:4:4 固定になる
const COLOR_SPACE_SRGB: u8 = 7;

/// VP9 のフレーム種別
///
/// VP9 spec Section 6.2 の `frame_type` フィールドに対応する
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vp9FrameType {
    /// キーフレーム (frame_type = 0)
    Key,
    /// non-key frame (frame_type = 1)。inter frame と intra-only frame の両方を含む
    NonKey,
}

/// VP9 のフレームサイズ表現
///
/// inter frame の `frame_size_with_refs` 経路 (VP9 spec Section 6.2.5) では、
/// 現在のフレームヘッダーだけからは frame_size が確定できず、
/// 参照スロットの寸法を借用する。この場合は [`Vp9FrameSize::UsesRefFrames`] を返し、
/// 呼び出し側が参照フレームの寸法テーブルから解決する
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vp9FrameSize {
    /// key / intra-only / inter で `frame_size_with_refs` の found_ref がすべて 0 だった場合の解決済みサイズ
    Resolved {
        /// フレーム幅 (1..=65536)
        width: u32,
        /// フレーム高さ (1..=65536)
        height: u32,
    },
    /// inter frame で `frame_size_with_refs` により参照フレームの寸法を借用する場合
    ///
    /// `ref_frame_slot` は `refresh_frame_flags` で管理される 8 スロット
    /// (0..=7) の中の参照インデックス。呼び出し側は自身が保持する参照フレームサイズテーブル
    /// (スロットごとの `(width, height)`) から該当スロットの寸法を読み出す
    UsesRefFrames {
        /// 参照フレームスロットのインデックス (0..=7)
        ref_frame_slot: u8,
    },
}

/// VP9 の uncompressed header から取得できるフレーム情報
///
/// VP9 spec Section 6.2 (`uncompressed_header`) の解析結果を保持する。
///
/// `show_existing_frame` が `Some` の場合、それ以外の色・寸法・error_resilient_mode
/// などのフィールドは header に含まれないため未定義扱いとする (呼び出し側は
/// `show_existing_frame` を先に判定して、参照する既存フレームを別ルートで
/// 特定すること)。
///
/// また non-key かつ非 intra_only の inter frame では、色設定 (`bit_depth` /
/// `color_space` / `color_range` / `subsampling_x` / `subsampling_y`) が header
/// に含まれない (前フレームから継承される仕様) ため、これらは 0 プレースホルダで
/// 埋められる。[`build_vp09_box`] を呼ぶ利用者は `frame_type == Vp9FrameType::Key` か
/// `intra_only == true` のフレームを sample entry の代表として選ぶこと
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Vp9FrameHeader {
    /// profile (0..=3)
    pub profile: u8,

    /// Some(0..=7) なら該当インデックスの復元済みフレームを表示する
    /// (このフレーム自体は新しいピクセルを持たない)。それ以外のフィールドは未定義扱い
    pub show_existing_frame: Option<u8>,

    /// フレーム種別 (キーフレーム / non-key frame)
    pub frame_type: Vp9FrameType,

    /// このフレームを表示するかどうか (VP9 spec の `show_frame`)
    pub show_frame: bool,

    /// error resilient mode の有無
    pub error_resilient_mode: bool,

    /// non-key frame で `intra_only` フラグが立っているかどうか。key frame では常に `false`
    pub intra_only: bool,

    /// ビット深度 (8 / 10 / 12 のいずれか)。inter frame では header に含まれないため 0
    pub bit_depth: u8,

    /// 色空間 (0..=7)。VP9 spec Section 7.2.2 の CS_UNKNOWN..=CS_RGB。
    /// inter frame では header に含まれないため 0
    pub color_space: u8,

    /// 色レンジ (0 = studio swing、1 = full swing)。
    /// color_space が sRGB のときは常に 1。inter frame では header に含まれないため 0
    pub color_range: u8,

    /// 水平方向 chroma subsampling (0 or 1)。color_space が sRGB のときは常に 0。
    /// inter frame では header に含まれないため 0
    pub subsampling_x: u8,

    /// 垂直方向 chroma subsampling (0 or 1)。inter frame では header に含まれないため 0
    pub subsampling_y: u8,

    /// フレームサイズ (`frame_size_with_refs` 経路では未解決状態)
    pub frame_size: Vp9FrameSize,

    /// `(render_width, render_height)`。header に含まれない場合 (`render_and_frame_size_different == 0`) は `None`
    pub render_size: Option<(u32, u32)>,
}

/// [`Vp09Box`] の構築に必要な、ストリームから一意に決まらない設定値
///
/// VP9 仕様および VP Codec ISO Media File Format Binding から確定する値
/// (profile / bit_depth / chroma_subsampling / video_full_range_flag /
/// codec_initialization_data) は [`build_vp09_box`] 側で解析結果から反映するため、
/// 本構造体には含めない。
///
/// - `level`: 単一フレームから確定できないため呼び出し側指定。`None` は 0 (Undefined) として書き込む
/// - `colour_primaries` / `transfer_characteristics` / `matrix_coefficients`:
///   VP9 の `color_space` から ISO/IEC 23001-8 の細分値へ一意対応しないため呼び出し側が明示する
/// - `width` / `height`: 対象サンプルエントリーが参照する全サンプルを収容できる値。
///   VP9 は動的解像度を持つため呼び出し側が集約する
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Vp9SampleEntryConfig {
    /// VP コーデックのレベル (`None` は Undefined を意味し、`vpcC.level` に 0 が入る)
    pub level: Option<u8>,

    /// 色域 (ISO/IEC 23001-8 の `ColourPrimaries`)
    pub colour_primaries: u8,

    /// 伝達特性 (ISO/IEC 23001-8 の `TransferCharacteristics`)
    pub transfer_characteristics: u8,

    /// マトリックス係数 (ISO/IEC 23001-8 の `MatrixCoefficients`)
    pub matrix_coefficients: u8,

    /// トラック全体の幅上限 ([`VisualSampleEntryFields::width`] に対応)
    pub width: u16,

    /// トラック全体の高さ上限 ([`VisualSampleEntryFields::height`] に対応)
    pub height: u16,
}

impl Vp9SampleEntryConfig {
    /// BT.709 系の `colour_primaries` (ISO/IEC 23001-8 Table 2 の 1 = ITU-R BT.709-6)
    pub const COLOUR_PRIMARIES_BT709: u8 = 1;

    /// BT.709 系の `transfer_characteristics` (ISO/IEC 23001-8 Table 3 の 1 = ITU-R BT.709-6)
    pub const TRANSFER_CHARACTERISTICS_BT709: u8 = 1;

    /// BT.709 系の `matrix_coefficients` (ISO/IEC 23001-8 Table 4 の 1 = ITU-R BT.709-6)
    pub const MATRIX_COEFFICIENTS_BT709: u8 = 1;

    /// BT.601 系の `colour_primaries` (ISO/IEC 23001-8 Table 2 の 6 = SMPTE 170M)
    pub const COLOUR_PRIMARIES_BT601: u8 = 6;

    /// BT.601 系の `transfer_characteristics` (ISO/IEC 23001-8 Table 3 の 6 = SMPTE 170M)
    pub const TRANSFER_CHARACTERISTICS_BT601: u8 = 6;

    /// BT.601 系の `matrix_coefficients` (ISO/IEC 23001-8 Table 4 の 6 = SMPTE 170M)
    pub const MATRIX_COEFFICIENTS_BT601: u8 = 6;

    /// BT.2020 系の `colour_primaries` (ISO/IEC 23001-8 Table 2 の 9 = ITU-R BT.2020)
    pub const COLOUR_PRIMARIES_BT2020: u8 = 9;

    /// BT.2020 系の `transfer_characteristics` (ISO/IEC 23001-8 Table 3 の 14 = BT.2020 10-bit)
    ///
    /// HDR (PQ = 16、HLG = 18) は別途指定すること
    pub const TRANSFER_CHARACTERISTICS_BT2020: u8 = 14;

    /// BT.2020 系の `matrix_coefficients` (ISO/IEC 23001-8 Table 4 の 9 = BT.2020 non-constant luminance)
    pub const MATRIX_COEFFICIENTS_BT2020: u8 = 9;

    /// sRGB 系の `colour_primaries` (ISO/IEC 23001-8 Table 2 の 1 = ITU-R BT.709-6、sRGB は BT.709 と同一色域)
    pub const COLOUR_PRIMARIES_SRGB: u8 = 1;

    /// sRGB 系の `transfer_characteristics` (ISO/IEC 23001-8 Table 3 の 13 = IEC 61966-2-1 sRGB / sYCC)
    pub const TRANSFER_CHARACTERISTICS_SRGB: u8 = 13;

    /// sRGB 系の `matrix_coefficients` (ISO/IEC 23001-8 Table 4 の 0 = Identity / RGB)
    pub const MATRIX_COEFFICIENTS_SRGB: u8 = 0;

    /// `colour_primaries` の Unspecified (ISO/IEC 23001-8 Table 2 の 2 = Unspecified)
    pub const COLOUR_PRIMARIES_UNSPECIFIED: u8 = 2;

    /// `transfer_characteristics` の Unspecified (ISO/IEC 23001-8 Table 3 の 2 = Unspecified)
    pub const TRANSFER_CHARACTERISTICS_UNSPECIFIED: u8 = 2;

    /// `matrix_coefficients` の Unspecified (ISO/IEC 23001-8 Table 4 の 2 = Unspecified)
    pub const MATRIX_COEFFICIENTS_UNSPECIFIED: u8 = 2;
}

/// VP9 フレーム全体を渡して uncompressed header を解析する
///
/// # 入力
///
/// - `input`: VP9 フレーム全体 (uncompressed header + compressed header + tile data)。
///   uncompressed header の途中まで読めれば残りは無視する
///
/// # エラー条件
///
/// 以下のいずれかで [`crate::Error`] を返す。
///
/// - 入力が uncompressed header の途中で切れている
/// - `frame_marker` が 2 と一致しない
/// - profile 3 の予約ビットが 0 でない
/// - key frame / intra-only frame の `sync_code` が `0x49 0x83 0x42` と一致しない
/// - color_config の予約ビットが 0 でない
/// - `subsampling_x == 0 && subsampling_y == 1` (仕様外組み合わせ)
/// - profile と bit_depth / subsampling の組み合わせが仕様外
///   (profile 0/1 は 8-bit のみ、profile 0/2 は 4:2:0 のみ)
/// - color_space が sRGB (7) なのに profile が 0 or 2 (sRGB は profile 1 or 3 のみ許容)
/// - key frame / intra-only frame の width または height が 0
///
/// # 対象外
///
/// compressed header や tile data の解析は行わない
pub fn parse_frame_header(input: &[u8]) -> Result<Vp9FrameHeader> {
    let mut reader = BitReader::new(input);

    let frame_marker = reader.read_bits(2)?;
    if frame_marker != FRAME_MARKER {
        return Err(Error::invalid_input("VP9 frame_marker must be 2"));
    }

    // profile は 2 ビット (low, high) を分けて読み、必要なら reserved_zero を検証する
    // (VP9 spec Section 6.2)
    let profile_low = reader.read_bit()?;
    let profile_high = reader.read_bit()?;
    let profile = (profile_high << 1) | profile_low;
    if profile == 3 {
        let reserved_zero = reader.read_bit()?;
        if reserved_zero != 0 {
            return Err(Error::invalid_input(
                "VP9 profile 3 reserved_zero must be 0",
            ));
        }
    }

    let show_existing_frame_bit = reader.read_bit()?;
    if show_existing_frame_bit != 0 {
        // show_existing_frame の場合は frame_to_show_map_idx (3 ビット) を読んで終了。
        // 他フィールドは header に含まれないので既定値で埋める
        let frame_to_show_map_idx = reader.read_bits(3)? as u8;
        return Ok(Vp9FrameHeader {
            profile,
            show_existing_frame: Some(frame_to_show_map_idx),
            frame_type: Vp9FrameType::NonKey,
            show_frame: true,
            error_resilient_mode: false,
            intra_only: false,
            bit_depth: 0,
            color_space: 0,
            color_range: 0,
            subsampling_x: 0,
            subsampling_y: 0,
            frame_size: Vp9FrameSize::Resolved {
                width: 0,
                height: 0,
            },
            render_size: None,
        });
    }

    let frame_type_bit = reader.read_bit()?;
    let frame_type = if frame_type_bit == 0 {
        Vp9FrameType::Key
    } else {
        Vp9FrameType::NonKey
    };
    let show_frame = reader.read_bit()? != 0;
    let error_resilient_mode = reader.read_bit()? != 0;

    match frame_type {
        Vp9FrameType::Key => {
            read_frame_sync_code(&mut reader)?;
            let color = read_color_config(&mut reader, profile)?;
            let (width, height) = read_frame_size(&mut reader)?;
            let render_size = read_render_size(&mut reader)?;
            Ok(Vp9FrameHeader {
                profile,
                show_existing_frame: None,
                frame_type: Vp9FrameType::Key,
                show_frame,
                error_resilient_mode,
                intra_only: false,
                bit_depth: color.bit_depth,
                color_space: color.color_space,
                color_range: color.color_range,
                subsampling_x: color.subsampling_x,
                subsampling_y: color.subsampling_y,
                frame_size: Vp9FrameSize::Resolved { width, height },
                render_size,
            })
        }
        Vp9FrameType::NonKey => {
            // intra_only は show_frame == 0 のときのみ header に含まれる
            let intra_only = if !show_frame {
                reader.read_bit()? != 0
            } else {
                false
            };

            // reset_frame_context (2 ビット) は uncompressed header に含まれるが
            // 解析結果に反映する必要がないので読み飛ばす
            if !error_resilient_mode {
                let _ = reader.read_bits(2)?;
            }

            if intra_only {
                read_frame_sync_code(&mut reader)?;
                // VP9 spec Section 6.2 では profile > 0 のときのみ color_config を読み、
                // profile 0 では 8-bit / BT.601 / studio swing / 4:2:0 を既定として仮定する
                let color = if profile > 0 {
                    read_color_config(&mut reader, profile)?
                } else {
                    ColorConfig {
                        bit_depth: 8,
                        color_space: 1,
                        color_range: 0,
                        subsampling_x: 1,
                        subsampling_y: 1,
                    }
                };
                let _refresh_frame_flags = reader.read_bits(8)?;
                let (width, height) = read_frame_size(&mut reader)?;
                let render_size = read_render_size(&mut reader)?;
                Ok(Vp9FrameHeader {
                    profile,
                    show_existing_frame: None,
                    frame_type: Vp9FrameType::NonKey,
                    show_frame,
                    error_resilient_mode,
                    intra_only: true,
                    bit_depth: color.bit_depth,
                    color_space: color.color_space,
                    color_range: color.color_range,
                    subsampling_x: color.subsampling_x,
                    subsampling_y: color.subsampling_y,
                    frame_size: Vp9FrameSize::Resolved { width, height },
                    render_size,
                })
            } else {
                let _refresh_frame_flags = reader.read_bits(8)?;
                // 参照フレームインデックス 3 個と sign_bias 3 個を読む
                let mut ref_frame_idx = [0u8; 3];
                for slot in ref_frame_idx.iter_mut() {
                    *slot = reader.read_bits(3)? as u8;
                    let _sign_bias = reader.read_bit()?;
                }

                // frame_size_with_refs: found_ref[i] が最初に立った時点で
                // その i の ref_frame_idx を採用して break、
                // 全て 0 なら frame_size() を明示的に読む (VP9 spec Section 6.2.5)
                let mut found_slot: Option<u8> = None;
                for slot in ref_frame_idx.iter() {
                    let found = reader.read_bit()? != 0;
                    if found {
                        found_slot = Some(*slot);
                        break;
                    }
                }
                let frame_size = if let Some(slot) = found_slot {
                    Vp9FrameSize::UsesRefFrames {
                        ref_frame_slot: slot,
                    }
                } else {
                    let (width, height) = read_frame_size(&mut reader)?;
                    Vp9FrameSize::Resolved { width, height }
                };
                let render_size = read_render_size(&mut reader)?;

                // color 系は inter frame では header に含まれないので 0 プレースホルダで埋める
                Ok(Vp9FrameHeader {
                    profile,
                    show_existing_frame: None,
                    frame_type: Vp9FrameType::NonKey,
                    show_frame,
                    error_resilient_mode,
                    intra_only: false,
                    bit_depth: 0,
                    color_space: 0,
                    color_range: 0,
                    subsampling_x: 0,
                    subsampling_y: 0,
                    frame_size,
                    render_size,
                })
            }
        }
    }
}

/// [`parse_frame_header`] 内で使う color_config の読み取り結果
struct ColorConfig {
    bit_depth: u8,
    color_space: u8,
    color_range: u8,
    subsampling_x: u8,
    subsampling_y: u8,
}

/// VP9 spec Section 6.2 の `frame_sync_code` (24 ビット固定バイト列 `0x49 0x83 0x42`) を読む
fn read_frame_sync_code(reader: &mut BitReader<'_>) -> Result<()> {
    let expected = FRAME_SYNC_CODE;
    for byte in expected.iter() {
        let actual = reader.read_bits(8)? as u8;
        if actual != *byte {
            return Err(Error::invalid_input(
                "VP9 frame_sync_code mismatch (expected 0x49 0x83 0x42)",
            ));
        }
    }
    Ok(())
}

/// VP9 spec Section 7.2.2 の `color_config` syntax を読む
fn read_color_config(reader: &mut BitReader<'_>, profile: u8) -> Result<ColorConfig> {
    let bit_depth = if profile >= 2 {
        // profile 2/3: 10-bit または 12-bit を 1 ビットで選択
        if reader.read_bit()? != 0 { 12 } else { 10 }
    } else {
        // profile 0/1: 8-bit 固定
        8
    };

    let color_space = reader.read_bits(3)? as u8;

    let (color_range, subsampling_x, subsampling_y) = if color_space != COLOR_SPACE_SRGB {
        let color_range = reader.read_bit()?;
        let (sx, sy) = if profile == 1 || profile == 3 {
            let sx = reader.read_bit()?;
            let sy = reader.read_bit()?;
            let reserved_zero = reader.read_bit()?;
            if reserved_zero != 0 {
                return Err(Error::invalid_input(
                    "VP9 color_config reserved_zero must be 0",
                ));
            }
            (sx, sy)
        } else {
            // profile 0/2: 4:2:0 固定
            (1, 1)
        };
        if sx == 0 && sy == 1 {
            return Err(Error::invalid_input(
                "VP9 subsampling_x=0 subsampling_y=1 is not allowed",
            ));
        }
        (color_range, sx, sy)
    } else {
        // sRGB は profile 1 または 3 のみ許容、4:4:4 固定、full range 固定
        if profile != 1 && profile != 3 {
            return Err(Error::invalid_input(
                "VP9 sRGB color_space requires profile 1 or 3",
            ));
        }
        let reserved_zero = reader.read_bit()?;
        if reserved_zero != 0 {
            return Err(Error::invalid_input(
                "VP9 color_config reserved_zero must be 0 (sRGB path)",
            ));
        }
        (1, 0, 0)
    };

    Ok(ColorConfig {
        bit_depth,
        color_space,
        color_range,
        subsampling_x,
        subsampling_y,
    })
}

/// VP9 spec Section 6.2.3 の `frame_size` syntax を読む
///
/// `frame_width_minus_1` / `frame_height_minus_1` はいずれも 16 ビット。
/// +1 したものが `(width, height)` となり、0 寸法は仕様上ありえない
fn read_frame_size(reader: &mut BitReader<'_>) -> Result<(u32, u32)> {
    let width_minus_1 = reader.read_bits(16)?;
    let height_minus_1 = reader.read_bits(16)?;
    let width = width_minus_1 + 1;
    let height = height_minus_1 + 1;
    if width == 0 || height == 0 {
        return Err(Error::invalid_input(
            "VP9 frame_size width or height is zero",
        ));
    }
    Ok((width, height))
}

/// VP9 spec Section 6.2.4 の `render_size` syntax を読む
///
/// `render_and_frame_size_different` が 0 のときは header に render_size は含まれず、
/// 表示側は frame_size をそのまま使う。この場合は `None` を返す
fn read_render_size(reader: &mut BitReader<'_>) -> Result<Option<(u32, u32)>> {
    let different = reader.read_bit()?;
    if different == 0 {
        return Ok(None);
    }
    let render_width_minus_1 = reader.read_bits(16)?;
    let render_height_minus_1 = reader.read_bits(16)?;
    Ok(Some((render_width_minus_1 + 1, render_height_minus_1 + 1)))
}

/// VP9 用の [`Vp09Box`] を構築する
///
/// # 固定値
///
/// - [`VpccBox::codec_initialization_data`] = 空バイト列 (VP9 では常に空)
/// - [`Vp09Box::unknown_boxes`] = 空 `Vec`
/// - [`VisualSampleEntryFields`] の `horizresolution` / `vertresolution` /
///   `frame_count` / `compressorname` / `depth`: 同構造体のデフォルト
/// - [`VisualSampleEntryFields::data_reference_index`] =
///   [`VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX`]
///   (自ファイル参照の単一 `dref` エントリー。特殊な dref 構成が必要な場合は
///   戻り値の [`Vp09Box::visual`] のフィールドを書き換える)
///
/// # ストリーム導出値 (`header` から `VpccBox` へ写す)
///
/// - [`VpccBox::profile`] = [`Vp9FrameHeader::profile`]
/// - [`VpccBox::bit_depth`] = [`Vp9FrameHeader::bit_depth`]
/// - [`VpccBox::chroma_subsampling`] = `subsampling_x` / `subsampling_y` から
///   VP Codec ISO Media File Format Binding の 3 ビット値へマッピング
///   (`(1,1)` → 1 = 4:2:0 colocated、`(1,0)` → 2 = 4:2:2、`(0,0)` → 3 = 4:4:4)
/// - [`VpccBox::video_full_range_flag`] = [`Vp9FrameHeader::color_range`]
///
/// # 呼び出し側指定値
///
/// [`Vp9SampleEntryConfig`] の各フィールドを参照
///
/// # 前提
///
/// `header` は色設定を確定できるフレーム (`frame_type == Vp9FrameType::Key`、
/// または `intra_only == true`) を渡すこと。inter frame の header では
/// `bit_depth` / `color_space` / `color_range` / `subsampling_*` が 0
/// プレースホルダになっており、そのまま渡すと `Vp09Box` に不正な値が入る
pub fn build_vp09_box(header: &Vp9FrameHeader, config: &Vp9SampleEntryConfig) -> Vp09Box {
    let chroma_subsampling_value = match (header.subsampling_x, header.subsampling_y) {
        (1, 1) => 1u8,
        (1, 0) => 2u8,
        (0, 0) => 3u8,
        // (0, 1) は parse_frame_header で拒否済み。ここに到達したら仕様外呼び出しで、
        // 最も安全な 4:4:4 (3) にフォールバックする
        _ => 3u8,
    };

    let vpcc_box = VpccBox {
        profile: header.profile,
        // level は 1 フレームからは決まらないので呼び出し側指定を使う。None は 0 (Undefined) に写す
        level: config.level.unwrap_or(0),
        bit_depth: Uint::new(header.bit_depth),
        chroma_subsampling: Uint::new(chroma_subsampling_value),
        video_full_range_flag: Uint::new(header.color_range),
        colour_primaries: config.colour_primaries,
        transfer_characteristics: config.transfer_characteristics,
        matrix_coefficients: config.matrix_coefficients,
        // VP9 の vpcC は codec_initialization_data を常に空バイト列とする仕様
        codec_initialization_data: Vec::new(),
    };

    let visual = VisualSampleEntryFields {
        data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
        width: config.width,
        height: config.height,
        horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
        vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
        frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
        compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
        depth: VisualSampleEntryFields::DEFAULT_DEPTH,
    };

    Vp09Box {
        visual,
        vpcc_box,
        unknown_boxes: Vec::new(),
    }
}

/// VP9 uncompressed header の MSB-first ビット読み取り
///
/// VP9 spec Section 6.2 の `f(n)` syntax element を読むために使う。入力が
/// 尽きた場合は `Err` を返す
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
            return Err(Error::invalid_input("VP9 uncompressed header truncated"));
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
        // 32 ビット超は VP9 syntax にないので上限は 32
        let mut value: u32 = 0;
        for _ in 0..n {
            value = (value << 1) | u32::from(self.read_bit()?);
        }
        Ok(value)
    }
}
