//! `shiguredo_mp4::bitstream::vp8` の決定的テスト
//!
//! 手動構築した frame tag / uncompressed data chunk のバイト列に対して
//! パーサーの受理・拒否条件を固定する。実 VP8 エンコーダーが必要な回帰は
//! 別途 fixture ベースのテストで補う想定 (別 issue で追加予定)。

use std::num::NonZeroU16;

use shiguredo_mp4::{
    Decode, Encode, ErrorKind, Uint,
    bitstream::vp8::{Vp8FrameType, Vp8SampleEntryConfig, build_vp08_box, parse_frame_header},
    boxes::{VisualSampleEntryFields, Vp08Box},
};

/// 有効なキーフレーム開始コード
const KEY_FRAME_START_CODE: [u8; 3] = [0x9D, 0x01, 0x2A];

/// キーフレームの frame tag と 7 バイトの uncompressed tail の全設定値を集約する
///
/// バリデーションはせず、渡した値をそのままビット位置に詰める。
/// clippy の `too_many_arguments` を回避するため helper 引数を struct にまとめ、
/// `KeyframeParams::valid()` をベースに差分だけ変えるパターンで各テストを書く
#[derive(Debug, Clone, Copy)]
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

/// キーフレームのバイト列 (frame tag + 開始コード + width + height) を組み立てる
fn build_keyframe_bytes(params: KeyframeParams) -> Vec<u8> {
    let mut bytes = Vec::new();
    // frame_type = 0 (Key)、上位に version / show_frame / first_partition_size を詰める
    let tag = ((params.first_partition_size & 0x7_FFFF) << 5)
        | ((u32::from(params.show_frame) & 0x1) << 4)
        | ((u32::from(params.version) & 0x7) << 1);
    bytes.push((tag & 0xFF) as u8);
    bytes.push(((tag >> 8) & 0xFF) as u8);
    bytes.push(((tag >> 16) & 0xFF) as u8);
    bytes.extend_from_slice(&params.start_code);
    let width_field = (params.width & 0x3FFF) | ((u16::from(params.horizontal_scale) & 0x3) << 14);
    bytes.extend_from_slice(&width_field.to_le_bytes());
    let height_field = (params.height & 0x3FFF) | ((u16::from(params.vertical_scale) & 0x3) << 14);
    bytes.extend_from_slice(&height_field.to_le_bytes());
    bytes
}

/// interframe のバイト列 (frame tag のみ) を組み立てる
///
/// payload が必要なテストは戻り値の `Vec` に呼び出し側で追加する
fn build_interframe_bytes(version: u8, show_frame: bool, first_partition_size: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let tag = ((first_partition_size & 0x7_FFFF) << 5)
        | ((u32::from(show_frame) & 0x1) << 4)
        | ((u32::from(version) & 0x7) << 1)
        | 0x1; // frame_type = 1 (Inter)
    bytes.push((tag & 0xFF) as u8);
    bytes.push(((tag >> 8) & 0xFF) as u8);
    bytes.push(((tag >> 16) & 0xFF) as u8);
    bytes
}

fn default_config() -> Vp8SampleEntryConfig {
    Vp8SampleEntryConfig {
        level: None,
        video_full_range_flag: false,
        colour_primaries: 1,
        transfer_characteristics: 1,
        matrix_coefficients: 1,
        width: 320,
        height: 240,
        data_reference_index: NonZeroU16::MIN,
    }
}

// ===== parse_frame_header: 受理系 =====

/// キーフレーム最小構成 (payload 0 バイト、first_partition_size = 0) を解析できる
#[test]
fn keyframe_minimal_parse() {
    let bytes = build_keyframe_bytes(KeyframeParams::valid());
    let header = parse_frame_header(&bytes).expect("最小キーフレームは解析成功する");
    assert_eq!(header.frame_type, Vp8FrameType::Key);
    assert_eq!(header.version, 0);
    assert!(header.show_frame);
    assert_eq!(header.first_partition_size, 0);
    let key = header
        .keyframe
        .expect("キーフレームは keyframe フィールドを持つ");
    assert_eq!(key.width, 320);
    assert_eq!(key.height, 240);
    assert_eq!(key.horizontal_scale, 0);
    assert_eq!(key.vertical_scale, 0);
}

/// interframe 最小構成を解析できる
#[test]
fn interframe_minimal_parse() {
    let bytes = build_interframe_bytes(0, false, 0);
    let header = parse_frame_header(&bytes).expect("最小 interframe は解析成功する");
    assert_eq!(header.frame_type, Vp8FrameType::Inter);
    assert_eq!(header.version, 0);
    assert!(!header.show_frame);
    assert_eq!(header.first_partition_size, 0);
    assert!(header.keyframe.is_none());
}

/// version 0..=3 を受理する
#[test]
fn accept_versions_zero_to_three() {
    for version in 0u8..=3 {
        let bytes = build_interframe_bytes(version, true, 0);
        let header = parse_frame_header(&bytes).expect("version 0..=3 は解析成功する");
        assert_eq!(header.version, version);
    }
}

/// show_frame の 2 値をどちらも受理する
#[test]
fn accept_show_frame_both_values() {
    for show in [false, true] {
        let bytes = build_interframe_bytes(0, show, 0);
        let header = parse_frame_header(&bytes).expect("show_frame の両値を受理する");
        assert_eq!(header.show_frame, show);
    }
}

/// キーフレームの width / height の各 14 ビット境界値を保持する
#[test]
fn keyframe_dimension_extremes_are_preserved() {
    // 14 ビットの最大値
    let max_dim = 0x3FFFu16;
    let bytes = build_keyframe_bytes(KeyframeParams {
        width: max_dim,
        height: max_dim,
        ..KeyframeParams::valid()
    });
    let key = parse_frame_header(&bytes)
        .expect("最大寸法のキーフレームは解析成功する")
        .keyframe
        .expect("キーフレーム");
    assert_eq!(key.width, max_dim);
    assert_eq!(key.height, max_dim);
}

/// キーフレームのスケール (2 ビット) の 4 値すべてを保持する
#[test]
fn keyframe_scale_all_values_are_preserved() {
    for scale in 0u8..=3 {
        let bytes = build_keyframe_bytes(KeyframeParams {
            horizontal_scale: scale,
            vertical_scale: scale,
            ..KeyframeParams::valid()
        });
        let key = parse_frame_header(&bytes)
            .expect("scale の全 4 値を受理する")
            .keyframe
            .expect("キーフレーム");
        assert_eq!(key.horizontal_scale, scale);
        assert_eq!(key.vertical_scale, scale);
    }
}

/// `first_partition_size` の 19 ビット最大値 (残入力に収まる限り) を保持する
#[test]
fn first_partition_size_max_value_within_bounds() {
    // frame tag に格納可能な最大値 = 0x7_FFFF = 524287
    // interframe: 全長 = 3 (frame tag) + first_partition_size 分を用意
    let size = 0x7_FFFFu32;
    let mut bytes = build_interframe_bytes(0, true, size);
    // 圧縮ペイロード分を確保。中身は 0 埋めで十分
    bytes.resize(bytes.len() + size as usize, 0);
    let header =
        parse_frame_header(&bytes).expect("first_partition_size 最大値が残入力内なら解析成功する");
    assert_eq!(header.first_partition_size, size);
}

/// `first_partition_size` が残入力ちょうどならば境界を許容する
#[test]
fn first_partition_size_matching_boundary_is_accepted() {
    let mut bytes = build_interframe_bytes(0, true, 128);
    bytes.resize(bytes.len() + 128, 0);
    let header =
        parse_frame_header(&bytes).expect("first_partition_size が残入力ちょうどなら受理される");
    assert_eq!(header.first_partition_size, 128);
}

// ===== parse_frame_header: 拒否系 =====

/// 3 バイト未満の入力 (frame tag 欠落) は拒否する
#[test]
fn reject_input_shorter_than_frame_tag() {
    for len in 0..3 {
        let bytes = vec![0u8; len];
        let err = parse_frame_header(&bytes).expect_err("frame tag 不足は拒否される");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// キーフレームで 10 バイト未満の入力 (uncompressed data chunk 欠落) は拒否する
#[test]
fn reject_keyframe_input_shorter_than_uncompressed_chunk() {
    // frame tag 3 バイトは足りるが、追加の 7 バイトが欠ける
    for len in 3..10 {
        let mut bytes = vec![0u8; len];
        // frame_type = 0 (Key) を明示 (bit 0 = 0)
        bytes[0] = 0x00;
        let err = parse_frame_header(&bytes).expect_err("キーフレームの tail 不足は拒否される");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// キーフレームの開始コードが `0x9D 0x01 0x2A` と異なる場合は拒否する
#[test]
fn reject_keyframe_bad_start_code() {
    let variations: [[u8; 3]; 4] = [
        [0x00, 0x00, 0x00],
        [0x9C, 0x01, 0x2A],
        [0x9D, 0x02, 0x2A],
        [0x9D, 0x01, 0x2B],
    ];
    for start_code in variations {
        let bytes = build_keyframe_bytes(KeyframeParams {
            start_code,
            ..KeyframeParams::valid()
        });
        let err = parse_frame_header(&bytes)
            .expect_err("開始コード不一致は拒否される (start_code={start_code:?})");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// version が 4..=7 (RFC 6386 で未定義) の場合は拒否する
#[test]
fn reject_reserved_version() {
    for version in 4u8..=7 {
        let bytes = build_interframe_bytes(version, true, 0);
        let err =
            parse_frame_header(&bytes).expect_err("version {version} は未定義なので拒否される");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// キーフレームで width が 0 の場合は拒否する
#[test]
fn reject_keyframe_zero_width() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        width: 0,
        ..KeyframeParams::valid()
    });
    let err = parse_frame_header(&bytes).expect_err("width=0 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// キーフレームで height が 0 の場合は拒否する
#[test]
fn reject_keyframe_zero_height() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        height: 0,
        ..KeyframeParams::valid()
    });
    let err = parse_frame_header(&bytes).expect_err("height=0 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// キーフレームで width も height も 0 の場合は拒否する
#[test]
fn reject_keyframe_zero_width_and_height() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        width: 0,
        height: 0,
        ..KeyframeParams::valid()
    });
    let err = parse_frame_header(&bytes).expect_err("両方 0 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// interframe で first_partition_size が残入力を超える場合は拒否する
#[test]
fn reject_interframe_first_partition_size_overflow() {
    // 残入力 = 0 なのに first_partition_size = 1 を要求
    let bytes = build_interframe_bytes(0, true, 1);
    let err = parse_frame_header(&bytes).expect_err("残入力超過は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// キーフレームで first_partition_size が残入力を超える場合は拒否する
#[test]
fn reject_keyframe_first_partition_size_overflow() {
    // 残入力 = 0 なのに first_partition_size = 1 を要求
    let bytes = build_keyframe_bytes(KeyframeParams {
        first_partition_size: 1,
        ..KeyframeParams::valid()
    });
    let err = parse_frame_header(&bytes).expect_err("キーフレームの残入力超過は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// `first_partition_size` が残入力 + 1 (境界の 1 バイト超過) を拒否する
#[test]
fn reject_first_partition_size_boundary_plus_one() {
    let mut bytes = build_interframe_bytes(0, true, 129);
    bytes.resize(bytes.len() + 128, 0);
    let err =
        parse_frame_header(&bytes).expect_err("first_partition_size が残入力 + 1 なら拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

// ===== build_vp08_box =====

/// デフォルト config で `Vp08Box` を構築し、固定値が仕様どおりであることを検証する
#[test]
fn build_vp08_box_fixed_values() {
    let config = default_config();
    let vp08 = build_vp08_box(&config).expect("デフォルト config で構築できる");

    // vpcC の固定値
    assert_eq!(vp08.vpcc_box.profile, 0);
    assert_eq!(vp08.vpcc_box.bit_depth.get(), 8);
    assert_eq!(vp08.vpcc_box.chroma_subsampling.get(), 1);
    assert!(vp08.vpcc_box.codec_initialization_data.is_empty());

    // Visual フィールドのデフォルト
    assert_eq!(
        vp08.visual.horizresolution,
        VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION
    );
    assert_eq!(
        vp08.visual.vertresolution,
        VisualSampleEntryFields::DEFAULT_VERTRESOLUTION
    );
    assert_eq!(
        vp08.visual.frame_count,
        VisualSampleEntryFields::DEFAULT_FRAME_COUNT
    );
    assert_eq!(
        vp08.visual.compressorname,
        VisualSampleEntryFields::NULL_COMPRESSORNAME
    );
    assert_eq!(vp08.visual.depth, VisualSampleEntryFields::DEFAULT_DEPTH);

    // unknown_boxes は常に空
    assert!(vp08.unknown_boxes.is_empty());
}

/// `Vp8SampleEntryConfig` の各フィールドが `Vp08Box` に反映される
#[test]
fn build_vp08_box_propagates_config_fields() {
    let dri = NonZeroU16::new(3).expect("3 は非ゼロ");
    let config = Vp8SampleEntryConfig {
        level: Some(31),
        video_full_range_flag: true,
        colour_primaries: 9,
        transfer_characteristics: 16,
        matrix_coefficients: 9,
        width: 1920,
        height: 1080,
        data_reference_index: dri,
    };
    let vp08 = build_vp08_box(&config).expect("config を反映できる");
    assert_eq!(vp08.vpcc_box.level, 31);
    assert_eq!(vp08.vpcc_box.video_full_range_flag, Uint::new(1));
    assert_eq!(vp08.vpcc_box.colour_primaries, 9);
    assert_eq!(vp08.vpcc_box.transfer_characteristics, 16);
    assert_eq!(vp08.vpcc_box.matrix_coefficients, 9);
    assert_eq!(vp08.visual.width, 1920);
    assert_eq!(vp08.visual.height, 1080);
    assert_eq!(vp08.visual.data_reference_index, dri);
}

/// `level: None` は VpccBox の level に 0 (unspecified) として書き込まれる
#[test]
fn build_vp08_box_level_none_maps_to_zero() {
    let mut config = default_config();
    config.level = None;
    let vp08 = build_vp08_box(&config).expect("None level を受理");
    assert_eq!(vp08.vpcc_box.level, 0);
}

/// 構築した `Vp08Box` が encode → decode でラウンドトリップする
#[test]
fn build_vp08_box_roundtrip() {
    let dri = NonZeroU16::new(2).expect("2 は非ゼロ");
    let config = Vp8SampleEntryConfig {
        level: Some(10),
        video_full_range_flag: false,
        colour_primaries: 1,
        transfer_characteristics: 1,
        matrix_coefficients: 1,
        width: 640,
        height: 480,
        data_reference_index: dri,
    };
    let vp08 = build_vp08_box(&config).expect("構築成功");
    let encoded = vp08.encode_to_vec().expect("encode 成功");
    let (decoded, size) = Vp08Box::decode(&encoded).expect("decode 成功");
    assert_eq!(size, encoded.len());
    assert_eq!(decoded, vp08);
}

// ===== 実 VP8 keyframe fixture テスト =====

/// libvpx (ffmpeg 経由) で生成した VP8 キーフレームの生バイト列
///
/// 生成コマンド (README にも記載する想定):
///
/// ```text
/// ffmpeg -y -f lavfi -i color=black:size=320x240:duration=0.5:rate=30 \
///     -c:v libvpx -b:v 100k -deadline good -cpu-used 0 /tmp/black-vp8.webm
/// ffmpeg -y -i /tmp/black-vp8.webm -c copy -f ivf -vframes 1 /tmp/black-vp8-1frame.ivf
/// dd if=/tmp/black-vp8-1frame.ivf of=tests/testdata/black-vp8-keyframe.vp8 \
///     bs=1 skip=44
/// ```
///
/// (最初の `ffmpeg` で VP8 を WebM で mux、次で 1 フレームだけ IVF に再パック、
/// 最後の `dd` で IVF ヘッダー 32 + IVF フレームヘッダー 12 = 44 バイトを剥がして
/// 生 VP8 キーフレームだけを取り出す)
const REAL_VP8_KEYFRAME: &[u8] = include_bytes!("testdata/black-vp8-keyframe.vp8");

/// 実 libvpx 出力のキーフレームを解析できることを確認する
///
/// 手動構築ケースが RFC 6386 のビット配置解釈と一致していても、実 libvpx の出力とは
/// 別経路でズレることがあるので、実データで受理系のリグレッションを固定する
#[test]
fn real_libvpx_keyframe_parses() {
    let header = parse_frame_header(REAL_VP8_KEYFRAME)
        .expect("libvpx 生成の実 VP8 キーフレームは解析成功する");
    assert_eq!(header.frame_type, Vp8FrameType::Key);
    // 生成時の解像度 320x240 が復元される
    let key = header
        .keyframe
        .expect("キーフレームは keyframe フィールドを持つ");
    assert_eq!(key.width, 320);
    assert_eq!(key.height, 240);
    // libvpx v1.15 の VP8 出力は version = 0 / show_frame = true を書く
    assert_eq!(header.version, 0);
    assert!(header.show_frame);
    // horiz / vert スケールは通常 0 (フレーム寸法どおり)
    assert_eq!(key.horizontal_scale, 0);
    assert_eq!(key.vertical_scale, 0);
    // first_partition_size が入力サイズを超えないこと (parse 成功 = 境界内)
    assert!((header.first_partition_size as usize) <= REAL_VP8_KEYFRAME.len() - 10);
}
