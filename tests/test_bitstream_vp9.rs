//! `shiguredo_mp4::bitstream::vp9` の決定的テスト
//!
//! 手動構築した VP9 uncompressed header のビット列に対してパーサーの
//! 受理・拒否条件を固定する。実 libvpx 出力による fixture テストは
//! `tests/testdata/black-vp9-keyframe.vp9` を用いた別テストで補う

use shiguredo_mp4::{
    Decode, Encode, ErrorKind, Uint,
    bitstream::vp9::{
        Vp9FrameSize, Vp9FrameType, Vp9SampleEntryConfig, build_vp09_box, parse_frame_header,
    },
    boxes::{VisualSampleEntryFields, Vp09Box},
};

/// VP9 uncompressed header の MSB-first ビット組み立て用
///
/// 実装側の `BitReader` と対称なテスト用の bit writer。
/// バリデーションはせず、渡した値をそのままビット位置に詰める
#[derive(Debug, Clone, Default)]
struct BitWriter {
    bytes: Vec<u8>,
    bit_pos: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self::default()
    }

    /// n ビットの `value` を MSB-first で書き込む
    fn push_bits(&mut self, value: u32, n: u32) {
        for i in (0..n).rev() {
            let bit = ((value >> i) & 1) as u8;
            if self.bit_pos == 0 {
                self.bytes.push(0);
            }
            let last = self.bytes.len() - 1;
            self.bytes[last] |= bit << (7 - self.bit_pos);
            self.bit_pos = (self.bit_pos + 1) % 8;
        }
    }

    fn push_bit(&mut self, bit: u8) {
        self.push_bits(u32::from(bit), 1);
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.push_bits(u32::from(*b), 8);
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// キーフレーム構築用の設定値。デフォルト値は `KeyframeParams::valid()` を使う
#[derive(Debug, Clone, Copy)]
struct KeyframeParams {
    profile: u8,
    show_frame: bool,
    error_resilient_mode: bool,
    sync_code: [u8; 3],
    bit_depth_10_or_12_bit: Option<bool>, // profile >= 2 のときのみ書き込む (true = 12-bit、false = 10-bit)
    color_space: u8,
    color_range: u8,
    subsampling_x: u8,
    subsampling_y: u8,
    write_reserved_zero: Option<u8>, // profile 1/3 か sRGB 経由で reserved_zero を書き込むときの値 (通常 0)
    profile3_reserved_zero: u8,      // profile == 3 のときに書き込む reserved_zero
    frame_width: u32,
    frame_height: u32,
    render_and_frame_size_different: bool,
    render_width: u32,
    render_height: u32,
}

impl KeyframeParams {
    fn valid() -> Self {
        Self {
            profile: 0,
            show_frame: true,
            error_resilient_mode: false,
            sync_code: [0x49, 0x83, 0x42],
            bit_depth_10_or_12_bit: None,
            color_space: 1, // BT.601 相当
            color_range: 0,
            subsampling_x: 1,
            subsampling_y: 1,
            write_reserved_zero: None,
            profile3_reserved_zero: 0,
            frame_width: 320,
            frame_height: 240,
            render_and_frame_size_different: false,
            render_width: 0,
            render_height: 0,
        }
    }
}

/// キーフレームの uncompressed header バイト列を組み立てる
fn build_keyframe_bytes(p: KeyframeParams) -> Vec<u8> {
    let mut w = BitWriter::new();
    // frame_marker = 2
    w.push_bits(2, 2);
    // profile: low, high
    w.push_bit(p.profile & 1);
    w.push_bit((p.profile >> 1) & 1);
    if p.profile == 3 {
        w.push_bit(p.profile3_reserved_zero);
    }
    // show_existing_frame = 0
    w.push_bit(0);
    // frame_type = 0 (KEY)
    w.push_bit(0);
    w.push_bit(u8::from(p.show_frame));
    w.push_bit(u8::from(p.error_resilient_mode));
    // frame_sync_code
    w.push_bytes(&p.sync_code);
    // color_config
    if p.profile >= 2 {
        w.push_bit(u8::from(p.bit_depth_10_or_12_bit.unwrap_or(false)));
    }
    w.push_bits(u32::from(p.color_space), 3);
    if p.color_space != 7 {
        w.push_bit(p.color_range);
        if p.profile == 1 || p.profile == 3 {
            w.push_bit(p.subsampling_x);
            w.push_bit(p.subsampling_y);
            w.push_bit(p.write_reserved_zero.unwrap_or(0));
        }
    } else {
        // sRGB 経由の reserved_zero
        w.push_bit(p.write_reserved_zero.unwrap_or(0));
    }
    // frame_size
    w.push_bits((p.frame_width - 1) & 0xFFFF, 16);
    w.push_bits((p.frame_height - 1) & 0xFFFF, 16);
    // render_size
    w.push_bit(u8::from(p.render_and_frame_size_different));
    if p.render_and_frame_size_different {
        w.push_bits((p.render_width - 1) & 0xFFFF, 16);
        w.push_bits((p.render_height - 1) & 0xFFFF, 16);
    }
    w.into_bytes()
}

fn default_config() -> Vp9SampleEntryConfig {
    Vp9SampleEntryConfig {
        level: Vp9SampleEntryConfig::LEVEL_UNDEFINED,
        colour_primaries: 1,
        transfer_characteristics: 1,
        matrix_coefficients: 1,
        width: 320,
        height: 240,
    }
}

// ===== parse_frame_header: 受理系 (キーフレーム) =====

/// profile 0 のキーフレーム最小構成を解析できる
#[test]
fn keyframe_profile0_minimal_parse() {
    let bytes = build_keyframe_bytes(KeyframeParams::valid());
    let header = parse_frame_header(&bytes).expect("profile 0 キーフレームは解析成功する");
    assert_eq!(header.profile, 0);
    assert_eq!(header.show_existing_frame, None);
    assert_eq!(header.frame_type, Vp9FrameType::Key);
    assert!(header.show_frame);
    assert!(!header.error_resilient_mode);
    assert!(!header.intra_only);
    assert_eq!(header.refresh_frame_flags, 0xFF);
    assert_eq!(header.bit_depth, 8);
    assert_eq!(header.color_space, 1);
    assert_eq!(header.color_range, 0);
    assert_eq!(header.subsampling_x, 1);
    assert_eq!(header.subsampling_y, 1);
    assert_eq!(
        header.frame_size,
        Vp9FrameSize::Resolved {
            width: 320,
            height: 240,
        }
    );
    assert_eq!(header.render_size, None);
}

/// profile 2 で 10-bit / 12-bit の両方を解析できる
#[test]
fn keyframe_profile2_bit_depth_variations() {
    for (flag, expected_bit_depth) in [(false, 10u8), (true, 12u8)] {
        let bytes = build_keyframe_bytes(KeyframeParams {
            profile: 2,
            bit_depth_10_or_12_bit: Some(flag),
            ..KeyframeParams::valid()
        });
        let header = parse_frame_header(&bytes).expect("profile 2 は 10/12-bit を解析成功する");
        assert_eq!(header.profile, 2);
        assert_eq!(header.bit_depth, expected_bit_depth);
    }
}

/// profile 3 で reserved_zero == 0 が正しく受理される
#[test]
fn keyframe_profile3_reserved_zero_accepted() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 3,
        bit_depth_10_or_12_bit: Some(false), // 10-bit
        subsampling_x: 0,
        subsampling_y: 0,
        write_reserved_zero: Some(0),
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("profile 3 reserved_zero=0 は受理される");
    assert_eq!(header.profile, 3);
}

/// profile 1 で 4:2:2 (subsampling_x=1, subsampling_y=0) を解析できる
#[test]
fn keyframe_profile1_subsampling_422() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 1,
        subsampling_x: 1,
        subsampling_y: 0,
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("profile 1 4:2:2 は解析成功する");
    assert_eq!(header.subsampling_x, 1);
    assert_eq!(header.subsampling_y, 0);
}

/// profile 1 で 4:4:4 (subsampling_x=0, subsampling_y=0) を解析できる
#[test]
fn keyframe_profile1_subsampling_444() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 1,
        subsampling_x: 0,
        subsampling_y: 0,
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("profile 1 4:4:4 は解析成功する");
    assert_eq!(header.subsampling_x, 0);
    assert_eq!(header.subsampling_y, 0);
}

/// sRGB (color_space = 7) が profile 1 で受理され、4:4:4 / full range 固定になる
#[test]
fn keyframe_srgb_profile1_accepted() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 1,
        color_space: 7,
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("sRGB は profile 1 で受理される");
    assert_eq!(header.color_space, 7);
    assert_eq!(header.color_range, 1); // sRGB は常に full range
    assert_eq!(header.subsampling_x, 0); // sRGB は常に 4:4:4
    assert_eq!(header.subsampling_y, 0);
}

/// sRGB が profile 3 で受理され、4:4:4 / full range 固定になる
///
/// profile 3 は profile 予約ビットと 10/12-bit 選択ビットが追加される経路
#[test]
fn keyframe_srgb_profile3_accepted() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 3,
        bit_depth_10_or_12_bit: Some(false),
        color_space: 7,
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("sRGB は profile 3 で受理される");
    assert_eq!(header.profile, 3);
    assert_eq!(header.color_space, 7);
    assert_eq!(header.color_range, 1);
    assert_eq!(header.subsampling_x, 0);
    assert_eq!(header.subsampling_y, 0);
    assert_eq!(header.bit_depth, 10);
}

/// render_size が frame_size と異なる場合に (render_width, render_height) を復元する
#[test]
fn keyframe_render_size_different_is_preserved() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        render_and_frame_size_different: true,
        render_width: 1920,
        render_height: 1080,
        frame_width: 3840,
        frame_height: 2160,
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("render_size 有りキーフレームは解析成功する");
    assert_eq!(header.render_size, Some((1920, 1080)));
    assert_eq!(
        header.frame_size,
        Vp9FrameSize::Resolved {
            width: 3840,
            height: 2160,
        }
    );
}

/// frame_size の 16 ビット最大値 (65536x65536) を復元する
#[test]
fn keyframe_frame_size_maximum_is_preserved() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        frame_width: 65536,
        frame_height: 65536,
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("最大寸法キーフレームは解析成功する");
    assert_eq!(
        header.frame_size,
        Vp9FrameSize::Resolved {
            width: 65536,
            height: 65536,
        }
    );
}

// ===== parse_frame_header: 受理系 (show_existing_frame) =====

/// show_existing_frame = 1 の場合、frame_to_show_map_idx (0..=7) を返して他フィールドは既定値になる
#[test]
fn show_existing_frame_captures_map_idx() {
    for idx in 0u8..=7 {
        // show_existing_frame パターンの手組み: frame_marker + profile=0 + show_existing=1 + idx
        let mut w = BitWriter::new();
        w.push_bits(2, 2); // frame_marker
        w.push_bit(0); // profile low
        w.push_bit(0); // profile high (profile=0)
        w.push_bit(1); // show_existing_frame = 1
        w.push_bits(u32::from(idx), 3);
        let bytes = w.into_bytes();
        let header =
            parse_frame_header(&bytes).expect("show_existing_frame は末尾で追加 read せず成功する");
        assert_eq!(header.show_existing_frame, Some(idx));
        assert_eq!(header.profile, 0);
        assert_eq!(header.refresh_frame_flags, 0);
        assert_eq!(header.frame_size, Vp9FrameSize::NotPresent);
    }
}

// ===== parse_frame_header: 拒否系 =====

/// 入力が frame_marker を読むのに足りない場合は拒否する
#[test]
fn reject_input_too_short_for_frame_marker() {
    let bytes: Vec<u8> = Vec::new();
    let err = parse_frame_header(&bytes).expect_err("空入力は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// frame_marker != 2 は拒否する
#[test]
fn reject_wrong_frame_marker() {
    for marker in [0u32, 1, 3] {
        let mut w = BitWriter::new();
        w.push_bits(marker, 2);
        // 十分な後続ビットを埋める (frame_marker 検証で先に落ちる想定)
        w.push_bytes(&[0u8; 32]);
        let bytes = w.into_bytes();
        let err =
            parse_frame_header(&bytes).expect_err(&format!("frame_marker={marker} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// profile 3 の reserved_zero が 1 なら拒否する
#[test]
fn reject_profile3_reserved_zero_one() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 3,
        bit_depth_10_or_12_bit: Some(false),
        subsampling_x: 0,
        subsampling_y: 0,
        profile3_reserved_zero: 1,
        write_reserved_zero: Some(0),
        ..KeyframeParams::valid()
    });
    let err = parse_frame_header(&bytes).expect_err("profile 3 reserved_zero=1 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// キーフレームの sync_code が仕様値と異なる場合は拒否する
#[test]
fn reject_wrong_sync_code() {
    let variants: [[u8; 3]; 3] = [[0x00, 0x00, 0x00], [0x49, 0x83, 0x41], [0x48, 0x83, 0x42]];
    for sync in variants {
        let bytes = build_keyframe_bytes(KeyframeParams {
            sync_code: sync,
            ..KeyframeParams::valid()
        });
        let err =
            parse_frame_header(&bytes).expect_err(&format!("sync_code={sync:?} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// color_config の reserved_zero (profile 1/3) が 1 なら拒否する
#[test]
fn reject_color_config_reserved_zero_one() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 1,
        subsampling_x: 1,
        subsampling_y: 0,
        write_reserved_zero: Some(1),
        ..KeyframeParams::valid()
    });
    let err = parse_frame_header(&bytes).expect_err("color_config reserved_zero=1 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// sRGB 経路の color_config reserved_zero が 1 なら拒否する
///
/// 非 sRGB の reserved_zero 拒否とは別分岐 (profile 1/3 で color_space=7 のとき
/// reserved_zero 1 ビットだけを読む)
#[test]
fn reject_srgb_color_config_reserved_zero_one() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 1,
        color_space: 7,
        write_reserved_zero: Some(1),
        ..KeyframeParams::valid()
    });
    let err = parse_frame_header(&bytes).expect_err("sRGB 経路の reserved_zero=1 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// profile 1/3 で `subsampling_x = 1 && subsampling_y = 1` (4:2:0) は仕様上許されないため拒否する
///
/// VP9 spec Section 7.2.2 の bitstream conformance と libvpx の
/// "4:2:0 color not supported in profile 1 or 3" に対応する
#[test]
fn reject_profile1_or_3_with_subsampling_420() {
    for profile in [1u8, 3u8] {
        let bytes = build_keyframe_bytes(KeyframeParams {
            profile,
            bit_depth_10_or_12_bit: Some(false),
            subsampling_x: 1,
            subsampling_y: 1,
            ..KeyframeParams::valid()
        });
        let err = parse_frame_header(&bytes)
            .expect_err(&format!("profile {profile} で 4:2:0 は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// profile 1 で 4:4:0 (subsampling_x=0, subsampling_y=1) は VP9 spec 上は合法なので受理する
///
/// VP Codec ISO Media File Format Binding には 4:4:0 に対応する値がないので
/// `build_vp09_box` 側で Err になる (別テストで確認)
#[test]
fn accept_profile1_subsampling_440() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 1,
        subsampling_x: 0,
        subsampling_y: 1,
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("profile 1 4:4:0 は解析成功する");
    assert_eq!(header.subsampling_x, 0);
    assert_eq!(header.subsampling_y, 1);
}

/// sRGB (color_space=7) を profile 0 で使うと拒否する (profile 1 or 3 のみ許容)
#[test]
fn reject_srgb_on_profile0() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 0,
        color_space: 7,
        ..KeyframeParams::valid()
    });
    let err = parse_frame_header(&bytes).expect_err("sRGB を profile 0 に指定すると拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// sRGB (color_space=7) を profile 2 で使うと拒否する (profile 1 or 3 のみ許容)
#[test]
fn reject_srgb_on_profile2() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 2,
        bit_depth_10_or_12_bit: Some(false),
        color_space: 7,
        ..KeyframeParams::valid()
    });
    let err = parse_frame_header(&bytes).expect_err("sRGB を profile 2 に指定すると拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// uncompressed header の途中で入力が切れると拒否する
#[test]
fn reject_truncated_header() {
    // profile 0 キーフレームの正常バイト列を組み立てて途中で切る
    let full = build_keyframe_bytes(KeyframeParams::valid());
    // frame_marker + profile 分だけ残して以降をカット (4 バイト目以降で必ずヘッダー途中)
    let truncated = &full[..3];
    let err = parse_frame_header(truncated).expect_err("切り詰められた入力は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

// ===== build_vp09_box =====

/// デフォルト config + キーフレーム header で構築した Vp09Box が固定値と導出値を正しく持つ
#[test]
fn build_vp09_box_fixed_and_derived_values() {
    let bytes = build_keyframe_bytes(KeyframeParams::valid());
    let header = parse_frame_header(&bytes).expect("キーフレーム解析");
    let config = default_config();
    let vp09 = build_vp09_box(&header, &config).expect("有効な header は構築成功する");

    // ストリーム導出値
    assert_eq!(vp09.vpcc_box.profile, 0);
    assert_eq!(vp09.vpcc_box.bit_depth.get(), 8);
    assert_eq!(vp09.vpcc_box.chroma_subsampling.get(), 1); // (1,1) → colocated
    assert_eq!(vp09.vpcc_box.video_full_range_flag.get(), 0);

    // 固定値
    assert_eq!(vp09.vpcc_box.level, Vp9SampleEntryConfig::LEVEL_UNDEFINED);
    assert!(vp09.vpcc_box.codec_initialization_data.is_empty());
    assert_eq!(
        vp09.visual.data_reference_index,
        VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX
    );
    assert_eq!(
        vp09.visual.horizresolution,
        VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION
    );
    assert_eq!(
        vp09.visual.vertresolution,
        VisualSampleEntryFields::DEFAULT_VERTRESOLUTION
    );
    assert_eq!(
        vp09.visual.frame_count,
        VisualSampleEntryFields::DEFAULT_FRAME_COUNT
    );
    assert_eq!(
        vp09.visual.compressorname,
        VisualSampleEntryFields::NULL_COMPRESSORNAME
    );
    assert_eq!(vp09.visual.depth, VisualSampleEntryFields::DEFAULT_DEPTH);
    assert!(vp09.unknown_boxes.is_empty());

    // 呼び出し側指定値
    assert_eq!(vp09.vpcc_box.colour_primaries, 1);
    assert_eq!(vp09.vpcc_box.transfer_characteristics, 1);
    assert_eq!(vp09.vpcc_box.matrix_coefficients, 1);
    assert_eq!(vp09.visual.width, 320);
    assert_eq!(vp09.visual.height, 240);
}

/// profile 1 の 4:2:2 header が chroma_subsampling = 2 (4:2:2) に写る
#[test]
fn build_vp09_box_chroma_subsampling_422() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 1,
        subsampling_x: 1,
        subsampling_y: 0,
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("profile 1 4:2:2");
    let vp09 = build_vp09_box(&header, &default_config()).expect("有効な header は構築成功する");
    assert_eq!(vp09.vpcc_box.chroma_subsampling.get(), 2);
}

/// profile 1 の 4:4:0 header は Vp09Box に格納できないため Err を返す
///
/// VP9 spec では合法だが、VP Codec ISO Media File Format Binding の
/// chroma_subsampling 3 ビット値に対応する値が存在しない
#[test]
fn build_vp09_box_rejects_440_subsampling() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 1,
        subsampling_x: 0,
        subsampling_y: 1,
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("profile 1 4:4:0 は解析成功する");
    let err = build_vp09_box(&header, &default_config())
        .expect_err("4:4:0 は Vp09Box に格納不能なので Err");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// profile 1 の 4:4:4 header が chroma_subsampling = 3 (4:4:4) に写る
#[test]
fn build_vp09_box_chroma_subsampling_444() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 1,
        subsampling_x: 0,
        subsampling_y: 0,
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("profile 1 4:4:4");
    let vp09 = build_vp09_box(&header, &default_config()).expect("有効な header は構築成功する");
    assert_eq!(vp09.vpcc_box.chroma_subsampling.get(), 3);
}

/// sRGB キーフレームで color_range = 1 (full range) が反映される
#[test]
fn build_vp09_box_srgb_full_range() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 1,
        color_space: 7,
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("sRGB キーフレーム");
    let vp09 = build_vp09_box(&header, &default_config()).expect("有効な header は構築成功する");
    assert_eq!(vp09.vpcc_box.video_full_range_flag, Uint::new(1));
    assert_eq!(vp09.vpcc_box.chroma_subsampling.get(), 3); // sRGB → 4:4:4
}

/// config の色特性が Vp09Box に反映される
#[test]
fn build_vp09_box_config_colour_reflected() {
    let bytes = build_keyframe_bytes(KeyframeParams::valid());
    let header = parse_frame_header(&bytes).expect("キーフレーム");
    let config = Vp9SampleEntryConfig {
        level: 31,
        colour_primaries: Vp9SampleEntryConfig::COLOUR_PRIMARIES_BT2020,
        transfer_characteristics: Vp9SampleEntryConfig::TRANSFER_CHARACTERISTICS_BT2020,
        matrix_coefficients: Vp9SampleEntryConfig::MATRIX_COEFFICIENTS_BT2020,
        width: 1920,
        height: 1080,
    };
    let vp09 = build_vp09_box(&header, &config).expect("有効な header は構築成功する");
    assert_eq!(vp09.vpcc_box.level, 31);
    assert_eq!(vp09.vpcc_box.colour_primaries, 9);
    assert_eq!(vp09.vpcc_box.transfer_characteristics, 14);
    assert_eq!(vp09.vpcc_box.matrix_coefficients, 9);
    assert_eq!(vp09.visual.width, 1920);
    assert_eq!(vp09.visual.height, 1080);
}

/// 構築した Vp09Box が encode → decode でラウンドトリップする
#[test]
fn build_vp09_box_roundtrip() {
    let bytes = build_keyframe_bytes(KeyframeParams {
        profile: 2,
        bit_depth_10_or_12_bit: Some(false), // 10-bit
        ..KeyframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("profile 2 10-bit キーフレーム");
    let config = Vp9SampleEntryConfig {
        level: 41,
        colour_primaries: Vp9SampleEntryConfig::COLOUR_PRIMARIES_BT709,
        transfer_characteristics: Vp9SampleEntryConfig::TRANSFER_CHARACTERISTICS_BT709,
        matrix_coefficients: Vp9SampleEntryConfig::MATRIX_COEFFICIENTS_BT709,
        width: 640,
        height: 480,
    };
    let vp09 = build_vp09_box(&header, &config).expect("有効な header は構築成功する");
    let encoded = vp09.encode_to_vec().expect("encode 成功");
    let (decoded, size) = Vp09Box::decode(&encoded).expect("decode 成功");
    assert_eq!(size, encoded.len());
    assert_eq!(decoded, vp09);
}

// ===== 実 VP9 keyframe fixture テスト =====

/// libvpx-vp9 (ffmpeg 経由) で生成した VP9 キーフレームの生バイト列
///
/// 生成環境: ffmpeg 7.1.1 + libvpx 1.15.1
///
/// 生成コマンド:
///
/// ```text
/// ffmpeg -y -f lavfi -i color=black:size=320x240:duration=0.5:rate=30 \
///     -c:v libvpx-vp9 -b:v 100k -deadline good -cpu-used 0 /tmp/black-vp9.webm
/// ffmpeg -y -i /tmp/black-vp9.webm -c copy -f ivf -vframes 1 /tmp/black-vp9-1frame.ivf
/// dd if=/tmp/black-vp9-1frame.ivf of=tests/testdata/black-vp9-keyframe.vp9 \
///     bs=1 skip=44
/// ```
///
/// (最初の `ffmpeg` で VP9 を WebM で mux、次で 1 フレームだけ IVF に再パック、
/// 最後の `dd` で IVF ヘッダー 32 + IVF フレームヘッダー 12 = 44 バイトを剥がして
/// 生 VP9 キーフレームだけを取り出す)
const REAL_VP9_KEYFRAME: &[u8] = include_bytes!("testdata/black-vp9-keyframe.vp9");

/// 実 libvpx-vp9 出力のキーフレームを解析できることを確認する
///
/// 手動構築ケースが VP9 spec のビット配置解釈と一致していても、実 libvpx-vp9 の
/// 出力とは別経路でズレることがあるので、実データで受理系のリグレッションを固定する
#[test]
fn real_libvpx_keyframe_parses() {
    let header = parse_frame_header(REAL_VP9_KEYFRAME)
        .expect("libvpx-vp9 生成の実 VP9 キーフレームは解析成功する");
    assert_eq!(header.frame_type, Vp9FrameType::Key);
    assert_eq!(header.show_existing_frame, None);
    // 生成時の profile / bit_depth / subsampling は既定 (profile 0 = 8-bit 4:2:0)
    assert_eq!(header.profile, 0);
    assert_eq!(header.bit_depth, 8);
    assert_eq!(header.refresh_frame_flags, 0xFF);
    assert_eq!(header.subsampling_x, 1);
    assert_eq!(header.subsampling_y, 1);
    // 生成時の解像度 320x240 が復元される
    assert_eq!(
        header.frame_size,
        Vp9FrameSize::Resolved {
            width: 320,
            height: 240,
        }
    );
    // 生成時の fixture では show_frame = true / error_resilient_mode = false / intra_only = false
    assert!(header.show_frame);
    assert!(!header.error_resilient_mode);
    assert!(!header.intra_only);
    // 生成時の fixture では render_size は frame_size と同一なので None
    assert_eq!(header.render_size, None);
    // 生成時の fixture では color_space = 0 (Unknown、libvpx-vp9 の既定)、color_range = 0 (studio swing)
    assert_eq!(header.color_space, 0);
    assert_eq!(header.color_range, 0);
}

// ===== intra-only frame テスト =====

/// intra-only frame の uncompressed header を組み立てる
struct IntraOnlyParams {
    profile: u8,
    error_resilient_mode: bool,
    sync_code: [u8; 3],
    bit_depth_10_or_12_bit: bool, // profile >= 2 のときのみ書き込む
    color_space: u8,
    color_range: u8,
    subsampling_x: u8,
    subsampling_y: u8,
    refresh_frame_flags: u8,
    frame_width: u32,
    frame_height: u32,
    render_and_frame_size_different: bool,
    render_width: u32,
    render_height: u32,
}

impl IntraOnlyParams {
    fn valid() -> Self {
        Self {
            profile: 0,
            error_resilient_mode: false,
            sync_code: [0x49, 0x83, 0x42],
            bit_depth_10_or_12_bit: false,
            color_space: 1,
            color_range: 0,
            subsampling_x: 1,
            subsampling_y: 1,
            refresh_frame_flags: 0,
            frame_width: 320,
            frame_height: 240,
            render_and_frame_size_different: false,
            render_width: 0,
            render_height: 0,
        }
    }
}

fn build_intra_only_bytes(p: IntraOnlyParams) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.push_bits(2, 2); // frame_marker
    w.push_bit(p.profile & 1);
    w.push_bit((p.profile >> 1) & 1);
    if p.profile == 3 {
        w.push_bit(0);
    }
    w.push_bit(0); // show_existing_frame
    w.push_bit(1); // frame_type = 1 (NonKey)
    w.push_bit(0); // show_frame = 0
    w.push_bit(u8::from(p.error_resilient_mode));
    w.push_bit(1); // intra_only = 1
    if !p.error_resilient_mode {
        w.push_bits(0, 2); // reset_frame_context
    }
    w.push_bytes(&p.sync_code);
    if p.profile > 0 {
        if p.profile >= 2 {
            w.push_bit(u8::from(p.bit_depth_10_or_12_bit));
        }
        w.push_bits(u32::from(p.color_space), 3);
        if p.color_space != 7 {
            w.push_bit(p.color_range);
            if p.profile == 1 || p.profile == 3 {
                w.push_bit(p.subsampling_x);
                w.push_bit(p.subsampling_y);
                w.push_bit(0); // reserved_zero
            }
        } else {
            w.push_bit(0); // reserved_zero (sRGB path)
        }
    }
    w.push_bits(u32::from(p.refresh_frame_flags), 8);
    w.push_bits((p.frame_width - 1) & 0xFFFF, 16);
    w.push_bits((p.frame_height - 1) & 0xFFFF, 16);
    w.push_bit(u8::from(p.render_and_frame_size_different));
    if p.render_and_frame_size_different {
        w.push_bits((p.render_width - 1) & 0xFFFF, 16);
        w.push_bits((p.render_height - 1) & 0xFFFF, 16);
    }
    w.into_bytes()
}

/// profile 0 の intra-only frame は color_config を持たず、デフォルト値
/// (bit_depth=8, color_space=1, color_range=0, subsampling=(1,1)) が入る
#[test]
fn intra_only_profile0_uses_default_color() {
    let bytes = build_intra_only_bytes(IntraOnlyParams::valid());
    let header = parse_frame_header(&bytes).expect("intra-only profile 0 は解析成功する");
    assert_eq!(header.frame_type, Vp9FrameType::NonKey);
    assert!(!header.show_frame);
    assert!(header.intra_only);
    assert_eq!(header.profile, 0);
    // profile 0 の intra-only では color_config が書かれないのでデフォルトが入る
    assert_eq!(header.bit_depth, 8);
    assert_eq!(header.color_space, 1);
    assert_eq!(header.color_range, 0);
    assert_eq!(header.subsampling_x, 1);
    assert_eq!(header.subsampling_y, 1);
    assert_eq!(
        header.frame_size,
        Vp9FrameSize::Resolved {
            width: 320,
            height: 240,
        }
    );
    assert_eq!(header.refresh_frame_flags, 0);
}

/// profile 0 の intra-only header で `build_vp09_box` が成功する
///
/// rustdoc は intra_only を sample entry の代表フレームとして認めており、
/// profile 0 では color_config が無くても既定の bit_depth=8 が入る
#[test]
fn build_vp09_box_accepts_intra_only_header() {
    let bytes = build_intra_only_bytes(IntraOnlyParams::valid());
    let header = parse_frame_header(&bytes).expect("intra-only profile 0 は解析成功する");
    let vp09 =
        build_vp09_box(&header, &default_config()).expect("intra-only header は構築成功する");
    assert_eq!(vp09.vpcc_box.profile, 0);
    assert_eq!(vp09.vpcc_box.bit_depth.get(), 8);
    assert_eq!(vp09.vpcc_box.chroma_subsampling.get(), 1);
}

/// intra-only frame の render_size が frame_size と異なるときにその値が復元される
#[test]
fn intra_only_render_size_different_is_preserved() {
    let bytes = build_intra_only_bytes(IntraOnlyParams {
        render_and_frame_size_different: true,
        render_width: 1920,
        render_height: 1080,
        ..IntraOnlyParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("render_size 有り intra-only は解析成功");
    assert!(header.intra_only);
    assert_eq!(header.render_size, Some((1920, 1080)));
}

/// profile 2 の intra-only frame は color_config を読み、bit_depth と color を復元する
#[test]
fn intra_only_profile2_reads_color_config() {
    for (flag, expected_bit_depth) in [(false, 10u8), (true, 12u8)] {
        let bytes = build_intra_only_bytes(IntraOnlyParams {
            profile: 2,
            bit_depth_10_or_12_bit: flag,
            color_space: 5, // BT.2020
            color_range: 1,
            ..IntraOnlyParams::valid()
        });
        let header = parse_frame_header(&bytes).unwrap_or_else(|_| {
            panic!("intra-only profile 2 の {expected_bit_depth}-bit は解析成功する")
        });
        assert!(header.intra_only);
        assert_eq!(header.bit_depth, expected_bit_depth);
        assert_eq!(header.color_space, 5);
        assert_eq!(header.color_range, 1);
    }
}

/// intra-only frame は error_resilient_mode でも parse できる (reset_frame_context を読み飛ばさない経路)
#[test]
fn intra_only_error_resilient_mode() {
    let bytes = build_intra_only_bytes(IntraOnlyParams {
        error_resilient_mode: true,
        ..IntraOnlyParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("error_resilient=true の intra-only は解析成功");
    assert!(header.intra_only);
    assert!(header.error_resilient_mode);
}

/// intra-only frame の `refresh_frame_flags` が header から復元される
#[test]
fn intra_only_refresh_frame_flags_are_preserved() {
    let bytes = build_intra_only_bytes(IntraOnlyParams {
        refresh_frame_flags: 0xA5,
        ..IntraOnlyParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("intra-only は解析成功する");
    assert_eq!(header.refresh_frame_flags, 0xA5);
}

/// intra-only frame の sync_code が不一致なら拒否する
#[test]
fn reject_intra_only_wrong_sync_code() {
    let bytes = build_intra_only_bytes(IntraOnlyParams {
        sync_code: [0x00, 0x00, 0x00],
        ..IntraOnlyParams::valid()
    });
    let err = parse_frame_header(&bytes).expect_err("intra-only の sync_code 不一致は拒否");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

// ===== inter frame テスト =====

/// inter frame の uncompressed header を組み立てる
struct InterframeParams {
    profile: u8,
    show_frame: bool,
    error_resilient_mode: bool,
    refresh_frame_flags: u8,
    ref_frame_idx: [u8; 3], // 各 0..=7
    ref_frame_sign_bias: [u8; 3],
    /// found_ref[i] が最初に true になった時点でその後の found_ref は書かない
    /// (VP9 spec Section 6.2.5 の break セマンティクスと一致させる)
    found_ref: [bool; 3],
    /// found_ref すべて false のときのみ書き込まれる
    frame_width: u32,
    frame_height: u32,
    render_and_frame_size_different: bool,
    render_width: u32,
    render_height: u32,
}

impl InterframeParams {
    fn valid() -> Self {
        Self {
            profile: 0,
            show_frame: true,
            error_resilient_mode: false,
            refresh_frame_flags: 0,
            ref_frame_idx: [0, 1, 2],
            ref_frame_sign_bias: [0, 0, 0],
            found_ref: [false, false, false],
            frame_width: 320,
            frame_height: 240,
            render_and_frame_size_different: false,
            render_width: 0,
            render_height: 0,
        }
    }
}

fn build_interframe_bytes(p: InterframeParams) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.push_bits(2, 2); // frame_marker
    w.push_bit(p.profile & 1);
    w.push_bit((p.profile >> 1) & 1);
    if p.profile == 3 {
        w.push_bit(0);
    }
    w.push_bit(0); // show_existing_frame
    w.push_bit(1); // frame_type = 1 (NonKey)
    w.push_bit(u8::from(p.show_frame));
    w.push_bit(u8::from(p.error_resilient_mode));
    if !p.show_frame {
        w.push_bit(0); // intra_only = 0 (inter)
    }
    if !p.error_resilient_mode {
        w.push_bits(0, 2); // reset_frame_context
    }
    w.push_bits(u32::from(p.refresh_frame_flags), 8);
    for i in 0..3 {
        w.push_bits(u32::from(p.ref_frame_idx[i]), 3);
        w.push_bit(p.ref_frame_sign_bias[i]);
    }
    // frame_size_with_refs
    let mut broke = false;
    for i in 0..3 {
        w.push_bit(u8::from(p.found_ref[i]));
        if p.found_ref[i] {
            broke = true;
            break;
        }
    }
    if !broke {
        w.push_bits((p.frame_width - 1) & 0xFFFF, 16);
        w.push_bits((p.frame_height - 1) & 0xFFFF, 16);
    }
    w.push_bit(u8::from(p.render_and_frame_size_different));
    if p.render_and_frame_size_different {
        w.push_bits((p.render_width - 1) & 0xFFFF, 16);
        w.push_bits((p.render_height - 1) & 0xFFFF, 16);
    }
    w.into_bytes()
}

/// inter frame で found_ref[0] = true のとき、その slot (ref_frame_idx[0]) を UsesRefFrames に格納する
#[test]
fn interframe_uses_first_ref_slot() {
    let bytes = build_interframe_bytes(InterframeParams {
        ref_frame_idx: [5, 1, 2],
        found_ref: [true, false, false],
        ..InterframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("inter frame は解析成功する");
    assert_eq!(header.frame_type, Vp9FrameType::NonKey);
    assert!(!header.intra_only);
    assert_eq!(
        header.frame_size,
        Vp9FrameSize::UsesRefFrames { ref_frame_slot: 5 }
    );
}

/// inter frame で found_ref[1] = true (先頭 false) のときは 2 番目の ref_frame_idx を採用する
#[test]
fn interframe_uses_second_ref_slot() {
    let bytes = build_interframe_bytes(InterframeParams {
        ref_frame_idx: [0, 7, 3],
        found_ref: [false, true, false],
        ..InterframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("inter frame は解析成功する");
    assert_eq!(
        header.frame_size,
        Vp9FrameSize::UsesRefFrames { ref_frame_slot: 7 }
    );
}

/// inter frame で found_ref すべて false のときは明示的 frame_size を Resolved で読む
#[test]
fn interframe_all_zero_found_ref_reads_explicit_frame_size() {
    let bytes = build_interframe_bytes(InterframeParams {
        found_ref: [false, false, false],
        frame_width: 1280,
        frame_height: 720,
        ..InterframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("found_ref 全 0 の inter frame は解析成功");
    assert_eq!(
        header.frame_size,
        Vp9FrameSize::Resolved {
            width: 1280,
            height: 720,
        }
    );
}

/// inter frame の色設定 (bit_depth / color_space / color_range / subsampling) は header に無いので 0 プレースホルダで返る
#[test]
fn interframe_color_fields_are_placeholder_zero() {
    let bytes = build_interframe_bytes(InterframeParams::valid());
    let header = parse_frame_header(&bytes).expect("inter frame は解析成功する");
    assert_eq!(header.bit_depth, 0);
    assert_eq!(header.color_space, 0);
    assert_eq!(header.color_range, 0);
    assert_eq!(header.subsampling_x, 0);
    assert_eq!(header.subsampling_y, 0);
}

/// inter frame の `refresh_frame_flags` が header から復元される
#[test]
fn interframe_refresh_frame_flags_are_preserved() {
    let bytes = build_interframe_bytes(InterframeParams {
        refresh_frame_flags: 0x3C,
        ..InterframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("inter frame は解析成功する");
    assert_eq!(header.refresh_frame_flags, 0x3C);
}

/// inter frame の header (色情報が 0 プレースホルダ) を build_vp09_box に渡すと Err
///
/// silent に 4:4:4 として書き出さないよう入り口で拒否する
#[test]
fn build_vp09_box_rejects_inter_frame_header() {
    let bytes = build_interframe_bytes(InterframeParams::valid());
    let header = parse_frame_header(&bytes).expect("inter frame は解析成功する");
    assert_eq!(header.bit_depth, 0); // inter は 0 プレースホルダ
    let err = build_vp09_box(&header, &default_config())
        .expect_err("inter frame header は build_vp09_box で拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// show_existing_frame の header (色情報なし) を build_vp09_box に渡すと Err
#[test]
fn build_vp09_box_rejects_show_existing_frame_header() {
    // show_existing_frame パターンを手組み
    let mut w = BitWriter::new();
    w.push_bits(2, 2); // frame_marker
    w.push_bit(0); // profile low
    w.push_bit(0); // profile high
    w.push_bit(1); // show_existing_frame = 1
    w.push_bits(0, 3); // frame_to_show_map_idx = 0
    let bytes = w.into_bytes();
    let header = parse_frame_header(&bytes).expect("show_existing_frame は解析成功する");
    assert_eq!(header.show_existing_frame, Some(0));
    let err = build_vp09_box(&header, &default_config())
        .expect_err("show_existing_frame header は build_vp09_box で拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// Vp9FrameHeader を手組みで subsampling_x/y に 2 以上を入れた場合、panic せず Err
///
/// pub フィールド経由の異常構築でも公開 API が panic しないことを担保する
#[test]
fn build_vp09_box_rejects_out_of_range_subsampling() {
    let bytes = build_keyframe_bytes(KeyframeParams::valid());
    let mut header = parse_frame_header(&bytes).expect("キーフレーム");
    header.subsampling_x = 2; // 手組みで範囲外を入れる
    let err = build_vp09_box(&header, &default_config())
        .expect_err("範囲外 subsampling は panic ではなく Err で拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// Vp9FrameHeader を手組みで bit_depth に 8/10/12 以外を入れた場合、panic せず Err
#[test]
fn build_vp09_box_rejects_invalid_bit_depth() {
    let bytes = build_keyframe_bytes(KeyframeParams::valid());
    for bit_depth in [9u8, 16, 255] {
        let mut header = parse_frame_header(&bytes).expect("キーフレーム");
        header.bit_depth = bit_depth;
        let err = build_vp09_box(&header, &default_config()).expect_err(&format!(
            "bit_depth={bit_depth} は panic ではなく Err で拒否される"
        ));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// Vp9FrameHeader を手組みで color_range に 2 以上を入れた場合、panic せず Err
#[test]
fn build_vp09_box_rejects_invalid_color_range() {
    let bytes = build_keyframe_bytes(KeyframeParams::valid());
    let mut header = parse_frame_header(&bytes).expect("キーフレーム");
    header.color_range = 2;
    let err = build_vp09_box(&header, &default_config())
        .expect_err("範囲外 color_range は panic ではなく Err で拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// Vp9FrameHeader を手組みで profile に 4 以上を入れた場合、panic せず Err
#[test]
fn build_vp09_box_rejects_invalid_profile() {
    let bytes = build_keyframe_bytes(KeyframeParams::valid());
    let mut header = parse_frame_header(&bytes).expect("キーフレーム");
    header.profile = 4;
    let err = build_vp09_box(&header, &default_config())
        .expect_err("範囲外 profile は panic ではなく Err で拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// inter frame の render_size が frame_size と異なるときにその値が復元される
#[test]
fn interframe_render_size_different_is_preserved() {
    let bytes = build_interframe_bytes(InterframeParams {
        render_and_frame_size_different: true,
        render_width: 1920,
        render_height: 1080,
        ..InterframeParams::valid()
    });
    let header = parse_frame_header(&bytes).expect("render_size 有り inter frame は解析成功");
    assert_eq!(header.render_size, Some((1920, 1080)));
}
