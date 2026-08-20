//! `shiguredo_mp4::bitstream::vp9` の Property-Based Testing
//!
//! 手動構築の VP9 uncompressed header のビット配置を noprop サンプラーで
//! ランダム生成し、`parse_frame_header` が RFC どおりに復元することを検証する。

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
/// - `color_space` は 0..=6 (sRGB=7 は別テスト) を境界化
/// - `subsampling` は profile 1/3 で `(1,1)` / `(1,0)` / `(0,0)` の 3 通り、profile 0/2 は `(1,1)` 固定
/// - `frame_width` / `frame_height` は 14 ビット境界 (`1` と `65536`) を境界指定
/// - `render_size` の有無は両値サンプル
#[test]
fn keyframe_bit_layout_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let profile = noprop::sample_u64_in(ctx, 0..=3) as u8;
        let show_frame = noprop::sample_bool(ctx);
        let error_resilient_mode = noprop::sample_bool(ctx);
        let bit_depth_10_or_12_bit = noprop::sample_bool(ctx);
        // color_space は 0..=6 のみ (sRGB は subsampling 固定の別ケースなので separately)
        let color_space = noprop::sample_u64_in(ctx, 0..=6) as u8;
        let color_range = noprop::sample_u64_in(ctx, 0..=1) as u8;
        // profile 1/3 は subsampling を可変にできる。(0,1) は仕様外なので除外
        let (subsampling_x, subsampling_y) = if profile == 1 || profile == 3 {
            match noprop::sample_u64_in(ctx, 0..=2) {
                0 => (1u8, 1u8),
                1 => (1u8, 0u8),
                _ => (0u8, 0u8),
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
        Ok(())
    })?;
    Ok(())
}

/// `build_vp09_box` が config + キーフレーム header を反映し encode/decode ラウンドトリップする
#[test]
fn build_vp09_box_config_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        // profile / subsampling を軽くランダム化。ヘッダーは常にキーフレームで固定条件
        let profile = noprop::sample_u64_in(ctx, 0..=3) as u8;
        let bit_depth_10_or_12_bit = noprop::sample_bool(ctx);
        let (subsampling_x, subsampling_y) = if profile == 1 || profile == 3 {
            match noprop::sample_u64_in(ctx, 0..=2) {
                0 => (1u8, 1u8),
                1 => (1u8, 0u8),
                _ => (0u8, 0u8),
            }
        } else {
            (1u8, 1u8)
        };

        let params = KeyframeBits {
            profile,
            show_frame: true,
            error_resilient_mode: false,
            bit_depth_10_or_12_bit,
            color_space: 1, // BT.601 (sRGB 分岐は別で扱う)
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
        let header = parse_frame_header(&bytes).expect("キーフレーム解析");

        let level = if noprop::sample_bool(ctx) {
            Some(noprop::sample_u8(ctx))
        } else {
            None
        };
        let config = Vp9SampleEntryConfig {
            level,
            colour_primaries: noprop::sample_u8(ctx),
            transfer_characteristics: noprop::sample_u8(ctx),
            matrix_coefficients: noprop::sample_u8(ctx),
            width: noprop::sample_u64_in(ctx, 1..=1920) as u16,
            height: noprop::sample_u64_in(ctx, 1..=1080) as u16,
        };
        let vp09 = build_vp09_box(&header, &config);

        // ストリーム導出値の反映確認
        assert_eq!(vp09.vpcc_box.profile, profile);
        assert_eq!(vp09.vpcc_box.bit_depth.get(), header.bit_depth);
        let expected_chroma = match (subsampling_x, subsampling_y) {
            (1, 1) => 1,
            (1, 0) => 2,
            (0, 0) => 3,
            _ => unreachable!(),
        };
        assert_eq!(vp09.vpcc_box.chroma_subsampling.get(), expected_chroma);
        assert_eq!(
            vp09.vpcc_box.video_full_range_flag.get(),
            header.color_range
        );

        // config 反映
        assert_eq!(vp09.vpcc_box.level, level.unwrap_or(0));
        assert_eq!(vp09.vpcc_box.colour_primaries, config.colour_primaries);
        assert_eq!(
            vp09.vpcc_box.transfer_characteristics,
            config.transfer_characteristics
        );
        assert_eq!(
            vp09.vpcc_box.matrix_coefficients,
            config.matrix_coefficients
        );
        assert_eq!(vp09.visual.width, config.width);
        assert_eq!(vp09.visual.height, config.height);

        // 固定値
        assert!(vp09.vpcc_box.codec_initialization_data.is_empty());
        assert!(vp09.unknown_boxes.is_empty());

        // encode → decode でラウンドトリップ
        let encoded = vp09.encode_to_vec().expect("encode 成功");
        let (decoded, size) = Vp09Box::decode(&encoded).expect("decode 成功");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, vp09);
        Ok(())
    })?;
    Ok(())
}
