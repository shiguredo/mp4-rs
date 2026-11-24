//! `shiguredo_mp4::bitstream::vp9` の Property-Based Testing
//!
//! 手動構築の VP9 uncompressed header のビット配置を noprop サンプラーで
//! ランダム生成し、`parse_frame_header` が VP9 spec どおりに復元することを検証する。

use shiguredo_mp4::{
    Decode, Encode,
    bitstream::vp9::{
        Vp9FrameSize, Vp9FrameType, Vp9SampleEntryConfig, build_vp09_box, parse_frame_header,
    },
    boxes::Vp09Box,
};

/// このファイルの PBT ケース数
const CASES: usize = 500;

/// VP9 uncompressed header の MSB-first ビット組み立て
#[derive(Debug, Clone, Default)]
struct BitWriter {
    bytes: Vec<u8>,
    bit_pos: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self::default()
    }

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

/// キーフレームの uncompressed header をパラメタから組み立てる。バリデーションはしない
struct KeyframeBits {
    profile: u8,
    show_frame: bool,
    error_resilient_mode: bool,
    bit_depth_10_or_12_bit: bool, // profile >= 2 で使用 (true = 12-bit、false = 10-bit)
    color_space: u8,
    color_range: u8,
    subsampling_x: u8,
    subsampling_y: u8,
    frame_width: u32,  // 1..=65536
    frame_height: u32, // 1..=65536
    render_and_frame_size_different: bool,
    render_width: u32,  // render_and_frame_size_different のとき使用
    render_height: u32, // 同上
}

fn build_keyframe_bytes(p: &KeyframeBits) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.push_bits(2, 2); // frame_marker = 2
    w.push_bit(p.profile & 1); // profile low
    w.push_bit((p.profile >> 1) & 1); // profile high
    if p.profile == 3 {
        w.push_bit(0); // profile 3 reserved_zero = 0
    }
    w.push_bit(0); // show_existing_frame = 0
    w.push_bit(0); // frame_type = 0 (KEY)
    w.push_bit(u8::from(p.show_frame));
    w.push_bit(u8::from(p.error_resilient_mode));
    w.push_bytes(&[0x49, 0x83, 0x42]); // sync_code
    if p.profile >= 2 {
        w.push_bit(u8::from(p.bit_depth_10_or_12_bit));
    }
    w.push_bits(u32::from(p.color_space), 3);
    if p.color_space != 7 {
        w.push_bit(p.color_range);
        if p.profile == 1 || p.profile == 3 {
            w.push_bit(p.subsampling_x);
            w.push_bit(p.subsampling_y);
            w.push_bit(0); // color_config reserved_zero = 0
        }
    } else {
        // sRGB 経路の reserved_zero
        w.push_bit(0);
    }
    w.push_bits((p.frame_width - 1) & 0xFFFF, 16);
    w.push_bits((p.frame_height - 1) & 0xFFFF, 16);
    w.push_bit(u8::from(p.render_and_frame_size_different));
    if p.render_and_frame_size_different {
        w.push_bits((p.render_width - 1) & 0xFFFF, 16);
        w.push_bits((p.render_height - 1) & 0xFFFF, 16);
    }
    w.into_bytes()
}

/// キーフレーム header のビット配置が profile / color 系 / 寸法・render_size を含めて往復する
///
/// - `profile` は 0..=3 の 4 値を境界化しつつ、profile ≥ 2 のときに 10/12-bit を切り替える
/// - `color_space` は 0..=6 を境界化 (sRGB=7 は固定レイアウトのため単体テストで確認する)
/// - `subsampling` は profile 1/3 で `(1,0)` / `(0,1)` / `(0,0)` の 3 通り
///   (4:2:0 (1,1) は VP9 spec で不許可、profile 0/2 は `(1,1)` 固定)
/// - `frame_width` / `frame_height` は 16 ビット境界 (`1` と `65536`) を境界指定
/// - `render_size` の有無は両値サンプル
#[test]
fn keyframe_bit_layout_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let profile = noprop::sample_u64_in(ctx, 0..=3) as u8;
        let show_frame = noprop::sample_bool(ctx);
        let error_resilient_mode = noprop::sample_bool(ctx);
        let bit_depth_10_or_12_bit = noprop::sample_bool(ctx);
        // color_space は 0..=6。sRGB=7 は reserved / 4:4:4 固定で、単体テストで確認する
        let color_space = noprop::sample_u64_in(ctx, 0..=6) as u8;
        let color_range = noprop::sample_u64_in(ctx, 0..=1) as u8;
        // profile 1/3 は 4:2:2 / 4:4:0 / 4:4:4 の 3 通り。
        // (1,1) = 4:2:0 は VP9 spec Section 7.2.2 の bitstream conformance で不許可なので除外
        let (subsampling_x, subsampling_y) = if profile == 1 || profile == 3 {
            match noprop::sample_u64_in(ctx, 0..=2) {
                0 => (1u8, 0u8), // 4:2:2
                1 => (0u8, 1u8), // 4:4:0 (Binding に写せないので後続の build 側で拒否される)
                _ => (0u8, 0u8), // 4:4:4
            }
        } else {
            (1u8, 1u8) // profile 0/2 は 4:2:0 固定
        };
        let frame_width =
            noprop::sample_with_boundaries(ctx, &[1u32, 65536], noprop::Ratio::one_nth(4), |ctx| {
                noprop::sample_u64_in(ctx, 1..=65536) as u32
            });
        let frame_height =
            noprop::sample_with_boundaries(ctx, &[1u32, 65536], noprop::Ratio::one_nth(4), |ctx| {
                noprop::sample_u64_in(ctx, 1..=65536) as u32
            });
        let render_and_frame_size_different = noprop::sample_bool(ctx);
        let render_width = noprop::sample_u64_in(ctx, 1..=65536) as u32;
        let render_height = noprop::sample_u64_in(ctx, 1..=65536) as u32;

        let params = KeyframeBits {
            profile,
            show_frame,
            error_resilient_mode,
            bit_depth_10_or_12_bit,
            color_space,
            color_range,
            subsampling_x,
            subsampling_y,
            frame_width,
            frame_height,
            render_and_frame_size_different,
            render_width,
            render_height,
        };
        let bytes = build_keyframe_bytes(&params);
        let header = parse_frame_header(&bytes).expect("有効なキーフレームは解析成功する");

        assert_eq!(header.frame_type, Vp9FrameType::Key);
        assert_eq!(header.profile, profile);
        assert_eq!(header.show_existing_frame, None);
        assert_eq!(header.show_frame, show_frame);
        assert_eq!(header.error_resilient_mode, error_resilient_mode);
        assert!(!header.intra_only);
        assert_eq!(header.refresh_frame_flags, 0xFF);
        let expected_bit_depth = if profile >= 2 {
            if bit_depth_10_or_12_bit { 12 } else { 10 }
        } else {
            8
        };
        assert_eq!(header.bit_depth, expected_bit_depth);
        assert_eq!(header.color_space, color_space);
        assert_eq!(header.color_range, color_range);
        assert_eq!(header.subsampling_x, subsampling_x);
        assert_eq!(header.subsampling_y, subsampling_y);
        assert_eq!(
            header.frame_size,
            Vp9FrameSize::Resolved {
                width: frame_width,
                height: frame_height,
            }
        );
        let expected_render = if render_and_frame_size_different {
            Some((render_width, render_height))
        } else {
            None
        };
        assert_eq!(header.render_size, expected_render);
        Ok(())
    })?;
    Ok(())
}

/// show_existing_frame = 1 で frame_to_show_map_idx (0..=7) の全 8 値が復元される
///
/// noprop の一様サンプルで 500 ケースあれば 8 値すべてを平均 62 回踏める。境界指定は不要
#[test]
fn show_existing_frame_map_idx_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let idx = noprop::sample_u64_in(ctx, 0..=7) as u8;
        let profile = noprop::sample_u64_in(ctx, 0..=2) as u8; // profile 3 は reserved_zero を書く必要がある

        let mut w = BitWriter::new();
        w.push_bits(2, 2); // frame_marker
        w.push_bit(profile & 1);
        w.push_bit((profile >> 1) & 1);
        w.push_bit(1); // show_existing_frame = 1
        w.push_bits(u32::from(idx), 3);
        let bytes = w.into_bytes();

        let header = parse_frame_header(&bytes).expect("show_existing_frame は解析成功する");
        assert_eq!(header.show_existing_frame, Some(idx));
        assert_eq!(header.profile, profile);
        assert_eq!(header.refresh_frame_flags, 0);
        assert_eq!(header.frame_size, Vp9FrameSize::NotPresent);
        Ok(())
    })?;
    Ok(())
}

/// VP Codec ISO Media File Format Binding のプロファイルの表に含まれる 1 つの組み合わせ
struct ProfileCombination {
    profile: u8,
    bit_depth: u8,
    subsampling_x: u8,
    subsampling_y: u8,
    /// `vpcC` の chroma_subsampling への変換結果 (1 = 4:2:0 colocated、2 = 4:2:2、3 = 4:4:4)
    expected_vpcc_chroma: u8,
}

/// VP Codec ISO Media File Format Binding のプロファイルの表に含まれる全 9 通りの組み合わせ
///
/// profile 0 は 8 ビット 4:2:0、profile 1 は 8 ビット 4:2:2 / 4:4:4、
/// profile 2 は 10 / 12 ビット 4:2:0、profile 3 は 10 / 12 ビット 4:2:2 / 4:4:4。
/// 4:4:0 (VP9 spec では profile 1 / 3 で合法) は Binding に写せないため含めない。拒否は単体テストで確認する
const PROFILE_TABLE: [ProfileCombination; 9] = [
    ProfileCombination {
        profile: 0,
        bit_depth: 8,
        subsampling_x: 1,
        subsampling_y: 1,
        expected_vpcc_chroma: 1,
    },
    ProfileCombination {
        profile: 1,
        bit_depth: 8,
        subsampling_x: 1,
        subsampling_y: 0,
        expected_vpcc_chroma: 2,
    },
    ProfileCombination {
        profile: 1,
        bit_depth: 8,
        subsampling_x: 0,
        subsampling_y: 0,
        expected_vpcc_chroma: 3,
    },
    ProfileCombination {
        profile: 2,
        bit_depth: 10,
        subsampling_x: 1,
        subsampling_y: 1,
        expected_vpcc_chroma: 1,
    },
    ProfileCombination {
        profile: 2,
        bit_depth: 12,
        subsampling_x: 1,
        subsampling_y: 1,
        expected_vpcc_chroma: 1,
    },
    ProfileCombination {
        profile: 3,
        bit_depth: 10,
        subsampling_x: 1,
        subsampling_y: 0,
        expected_vpcc_chroma: 2,
    },
    ProfileCombination {
        profile: 3,
        bit_depth: 10,
        subsampling_x: 0,
        subsampling_y: 0,
        expected_vpcc_chroma: 3,
    },
    ProfileCombination {
        profile: 3,
        bit_depth: 12,
        subsampling_x: 1,
        subsampling_y: 0,
        expected_vpcc_chroma: 2,
    },
    ProfileCombination {
        profile: 3,
        bit_depth: 12,
        subsampling_x: 0,
        subsampling_y: 0,
        expected_vpcc_chroma: 3,
    },
];

/// `build_vp09_box` が config + キーフレーム header を反映し encode/decode ラウンドトリップする
///
/// 各ケースで、プロファイルの表に含まれる全 9 通りの組み合わせ ([`PROFILE_TABLE`]) を順に試す。
/// どの組み合わせも構築に成功し、profile・ビット深度・クロマの変換結果が保たれ、
/// encode/decode でラウンドトリップすることを確かめる。`color_range`・寸法・config は組み合わせごとにランダムに選ぶ
#[test]
fn build_vp09_box_config_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        for combination in &PROFILE_TABLE {
            let ProfileCombination {
                profile,
                bit_depth,
                subsampling_x,
                subsampling_y,
                expected_vpcc_chroma,
            } = *combination;
            let params = KeyframeBits {
                profile,
                show_frame: true,
                error_resilient_mode: false,
                // profile 2 / 3 でだけ書き込まれる (true = 12 ビット、false = 10 ビット)
                bit_depth_10_or_12_bit: bit_depth == 12,
                color_space: 1, // BT.601 (sRGB は固定レイアウトのため単体テストで確認する)
                color_range: noprop::sample_u64_in(ctx, 0..=1) as u8,
                subsampling_x,
                subsampling_y,
                frame_width: noprop::sample_u64_in(ctx, 1..=1920) as u32,
                frame_height: noprop::sample_u64_in(ctx, 1..=1080) as u32,
                render_and_frame_size_different: false,
                render_width: 0,
                render_height: 0,
            };
            let bytes = build_keyframe_bytes(&params);
            let label = format!(
                "profile = {profile}, bit_depth = {bit_depth}, \
                 subsampling = ({subsampling_x}, {subsampling_y})"
            );
            let header = parse_frame_header(&bytes)
                .unwrap_or_else(|e| panic!("キーフレームは解析成功する: {label}: {e:?}"));
            // 生成器が意図どおりの header を作れていること (生成器の誤りを検出する)
            assert_eq!(
                (
                    header.profile,
                    header.bit_depth,
                    header.subsampling_x,
                    header.subsampling_y
                ),
                (profile, bit_depth, subsampling_x, subsampling_y),
                "生成した header の解析結果が意図した組み合わせと一致しない: {label}"
            );

            let level = noprop::sample_u8(ctx);
            let config = Vp9SampleEntryConfig {
                level,
                colour_primaries: noprop::sample_u8(ctx),
                transfer_characteristics: noprop::sample_u8(ctx),
                matrix_coefficients: noprop::sample_u8(ctx),
                width: noprop::sample_u64_in(ctx, 1..=1920) as u16,
                height: noprop::sample_u64_in(ctx, 1..=1080) as u16,
            };
            let vp09 = build_vp09_box(&header, &config).unwrap_or_else(|e| {
                panic!("表に含まれる組み合わせは構築に成功する: {label}: {e:?}")
            });

            // ストリーム導出値の反映確認
            assert_eq!(
                vp09.vpcc_box.profile, profile,
                "vpcC の profile が組み合わせと一致しない: {label}"
            );
            assert_eq!(
                vp09.vpcc_box.bit_depth.get(),
                bit_depth,
                "vpcC の bit_depth が組み合わせと一致しない: {label}"
            );
            assert_eq!(
                vp09.vpcc_box.chroma_subsampling.get(),
                expected_vpcc_chroma,
                "vpcC の chroma_subsampling が組み合わせの変換結果と一致しない: {label}"
            );
            assert_eq!(
                vp09.vpcc_box.video_full_range_flag.get(),
                header.color_range,
                "vpcC の video_full_range_flag が header の color_range と一致しない: {label}"
            );

            // config 反映
            assert_eq!(
                vp09.vpcc_box.level, level,
                "vpcC の level が config と一致しない: {label}"
            );
            assert_eq!(
                vp09.vpcc_box.colour_primaries, config.colour_primaries,
                "vpcC の colour_primaries が config と一致しない: {label}"
            );
            assert_eq!(
                vp09.vpcc_box.transfer_characteristics, config.transfer_characteristics,
                "vpcC の transfer_characteristics が config と一致しない: {label}"
            );
            assert_eq!(
                vp09.vpcc_box.matrix_coefficients, config.matrix_coefficients,
                "vpcC の matrix_coefficients が config と一致しない: {label}"
            );
            assert_eq!(
                vp09.visual.width, config.width,
                "visual の width が config と一致しない: {label}"
            );
            assert_eq!(
                vp09.visual.height, config.height,
                "visual の height が config と一致しない: {label}"
            );

            // 固定値
            assert!(
                vp09.vpcc_box.codec_initialization_data.is_empty(),
                "vpcC の codec_initialization_data が空ではない: {label}"
            );
            assert!(
                vp09.unknown_boxes.is_empty(),
                "unknown_boxes が空ではない: {label}"
            );

            // encode → decode でラウンドトリップ
            let encoded = vp09
                .encode_to_vec()
                .unwrap_or_else(|e| panic!("encode は成功する: {label}: {e:?}"));
            let (decoded, size) = Vp09Box::decode(&encoded)
                .unwrap_or_else(|e| panic!("decode は成功する: {label}: {e:?}"));
            assert_eq!(
                size,
                encoded.len(),
                "decode が消費したサイズが encode の長さと一致しない: {label}"
            );
            assert_eq!(
                decoded, vp09,
                "decode 結果が元の Vp09Box と一致しない: {label}"
            );
        }
        Ok(())
    })?;
    Ok(())
}

/// intra-only frame の bit builder (render_size なしで固定、色設定は profile > 0 のみ書き込まれる)
struct IntraOnlyBits {
    profile: u8,
    error_resilient_mode: bool,
    bit_depth_10_or_12_bit: bool,
    color_space: u8,
    color_range: u8,
    subsampling_x: u8,
    subsampling_y: u8,
    refresh_frame_flags: u8,
    frame_width: u32,
    frame_height: u32,
}

fn build_intra_only_bytes(p: &IntraOnlyBits) -> Vec<u8> {
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
    w.push_bytes(&[0x49, 0x83, 0x42]); // sync_code
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
    w.push_bit(0); // render_and_frame_size_different = 0
    w.into_bytes()
}

/// intra-only frame の profile / color / refresh_frame_flags / frame_size のビット配置往復
///
/// profile 0 では color_config が書かれず既定値、profile > 0 では color_config を経由する。
/// error_resilient_mode の両値もサンプルする (reset_frame_context を書くかどうかの分岐)
#[test]
fn intra_only_bit_layout_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let profile = noprop::sample_u64_in(ctx, 0..=3) as u8;
        let error_resilient_mode = noprop::sample_bool(ctx);
        let bit_depth_10_or_12_bit = noprop::sample_bool(ctx);
        // color_space は 0..=6。sRGB は固定レイアウトのため単体テストで確認する
        let color_space = noprop::sample_u64_in(ctx, 0..=6) as u8;
        let color_range = noprop::sample_u64_in(ctx, 0..=1) as u8;
        // profile 1/3 は 4:2:2 / 4:4:4 (4:2:0 は spec 違反、4:4:0 は Vp09Box 化不能なので
        // build_vp09_box_config_roundtrip と同じく除外)
        let (subsampling_x, subsampling_y) = if profile == 1 || profile == 3 {
            match noprop::sample_u64_in(ctx, 0..=1) {
                0 => (1u8, 0u8), // 4:2:2
                _ => (0u8, 0u8), // 4:4:4
            }
        } else {
            (1u8, 1u8) // profile 0/2 は 4:2:0 固定
        };
        let refresh_frame_flags = noprop::sample_u8(ctx);
        let frame_width = noprop::sample_u64_in(ctx, 1..=1920) as u32;
        let frame_height = noprop::sample_u64_in(ctx, 1..=1080) as u32;

        let params = IntraOnlyBits {
            profile,
            error_resilient_mode,
            bit_depth_10_or_12_bit,
            color_space,
            color_range,
            subsampling_x,
            subsampling_y,
            refresh_frame_flags,
            frame_width,
            frame_height,
        };
        let bytes = build_intra_only_bytes(&params);
        let header = parse_frame_header(&bytes).expect("intra-only frame は解析成功する");

        assert_eq!(header.frame_type, Vp9FrameType::NonKey);
        assert!(header.intra_only);
        assert!(!header.show_frame);
        assert_eq!(header.profile, profile);
        assert_eq!(header.error_resilient_mode, error_resilient_mode);
        assert_eq!(header.refresh_frame_flags, refresh_frame_flags);
        if profile == 0 {
            assert_eq!(header.bit_depth, 8);
            assert_eq!(header.color_space, 1);
            assert_eq!(header.color_range, 0);
            assert_eq!(header.subsampling_x, 1);
            assert_eq!(header.subsampling_y, 1);
        } else {
            let expected_bit_depth = if profile >= 2 {
                if bit_depth_10_or_12_bit { 12 } else { 10 }
            } else {
                8
            };
            assert_eq!(header.bit_depth, expected_bit_depth);
            assert_eq!(header.color_space, color_space);
            assert_eq!(header.color_range, color_range);
            assert_eq!(header.subsampling_x, subsampling_x);
            assert_eq!(header.subsampling_y, subsampling_y);
        }
        assert_eq!(
            header.frame_size,
            Vp9FrameSize::Resolved {
                width: frame_width,
                height: frame_height,
            }
        );
        Ok(())
    })?;
    Ok(())
}

/// inter frame の bit builder (found_ref の break セマンティクスと intra_only=0 の分岐を含む)
struct InterframeBits {
    profile: u8,
    show_frame: bool,
    error_resilient_mode: bool,
    refresh_frame_flags: u8,
    ref_frame_idx: [u8; 3],
    ref_frame_sign_bias: [u8; 3],
    /// found_ref[i] が最初に true になった時点で残りは書かない (VP9 spec 6.2.5)
    found_ref: [bool; 3],
    /// found_ref すべて false のときだけ使う
    frame_width: u32,
    frame_height: u32,
    render_and_frame_size_different: bool,
    render_width: u32,
    render_height: u32,
}

fn build_interframe_bytes(p: &InterframeBits) -> Vec<u8> {
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

/// inter frame の各フィールドがビット配置往復する
///
/// - `found_ref` は 4 通り (全 0 / [0]=1 / [0,1]=[0,1] / [0,0,1]) を境界化して
///   Resolved 経路 / UsesRefFrames の各スロット経路の両方を踏む
/// - `show_frame` と `error_resilient_mode` の両値をサンプルし、`intra_only` /
///   `reset_frame_context` の書き込み分岐を全部触る
/// - `render_size` の有無も両値
#[test]
fn interframe_bit_layout_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let profile = noprop::sample_u64_in(ctx, 0..=3) as u8;
        let show_frame = noprop::sample_bool(ctx);
        let error_resilient_mode = noprop::sample_bool(ctx);
        let refresh_frame_flags = noprop::sample_u8(ctx);
        let ref_frame_idx = [
            noprop::sample_u64_in(ctx, 0..=7) as u8,
            noprop::sample_u64_in(ctx, 0..=7) as u8,
            noprop::sample_u64_in(ctx, 0..=7) as u8,
        ];
        let ref_frame_sign_bias = [
            noprop::sample_u64_in(ctx, 0..=1) as u8,
            noprop::sample_u64_in(ctx, 0..=1) as u8,
            noprop::sample_u64_in(ctx, 0..=1) as u8,
        ];
        // found_ref の 4 通りを境界化する: 全 0 / [true, _, _] / [false, true, _] / [false, false, true]
        let found_ref: [bool; 3] = match noprop::sample_u64_in(ctx, 0..=3) {
            0 => [false, false, false],
            1 => [true, false, false],
            2 => [false, true, false],
            _ => [false, false, true],
        };
        let frame_width = noprop::sample_u64_in(ctx, 1..=1920) as u32;
        let frame_height = noprop::sample_u64_in(ctx, 1..=1080) as u32;
        let render_and_frame_size_different = noprop::sample_bool(ctx);
        let render_width = noprop::sample_u64_in(ctx, 1..=1920) as u32;
        let render_height = noprop::sample_u64_in(ctx, 1..=1080) as u32;

        let params = InterframeBits {
            profile,
            show_frame,
            error_resilient_mode,
            refresh_frame_flags,
            ref_frame_idx,
            ref_frame_sign_bias,
            found_ref,
            frame_width,
            frame_height,
            render_and_frame_size_different,
            render_width,
            render_height,
        };
        let bytes = build_interframe_bytes(&params);
        let header = parse_frame_header(&bytes).expect("inter frame は解析成功する");

        assert_eq!(header.frame_type, Vp9FrameType::NonKey);
        assert!(!header.intra_only);
        assert_eq!(header.profile, profile);
        assert_eq!(header.show_frame, show_frame);
        assert_eq!(header.error_resilient_mode, error_resilient_mode);
        assert_eq!(header.refresh_frame_flags, refresh_frame_flags);
        // inter では color 系は 0 プレースホルダ
        assert_eq!(header.bit_depth, 0);
        assert_eq!(header.color_space, 0);
        assert_eq!(header.color_range, 0);
        assert_eq!(header.subsampling_x, 0);
        assert_eq!(header.subsampling_y, 0);
        // frame_size は found_ref の最初の true に対応するスロット、または Resolved
        let expected_frame_size = if let Some(i) = found_ref.iter().position(|f| *f) {
            Vp9FrameSize::UsesRefFrames {
                ref_frame_slot: ref_frame_idx[i],
            }
        } else {
            Vp9FrameSize::Resolved {
                width: frame_width,
                height: frame_height,
            }
        };
        assert_eq!(header.frame_size, expected_frame_size);
        // render_size
        let expected_render = if render_and_frame_size_different {
            Some((render_width, render_height))
        } else {
            None
        };
        assert_eq!(header.render_size, expected_render);
        Ok(())
    })?;
    Ok(())
}
