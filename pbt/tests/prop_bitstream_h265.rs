//! `shiguredo_mp4::bitstream::h265` の Property-Based Testing
//!
//! Annex B と length-prefixed の相互変換のラウンドトリップ、およびテスト内に
//! 留めた SPS ビルダーで生成した正当な SPS の解析不変条件を noprop で検証する。

use std::cell::Cell;

use shiguredo_mp4::{
    Decode, Encode,
    bitstream::h265::{
        H265SampleEntryConfig, LengthSize, annexb_to_length_prefixed, build_hev1_box,
        build_hvc1_box, length_prefixed_to_annexb, parse_annexb_nal_units,
        parse_length_prefixed_nal_units, parse_sps,
    },
    boxes::{Hev1Box, Hvc1Box},
};

/// このファイルの PBT ケース数
const CASES: usize = 500;

/// SPS の RBSP を組み立てる MSB-first ビットライター
#[derive(Debug, Clone, Default)]
struct BitWriter {
    bytes: Vec<u8>,
    bit_pos: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self::default()
    }

    fn push_bits(&mut self, value: u64, n: u32) {
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
        self.push_bits(u64::from(bit), 1);
    }

    /// 符号なし Exp-Golomb (`ue(v)`) を書き込む
    fn push_ue(&mut self, value: u32) {
        let code_num = value + 1;
        let mut zeros = 0;
        let mut v = code_num;
        while v > 1 {
            v >>= 1;
            zeros += 1;
        }
        self.push_bits(0, zeros);
        self.push_bits(u64::from(code_num), zeros + 1);
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// クロップ無しの `seq_parameter_set_rbsp` 構築パラメタ
///
/// 幅・高さの期待値を単純な式 (そのままの符号化寸法) で検証できるように、
/// クロップ無し固定にする。クロップの式は単体テストで固定ケースを確認している
#[derive(Debug, Clone, Copy)]
struct SpsBits {
    general_profile_space: u8,
    general_tier_flag: bool,
    general_profile_idc: u8,
    general_profile_compatibility_flags: u32,
    general_constraint_indicator_flags: u64,
    general_level_idc: u8,
    sps_max_sub_layers_minus1: u8,
    sps_temporal_id_nesting_flag: bool,
    sub_layer_profile_present_flag: bool,
    sub_layer_level_present_flag: bool,
    chroma_format_idc: u8,
    separate_colour_plane_flag: bool,
    pic_width_in_luma_samples: u32,
    pic_height_in_luma_samples: u32,
    bit_depth_luma_minus8: u8,
    bit_depth_chroma_minus8: u8,
}

/// ランダムな SPS パラメタを生成する
///
/// - `sps_max_sub_layers_minus1 == 0` のとき `sps_temporal_id_nesting_flag` は
///   パーサーが 1 を要求する (7.4.3.2.1) ため 1 に固定する
/// - クロップ無し固定にすることで、幅・高さの期待値を符号化寸法そのもので
///   検証できるようにする
fn sample_sps_bits(ctx: &mut noprop::TestCaseContext) -> SpsBits {
    let sps_max_sub_layers_minus1 = noprop::sample_u64_in(ctx, 0..=6) as u8;
    let sps_temporal_id_nesting_flag = sps_max_sub_layers_minus1 == 0 || noprop::sample_bool(ctx);
    let chroma_format_idc = noprop::sample_u64_in(ctx, 0..=3) as u8;
    SpsBits {
        general_profile_space: noprop::sample_u64_in(ctx, 0..=3) as u8,
        general_tier_flag: noprop::sample_bool(ctx),
        general_profile_idc: noprop::sample_u64_in(ctx, 0..=31) as u8,
        general_profile_compatibility_flags: noprop::sample_u32(ctx),
        general_constraint_indicator_flags: noprop::sample_u64_in(ctx, 0..=0xFFFF_FFFF_FFFF),
        general_level_idc: noprop::sample_u8(ctx),
        sps_max_sub_layers_minus1,
        sps_temporal_id_nesting_flag,
        sub_layer_profile_present_flag: sps_max_sub_layers_minus1 > 0 && noprop::sample_bool(ctx),
        sub_layer_level_present_flag: sps_max_sub_layers_minus1 > 0 && noprop::sample_bool(ctx),
        chroma_format_idc,
        separate_colour_plane_flag: chroma_format_idc == 3 && noprop::sample_bool(ctx),
        pic_width_in_luma_samples: noprop::sample_u64_in(ctx, 16..=4096) as u32,
        pic_height_in_luma_samples: noprop::sample_u64_in(ctx, 16..=4096) as u32,
        bit_depth_luma_minus8: noprop::sample_u64_in(ctx, 0..=7) as u8,
        bit_depth_chroma_minus8: noprop::sample_u64_in(ctx, 0..=7) as u8,
    }
}

/// NAL ヘッダー (type 33 / layer 0 / TemporalId 0) 付きのクロップ無し SPS EBSP を組み立てる
fn build_sps(p: &SpsBits) -> Vec<u8> {
    let mut w = BitWriter::new();
    // NAL ヘッダー: forbidden_zero_bit = 0 / nal_unit_type = 33 / nuh_layer_id = 0 /
    // nuh_temporal_id_plus1 = 1 (TemporalId = 0)
    w.push_bits(0x42, 8);
    w.push_bits(0x01, 8);
    w.push_bits(0, 4); // sps_video_parameter_set_id
    w.push_bits(u64::from(p.sps_max_sub_layers_minus1), 3);
    w.push_bit(u8::from(p.sps_temporal_id_nesting_flag));
    // profile_tier_level(1, sps_max_sub_layers_minus1) (7.3.3)
    w.push_bits(u64::from(p.general_profile_space), 2);
    w.push_bit(u8::from(p.general_tier_flag));
    w.push_bits(u64::from(p.general_profile_idc), 5);
    w.push_bits(u64::from(p.general_profile_compatibility_flags), 32);
    w.push_bits(p.general_constraint_indicator_flags & 0xFFFF_FFFF_FFFF, 48);
    w.push_bits(u64::from(p.general_level_idc), 8);
    for _ in 0..p.sps_max_sub_layers_minus1 {
        w.push_bit(u8::from(p.sub_layer_profile_present_flag));
        w.push_bit(u8::from(p.sub_layer_level_present_flag));
    }
    if p.sps_max_sub_layers_minus1 > 0 {
        for _ in p.sps_max_sub_layers_minus1..8 {
            w.push_bits(0, 2); // reserved_zero_2bits
        }
    }
    for _ in 0..p.sps_max_sub_layers_minus1 {
        if p.sub_layer_profile_present_flag {
            // sub-layer profile (2 + 1 + 5 + 32 + 48 = 88 bits) を 0 で埋める。
            // push_bits は u64 シフトのため 64 ビットを超えるとパニックするので分割する
            w.push_bits(0, 64);
            w.push_bits(0, 24);
        }
        if p.sub_layer_level_present_flag {
            w.push_bits(0, 8); // sub_layer_level_idc
        }
    }
    w.push_ue(0); // sps_seq_parameter_set_id
    w.push_ue(u32::from(p.chroma_format_idc));
    if p.chroma_format_idc == 3 {
        w.push_bit(u8::from(p.separate_colour_plane_flag));
    }
    w.push_ue(p.pic_width_in_luma_samples);
    w.push_ue(p.pic_height_in_luma_samples);
    w.push_bit(0); // conformance_window_flag = 0
    w.push_ue(u32::from(p.bit_depth_luma_minus8));
    w.push_ue(u32::from(p.bit_depth_chroma_minus8));
    w.into_bytes()
}

/// ランダムな NAL 本体を生成する
///
/// 2 バイト NAL ヘッダーを有効な値 (forbidden_zero_bit = 0 / tid != 0 /
/// layer != 63) で作り、残りは開始コード (`00 00 01` / `00 00 00 01`) を
/// 本体に含ませないよう 1..=255 から取る (末尾ゼロも持たせない)
fn sample_nal_body(ctx: &mut noprop::TestCaseContext, max_len: usize) -> Vec<u8> {
    let len = noprop::sample_usize_in(ctx, 2..=max_len);
    let mut body = Vec::new();
    // 2 バイト NAL ヘッダー: nal_unit_type は任意、nuh_layer_id = 0、
    // nuh_temporal_id_plus1 = 1
    body.push((noprop::sample_u64_in(ctx, 0..=63) as u8) << 1);
    body.push(0x01);
    for _ in 2..len {
        body.push(noprop::sample_u64_in(ctx, 1..=255) as u8);
    }
    body
}

/// ランダムなパラメータセット (VPS / PPS 用の非空 NAL) を生成する
///
/// `nal_unit_type` は 32 (VPS) / 34 (PPS) を呼び出し側が渡す
fn sample_parameter_set(ctx: &mut noprop::TestCaseContext, nal_unit_type: u8) -> Vec<u8> {
    let mut nal = vec![nal_unit_type << 1, 0x01];
    let payload_len = noprop::sample_usize_in(ctx, 0..=8);
    for _ in 0..payload_len {
        nal.push(noprop::sample_u64_in(ctx, 1..=255) as u8);
    }
    nal
}

/// 4 バイト開始コードで NAL 本体を連結した Annex B バイト列を作る
fn build_annexb(bodies: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for body in bodies {
        out.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        out.extend_from_slice(body);
    }
    out
}

/// 大端序の長さフィールドを付けて length-prefixed バイト列を作る
fn build_length_prefixed(bodies: &[Vec<u8>], length_size: LengthSize) -> Vec<u8> {
    let mut out = Vec::new();
    for body in bodies {
        match length_size {
            LengthSize::OneByte => out.push(body.len() as u8),
            LengthSize::TwoBytes => out.extend_from_slice(&(body.len() as u16).to_be_bytes()),
            LengthSize::FourBytes => out.extend_from_slice(&(body.len() as u32).to_be_bytes()),
        }
        out.extend_from_slice(body);
    }
    out
}

/// 長さフィールド幅を 1 / 2 / 4 からサンプリングする
fn sample_length_size(ctx: &mut noprop::TestCaseContext) -> LengthSize {
    match noprop::sample_u64_in(ctx, 0..=2) {
        0 => LengthSize::OneByte,
        1 => LengthSize::TwoBytes,
        _ => LengthSize::FourBytes,
    }
}

/// Annex B と length-prefixed の相互変換がラウンドトリップする
///
/// - 生成した NAL 本体列を length-prefixed に組み、Annex B へ変換して戻した結果が
///   元の length-prefixed と一致する
/// - 逆に Annex B に組み、length-prefixed へ変換して戻した結果が元の Annex B と一致する
/// - `parse_length_prefixed_nal_units` が NAL 境界を重複なく覆い、元の本体列を
///   そのまま返す
#[test]
fn annexb_length_prefixed_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let multi_nal_cases = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);

    runner.run(CASES, |ctx| {
        let length_size = sample_length_size(ctx);
        // 0 / 1 / 複数 NAL を境界化する (幅 1 でも収まるよう本体長は 1..=100)
        let nal_count = noprop::sample_with_boundaries(
            ctx,
            &[0usize, 1, 4],
            noprop::Ratio::one_nth(4),
            |ctx| noprop::sample_usize_in(ctx, 0..=4),
        );
        let mut bodies = Vec::new();
        for _ in 0..nal_count {
            bodies.push(sample_nal_body(ctx, 100));
        }

        // length-prefixed → Annex B → length-prefixed
        let lp = build_length_prefixed(&bodies, length_size);
        let ab = length_prefixed_to_annexb(&lp, length_size).expect("lp → annexb は成功する");
        let lp_back = annexb_to_length_prefixed(&ab, length_size).expect("annexb → lp は成功する");
        assert_eq!(lp_back, lp, "length-prefixed がラウンドトリップする");

        // Annex B → length-prefixed → Annex B
        let ab_original = build_annexb(&bodies);
        let lp2 =
            annexb_to_length_prefixed(&ab_original, length_size).expect("annexb → lp は成功する");
        let ab_back = length_prefixed_to_annexb(&lp2, length_size).expect("lp → annexb は成功する");
        assert_eq!(ab_back, ab_original, "Annex B がラウンドトリップする");

        // NAL 境界が入力を重複なく覆い、元の本体列がそのまま復元される
        let nals = parse_length_prefixed_nal_units(&lp, length_size)
            .expect("length-prefixed 列は解析成功する");
        assert_eq!(nals.len(), bodies.len(), "NAL 個数が一致する");
        for (nal, body) in nals.iter().zip(&bodies) {
            assert_eq!(nal.data, body.as_slice(), "NAL 本体が一致する");
        }

        // Annex B 側も同じく NAL 境界が重複なく入力を覆う
        let nals = parse_annexb_nal_units(&ab_original).expect("Annex B は解析成功する");
        assert_eq!(nals.len(), bodies.len(), "NAL 個数が一致する");
        for (nal, body) in nals.iter().zip(&bodies) {
            assert_eq!(nal.data, body.as_slice(), "NAL 本体が一致する");
        }

        if nal_count >= 2 {
            multi_nal_cases.set(multi_nal_cases.get() + 1);
        }
        Ok(())
    })?;

    assert!(
        multi_nal_cases.get() > 0,
        "複数 NAL のラウンドトリップを一度も踏んでいない\n{runner}"
    );
    Ok(())
}

/// 生成した正当な SPS の解析結果が不変条件を満たす
///
/// - `general_profile_space` / tier / `general_profile_idc` /
///   compatibility / constraint / `general_level_idc` がそのまま復元される
/// - `sps_max_sub_layers_minus1` / `sps_temporal_id_nesting_flag` が復元される
/// - 幅・高さがクロップ無し SPS の符号化寸法に一致する
/// - `chroma_format_idc` / bit depth が復元される
#[test]
fn sps_bit_layout_invariants() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let sub_layer_cases = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);

    runner.run(CASES, |ctx| {
        let bits = sample_sps_bits(ctx);
        let nal = build_sps(&bits);
        let sps = parse_sps(&nal).expect("生成した SPS は解析成功する");

        assert_eq!(sps.general_profile_space, bits.general_profile_space);
        assert_eq!(
            sps.general_tier_flag,
            u8::from(bits.general_tier_flag),
            "general_tier_flag が一致する"
        );
        assert_eq!(sps.general_profile_idc, bits.general_profile_idc);
        assert_eq!(
            sps.general_profile_compatibility_flags,
            bits.general_profile_compatibility_flags
        );
        assert_eq!(
            sps.general_constraint_indicator_flags,
            bits.general_constraint_indicator_flags
        );
        assert_eq!(sps.general_level_idc, bits.general_level_idc);
        assert_eq!(
            sps.sps_max_sub_layers_minus1,
            bits.sps_max_sub_layers_minus1
        );
        assert_eq!(
            sps.sps_temporal_id_nesting_flag,
            u8::from(bits.sps_temporal_id_nesting_flag)
        );

        // クロップ無し固定なので幅・高さは符号化寸法そのもの
        assert_eq!(
            u64::from(sps.width),
            u64::from(bits.pic_width_in_luma_samples),
            "幅が一致する"
        );
        assert_eq!(
            u64::from(sps.height),
            u64::from(bits.pic_height_in_luma_samples),
            "高さが一致する"
        );

        assert_eq!(sps.chroma_format_idc, bits.chroma_format_idc);
        assert_eq!(sps.bit_depth_luma_minus8, bits.bit_depth_luma_minus8);
        assert_eq!(sps.bit_depth_chroma_minus8, bits.bit_depth_chroma_minus8);

        if bits.sps_max_sub_layers_minus1 > 0 {
            sub_layer_cases.set(sub_layer_cases.get() + 1);
        }
        Ok(())
    })?;

    assert!(
        sub_layer_cases.get() > 0,
        "sps_max_sub_layers_minus1 > 0 の分岐を一度も踏んでいない\n{runner}"
    );
    Ok(())
}

/// `build_hev1_box` / `build_hvc1_box` が生成した SPS から hvcC /
/// VisualSampleEntry の欄を正しく導出する
///
/// - profile / level / chroma / bit depth / temporal 欄が SPS から写る
/// - 幅・高さがクロップ無し SPS の寸法に一致する
/// - VPS / SPS / PPS の EBSP が `nalu_arrays` に入力順で格納される
/// - `array_completeness` が hev1 で 0、hvc1 で 1 になる
/// - 長さ幅 1 / 2 / 4 が `length_size_minus_one` (0 / 1 / 3) に写る
/// - encode → decode でラウンドトリップする
#[test]
fn build_sample_entry_invariants() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(CASES, |ctx| {
        let bits = sample_sps_bits(ctx);
        let sps = build_sps(&bits);
        let parsed = parse_sps(&sps).expect("生成した SPS は解析成功する");

        let vps = sample_parameter_set(ctx, 32);
        let pps = sample_parameter_set(ctx, 34);
        let length_size = sample_length_size(ctx);
        let config = H265SampleEntryConfig { length_size };

        let hev1 = build_hev1_box(
            core::slice::from_ref(&vps),
            core::slice::from_ref(&sps),
            core::slice::from_ref(&pps),
            &config,
        )
        .expect("有効な VPS / SPS / PPS は構築成功する");

        // ストリーム導出値
        assert_eq!(
            hev1.hvcc_box.general_profile_space.get(),
            bits.general_profile_space
        );
        assert_eq!(
            hev1.hvcc_box.general_tier_flag.get(),
            u8::from(bits.general_tier_flag)
        );
        assert_eq!(
            hev1.hvcc_box.general_profile_idc.get(),
            bits.general_profile_idc
        );
        assert_eq!(
            hev1.hvcc_box.general_profile_compatibility_flags,
            bits.general_profile_compatibility_flags
        );
        assert_eq!(
            hev1.hvcc_box.general_constraint_indicator_flags.get(),
            bits.general_constraint_indicator_flags
        );
        assert_eq!(hev1.hvcc_box.general_level_idc, bits.general_level_idc);
        assert_eq!(
            hev1.hvcc_box.chroma_format_idc.get(),
            bits.chroma_format_idc
        );
        assert_eq!(
            hev1.hvcc_box.bit_depth_luma_minus8.get(),
            bits.bit_depth_luma_minus8
        );
        assert_eq!(
            hev1.hvcc_box.bit_depth_chroma_minus8.get(),
            bits.bit_depth_chroma_minus8
        );
        assert_eq!(
            hev1.hvcc_box.num_temporal_layers.get(),
            bits.sps_max_sub_layers_minus1 + 1
        );
        assert_eq!(
            hev1.hvcc_box.temporal_id_nested.get(),
            u8::from(bits.sps_temporal_id_nesting_flag)
        );
        assert_eq!(hev1.visual.width, parsed.width);
        assert_eq!(hev1.visual.height, parsed.height);

        // nalu_arrays は VPS / SPS / PPS の 3 配列に入力順で格納される
        assert_eq!(hev1.hvcc_box.nalu_arrays.len(), 3);
        assert_eq!(hev1.hvcc_box.nalu_arrays[0].nal_unit_type.get(), 32);
        assert_eq!(hev1.hvcc_box.nalu_arrays[0].nalus, vec![vps.clone()]);
        assert_eq!(hev1.hvcc_box.nalu_arrays[1].nal_unit_type.get(), 33);
        assert_eq!(hev1.hvcc_box.nalu_arrays[1].nalus, vec![sps.clone()]);
        assert_eq!(hev1.hvcc_box.nalu_arrays[2].nal_unit_type.get(), 34);
        assert_eq!(hev1.hvcc_box.nalu_arrays[2].nalus, vec![pps.clone()]);

        // hev1 は completeness が 0
        for array in &hev1.hvcc_box.nalu_arrays {
            assert_eq!(
                array.array_completeness.get(),
                0,
                "hev1 の completeness は 0"
            );
        }

        // 固定値
        assert_eq!(hev1.hvcc_box.min_spatial_segmentation_idc.get(), 0);
        assert_eq!(hev1.hvcc_box.parallelism_type.get(), 0);
        assert_eq!(hev1.hvcc_box.avg_frame_rate, 0);
        assert_eq!(hev1.hvcc_box.constant_frame_rate.get(), 0);

        // 呼び出し側指定値 (幅 1 / 2 / 4 → length_size_minus_one = 0 / 1 / 3)
        assert_eq!(
            hev1.hvcc_box.length_size_minus_one.get(),
            length_size.length_size_minus_one()
        );

        // encode → decode でラウンドトリップ
        let encoded = hev1.encode_to_vec().expect("encode 成功");
        let (decoded, size) = Hev1Box::decode(&encoded).expect("decode 成功");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, hev1);

        // hvc1 は completeness が 1 で、それ以外は hev1 と同じ
        let hvc1 = build_hvc1_box(
            core::slice::from_ref(&vps),
            core::slice::from_ref(&sps),
            core::slice::from_ref(&pps),
            &config,
        )
        .expect("有効な VPS / SPS / PPS は構築成功する");
        for array in &hvc1.hvcc_box.nalu_arrays {
            assert_eq!(
                array.array_completeness.get(),
                1,
                "hvc1 の completeness は 1"
            );
        }
        let encoded = hvc1.encode_to_vec().expect("encode 成功");
        let (decoded, size) = Hvc1Box::decode(&encoded).expect("decode 成功");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, hvc1);

        Ok(())
    })?;
    Ok(())
}
