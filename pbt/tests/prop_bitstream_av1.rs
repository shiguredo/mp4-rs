//! `shiguredo_mp4::bitstream::av1` の Property-Based Testing
//!
//! 手動構築した LEB128 / OBU 列 / Sequence Header を noprop で生成し、
//! 公開 API の復元と被覆不変条件を検証する

use std::cell::Cell;

use shiguredo_mp4::bitstream::av1::{
    Av1ObuParseContext, Av1ObuType, Av1SampleEntryConfig, build_av01_box, decode_leb128,
    parse_obus, parse_sequence_header,
};

/// このファイルの PBT ケース数
const CASES: usize = 500;

/// 予約を含む、Tile List 以外の `obu_type` (4 ビット値)
const OBU_TYPES: [u8; 15] = [0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15];

/// MSB-first ビット組み立て
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

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

fn encode_leb128_with_len(value: u32, nbytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(nbytes);
    let mut remaining = value;
    for i in 0..nbytes {
        let mut byte = (remaining & 0x7F) as u8;
        remaining >>= 7;
        if i + 1 < nbytes {
            byte |= 0x80;
        }
        out.push(byte);
    }
    out
}

fn shortest_leb128_len(value: u32) -> usize {
    if value == 0 {
        1
    } else {
        let bits = 32 - value.leading_zeros();
        bits.div_ceil(7) as usize
    }
}

fn wrap_obu(obu_type: u8, payload: &[u8]) -> Vec<u8> {
    let header = obu_type << 3 | 0b0000_0010; // has_size=1, 他 0
    let mut out = vec![header];
    out.extend(encode_leb128_with_len(
        payload.len() as u32,
        shortest_leb128_len(payload.len() as u32),
    ));
    out.extend_from_slice(payload);
    out
}

/// `reduced_still_sequence_header` で組み立てる `seq_profile` と色の設定
/// (AV1 spec §5.5.1 の `seq_profile` と §5.5.2 の `color_config()`)
#[derive(Debug, Clone, Copy)]
enum ReducedStillColor {
    /// profile 0、8 ビット、4:2:0。`chroma_sample_position` は 0 (`CSP_UNKNOWN`)
    Profile0EightBit420,
    /// profile 2、12 ビット (`high_bitdepth` と `twelve_bit` が 1)、4:2:0。
    /// `chroma_sample_position` は指定値 (0..=2)
    Profile2TwelveBit420 { chroma_sample_position: u8 },
}

/// `reduced_still_picture_header = 1` の Sequence Header OBU のペイロードを組み立てる
///
/// ビットの並びは AV1 spec §5.5.1 General sequence header OBU syntax と §5.5.2 Color config syntax に従う
fn reduced_still_sequence_header(width: u32, height: u32, color: ReducedStillColor) -> Vec<u8> {
    let mut w = BitWriter::new();
    let seq_profile = match color {
        ReducedStillColor::Profile0EightBit420 => 0,
        ReducedStillColor::Profile2TwelveBit420 { .. } => 2,
    };
    w.push_bits(seq_profile, 3);
    w.push_bit(1); // still_picture
    w.push_bit(1); // reduced_still_picture_header
    w.push_bits(0, 5); // seq_level_idx[0]
    w.push_bits(15, 4); // frame_width_bits_minus_1
    w.push_bits(15, 4); // frame_height_bits_minus_1
    w.push_bits(width - 1, 16); // max_frame_width_minus_1 (frame_width_bits_minus_1 + 1 = 16 ビット)
    w.push_bits(height - 1, 16); // max_frame_height_minus_1 (frame_height_bits_minus_1 + 1 = 16 ビット)
    w.push_bit(0); // use_128x128_superblock
    w.push_bit(0); // enable_filter_intra
    w.push_bit(0); // enable_intra_edge_filter
    w.push_bit(0); // enable_superres
    w.push_bit(0); // enable_cdef
    w.push_bit(0); // enable_restoration
    match color {
        ReducedStillColor::Profile0EightBit420 => {
            w.push_bit(0); // high_bitdepth
            w.push_bit(0); // mono_chrome
            w.push_bit(0); // color_description_present_flag
            w.push_bit(0); // color_range
            // color_description_present_flag = 0 なので color_config() は color_range を読む側の分岐に入り、
            // seq_profile == 0 では subsampling_x / subsampling_y は構文に現れず、どちらも 1 (4:2:0) になる
            w.push_bits(0, 2); // chroma_sample_position
        }
        ReducedStillColor::Profile2TwelveBit420 {
            chroma_sample_position,
        } => {
            w.push_bit(1); // high_bitdepth
            w.push_bit(1); // twelve_bit (seq_profile == 2 かつ high_bitdepth のときだけ現れる)
            w.push_bit(0); // mono_chrome
            w.push_bit(0); // color_description_present_flag
            w.push_bit(0); // color_range
            w.push_bit(1); // subsampling_x (BitDepth == 12 のときだけ現れる)
            w.push_bit(1); // subsampling_y (subsampling_x == 1 のときだけ現れる)
            w.push_bits(u32::from(chroma_sample_position), 2); // chroma_sample_position
        }
    }
    w.push_bit(0); // separate_uv_delta_q
    w.push_bit(0); // film_grain_params_present
    w.into_bytes()
}

/// LEB128 の最短・非最短が同じ値に戻ること
///
/// - 値域は 0 と `u32::MAX` を境界に含める (p ≈ 1/4 で境界)
/// - 非最短は shortest..=8 バイト。8 バイト到達は値の大きさに依存せずバイト数を上げる
#[test]
fn leb128_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let saw_non_shortest = Cell::new(0usize);
    let saw_eight_bytes = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let value = noprop::sample_with_boundaries(
            ctx,
            &[0u32, u32::MAX],
            noprop::Ratio::one_nth(4),
            noprop::sample_u32,
        );
        let shortest = shortest_leb128_len(value);
        let nbytes =
            noprop::sample_with_boundaries(ctx, &[shortest, 8], noprop::Ratio::one_nth(4), |ctx| {
                noprop::sample_usize_in(ctx, shortest..=8)
            });
        let encoded = encode_leb128_with_len(value, nbytes);
        let (decoded, consumed) = decode_leb128(&encoded).expect("合法 LEB128 は成功する");
        assert_eq!(decoded, value);
        assert_eq!(consumed, nbytes);
        if nbytes > shortest {
            saw_non_shortest.set(saw_non_shortest.get() + 1);
        }
        if nbytes == 8 {
            saw_eight_bytes.set(saw_eight_bytes.get() + 1);
        }
        Ok(())
    })?;
    assert!(
        saw_non_shortest.get() > 0,
        "非最短 LEB128 を一度も見ていない\n{runner}"
    );
    assert!(
        saw_eight_bytes.get() > 0,
        "8 バイト LEB128 を一度も見ていない\n{runner}"
    );
    Ok(())
}

/// 構築した OBU 列が入力を重複なく覆うこと
///
/// OBU 個数 0 (空 configOBUs) / 1 / 8 を境界に含める。
/// Tile List は生成しない (公開 API が拒否するため、被覆検証の対象外)
#[test]
fn obu_covers_input_without_overlap() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let saw_empty = Cell::new(0usize);
    let saw_multi = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let count = noprop::sample_with_boundaries(
            ctx,
            &[0usize, 1, 8],
            noprop::Ratio::one_nth(5),
            |ctx| noprop::sample_usize_in(ctx, 0..=8),
        );
        let mut input = Vec::new();
        let mut expected_types = Vec::new();
        for _ in 0..count {
            let type_idx = noprop::sample_usize_in(ctx, 0..OBU_TYPES.len());
            let obu_type = OBU_TYPES[type_idx];
            let payload_len = noprop::sample_usize_in(ctx, 0..=16);
            let payload = noprop::sample_bytes_vec(ctx, payload_len);
            input.extend(wrap_obu(obu_type, &payload));
            expected_types.push(obu_type);
        }
        let obus = parse_obus(&input, Av1ObuParseContext::ConfigObus)
            .expect("合法な size 付き OBU 列は成功する");
        assert_eq!(obus.len(), count);
        let mut covered = 0usize;
        for (i, obu) in obus.iter().enumerate() {
            assert_eq!(
                obu.obu.as_ptr(),
                input[covered..].as_ptr(),
                "OBU が入力上で連続している"
            );
            covered += obu.obu.len();
            match (expected_types[i], obu.obu_type) {
                (1, Av1ObuType::SequenceHeader)
                | (2, Av1ObuType::TemporalDelimiter)
                | (3, Av1ObuType::FrameHeader)
                | (4, Av1ObuType::TileGroup)
                | (5, Av1ObuType::Metadata)
                | (6, Av1ObuType::Frame)
                | (7, Av1ObuType::RedundantFrameHeader)
                | (15, Av1ObuType::Padding) => {}
                (t, Av1ObuType::Reserved(r)) => assert_eq!(t, r),
                (t, other) => panic!("想定外の種別 {t} -> {other:?}"),
            }
        }
        assert_eq!(covered, input.len());
        if count == 0 {
            saw_empty.set(saw_empty.get() + 1);
        }
        if count >= 2 {
            saw_multi.set(saw_multi.get() + 1);
        }
        Ok(())
    })?;
    assert!(saw_empty.get() > 0, "空列を一度も見ていない\n{runner}");
    assert!(saw_multi.get() > 0, "複数 OBU を一度も見ていない\n{runner}");
    Ok(())
}

/// 正当な Sequence Header から構築した `Av01Box` について、次がすべて成り立つこと
///
/// - 幅・高さが、Sequence Header の生成に使った値と一致する
/// - av1C の `seq_profile`、`high_bitdepth`、`twelve_bit`、`chroma_sample_position` が、Sequence Header の解析結果と一致する
/// - av1C の configOBUs が、`build_av01_box` に渡した configOBUs と一致する
///
/// 色の設定は、profile 0 の 8 ビット 4:2:0 と、色差サンプル位置が非ゼロの正常値 (1 または 2) となる
/// profile 2 の 12 ビット 4:2:0 を半々に生成する。後者で `high_bitdepth`、`twelve_bit`、
/// `chroma_sample_position` が av1C に写ることを確かめる
#[test]
fn build_av01_box_matches_sequence_header() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let saw_delay = Cell::new(0usize);
    let saw_twelve_bit = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let width =
            noprop::sample_with_boundaries(ctx, &[1u32, 65535], noprop::Ratio::one_nth(4), |ctx| {
                noprop::sample_u64_in(ctx, 1..=65535) as u32
            });
        let height =
            noprop::sample_with_boundaries(ctx, &[1u32, 65535], noprop::Ratio::one_nth(4), |ctx| {
                noprop::sample_u64_in(ctx, 1..=65535) as u32
            });
        let color = if noprop::sample_bool(ctx) {
            ReducedStillColor::Profile0EightBit420
        } else {
            ReducedStillColor::Profile2TwelveBit420 {
                chroma_sample_position: noprop::sample_choice(ctx, &[1u8, 2]),
            }
        };
        let payload = reduced_still_sequence_header(width, height, color);
        let seq = parse_sequence_header(&payload).expect("合法 SH は成功する");
        let config_obus = wrap_obu(1, &payload);
        let delay = if noprop::sample_bool(ctx) {
            Some(noprop::sample_u64_in(ctx, 0..=15) as u8)
        } else {
            None
        };
        let box_ = build_av01_box(
            &seq,
            &config_obus,
            &Av1SampleEntryConfig {
                initial_presentation_delay_minus_one: delay,
            },
        )
        .expect("一致する SH から構築できる");
        assert_eq!(u32::from(box_.visual.width), width);
        assert_eq!(u32::from(box_.visual.height), height);
        assert_eq!(box_.av1c_box.seq_profile.get(), seq.seq_profile);
        assert_eq!(
            box_.av1c_box.high_bitdepth.get(),
            u8::from(seq.high_bitdepth)
        );
        assert_eq!(box_.av1c_box.twelve_bit.get(), u8::from(seq.twelve_bit));
        assert_eq!(
            box_.av1c_box.chroma_sample_position.get(),
            seq.chroma_sample_position
        );
        assert_eq!(box_.av1c_box.config_obus, config_obus);
        if let ReducedStillColor::Profile2TwelveBit420 {
            chroma_sample_position,
        } = color
        {
            // 生成した値が解析結果にもそのまま現れていること (生成器の誤りを検出する)
            assert!(
                seq.high_bitdepth && seq.twelve_bit,
                "profile 2 の 12 ビットとして生成した SH の解析結果が 12 ビットではない"
            );
            assert_eq!(
                seq.chroma_sample_position, chroma_sample_position,
                "生成した chroma_sample_position が解析結果と一致しない"
            );
            saw_twelve_bit.set(saw_twelve_bit.get() + 1);
        }
        if delay.is_some() {
            saw_delay.set(saw_delay.get() + 1);
        }
        Ok(())
    })?;
    assert!(
        saw_delay.get() > 0,
        "initial_presentation_delay 指定を一度も見ていない\n{runner}"
    );
    assert!(
        saw_twelve_bit.get() > 0,
        "profile 2 の 12 ビット 4:2:0 を一度も見ていない\n{runner}"
    );
    Ok(())
}
