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

fn reduced_still_sequence_header(width: u32, height: u32) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.push_bits(0, 3);
    w.push_bit(1);
    w.push_bit(1);
    w.push_bits(0, 5);
    w.push_bits(15, 4);
    w.push_bits(15, 4);
    w.push_bits(width - 1, 16);
    w.push_bits(height - 1, 16);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0);
    w.push_bits(0, 2);
    w.push_bit(0);
    w.push_bit(0);
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

/// 正当な Sequence Header から構築した `Av01Box` の幅高さと av1C 欄が一致すること
#[test]
fn build_av01_box_matches_sequence_header() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let saw_delay = Cell::new(0usize);
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
        let payload = reduced_still_sequence_header(width, height);
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
        assert_eq!(box_.av1c_box.config_obus, config_obus);
        if delay.is_some() {
            saw_delay.set(saw_delay.get() + 1);
        }
        Ok(())
    })?;
    assert!(
        saw_delay.get() > 0,
        "initial_presentation_delay 指定を一度も見ていない\n{runner}"
    );
    Ok(())
}
