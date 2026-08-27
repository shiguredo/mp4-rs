//! `shiguredo_mp4::bitstream::h264` の Property-Based Testing
//!
//! Annex B と length-prefixed の相互変換のラウンドトリップ、およびテスト内に
//! 留めた SPS ビルダーで生成した正当な SPS の解析不変条件を noprop で検証する。

use std::cell::Cell;

use shiguredo_mp4::{
    Decode, Encode,
    bitstream::h264::{
        H264ProfileLevelId, H264SampleEntryConfig, LengthSize, annexb_to_length_prefixed,
        build_avc1_box, length_prefixed_to_annexb, parse_annexb_nal_units,
        parse_length_prefixed_nal_units, parse_sps,
    },
    boxes::Avc1Box,
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

    /// 符号付き Exp-Golomb (`se(v)`) を書き込む
    fn push_se(&mut self, value: i64) {
        let code_num = if value > 0 {
            (2 * value - 1) as u32
        } else {
            (-2 * value) as u32
        };
        self.push_ue(code_num);
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// SPS 追加構文 (chroma_format_idc 以降) を読む `profile_idc` (ITU-T H.264 7.3.2.1.1)
fn is_extended_profile(profile_idc: u8) -> bool {
    matches!(
        profile_idc,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
    )
}

/// クロップ無しの `seq_parameter_set_rbsp` 構築パラメタ
#[derive(Debug, Clone, Copy)]
struct SpsBits {
    profile_idc: u8,
    constraint_set_flags: u8,
    level_idc: u8,
    chroma_format_idc: u8,
    separate_colour_plane_flag: bool,
    bit_depth_luma_minus8: u8,
    bit_depth_chroma_minus8: u8,
    pic_order_cnt_type: u8,
    pic_width_in_mbs_minus1: u32,
    pic_height_in_map_units_minus1: u32,
    frame_mbs_only_flag: bool,
}

/// ランダムな SPS パラメタを生成する
///
/// クロップ無し固定にすることで、幅・高さの期待値を単純な式
/// (`(width_mbs + 1) * 16` と `(2 - fmo) * height_map * 16`) で検証できるようにする。
/// クロップの式は単体テストで固定ケースを確認している
fn sample_sps_bits(ctx: &mut noprop::TestCaseContext) -> SpsBits {
    let profile_idc = match noprop::sample_u64_in(ctx, 0..=7) {
        0 => 66u8,
        1 => 77,
        2 => 88,
        3 => 100,
        4 => 110,
        5 => 122,
        6 => 244,
        _ => 44,
    };
    let extended = is_extended_profile(profile_idc);
    let chroma_format_idc = if extended {
        noprop::sample_u64_in(ctx, 0..=3) as u8
    } else {
        1
    };
    SpsBits {
        profile_idc,
        constraint_set_flags: noprop::sample_u8(ctx),
        level_idc: noprop::sample_u8(ctx),
        chroma_format_idc,
        separate_colour_plane_flag: chroma_format_idc == 3 && noprop::sample_bool(ctx),
        bit_depth_luma_minus8: if extended {
            noprop::sample_u64_in(ctx, 0..=6) as u8
        } else {
            0
        },
        bit_depth_chroma_minus8: if extended {
            noprop::sample_u64_in(ctx, 0..=6) as u8
        } else {
            0
        },
        pic_order_cnt_type: noprop::sample_u64_in(ctx, 0..=2) as u8,
        pic_width_in_mbs_minus1: noprop::sample_u64_in(ctx, 1..=120) as u32,
        pic_height_in_map_units_minus1: noprop::sample_u64_in(ctx, 1..=68) as u32,
        frame_mbs_only_flag: noprop::sample_bool(ctx),
    }
}

/// NAL ヘッダー (type 7) 付きのクロップ無し SPS EBSP を組み立てる
fn build_sps(p: &SpsBits) -> Vec<u8> {
    let mut w = BitWriter::new();
    // NAL ヘッダー: forbidden_zero_bit = 0 / nal_ref_idc = 3 / nal_unit_type = 7
    w.push_bits(0x67, 8);
    w.push_bits(u64::from(p.profile_idc), 8);
    w.push_bits(u64::from(p.constraint_set_flags), 8);
    w.push_bits(u64::from(p.level_idc), 8);
    w.push_ue(0); // seq_parameter_set_id
    if is_extended_profile(p.profile_idc) {
        w.push_ue(u32::from(p.chroma_format_idc));
        if p.chroma_format_idc == 3 {
            w.push_bit(u8::from(p.separate_colour_plane_flag));
        }
        w.push_ue(u32::from(p.bit_depth_luma_minus8));
        w.push_ue(u32::from(p.bit_depth_chroma_minus8));
        w.push_bit(0); // qpprime_y_zero_transform_bypass_flag
        w.push_bit(0); // seq_scaling_matrix_present_flag = 0
    }
    w.push_ue(0); // log2_max_frame_num_minus4
    w.push_ue(u32::from(p.pic_order_cnt_type));
    if p.pic_order_cnt_type == 0 {
        w.push_ue(0); // log2_max_pic_order_cnt_lsb_minus4
    } else if p.pic_order_cnt_type == 1 {
        w.push_bit(0); // delta_pic_order_always_zero_flag
        w.push_se(0); // offset_for_non_ref_pic
        w.push_se(0); // offset_for_top_to_bottom_field
        w.push_ue(0); // num_ref_frames_in_pic_order_cnt_cycle
    }
    w.push_ue(1); // max_num_ref_frames
    w.push_bit(0); // gaps_in_frame_num_value_allowed_flag
    w.push_ue(p.pic_width_in_mbs_minus1);
    w.push_ue(p.pic_height_in_map_units_minus1);
    w.push_bit(u8::from(p.frame_mbs_only_flag));
    if !p.frame_mbs_only_flag {
        w.push_bit(0); // mb_adaptive_frame_field_flag
    }
    w.push_bit(1); // direct_8x8_inference_flag
    w.push_bit(0); // frame_cropping_flag = 0
    w.into_bytes()
}

/// ランダムな NAL 本体を生成する
///
/// 開始コード (`00 00 01` / `00 00 00 01`) を本体に含ませず、末尾ゼロも持たせない
/// (Annex B 走査の trailing_zero_8bits 除去と境界分割がラウンドトリップを壊さない
/// ようにするため、全てのバイトを 1..=255 から取る)
fn sample_nal_body(ctx: &mut noprop::TestCaseContext, max_len: usize) -> Vec<u8> {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut body = Vec::new();
    // NAL ヘッダー: forbidden_zero_bit = 0 を保ち、非ゼロにする (型 0 でも可)
    body.push(noprop::sample_u64_in(ctx, 1..=0x7F) as u8);
    for _ in 1..len {
        body.push(noprop::sample_u64_in(ctx, 1..=255) as u8);
    }
    body
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

/// 任意の 3 バイトについて `H264ProfileLevelId::from_hex` と `to_hex` がラウンドトリップする
///
/// - `to_hex` がちょうど 6 桁の小文字 base16 を返す
/// - `H264ProfileLevelId::from_hex(&id.to_hex())` が元の値を返す
#[test]
fn h264_profile_level_id_hex_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(CASES, |ctx| {
        let id = H264ProfileLevelId {
            profile_idc: noprop::sample_u8(ctx),
            profile_iop: noprop::sample_u8(ctx),
            level_idc: noprop::sample_u8(ctx),
        };

        let hex = id.to_hex();
        assert_eq!(hex.len(), 6, "to_hex は 6 桁を返す");
        assert!(
            hex.bytes()
                .all(|c| c.is_ascii_digit() || matches!(c, b'a'..=b'f')),
            "to_hex は小文字 base16 のみを返す"
        );

        let roundtrip = H264ProfileLevelId::from_hex(&hex)
            .expect("to_hex の出力は from_hex でデコード成功する");
        assert_eq!(roundtrip, id, "from_hex(&id.to_hex()) が元の値を返す");
        Ok(())
    })?;
    Ok(())
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
/// - `profile_idc` / `profile_iop` / `level_idc` が [`H264Sps::profile_level_id`] にそのまま復元される
/// - 幅・高さが `(width_mbs + 1) * 16` と `(2 - fmo) * height_map * 16` に一致する
/// - `chroma_format_idc` / bit depth が、SPS 追加構文の有無で推論値か実値になる
#[test]
fn sps_bit_layout_invariants() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let extended_cases = Cell::new(0usize);
    let poc1_cases = Cell::new(0usize);
    let fmo0_cases = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);

    runner.run(CASES, |ctx| {
        let bits = sample_sps_bits(ctx);
        let nal = build_sps(&bits);
        let sps = parse_sps(&nal).expect("生成した SPS は解析成功する");

        assert_eq!(sps.profile_level_id.profile_idc, bits.profile_idc);
        assert_eq!(sps.profile_level_id.profile_iop, bits.constraint_set_flags);
        assert_eq!(sps.profile_level_id.level_idc, bits.level_idc);

        // クロップ無し固定なので幅・高さは単純な式で期待値が定まる
        let expected_width = (u64::from(bits.pic_width_in_mbs_minus1) + 1) * 16;
        let expected_height = (2 - u64::from(bits.frame_mbs_only_flag))
            * u64::from(bits.pic_height_in_map_units_minus1 + 1)
            * 16;
        assert_eq!(u64::from(sps.width), expected_width, "幅が一致する");
        assert_eq!(u64::from(sps.height), expected_height, "高さが一致する");

        let extended = is_extended_profile(bits.profile_idc);
        if extended {
            // SPS 追加構文の実値が反映される
            assert_eq!(sps.chroma_format_idc, bits.chroma_format_idc);
            assert_eq!(sps.bit_depth_luma_minus8, bits.bit_depth_luma_minus8);
            assert_eq!(sps.bit_depth_chroma_minus8, bits.bit_depth_chroma_minus8);
            extended_cases.set(extended_cases.get() + 1);
        } else {
            // SPS 追加構文が無い profile_idc では推論値 (4:2:0 / 8-bit) になる
            assert_eq!(sps.chroma_format_idc, 1);
            assert_eq!(sps.bit_depth_luma_minus8, 0);
            assert_eq!(sps.bit_depth_chroma_minus8, 0);
        }
        if bits.pic_order_cnt_type == 1 {
            poc1_cases.set(poc1_cases.get() + 1);
        }
        if !bits.frame_mbs_only_flag {
            fmo0_cases.set(fmo0_cases.get() + 1);
        }
        Ok(())
    })?;

    assert!(
        extended_cases.get() > 0,
        "SPS 追加構文付き profile_idc を一度も踏んでいない\n{runner}"
    );
    assert!(
        poc1_cases.get() > 0,
        "pic_order_cnt_type == 1 の分岐を一度も踏んでいない\n{runner}"
    );
    assert!(
        fmo0_cases.get() > 0,
        "frame_mbs_only_flag == 0 の分岐を一度も踏んでいない\n{runner}"
    );
    Ok(())
}

/// `build_avc1_box` が生成した SPS から avcC / VisualSampleEntry の欄を正しく導出する
///
/// - profile / constraint / level が SPS から写る
/// - 幅・高さがクロップ無し SPS の寸法に一致する
/// - 66 / 77 / 88 では `chroma_format` 等が `None`、それ以外では SPS の値になる
/// - 長さ幅 1 / 2 / 4 が `length_size_minus_one` (0 / 1 / 3) に写る
/// - encode → decode でラウンドトリップする
#[test]
fn build_avc1_box_invariants() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(CASES, |ctx| {
        let bits = sample_sps_bits(ctx);
        let sps = build_sps(&bits);
        let parsed = parse_sps(&sps).expect("生成した SPS は解析成功する");

        // PPS (type 8 の非空 NAL)
        let mut pps = vec![0x40 | 8];
        let pps_payload_len = noprop::sample_usize_in(ctx, 0..=8);
        for _ in 0..pps_payload_len {
            pps.push(noprop::sample_u64_in(ctx, 1..=255) as u8);
        }

        let length_size = sample_length_size(ctx);
        let avc1 = build_avc1_box(
            core::slice::from_ref(&sps),
            &[pps],
            &H264SampleEntryConfig { length_size },
        )
        .expect("有効な SPS / PPS は構築成功する");

        // ストリーム導出値
        assert_eq!(avc1.avcc_box.avc_profile_indication, bits.profile_idc);
        assert_eq!(
            avc1.avcc_box.profile_compatibility,
            bits.constraint_set_flags
        );
        assert_eq!(avc1.avcc_box.avc_level_indication, bits.level_idc);
        assert_eq!(avc1.visual.width, parsed.width);
        assert_eq!(avc1.visual.height, parsed.height);
        assert_eq!(avc1.avcc_box.sps_list, vec![sps]);

        // 66 / 77 / 88 では追加欄が None、それ以外では SPS の値
        let baseline_like = matches!(bits.profile_idc, 66 | 77 | 88);
        assert_eq!(
            avc1.avcc_box.chroma_format.is_some(),
            !baseline_like,
            "chroma_format の有無"
        );
        if !baseline_like {
            assert_eq!(
                avc1.avcc_box.chroma_format.map(|c| c.get()),
                Some(bits.chroma_format_idc)
            );
            assert_eq!(
                avc1.avcc_box.bit_depth_luma_minus8.map(|b| b.get()),
                Some(bits.bit_depth_luma_minus8)
            );
            assert_eq!(
                avc1.avcc_box.bit_depth_chroma_minus8.map(|b| b.get()),
                Some(bits.bit_depth_chroma_minus8)
            );
        }

        // 呼び出し側指定値 (幅 1 / 2 / 4 → length_size_minus_one = 0 / 1 / 3)
        assert_eq!(
            avc1.avcc_box.length_size_minus_one.get(),
            length_size.length_size_minus_one()
        );

        // 固定値
        assert!(avc1.unknown_boxes.is_empty());
        assert!(avc1.avcc_box.sps_ext_list.is_empty());

        // encode → decode でラウンドトリップ
        let encoded = avc1.encode_to_vec().expect("encode 成功");
        let (decoded, size) = Avc1Box::decode(&encoded).expect("decode 成功");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, avc1);
        Ok(())
    })?;
    Ok(())
}
