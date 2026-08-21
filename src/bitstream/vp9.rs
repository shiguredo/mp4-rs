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

/// キーフレームの `refresh_frame_flags`
///
/// VP9 spec Section 6.2 ではキーフレーム経路で `refresh_frame_flags = 0xFF` を
/// 代入するだけで、この 8 ビットはストリームに現れない
const REFRESH_FRAME_FLAGS_ALL: u8 = 0xFF;

/// `show_existing_frame` の `refresh_frame_flags`
///
/// VP9 spec Section 6.2 では show_existing 経路で `refresh_frame_flags = 0` を
/// 代入する。参照スロットは更新しない
const REFRESH_FRAME_FLAGS_NONE: u8 = 0;

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
/// 呼び出し側が参照フレームの寸法テーブルから解決する。
/// テーブルの更新には [`Vp9FrameHeader::refresh_frame_flags`] を使う
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
    /// (スロットごとの `(width, height)`) から該当スロットの寸法を読み出す。
    /// 現在フレームのデコード後にどのスロットを更新するかは
    /// [`Vp9FrameHeader::refresh_frame_flags`] を見る
    UsesRefFrames {
        /// 参照フレームスロットのインデックス (0..=7)
        ref_frame_slot: u8,
    },
    /// `show_existing_frame` 経路。`frame_size` 構文が header に含まれない
    ///
    /// 寸法は `show_existing_frame` が指す復元済みフレーム側を使う。
    /// 0 埋めの [`Vp9FrameSize::Resolved`] では表さない
    NotPresent,
}

/// VP9 の uncompressed header から取得できるフレーム情報
///
/// VP9 spec Section 6.2 (`uncompressed_header`) の解析結果を保持する。
///
/// `show_existing_frame` が `Some` の場合、`frame_size` は [`Vp9FrameSize::NotPresent`]、
/// `refresh_frame_flags` は 0 になる。色設定や error_resilient_mode などは header に
/// 含まれないため未定義扱いとする (呼び出し側は `show_existing_frame` を先に判定して、
/// 参照する既存フレームを別ルートで特定すること)。
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
    /// (このフレーム自体は新しいピクセルを持たない)
    ///
    /// この経路では `frame_size` は [`Vp9FrameSize::NotPresent`]、
    /// `refresh_frame_flags` は 0。色設定などは header に含まれないため未定義扱い
    pub show_existing_frame: Option<u8>,

    /// フレーム種別 (キーフレーム / non-key frame)
    pub frame_type: Vp9FrameType,

    /// このフレームを表示するかどうか (VP9 spec の `show_frame`)
    pub show_frame: bool,

    /// error resilient mode の有無
    pub error_resilient_mode: bool,

    /// non-key frame で `intra_only` フラグが立っているかどうか。key frame では常に `false`
    pub intra_only: bool,

    /// どの参照スロット (0..=7) を現在フレームで更新するか (VP9 spec Section 6.2 / 8.10)
    ///
    /// - キーフレーム: ストリームに含まれず常に `0xFF` (全スロット更新)
    /// - `show_existing_frame`: 常に `0` (更新なし)
    /// - intra-only / inter: header から読んだ 8 ビット値
    ///
    /// [`Vp9FrameSize::UsesRefFrames`] を解く呼び出し側は、この値で寸法テーブルを更新する
    pub refresh_frame_flags: u8,

    /// ビット深度 (8 / 10 / 12 のいずれか)
    ///
    /// inter frame / `show_existing_frame` では header に含まれないため 0 プレースホルダ。
    /// [`build_vp09_box`] はこの 0 を検出して Err を返し、代表フレームに使えないことを
    /// 呼び出し側に伝える
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

    /// フレームサイズ
    ///
    /// - key / intra-only / 明示サイズの inter: [`Vp9FrameSize::Resolved`]
    /// - `frame_size_with_refs` で参照寸法を借用: [`Vp9FrameSize::UsesRefFrames`]
    /// - `show_existing_frame`: [`Vp9FrameSize::NotPresent`]
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
/// - profile 1/3 で `subsampling_x == 1 && subsampling_y == 1` (4:2:0)
///   (VP9 spec Section 7.2.2 の bitstream conformance で不許可)
/// - color_space が sRGB (7) なのに profile が 0 or 2 (sRGB は profile 1 or 3 のみ許容)
///
/// なお `frame_width_minus_1` / `frame_height_minus_1` に +1 したものが frame の
/// 幅・高さになる仕様上、0 寸法は表現不能なので専用のエラー分類は持たない
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
        // frame_size は header に含まれないので NotPresent。refresh_frame_flags は
        // spec 上 0。色設定などは 0 プレースホルダ
        let frame_to_show_map_idx = reader.read_bits(3)? as u8;
        return Ok(Vp9FrameHeader {
            profile,
            show_existing_frame: Some(frame_to_show_map_idx),
            frame_type: Vp9FrameType::NonKey,
            show_frame: true,
            error_resilient_mode: false,
            intra_only: false,
            refresh_frame_flags: REFRESH_FRAME_FLAGS_NONE,
            bit_depth: 0,
            color_space: 0,
            color_range: 0,
            subsampling_x: 0,
            subsampling_y: 0,
            frame_size: Vp9FrameSize::NotPresent,
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
                refresh_frame_flags: REFRESH_FRAME_FLAGS_ALL,
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
                let refresh_frame_flags = reader.read_bits(8)? as u8;
                let (width, height) = read_frame_size(&mut reader)?;
                let render_size = read_render_size(&mut reader)?;
                Ok(Vp9FrameHeader {
                    profile,
                    show_existing_frame: None,
                    frame_type: Vp9FrameType::NonKey,
                    show_frame,
                    error_resilient_mode,
                    intra_only: true,
                    refresh_frame_flags,
                    bit_depth: color.bit_depth,
                    color_space: color.color_space,
                    color_range: color.color_range,
                    subsampling_x: color.subsampling_x,
                    subsampling_y: color.subsampling_y,
                    frame_size: Vp9FrameSize::Resolved { width, height },
                    render_size,
                })
            } else {
                let refresh_frame_flags = reader.read_bits(8)? as u8;
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
                    refresh_frame_flags,
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
///
/// profile 0/1 の bit_depth は構文上 8 固定、profile 0/2 の subsampling は
/// 構文上 4:2:0 固定で、これらの組み合わせはエラーではなく代入で表現する
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
            // VP9 spec Section 7.2.2 の bitstream conformance:
            // profile 1/3 では subsampling_x == 1 && subsampling_y == 1 (4:2:0) が禁止
            // (libvpx の read_bitdepth_colorspace_sampling も同様に
            //  "4:2:0 color not supported in profile 1 or 3" として拒否している)
            if sx == 1 && sy == 1 {
                return Err(Error::invalid_input(
                    "VP9 4:2:0 (subsampling 1,1) is not allowed in profile 1 or 3",
                ));
            }
            (sx, sy)
        } else {
            // profile 0/2: 4:2:0 固定 (color_config には書かれない)
            (1, 1)
        };
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
/// +1 したものが `(width, height)` となり、値域は 1..=65536 で 0 寸法は表現不能
fn read_frame_size(reader: &mut BitReader<'_>) -> Result<(u32, u32)> {
    let width_minus_1 = reader.read_bits(16)?;
    let height_minus_1 = reader.read_bits(16)?;
    Ok((width_minus_1 + 1, height_minus_1 + 1))
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
/// または `intra_only == true`) を渡すこと。inter frame や
/// `show_existing_frame` の header では色情報 (`bit_depth` / `color_space` /
/// `color_range` / `subsampling_*`) が 0 プレースホルダになっており、
/// この関数は下記「エラー条件」で検出して `Err` を返す
///
/// # エラー条件
///
/// - `header.bit_depth == 0` (inter frame や `show_existing_frame` 由来の
///   プレースホルダ header は色情報を持たないので `Vp09Box` の代表フレームに使えない)
/// - `header.bit_depth` が 8 / 10 / 12 以外
/// - `header.color_range` が 0 / 1 以外
/// - `header.profile` が 0..=3 以外
/// - `header.subsampling_x == 0 && header.subsampling_y == 1` (VP9 の 4:4:0) は
///   VP Codec ISO Media File Format Binding の `chroma_subsampling` 3 ビット値に
///   対応値がないため `Vp09Box` に格納できない
/// - `header.subsampling_x` / `header.subsampling_y` が 0 / 1 以外
///   ([`Vp9FrameHeader`] は pub フィールドを持つので、parse を通さない手組み構築でも
///   panic ではなく Err で拒否する)
pub fn build_vp09_box(header: &Vp9FrameHeader, config: &Vp9SampleEntryConfig) -> Result<Vp09Box> {
    if header.bit_depth == 0 {
        // inter frame / show_existing_frame の header は色情報 (bit_depth 等) が
        // 0 プレースホルダで書かれる。silent に vpcC へ流し込むと 4:4:4 として
        // 誤って書き出してしまうので入り口で拒否する
        return Err(Error::invalid_input(
            "VP9 build_vp09_box requires a key or intra_only frame header \
             (bit_depth == 0 indicates inter frame or show_existing_frame)",
        ));
    }
    if header.bit_depth != 8 && header.bit_depth != 10 && header.bit_depth != 12 {
        // Uint<u8, 4, 4> に 16 以上を渡すと encode 時に debug panic する。
        // Binding の bitDepth は 8 / 10 / 12 のみなので、手組みの範囲外値は Err
        return Err(Error::invalid_input("VP9 bit_depth must be 8, 10, or 12"));
    }
    if header.color_range > 1 {
        // Uint<u8, 1> に 2 以上を渡すと chroma_subsampling 側のビットを侵食する
        return Err(Error::invalid_input("VP9 color_range must be 0 or 1"));
    }
    if header.profile > 3 {
        return Err(Error::invalid_input("VP9 profile must be 0..=3"));
    }
    let chroma_subsampling_value = match (header.subsampling_x, header.subsampling_y) {
        (1, 1) => 1u8,
        (1, 0) => 2u8,
        (0, 0) => 3u8,
        (0, 1) => {
            // VP9 spec では profile 1/3 で 4:4:0 は合法だが、VP Codec ISO Media
            // File Format Binding の chroma_subsampling には 4:4:0 に対応する値が
            // ないため Vp09Box には格納できない
            return Err(Error::invalid_input(
                "VP9 4:4:0 (subsampling 0,1) cannot be represented in VpccBox::chroma_subsampling",
            ));
        }
        // Vp9FrameHeader は pub フィールドを持つため、parse を通さない構築で
        // 2 以上が渡されうる。panic ではなく Err で防衛する
        _ => {
            return Err(Error::invalid_input(
                "VP9 subsampling_x / subsampling_y must be 0 or 1",
            ));
        }
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

    Ok(Vp09Box {
        visual,
        vpcc_box,
        unknown_boxes: Vec::new(),
    })
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
