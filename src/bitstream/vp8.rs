//! VP8 ビットストリーム処理ユーティリティ
//!
//! VP8 フレームの uncompressed data chunk 部分を解析し、キーフレーム判定、
//! 解像度、`vp08` / `vpcC` の構築に必要なストリーム情報を得る API を提供する。
//!
//! 参照仕様は以下のとおり。
//!
//! - RFC 6386 「VP8 Data Format and Decoding Guide」の Section 9.1 (`Uncompressed Data Chunk`)
//! - VP Codec ISO Media File Format Binding <https://www.webmproject.org/vp9/mp4/>
//!   (URL パスに `vp9` を含むが同ページで VP8 (`vp08`) のサンプルエントリーも規定する
//!   VP8 / VP9 共通の binding)

use alloc::vec::Vec;
use core::num::NonZeroU16;

use crate::{
    Error, Result, Uint,
    boxes::{VisualSampleEntryFields, Vp08Box, VpccBox},
};

/// VP8 のキーフレーム開始コード
///
/// RFC 6386 Section 9.1 により、キーフレームでは frame tag に続く 3 バイトが
/// この固定バイト列でなければならない
const KEY_FRAME_START_CODE: [u8; 3] = [0x9D, 0x01, 0x2A];

/// frame tag に続くキーフレーム固有領域のバイト数
///
/// 3 バイト (開始コード) + 2 バイト (width) + 2 バイト (height) の 7 バイト
const KEY_FRAME_TAIL_SIZE: usize = 7;

/// frame tag のバイト数
const FRAME_TAG_SIZE: usize = 3;

/// `Vp8FrameHeader::first_partition_size` の最大値 (19 ビット)
const FIRST_PARTITION_SIZE_MAX: u32 = (1 << 19) - 1;

/// VP8 のフレーム種別
///
/// RFC 6386 Section 9.1 の `frame_type` フィールドに対応する
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vp8FrameType {
    /// キーフレーム (frame_type = 0)
    Key,
    /// interframe (frame_type = 1)
    Inter,
}

/// VP8 の uncompressed data chunk から取得できるフレーム情報
///
/// RFC 6386 Section 9.1 の frame tag 4 フィールドを保持する。
/// キーフレームの場合は `keyframe` に width / height / スケールが入る。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Vp8FrameHeader {
    /// フレーム種別 (キーフレーム / interframe)
    pub frame_type: Vp8FrameType,

    /// version (RFC 6386 の 3 ビット値。0..=3 が定義済み)
    pub version: u8,

    /// このフレームを表示するかどうか (RFC 6386 の `show_frame`)
    pub show_frame: bool,

    /// 後続の第 1 データパーティションのサイズ (RFC 6386 の 19 ビット値)
    pub first_partition_size: u32,

    /// キーフレーム固有の情報 (キーフレームのみ `Some`)
    ///
    /// interframe では `None` になる。開始コード 3 バイトと width / height の
    /// 各 2 バイトは frame tag に続いて uncompressed data chunk として現れる
    pub keyframe: Option<Vp8KeyFrameInfo>,
}

/// キーフレームの uncompressed data chunk から取得できる情報
///
/// RFC 6386 Section 9.1 の「開始コード + width + height」7 バイト分に対応する。
/// width / height はそれぞれ 14 ビットで表現され、上位 2 ビットが水平・垂直方向の
/// スケールに割り当てられている
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Vp8KeyFrameInfo {
    /// フレーム幅 (14 ビット値。1..=16383)
    pub width: u16,

    /// フレーム高さ (14 ビット値。1..=16383)
    pub height: u16,

    /// 水平方向のスケール指定 (2 ビット値。0..=3)
    ///
    /// RFC 6386 Section 9.1 の `horizontal_scale` に対応。値は表示サイズを
    /// `width` から拡大するときの比率選択で、0 = 1/1、1 = 5/4、2 = 5/3、3 = 2/1 を意味する。
    /// `width` 自体はスケール適用前の内部フレーム解像度なので、実表示幅は
    /// `width` にこの比率を乗じた値になる
    pub horizontal_scale: u8,

    /// 垂直方向のスケール指定 (2 ビット値。0..=3)
    ///
    /// RFC 6386 Section 9.1 の `vertical_scale` に対応。値の意味は
    /// [`Vp8KeyFrameInfo::horizontal_scale`] と同じで、`height` に対して同一の比率テーブルが適用される
    pub vertical_scale: u8,
}

/// VP8 フレーム全体を渡して uncompressed data chunk を解析する
///
/// # 入力
///
/// - `input`: VP8 フレーム全体 (frame tag + 圧縮ペイロード)。キーフレームの場合は
///   `input[3..10]` の 7 バイトも uncompressed data chunk として解釈される
///
/// # エラー条件
///
/// 以下のいずれかで [`crate::Error`] を返す。
///
/// - `input` が 3 バイト未満 (frame tag 不足)
/// - キーフレームで `input` が 10 バイト未満 (frame tag + 開始コード + width + height 不足)
/// - キーフレームの開始コードが `0x9D 0x01 0x2A` と一致しない
/// - `version` が 4..=7 (RFC 6386 未定義領域)
/// - キーフレームの `width` または `height` が 0
/// - `first_partition_size` が `input` 末尾を超える
///   (interframe: `input.len() - 3` と比較、キーフレーム: `input.len() - 10` と比較)
///
/// # 対象外
///
/// 圧縮ヘッダーやマクロブロックの解析は行わない
pub fn parse_frame_header(input: &[u8]) -> Result<Vp8FrameHeader> {
    if input.len() < FRAME_TAG_SIZE {
        return Err(Error::invalid_input("VP8 frame tag requires 3 bytes"));
    }

    // frame tag の 3 バイトを LE で 24 ビットに詰め、ビット位置ごとに切り出す
    // (RFC 6386 Section 9.1 の frame tag ビット配置)
    let tag = u32::from(input[0]) | (u32::from(input[1]) << 8) | (u32::from(input[2]) << 16);
    let frame_type_bit = tag & 0x1;
    let version = ((tag >> 1) & 0x7) as u8;
    let show_frame = ((tag >> 4) & 0x1) != 0;
    let first_partition_size = (tag >> 5) & FIRST_PARTITION_SIZE_MAX;

    // 未定義値は将来の拡張と誤認しないよう入り口で保守的に拒否する
    if version >= 4 {
        return Err(Error::invalid_input("VP8 version 4..=7 is reserved"));
    }

    let frame_type = if frame_type_bit == 0 {
        Vp8FrameType::Key
    } else {
        Vp8FrameType::Inter
    };

    let keyframe = if matches!(frame_type, Vp8FrameType::Key) {
        // キーフレームでは frame tag に続けて 7 バイト (開始コード 3 + width 2 + height 2)
        // が必ず uncompressed data chunk として現れる
        if input.len() < FRAME_TAG_SIZE + KEY_FRAME_TAIL_SIZE {
            return Err(Error::invalid_input(
                "VP8 keyframe requires 10 bytes (frame tag + start code + width + height)",
            ));
        }
        if input[3..6] != KEY_FRAME_START_CODE {
            return Err(Error::invalid_input(
                "VP8 keyframe start code mismatch (expected 0x9D 0x01 0x2A)",
            ));
        }

        // width / height は LE 16 ビットとして読み、下位 14 ビットが値、
        // 上位 2 ビットが水平・垂直スケール
        let width_field = u16::from_le_bytes([input[6], input[7]]);
        let width = width_field & 0x3FFF;
        let horizontal_scale = ((width_field >> 14) & 0x3) as u8;

        let height_field = u16::from_le_bytes([input[8], input[9]]);
        let height = height_field & 0x3FFF;
        let vertical_scale = ((height_field >> 14) & 0x3) as u8;

        // ゼロ寸法は VP8 フレームとして意味を持たないため拒否する
        if width == 0 || height == 0 {
            return Err(Error::invalid_input("VP8 keyframe width or height is zero"));
        }

        Some(Vp8KeyFrameInfo {
            width,
            height,
            horizontal_scale,
            vertical_scale,
        })
    } else {
        None
    };

    // first_partition_size が実データの末尾を超えないことを検証する。
    // uncompressed data chunk のサイズ (interframe: 3 バイト、キーフレーム: 10 バイト)
    // を引いた残りが第 1 パーティションを収容できる最大長になる
    let uncompressed_chunk_size = if keyframe.is_some() {
        FRAME_TAG_SIZE + KEY_FRAME_TAIL_SIZE
    } else {
        FRAME_TAG_SIZE
    };
    let remaining = input.len() - uncompressed_chunk_size;
    if first_partition_size as usize > remaining {
        return Err(Error::invalid_input(
            "VP8 first_partition_size exceeds input size",
        ));
    }

    Ok(Vp8FrameHeader {
        frame_type,
        version,
        show_frame,
        first_partition_size,
        keyframe,
    })
}

/// [`Vp08Box`] の構築に必要な、ストリームから一意に決まらない設定値
///
/// VP8 仕様および VP Codec ISO Media File Format Binding から確定する値
/// (profile / bit_depth / chroma_subsampling / codec_initialization_data) は
/// [`build_vp08_box`] 側で固定するため、本構造体には含めない。
///
/// - `level`: 単一フレームから確定できないため `Option`。`None` の場合は
///   ISO/IEC 14496-15 の慣例に合わせて 0 (unspecified) を書き込む
/// - `colour_primaries` / `transfer_characteristics` / `matrix_coefficients` /
///   `video_full_range_flag`: VP8 の color_space / clamping_type と一意対応しないため
///   呼び出し側が明示する
/// - `width` / `height`: 対象サンプルエントリーが参照する全サンプルを収容できる値。
///   単一キーフレームの値を無条件にトラック全体の値にしないため呼び出し側が集約する
/// - `data_reference_index`: `dref` エントリー参照
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Vp8SampleEntryConfig {
    /// VP コーデックのレベル (`None` は unspecified を意味し、`vpcC.level` に 0 が入る)
    pub level: Option<u8>,

    /// 映像レンジフラグ (`true` = full-range、`false` = limited-range)
    pub video_full_range_flag: bool,

    /// 色域 (ISO/IEC 23001-8 の `ColourPrimaries`)
    pub colour_primaries: u8,

    /// 伝達特性 (ISO/IEC 23001-8 の `TransferCharacteristics`)
    pub transfer_characteristics: u8,

    /// マトリックス係数 (ISO/IEC 23001-8 の `MatrixCoefficients`)
    pub matrix_coefficients: u8,

    /// トラック全体の幅上限 (`VisualSampleEntryFields::width`)
    pub width: u16,

    /// トラック全体の高さ上限 (`VisualSampleEntryFields::height`)
    pub height: u16,

    /// `dref` 内のエントリーを 1-based で指す (`VisualSampleEntryFields::data_reference_index`)
    pub data_reference_index: NonZeroU16,
}

/// VP8 用の [`Vp08Box`] を構築する
///
/// VP8 仕様および VP Codec ISO Media File Format Binding から確定する値は
/// この関数側で固定する。呼び出し側が明示するのは `config` に列挙された項目のみ。
///
/// # 固定値
///
/// - `VpccBox::profile` = 0 (VP8 は profile 0 のみ)
/// - `VpccBox::bit_depth` = 8 (VP8 は 8-bit のみ)
/// - `VpccBox::chroma_subsampling` = 1
///   (VP8 は YUV 4:2:0 固定。VP Codec ISO Media File Format Binding の 3 ビット値では
///   0 = 4:2:0 vertical、1 = 4:2:0 colocated。VP8 仕様は chroma siting を規定しないため、
///   MP4 コンテナへ格納するときの既定値としてこの関数側で 1 (colocated) を採用する)
/// - `VpccBox::codec_initialization_data` = 空バイト列
/// - `VisualSampleEntryFields::horizresolution` / `vertresolution` / `frame_count` /
///   `compressorname` / `depth`: `VisualSampleEntryFields` のデフォルト
/// - `Vp08Box::unknown_boxes` = 空 `Vec`
///
/// # 呼び出し側指定値
///
/// [`Vp8SampleEntryConfig`] の各フィールドを参照。
///
/// # 対象外
///
/// - 特定利用側の慣習 (BT.709 / limited range など) を暗黙の固定値として持ち込まない。
///   色特性は呼び出し側が明示する
pub fn build_vp08_box(config: &Vp8SampleEntryConfig) -> Result<Vp08Box> {
    let vpcc_box = VpccBox {
        // profile 0 は VP8 全体で共通
        profile: 0,
        // level は 1 フレームから決まらないので呼び出し側指定を使う。
        // None (unspecified) は 0 で表す
        level: config.level.unwrap_or(0),
        // bit_depth は VP8 全体で 8 固定
        bit_depth: Uint::new(8),
        // chroma_subsampling は VP8 全体で 4:2:0。値 1 (colocated) を採用する根拠は
        // 関数の doc コメント参照
        chroma_subsampling: Uint::new(1),
        video_full_range_flag: Uint::new(u8::from(config.video_full_range_flag)),
        colour_primaries: config.colour_primaries,
        transfer_characteristics: config.transfer_characteristics,
        matrix_coefficients: config.matrix_coefficients,
        // VP8 の vpcC は codec_initialization_data を常に空バイト列とする仕様
        codec_initialization_data: Vec::new(),
    };

    let visual = VisualSampleEntryFields {
        data_reference_index: config.data_reference_index,
        width: config.width,
        height: config.height,
        horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
        vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
        frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
        compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
        depth: VisualSampleEntryFields::DEFAULT_DEPTH,
    };

    Ok(Vp08Box {
        visual,
        vpcc_box,
        unknown_boxes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// キーフレーム / interframe 双方の frame tag / tail を組み立てる引数群
    ///
    /// テスト側で全パラメタを一括に指定できるようにするための集約構造体。
    /// clippy の `too_many_arguments` を避けつつ、テスト側の可読性を保つ
    struct KeyframeParams {
        version: u8,
        show_frame: bool,
        first_partition_size: u32,
        width: u16,
        horizontal_scale: u8,
        height: u16,
        vertical_scale: u8,
        start_code: [u8; 3],
    }

    impl KeyframeParams {
        /// 有効値で全フィールドを初期化した最小構成
        fn valid() -> Self {
            Self {
                version: 0,
                show_frame: true,
                first_partition_size: 0,
                width: 320,
                horizontal_scale: 0,
                height: 240,
                vertical_scale: 0,
                start_code: KEY_FRAME_START_CODE,
            }
        }
    }

    /// キーフレームの frame tag と 7 バイトの uncompressed tail を組み立てる
    ///
    /// バリデーションはせず、渡した値をそのままビット位置に詰める
    fn build_keyframe_bytes(params: KeyframeParams) -> alloc::vec::Vec<u8> {
        let mut bytes = alloc::vec::Vec::new();
        let tag = ((params.first_partition_size & FIRST_PARTITION_SIZE_MAX) << 5)
            | ((u32::from(params.show_frame) & 0x1) << 4)
            | ((u32::from(params.version) & 0x7) << 1);
        // frame_type = 0 (Key) なので bit 0 は 0
        bytes.push((tag & 0xFF) as u8);
        bytes.push(((tag >> 8) & 0xFF) as u8);
        bytes.push(((tag >> 16) & 0xFF) as u8);
        bytes.extend_from_slice(&params.start_code);
        let width_field =
            (params.width & 0x3FFF) | ((u16::from(params.horizontal_scale) & 0x3) << 14);
        bytes.extend_from_slice(&width_field.to_le_bytes());
        let height_field =
            (params.height & 0x3FFF) | ((u16::from(params.vertical_scale) & 0x3) << 14);
        bytes.extend_from_slice(&height_field.to_le_bytes());
        bytes
    }

    /// interframe の frame tag のみを組み立てる (payload は呼び出し側で追加する)
    fn build_interframe_bytes(
        version: u8,
        show_frame: bool,
        first_partition_size: u32,
    ) -> alloc::vec::Vec<u8> {
        let mut bytes = alloc::vec::Vec::new();
        let tag = ((first_partition_size & FIRST_PARTITION_SIZE_MAX) << 5)
            | ((u32::from(show_frame) & 0x1) << 4)
            | ((u32::from(version) & 0x7) << 1)
            | 0x1; // frame_type = 1 (Inter)
        bytes.push((tag & 0xFF) as u8);
        bytes.push(((tag >> 8) & 0xFF) as u8);
        bytes.push(((tag >> 16) & 0xFF) as u8);
        bytes
    }

    #[test]
    fn parse_keyframe_minimal() {
        let bytes = build_keyframe_bytes(KeyframeParams::valid());
        let header = parse_frame_header(&bytes).expect("最小キーフレームは解析成功する");
        assert_eq!(header.frame_type, Vp8FrameType::Key);
        assert_eq!(header.version, 0);
        assert!(header.show_frame);
        assert_eq!(header.first_partition_size, 0);
        let key = header.keyframe.expect("キーフレームは keyframe を持つ");
        assert_eq!(key.width, 320);
        assert_eq!(key.height, 240);
        assert_eq!(key.horizontal_scale, 0);
        assert_eq!(key.vertical_scale, 0);
    }

    #[test]
    fn parse_interframe_minimal() {
        let bytes = build_interframe_bytes(1, false, 0);
        let header = parse_frame_header(&bytes).expect("最小 interframe は解析成功する");
        assert_eq!(header.frame_type, Vp8FrameType::Inter);
        assert_eq!(header.version, 1);
        assert!(!header.show_frame);
        assert_eq!(header.first_partition_size, 0);
        assert!(header.keyframe.is_none());
    }

    #[test]
    fn reject_short_input() {
        assert!(parse_frame_header(&[]).is_err());
        assert!(parse_frame_header(&[0x00]).is_err());
        assert!(parse_frame_header(&[0x00, 0x00]).is_err());
    }

    #[test]
    fn reject_keyframe_short_input() {
        // frame tag だけは 3 バイトで足りるが、キーフレームはさらに 7 バイト必要
        let bytes = alloc::vec![0x00, 0x00, 0x00];
        assert!(parse_frame_header(&bytes).is_err());
    }

    #[test]
    fn reject_keyframe_bad_start_code() {
        let bytes = build_keyframe_bytes(KeyframeParams {
            start_code: [0x00, 0x00, 0x00],
            ..KeyframeParams::valid()
        });
        assert!(parse_frame_header(&bytes).is_err());
    }

    #[test]
    fn reject_reserved_version() {
        for version in 4u8..=7 {
            let bytes = build_interframe_bytes(version, false, 0);
            assert!(
                parse_frame_header(&bytes).is_err(),
                "version {version} は未定義なので拒否されるべき",
            );
        }
    }

    #[test]
    fn reject_zero_dimension() {
        let width_zero = build_keyframe_bytes(KeyframeParams {
            width: 0,
            ..KeyframeParams::valid()
        });
        assert!(parse_frame_header(&width_zero).is_err());
        let height_zero = build_keyframe_bytes(KeyframeParams {
            height: 0,
            ..KeyframeParams::valid()
        });
        assert!(parse_frame_header(&height_zero).is_err());
    }
}
