//! `shiguredo_mp4::bitstream::av1` の決定的テスト
//!
//! 手動構築した LEB128 / OBU / Sequence Header / フレーム先頭部に対して
//! パーサーの受理・拒否条件を固定する。実エンコーダー出力による回帰は
//! `tests/testdata/black-av1-video.mp4` を用いた fixture テストで補う

use shiguredo_mp4::{
    Decode, Either, Encode, ErrorKind, Mp4File, Uint,
    bitstream::av1::{
        Av1FrameHeaderPrefix, Av1FrameType, Av1ObuParseContext, Av1ObuType, Av1SampleEntryConfig,
        Av1SequenceHeader, build_av01_box, decode_leb128, parse_frame_header_prefix, parse_obus,
        parse_sequence_header,
    },
    boxes::{RootBox, SampleEntry, StszBox, VisualSampleEntryFields},
};

/// Sequence Header の `obu_type` 値
const OBU_SEQUENCE_HEADER: u8 = 1;
/// Temporal Delimiter の `obu_type` 値
const OBU_TEMPORAL_DELIMITER: u8 = 2;
/// Frame Header の `obu_type` 値
const OBU_FRAME_HEADER: u8 = 3;
/// Tile List の `obu_type` 値
const OBU_TILE_LIST: u8 = 8;
/// Padding の `obu_type` 値
const OBU_PADDING: u8 = 15;
/// Metadata の `obu_type` 値
const OBU_METADATA: u8 = 5;

/// MSB-first ビット組み立て。実装側の `BitReader` と対称
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

/// 最短の LEB128 を符号化する (テスト用)
fn encode_leb128(mut value: u32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
    out
}

/// OBU ヘッダー 1 バイトを組み立てる
fn obu_header_byte(obu_type: u8, extension: bool, has_size: bool, reserved: u8) -> u8 {
    (obu_type << 3) | (u8::from(extension) << 2) | (u8::from(has_size) << 1) | (reserved & 1)
}

/// payload を Low Overhead Bitstream Format の 1 OBU に包む
fn wrap_obu(obu_type: u8, payload: &[u8], has_size: bool) -> Vec<u8> {
    let mut out = vec![obu_header_byte(obu_type, false, has_size, 0)];
    if has_size {
        out.extend(encode_leb128(payload.len() as u32));
    }
    out.extend_from_slice(payload);
    out
}

/// profile 0 / 8-bit / 4:2:0 / reduced still picture の Sequence Header payload
fn reduced_still_sequence_header(width: u32, height: u32) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.push_bits(0, 3); // seq_profile = 0
    w.push_bit(1); // still_picture
    w.push_bit(1); // reduced_still_picture_header
    w.push_bits(0, 5); // seq_level_idx[0]
    w.push_bits(15, 4); // frame_width_bits_minus_1 (16 ビット欄)
    w.push_bits(15, 4); // frame_height_bits_minus_1
    w.push_bits(width - 1, 16);
    w.push_bits(height - 1, 16);
    w.push_bit(0); // use_128x128_superblock
    w.push_bit(0); // enable_filter_intra
    w.push_bit(0); // enable_intra_edge_filter
    w.push_bit(0); // enable_superres
    w.push_bit(0); // enable_cdef
    w.push_bit(0); // enable_restoration
    w.push_bit(0); // high_bitdepth
    w.push_bit(0); // mono_chrome
    w.push_bit(0); // color_description_present_flag
    w.push_bit(0); // color_range
    w.push_bits(0, 2); // chroma_sample_position
    w.push_bit(0); // separate_uv_delta_q
    w.push_bit(0); // film_grain_params_present
    w.into_bytes()
}

/// 通常ヘッダーで operating point を 2 個持つ Sequence Header payload
fn two_operating_point_sequence_header(width: u32, height: u32) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.push_bits(0, 3); // seq_profile
    w.push_bit(0); // still_picture
    w.push_bit(0); // reduced_still_picture_header
    w.push_bit(0); // timing_info_present_flag
    w.push_bit(0); // initial_display_delay_present_flag
    w.push_bits(1, 5); // operating_points_cnt_minus_1 = 1 (2 個)
    // OP 0: level 8 (>7) なので tier ビットあり
    w.push_bits(0, 12); // operating_point_idc[0]
    w.push_bits(8, 5); // seq_level_idx[0]
    w.push_bit(1); // seq_tier[0]
    // OP 1
    w.push_bits(1, 12);
    w.push_bits(0, 5); // level 0、tier は構文に現れない
    w.push_bits(15, 4);
    w.push_bits(15, 4);
    w.push_bits(width - 1, 16);
    w.push_bits(height - 1, 16);
    w.push_bit(0); // frame_id_numbers_present_flag
    w.push_bit(0); // use_128x128_superblock
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0); // enable_interintra_compound
    w.push_bit(0); // enable_masked_compound
    w.push_bit(0); // enable_warped_motion
    w.push_bit(0); // enable_dual_filter
    w.push_bit(0); // enable_order_hint
    w.push_bit(1); // seq_choose_screen_content_tools
    // seq_force_screen_content_tools = SELECT (=2) なので integer mv 選択へ
    w.push_bit(1); // seq_choose_integer_mv
    w.push_bit(0); // enable_superres
    w.push_bit(0); // enable_cdef
    w.push_bit(0); // enable_restoration
    w.push_bit(0); // high_bitdepth
    w.push_bit(0); // mono_chrome
    w.push_bit(0); // color_description_present_flag
    w.push_bit(0); // color_range
    w.push_bits(0, 2);
    w.push_bit(0); // separate_uv_delta_q
    w.push_bit(0); // film_grain
    w.into_bytes()
}

/// `still_picture = 1` だが `reduced_still_picture_header = 0` の Sequence Header。
/// av1C レコード欄と寸法は `reduced_still_sequence_header` と同じになる
fn still_picture_non_reduced_sequence_header(width: u32, height: u32) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.push_bits(0, 3); // seq_profile
    w.push_bit(1); // still_picture
    w.push_bit(0); // reduced_still_picture_header
    w.push_bit(0); // timing_info_present_flag
    w.push_bit(0); // initial_display_delay_present_flag
    w.push_bits(0, 5); // operating_points_cnt_minus_1 = 0 (1 個)
    w.push_bits(0, 12); // operating_point_idc[0]
    w.push_bits(0, 5); // seq_level_idx[0] <= 7 なので tier は構文に現れない
    w.push_bits(15, 4);
    w.push_bits(15, 4);
    w.push_bits(width - 1, 16);
    w.push_bits(height - 1, 16);
    w.push_bit(0); // frame_id_numbers_present_flag
    w.push_bit(0); // use_128x128_superblock
    w.push_bit(0);
    w.push_bit(0);
    w.push_bit(0); // enable_interintra_compound
    w.push_bit(0); // enable_masked_compound
    w.push_bit(0); // enable_warped_motion
    w.push_bit(0); // enable_dual_filter
    w.push_bit(0); // enable_order_hint
    w.push_bit(1); // seq_choose_screen_content_tools
    w.push_bit(1); // seq_choose_integer_mv
    w.push_bit(0); // enable_superres
    w.push_bit(0); // enable_cdef
    w.push_bit(0); // enable_restoration
    w.push_bit(0); // high_bitdepth
    w.push_bit(0); // mono_chrome
    w.push_bit(0); // color_description_present_flag
    w.push_bit(0); // color_range
    w.push_bits(0, 2);
    w.push_bit(0); // separate_uv_delta_q
    w.push_bit(0); // film_grain
    w.into_bytes()
}

fn parse_sh(payload: &[u8]) -> Av1SequenceHeader {
    parse_sequence_header(payload).expect("有効な Sequence Header は解析成功する")
}

mod leb128 {
    use super::*;

    /// 1 バイトの最短表現
    #[test]
    fn single_byte() {
        let (v, n) = decode_leb128(&[0x00]).expect("0 は 1 バイトで終端する");
        assert_eq!((v, n), (0, 1));
        let (v, n) = decode_leb128(&[0x7F]).expect("127 は 1 バイト");
        assert_eq!((v, n), (127, 1));
    }

    /// 複数バイトの最短表現
    #[test]
    fn multi_byte() {
        let encoded = encode_leb128(128);
        let (v, n) = decode_leb128(&encoded).expect("128 は 2 バイト");
        assert_eq!(v, 128);
        assert_eq!(n, 2);
    }

    /// 非最短表現も受理する (AV1 spec §4.10.5)
    #[test]
    fn non_shortest() {
        // 値 1 を 2 バイト (0x81, 0x00) で表す
        let (v, n) = decode_leb128(&[0x81, 0x00]).expect("非最短を受理する");
        assert_eq!((v, n), (1, 2));
    }

    /// 終端ビットが来る前に入力が尽きたら拒否する
    #[test]
    fn truncated() {
        let err = decode_leb128(&[0x80]).expect_err("未終端は拒否する");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// 8 バイト目の continuation bit が 1 なら拒否する
    #[test]
    fn eighth_byte_continuation() {
        let err = decode_leb128(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80])
            .expect_err("8 バイト目の continuation は拒否する");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// `(1 << 32) - 1` を超える値は拒否する
    #[test]
    fn exceeds_u32() {
        // 5 バイト分の continuation で 35 ビットの 1 を集め、6 バイト目で終端する
        let err =
            decode_leb128(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]).expect_err("u32 超過は拒否する");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

mod obu {
    use super::*;

    /// configOBUs の空入力は受理する
    #[test]
    fn config_empty_ok() {
        let obus = parse_obus(&[], Av1ObuParseContext::ConfigObus).expect("空の configOBUs は許容");
        assert!(obus.is_empty());
    }

    /// サンプルの空入力は拒否する
    #[test]
    fn sample_empty_rejected() {
        let err = parse_obus(&[], Av1ObuParseContext::Sample)
            .expect_err("空サンプルは Temporal Unit ではない");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// configOBUs でサイズ省略は拒否する
    #[test]
    fn config_requires_size() {
        let bytes = wrap_obu(OBU_PADDING, &[0x80], false);
        let err = parse_obus(&bytes, Av1ObuParseContext::ConfigObus)
            .expect_err("configOBUs は size 必須");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// サンプルの最後の OBU だけサイズ省略できる
    #[test]
    fn sample_last_may_omit_size() {
        let mut bytes = wrap_obu(OBU_TEMPORAL_DELIMITER, &[], true);
        bytes.extend(wrap_obu(OBU_PADDING, &[0x80], false));
        let obus = parse_obus(&bytes, Av1ObuParseContext::Sample).expect("最後の省略は受理");
        assert_eq!(obus.len(), 2);
        assert_eq!(obus[0].obu_type, Av1ObuType::TemporalDelimiter);
        assert_eq!(obus[1].obu_type, Av1ObuType::Padding);
        assert_eq!(obus[1].payload, &[0x80]);
    }

    /// forbidden bit は拒否する
    #[test]
    fn forbidden_bit() {
        let mut bytes = wrap_obu(OBU_PADDING, &[], true);
        bytes[0] |= 0x80;
        let err =
            parse_obus(&bytes, Av1ObuParseContext::ConfigObus).expect_err("forbidden_bit は 0");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// reserved bit は拒否する (完全な OBU で reserved だけを 1 にする)
    #[test]
    fn reserved_bit() {
        let mut bytes = wrap_obu(OBU_PADDING, &[0x80], true);
        bytes[0] |= 0x01;
        // leb128 と payload を備えた完全な OBU なので、reserved 検査を外せば成功する。
        // reason で「reserved が原因」であることを固定する
        let err = parse_obus(&bytes, Av1ObuParseContext::ConfigObus).expect_err("reserved は拒否");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
        assert_eq!(err.reason, "AV1 obu_reserved_1bit must be 0");
    }

    /// extension header が 1 バイト足りない
    #[test]
    fn short_extension() {
        let bytes = vec![obu_header_byte(OBU_FRAME_HEADER, true, true, 0)];
        // extension バイトが無い入力は、extension 検査を外すと空スライスへの leb128 で
        // 同じ InvalidInput になる。reason で「extension header が原因」であることを固定する
        let err =
            parse_obus(&bytes, Av1ObuParseContext::ConfigObus).expect_err("extension header 不足");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
        assert_eq!(err.reason, "AV1 obu_extension_header is truncated");
    }

    /// 宣言サイズが入力を超える
    #[test]
    fn declared_size_overflow() {
        let mut bytes = vec![obu_header_byte(OBU_PADDING, false, true, 0)];
        bytes.extend(encode_leb128(8));
        bytes.push(0x00);
        let err = parse_obus(&bytes, Av1ObuParseContext::ConfigObus).expect_err("obu_size 超過");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// Tile List は両コンテキストで拒否する
    #[test]
    fn tile_list_rejected() {
        let bytes = wrap_obu(OBU_TILE_LIST, &[], true);
        for ctx in [Av1ObuParseContext::ConfigObus, Av1ObuParseContext::Sample] {
            let err = parse_obus(&bytes, ctx).expect_err("TILE_LIST は Binding で拒否");
            assert_eq!(err.kind, ErrorKind::InvalidInput);
            assert_eq!(
                err.reason,
                "AV1 OBU_TILE_LIST is not supported by AV1 Codec ISO Media File Format Binding"
            );
        }
    }

    /// SHOULD NOT の Temporal Delimiter / Padding は受理する
    #[test]
    fn should_not_types_accepted() {
        let mut bytes = wrap_obu(OBU_TEMPORAL_DELIMITER, &[], true);
        bytes.extend(wrap_obu(OBU_PADDING, &[0x80], true));
        bytes.extend(wrap_obu(7, &[], true)); // OBU_REDUNDANT_FRAME_HEADER
        let obus = parse_obus(&bytes, Av1ObuParseContext::ConfigObus).expect("SHOULD NOT は受理");
        assert_eq!(obus[0].obu_type, Av1ObuType::TemporalDelimiter);
        assert_eq!(obus[1].obu_type, Av1ObuType::Padding);
        assert_eq!(obus[2].obu_type, Av1ObuType::RedundantFrameHeader);
    }

    /// Sequence Header の extension flag は拒否する (AV1 spec §6.2.2 は非 layer-specific)
    #[test]
    fn sequence_header_extension_rejected() {
        let mut bytes = vec![obu_header_byte(OBU_SEQUENCE_HEADER, true, true, 0)];
        bytes.push(0); // extension header (temporal=0, spatial=0, reserved=0)
        bytes.extend(encode_leb128(0));
        let err = parse_obus(&bytes, Av1ObuParseContext::ConfigObus)
            .expect_err("Sequence Header は layer-specific ではない");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
        assert_eq!(
            err.reason,
            "AV1 Sequence Header OBU must have obu_extension_flag equal to 0"
        );
    }

    /// Temporal Delimiter の extension flag は拒否する (同じく非 layer-specific)
    #[test]
    fn temporal_delimiter_extension_rejected() {
        let mut bytes = vec![obu_header_byte(OBU_TEMPORAL_DELIMITER, true, true, 0)];
        bytes.push(0); // extension header (temporal=0, spatial=0, reserved=0)
        bytes.extend(encode_leb128(0));
        let err = parse_obus(&bytes, Av1ObuParseContext::ConfigObus)
            .expect_err("Temporal Delimiter は layer-specific ではない");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
        assert_eq!(
            err.reason,
            "AV1 Temporal Delimiter OBU must have obu_extension_flag equal to 0"
        );
    }

    /// Padding は Either なので extension header 付きでも受理する
    #[test]
    fn padding_extension_accepted() {
        let mut bytes = vec![obu_header_byte(OBU_PADDING, true, true, 0)];
        bytes.push(0); // extension header (temporal=0, spatial=0, reserved=0)
        bytes.extend(encode_leb128(1));
        bytes.push(0x80);
        let obus = parse_obus(&bytes, Av1ObuParseContext::ConfigObus)
            .expect("Padding は extension 付きでも受理する");
        assert_eq!(obus[0].obu_type, Av1ObuType::Padding);
        assert_eq!(obus[0].payload, &[0x80]);
    }

    /// 予約済み `obu_type` (0) はサイズで読み飛ばし、列挙結果に含める
    #[test]
    fn reserved_type_skipped() {
        let bytes = wrap_obu(0, &[0xFF], true);
        let obus = parse_obus(&bytes, Av1ObuParseContext::ConfigObus).expect("予約は読み飛ばす");
        assert_eq!(obus[0].obu_type, Av1ObuType::Reserved(0));
        assert_eq!(obus[0].payload, &[0xFF]);
    }
}

mod sequence_header {
    use super::*;

    /// profile 0 / 8-bit / 4:2:0 の reduced still picture
    #[test]
    fn profile0_8bit_420() {
        let payload = reduced_still_sequence_header(320, 240);
        let sh = parse_sh(&payload);
        assert_eq!(sh.seq_profile, 0);
        assert!(!sh.high_bitdepth);
        assert!(!sh.twelve_bit);
        assert!(!sh.monochrome);
        assert_eq!((sh.chroma_subsampling_x, sh.chroma_subsampling_y), (1, 1));
        assert_eq!(sh.max_frame_width, 320);
        assert_eq!(sh.max_frame_height, 240);
        assert!(sh.reduced_still_picture_header);
        // reduced_still_picture_header == 1 のときは operating point は暗黙値
        assert_eq!(sh.operating_points_cnt_minus_1, 0);
        assert_eq!(sh.operating_point_idc_0, 0);
    }

    /// 複数 operating point を拒否せず、index 0 の level / tier / idc を公開する
    #[test]
    fn multiple_operating_points() {
        let payload = two_operating_point_sequence_header(64, 64);
        let sh = parse_sh(&payload);
        assert_eq!(sh.operating_points_cnt_minus_1, 1);
        assert_eq!(sh.operating_point_idc_0, 0);
        assert_eq!(sh.seq_level_idx_0, 8);
        assert_eq!(sh.seq_tier_0, 1);
        assert!(!sh.reduced_still_picture_header);
    }

    /// `operating_point_idc[0]` が非 0 でも公開する（拒否しない）
    #[test]
    fn non_zero_operating_point_idc_0() {
        let mut w = BitWriter::new();
        w.push_bits(0, 3); // seq_profile
        w.push_bit(0); // still_picture
        w.push_bit(0); // reduced_still_picture_header
        w.push_bit(0); // timing_info_present_flag
        w.push_bit(0); // initial_display_delay_present_flag
        w.push_bits(0, 5); // operating_points_cnt_minus_1 = 0 (1 個)
        w.push_bits(0xABC, 12); // operating_point_idc[0]
        w.push_bits(0, 5); // seq_level_idx[0]
        w.push_bits(15, 4);
        w.push_bits(15, 4);
        w.push_bits(15, 16);
        w.push_bits(15, 16);
        w.push_bit(0); // frame_id_numbers_present_flag
        w.push_bit(0); // use_128x128_superblock
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0); // enable_interintra_compound
        w.push_bit(0); // enable_masked_compound
        w.push_bit(0); // enable_warped_motion
        w.push_bit(0); // enable_dual_filter
        w.push_bit(0); // enable_order_hint
        w.push_bit(1); // seq_choose_screen_content_tools
        w.push_bit(1); // seq_choose_integer_mv
        w.push_bit(0); // enable_superres
        w.push_bit(0); // enable_cdef
        w.push_bit(0); // enable_restoration
        w.push_bit(0); // high_bitdepth
        w.push_bit(0); // mono_chrome
        w.push_bit(0); // color_description_present_flag
        w.push_bit(0); // color_range
        w.push_bits(0, 2); // chroma_sample_position
        w.push_bit(0); // separate_uv_delta_q
        w.push_bit(0); // film_grain
        let sh = parse_sh(&w.into_bytes());
        assert_eq!(sh.operating_points_cnt_minus_1, 0);
        assert_eq!(sh.operating_point_idc_0, 0xABC);
    }

    /// seq_profile 3 は予約なので拒否する
    #[test]
    fn reserved_profile() {
        let mut w = BitWriter::new();
        w.push_bits(3, 3);
        w.push_bit(1);
        w.push_bit(1);
        let err = parse_sequence_header(&w.into_bytes()).expect_err("profile 3 は予約");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// chroma_sample_position == 3 (`CSP_RESERVED`) は拒否する
    #[test]
    fn reserved_chroma_sample_position() {
        let mut w = BitWriter::new();
        w.push_bits(0, 3); // seq_profile
        w.push_bit(1); // still_picture
        w.push_bit(1); // reduced_still_picture_header
        w.push_bits(0, 5); // seq_level_idx[0]
        w.push_bits(15, 4);
        w.push_bits(15, 4);
        w.push_bits(15, 16);
        w.push_bits(15, 16);
        w.push_bit(0); // use_128x128_superblock
        w.push_bit(0); // enable_filter_intra
        w.push_bit(0); // enable_intra_edge_filter
        w.push_bit(0); // enable_superres
        w.push_bit(0); // enable_cdef
        w.push_bit(0); // enable_restoration
        w.push_bit(0); // high_bitdepth
        w.push_bit(0); // mono_chrome
        w.push_bit(0); // color_description_present_flag
        w.push_bit(0); // color_range
        w.push_bits(3, 2); // chroma_sample_position = CSP_RESERVED
        w.push_bit(0); // separate_uv_delta_q
        w.push_bit(0); // film_grain_params_present
        let err = parse_sequence_header(&w.into_bytes()).expect_err("CSP_RESERVED は拒否");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
        assert_eq!(
            err.reason,
            "AV1 chroma_sample_position 3 is reserved (CSP_RESERVED)"
        );
    }

    /// profile 0 / 10-bit (`high_bitdepth = 1`, `twelve_bit` は構文に現れない)
    #[test]
    fn profile0_10bit() {
        let mut w = BitWriter::new();
        w.push_bits(0, 3);
        w.push_bit(1);
        w.push_bit(1);
        w.push_bits(0, 5);
        w.push_bits(15, 4);
        w.push_bits(15, 4);
        w.push_bits(15, 16);
        w.push_bits(15, 16);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(1); // high_bitdepth
        w.push_bit(0); // mono_chrome
        w.push_bit(0);
        w.push_bit(0);
        w.push_bits(1, 2); // chroma_sample_position
        w.push_bit(0);
        w.push_bit(0);
        let sh = parse_sh(&w.into_bytes());
        assert!(sh.high_bitdepth);
        assert!(!sh.twelve_bit);
        assert_eq!(sh.chroma_sample_position, 1);
    }

    /// profile 2 / 12-bit / 4:2:0
    #[test]
    fn profile2_12bit_420() {
        let mut w = BitWriter::new();
        w.push_bits(2, 3);
        w.push_bit(1);
        w.push_bit(1);
        w.push_bits(0, 5);
        w.push_bits(15, 4);
        w.push_bits(15, 4);
        w.push_bits(15, 16);
        w.push_bits(15, 16);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(1); // high_bitdepth
        w.push_bit(1); // twelve_bit
        w.push_bit(0); // mono_chrome
        w.push_bit(0); // color_description_present_flag
        w.push_bit(0); // color_range
        w.push_bit(1); // subsampling_x (12-bit 経路)
        w.push_bit(1); // subsampling_y
        w.push_bits(2, 2); // chroma_sample_position
        w.push_bit(0); // separate_uv_delta_q
        w.push_bit(0);
        let sh = parse_sh(&w.into_bytes());
        assert_eq!(sh.seq_profile, 2);
        assert!(sh.high_bitdepth);
        assert!(sh.twelve_bit);
        assert_eq!((sh.chroma_subsampling_x, sh.chroma_subsampling_y), (1, 1));
        assert_eq!(sh.chroma_sample_position, 2);
    }

    /// profile 1 は 4:4:4 固定で chroma_sample_position は構文に現れない
    #[test]
    fn profile1_444() {
        let mut w = BitWriter::new();
        w.push_bits(1, 3);
        w.push_bit(1);
        w.push_bit(1);
        w.push_bits(0, 5);
        w.push_bits(15, 4);
        w.push_bits(15, 4);
        w.push_bits(15, 16);
        w.push_bits(15, 16);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0); // high_bitdepth
        // profile 1 は mono_chrome を書かない
        w.push_bit(0); // color_description_present_flag
        w.push_bit(0); // color_range
        // 4:4:4 なので chroma_sample_position なし
        w.push_bit(0); // separate_uv_delta_q
        w.push_bit(0);
        let sh = parse_sh(&w.into_bytes());
        assert_eq!(sh.seq_profile, 1);
        assert!(!sh.monochrome);
        assert_eq!((sh.chroma_subsampling_x, sh.chroma_subsampling_y), (0, 0));
        assert_eq!(sh.chroma_sample_position, 0);
    }

    /// monochrome は subsampling を 4:2:0 相当に代入し chroma_sample_position は 0
    #[test]
    fn monochrome() {
        let mut w = BitWriter::new();
        w.push_bits(0, 3);
        w.push_bit(1);
        w.push_bit(1);
        w.push_bits(0, 5);
        w.push_bits(15, 4);
        w.push_bits(15, 4);
        w.push_bits(15, 16);
        w.push_bits(15, 16);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0); // high_bitdepth
        w.push_bit(1); // mono_chrome
        w.push_bit(0); // color_description_present_flag
        w.push_bit(1); // color_range
        // mono はここで return。separate_uv / chroma_sample_position は無い
        w.push_bit(0); // film_grain
        let sh = parse_sh(&w.into_bytes());
        assert!(sh.monochrome);
        assert_eq!((sh.chroma_subsampling_x, sh.chroma_subsampling_y), (1, 1));
        assert_eq!(sh.chroma_sample_position, 0);
    }

    /// identity RGB (BT.709 + sRGB + MC_IDENTITY) は color_range を符号化せず 4:4:4 を代入する
    #[test]
    fn identity_rgb_444() {
        let mut w = BitWriter::new();
        w.push_bits(0, 3); // seq_profile
        w.push_bit(1); // still_picture
        w.push_bit(1); // reduced_still_picture_header
        w.push_bits(0, 5); // seq_level_idx[0]
        w.push_bits(15, 4);
        w.push_bits(15, 4);
        w.push_bits(15, 16);
        w.push_bits(15, 16);
        w.push_bit(0); // use_128x128_superblock
        w.push_bit(0); // enable_filter_intra
        w.push_bit(0); // enable_intra_edge_filter
        w.push_bit(0); // enable_superres
        w.push_bit(0); // enable_cdef
        w.push_bit(0); // enable_restoration
        w.push_bit(0); // high_bitdepth
        w.push_bit(0); // mono_chrome
        w.push_bit(1); // color_description_present_flag
        w.push_bits(1, 8); // color_primaries = CP_BT_709
        w.push_bits(13, 8); // transfer_characteristics = TC_SRGB
        w.push_bits(0, 8); // matrix_coefficients = MC_IDENTITY
        // identity は color_range を符号化しない。separate_uv / film_grain を 1 にして、
        // 誤って color_range を読む実装だと (1,1) になることを固定する
        w.push_bit(1); // separate_uv_delta_q
        w.push_bit(1); // film_grain_params_present
        let sh = parse_sh(&w.into_bytes());
        assert_eq!((sh.chroma_subsampling_x, sh.chroma_subsampling_y), (0, 0));
        assert_eq!(sh.chroma_sample_position, 0);
    }

    /// profile 2 かつ BitDepth != 12 は構文上 4:2:2 を代入し sx / sy を読まない
    #[test]
    fn profile2_non_12bit_422() {
        let mut w = BitWriter::new();
        w.push_bits(2, 3); // seq_profile
        w.push_bit(1);
        w.push_bit(1);
        w.push_bits(0, 5);
        w.push_bits(15, 4);
        w.push_bits(15, 4);
        w.push_bits(15, 16);
        w.push_bits(15, 16);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0);
        w.push_bit(0); // high_bitdepth = 0 (8-bit、BitDepth != 12)
        w.push_bit(0); // mono_chrome
        w.push_bit(0); // color_description_present_flag
        w.push_bit(0); // color_range
        // sx / sy は書かない。separate_uv / film_grain を 1 にして、
        // 誤って sx / sy を読む実装だと (1,1) になることを固定する
        w.push_bit(1); // separate_uv_delta_q
        w.push_bit(1); // film_grain_params_present
        let sh = parse_sh(&w.into_bytes());
        assert_eq!(sh.seq_profile, 2);
        assert_eq!((sh.chroma_subsampling_x, sh.chroma_subsampling_y), (1, 0));
        assert_eq!(sh.chroma_sample_position, 0);
    }

    /// timing_info (uvlc 込み) / decoder_model_info / initial_display_delay /
    /// frame_id_numbers_present_flag を立てた合法 SH を解析できる
    #[test]
    fn timing_decoder_model_frame_id() {
        let mut w = BitWriter::new();
        w.push_bits(0, 3); // seq_profile
        w.push_bit(0); // still_picture
        w.push_bit(0); // reduced_still_picture_header
        w.push_bit(1); // timing_info_present_flag
        w.push_bits(1, 32); // num_units_in_display_tick
        w.push_bits(30, 32); // time_scale
        w.push_bit(1); // equal_picture_interval
        w.push_bit(1); // num_ticks_per_picture_minus_1 = uvlc(0)
        w.push_bit(1); // decoder_model_info_present_flag
        w.push_bits(4, 5); // buffer_delay_length_minus_1
        w.push_bits(1, 32); // num_units_in_decoding_tick
        w.push_bits(0, 5); // buffer_removal_time_length_minus_1
        w.push_bits(0, 5); // frame_presentation_time_length_minus_1
        w.push_bit(1); // initial_display_delay_present_flag
        w.push_bits(0, 5); // operating_points_cnt_minus_1 = 0 (1 個)
        w.push_bits(0, 12); // operating_point_idc[0]
        w.push_bits(8, 5); // seq_level_idx[0] (>7 なので tier ビットあり)
        w.push_bit(1); // seq_tier[0]
        w.push_bit(1); // decoder_model_present_for_this_op[0]
        w.push_bits(10, 5); // decoder_buffer_delay[0] (n = 4 + 1 = 5)
        w.push_bits(20, 5); // encoder_buffer_delay[0]
        w.push_bit(0); // low_delay_mode_flag[0]
        w.push_bit(1); // initial_display_delay_present_for_this_op[0]
        w.push_bits(3, 4); // initial_display_delay_minus_1[0]
        w.push_bits(15, 4);
        w.push_bits(15, 4);
        w.push_bits(319, 16);
        w.push_bits(239, 16);
        w.push_bit(1); // frame_id_numbers_present_flag
        w.push_bits(0, 4); // delta_frame_id_length_minus_2
        w.push_bits(0, 3); // additional_frame_id_length_minus_1
        w.push_bit(0); // use_128x128_superblock
        w.push_bit(0); // enable_filter_intra
        w.push_bit(0); // enable_intra_edge_filter
        w.push_bit(0); // enable_interintra_compound
        w.push_bit(0); // enable_masked_compound
        w.push_bit(0); // enable_warped_motion
        w.push_bit(0); // enable_dual_filter
        w.push_bit(0); // enable_order_hint
        w.push_bit(1); // seq_choose_screen_content_tools
        w.push_bit(1); // seq_choose_integer_mv
        w.push_bit(0); // enable_superres
        w.push_bit(0); // enable_cdef
        w.push_bit(0); // enable_restoration
        w.push_bit(0); // high_bitdepth
        w.push_bit(0); // mono_chrome
        w.push_bit(0); // color_description_present_flag
        w.push_bit(0); // color_range
        w.push_bits(0, 2); // chroma_sample_position
        w.push_bit(0); // separate_uv_delta_q
        w.push_bit(0); // film_grain_params_present
        let sh = parse_sh(&w.into_bytes());
        assert_eq!(sh.seq_level_idx_0, 8);
        assert_eq!(sh.seq_tier_0, 1);
        assert_eq!(sh.max_frame_width, 320);
        assert_eq!(sh.max_frame_height, 240);
        assert!(!sh.reduced_still_picture_header);
        assert_eq!((sh.chroma_subsampling_x, sh.chroma_subsampling_y), (1, 1));
    }
}

mod frame_prefix {
    use super::*;

    /// reduced still picture は Key / show_frame=1 を代入する
    #[test]
    fn reduced_still_picture() {
        let sh = parse_sh(&reduced_still_sequence_header(16, 16));
        let prefix = parse_frame_header_prefix(&[], &sh).expect("reduced はビットを読まない");
        assert_eq!(
            prefix,
            Av1FrameHeaderPrefix::NewFrame {
                frame_type: Av1FrameType::Key,
                show_frame: true,
            }
        );
        assert!(prefix.is_rap());
    }

    /// 通常ヘッダーで Key / show_frame=1 を読む
    #[test]
    fn key_show_frame() {
        let sh = parse_sh(&two_operating_point_sequence_header(16, 16));
        let mut w = BitWriter::new();
        w.push_bit(0); // show_existing_frame
        w.push_bits(0, 2); // KEY_FRAME
        w.push_bit(1); // show_frame
        let prefix = parse_frame_header_prefix(&w.into_bytes(), &sh).expect("通常ヘッダーの先頭部");
        assert_eq!(
            prefix,
            Av1FrameHeaderPrefix::NewFrame {
                frame_type: Av1FrameType::Key,
                show_frame: true,
            }
        );
        assert!(prefix.is_rap());
    }

    /// show_existing_frame=1 は RAP にならない
    #[test]
    fn show_existing_frame() {
        let sh = parse_sh(&two_operating_point_sequence_header(16, 16));
        let mut w = BitWriter::new();
        w.push_bit(1);
        let prefix =
            parse_frame_header_prefix(&w.into_bytes(), &sh).expect("show_existing は早期 return");
        assert_eq!(prefix, Av1FrameHeaderPrefix::ShowExistingFrame);
        assert!(!prefix.is_rap());
    }
}

mod build {
    use super::*;

    fn seq_320x240() -> Av1SequenceHeader {
        parse_sh(&reduced_still_sequence_header(320, 240))
    }

    /// Sequence Header から幅・高さと av1C 欄を埋める。
    /// level / tier が 0 以外の SH を使い、写し忘れを 0 初期値と区別する
    #[test]
    fn fills_visual_and_av1c() {
        let payload = two_operating_point_sequence_header(320, 240);
        let seq = parse_sh(&payload);
        let config_obus = wrap_obu(OBU_SEQUENCE_HEADER, &payload, true);
        let box_ = build_av01_box(
            &seq,
            &config_obus,
            &Av1SampleEntryConfig {
                initial_presentation_delay_minus_one: None,
            },
        )
        .expect("一致する SH は構築できる");
        assert_eq!(box_.visual.width, 320);
        assert_eq!(box_.visual.height, 240);
        assert_eq!(
            box_.visual.data_reference_index,
            VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX
        );
        assert_eq!(
            box_.visual.compressorname,
            VisualSampleEntryFields::NULL_COMPRESSORNAME
        );
        assert_eq!(box_.av1c_box.seq_profile.get(), 0);
        assert_eq!(box_.av1c_box.seq_level_idx_0.get(), 8);
        assert_eq!(box_.av1c_box.seq_tier_0.get(), 1);
        assert_eq!(box_.av1c_box.high_bitdepth.get(), 0);
        assert_eq!(box_.av1c_box.twelve_bit.get(), 0);
        assert_eq!(box_.av1c_box.monochrome.get(), 0);
        assert_eq!(box_.av1c_box.chroma_subsampling_x.get(), 1);
        assert_eq!(box_.av1c_box.chroma_subsampling_y.get(), 1);
        assert_eq!(box_.av1c_box.chroma_sample_position.get(), 0);
        assert!(box_.av1c_box.initial_presentation_delay_minus_one.is_none());
        assert_eq!(box_.av1c_box.config_obus, config_obus);
        assert!(box_.unknown_boxes.is_empty());
    }

    /// 空の configOBUs でもレコード欄は seq から埋まる
    #[test]
    fn empty_config_obus() {
        let seq = seq_320x240();
        let box_ = build_av01_box(
            &seq,
            &[],
            &Av1SampleEntryConfig {
                initial_presentation_delay_minus_one: Some(3),
            },
        )
        .expect("空 configOBUs は許容");
        assert_eq!(box_.visual.width, 320);
        assert_eq!(
            box_.av1c_box.initial_presentation_delay_minus_one,
            Some(Uint::new(3))
        );
        assert!(box_.av1c_box.config_obus.is_empty());
    }

    /// Sequence Header が先頭以外なら拒否する
    #[test]
    fn sequence_header_not_first() {
        let seq = seq_320x240();
        let mut config_obus = wrap_obu(OBU_METADATA, &[0x01], true);
        config_obus.extend(wrap_obu(
            OBU_SEQUENCE_HEADER,
            &reduced_still_sequence_header(320, 240),
            true,
        ));
        let err = build_av01_box(
            &seq,
            &config_obus,
            &Av1SampleEntryConfig {
                initial_presentation_delay_minus_one: None,
            },
        )
        .expect_err("SH は先頭でなければならない");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// Sequence Header が 2 個なら拒否する
    #[test]
    fn two_sequence_headers() {
        let seq = seq_320x240();
        let payload = reduced_still_sequence_header(320, 240);
        let mut config_obus = wrap_obu(OBU_SEQUENCE_HEADER, &payload, true);
        config_obus.extend(wrap_obu(OBU_SEQUENCE_HEADER, &payload, true));
        let err = build_av01_box(
            &seq,
            &config_obus,
            &Av1SampleEntryConfig {
                initial_presentation_delay_minus_one: None,
            },
        )
        .expect_err("SH は高々 1");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// configOBUs 内 SH と引数 seq が一致しない
    #[test]
    fn mismatched_sequence_header() {
        let seq = seq_320x240();
        let config_obus = wrap_obu(
            OBU_SEQUENCE_HEADER,
            &reduced_still_sequence_header(16, 16),
            true,
        );
        let err = build_av01_box(
            &seq,
            &config_obus,
            &Av1SampleEntryConfig {
                initial_presentation_delay_minus_one: None,
            },
        )
        .expect_err("寸法不一致");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// av1C レコード欄と寸法が同じでも `reduced_still_picture_header` が違えば拒否する
    #[test]
    fn mismatched_reduced_still_picture_header() {
        let seq = parse_sh(&reduced_still_sequence_header(320, 240));
        let other = parse_sh(&still_picture_non_reduced_sequence_header(320, 240));
        assert!(seq.reduced_still_picture_header);
        assert!(!other.reduced_still_picture_header);
        assert_eq!(seq.seq_profile, other.seq_profile);
        assert_eq!(
            seq.operating_points_cnt_minus_1,
            other.operating_points_cnt_minus_1
        );
        assert_eq!(seq.operating_point_idc_0, other.operating_point_idc_0);
        assert_eq!(seq.seq_level_idx_0, other.seq_level_idx_0);
        assert_eq!(seq.seq_tier_0, other.seq_tier_0);
        assert_eq!(seq.high_bitdepth, other.high_bitdepth);
        assert_eq!(seq.twelve_bit, other.twelve_bit);
        assert_eq!(seq.monochrome, other.monochrome);
        assert_eq!(seq.chroma_subsampling_x, other.chroma_subsampling_x);
        assert_eq!(seq.chroma_subsampling_y, other.chroma_subsampling_y);
        assert_eq!(seq.chroma_sample_position, other.chroma_sample_position);
        assert_eq!(seq.max_frame_width, other.max_frame_width);
        assert_eq!(seq.max_frame_height, other.max_frame_height);
        let config_obus = wrap_obu(
            OBU_SEQUENCE_HEADER,
            &still_picture_non_reduced_sequence_header(320, 240),
            true,
        );
        let err = build_av01_box(
            &seq,
            &config_obus,
            &Av1SampleEntryConfig {
                initial_presentation_delay_minus_one: None,
            },
        )
        .expect_err("reduced_still だけ違う SH は不一致");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
        assert_eq!(
            err.reason,
            "AV1 configOBUs Sequence Header does not match the provided Av1SequenceHeader"
        );
    }

    /// 65536 は Visual Sample Entry に入らない
    #[test]
    fn dimension_overflow() {
        let mut seq = seq_320x240();
        seq.max_frame_width = 65536;
        let err = build_av01_box(
            &seq,
            &[],
            &Av1SampleEntryConfig {
                initial_presentation_delay_minus_one: None,
            },
        )
        .expect_err("65536 は u16 に入らない");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// 構築結果をエンコードしてデコードしても同じ。
    /// level / tier / chroma が 0 以外の SH を使い、av1C のビット詰めも固定する
    #[test]
    fn encode_decode_roundtrip() {
        let payload = two_operating_point_sequence_header(320, 240);
        let seq = parse_sh(&payload);
        let config_obus = wrap_obu(OBU_SEQUENCE_HEADER, &payload, true);
        let box_ = build_av01_box(
            &seq,
            &config_obus,
            &Av1SampleEntryConfig {
                initial_presentation_delay_minus_one: None,
            },
        )
        .expect("構築できる");
        let encoded = box_.encode_to_vec().expect("encode");
        let (decoded, _) = shiguredo_mp4::boxes::Av01Box::decode(&encoded).expect("decode");
        assert_eq!(decoded, box_);
    }
}

/// 既存の AV1 MP4 fixture から configOBUs を解析し、サンプルを stsz で分割して
/// それぞれを Binding §2.4 の Temporal Unit として解析できる
#[test]
fn fixture_black_av1_video_config_obus() {
    let input_bytes = include_bytes!("testdata/black-av1-video.mp4");
    let (file, _) = Mp4File::decode(&input_bytes[..]).expect("fixture をデコードできる");

    let mdat_payload = file
        .boxes
        .iter()
        .find_map(|root| match root {
            RootBox::Mdat(mdat) => Some(mdat.payload.as_slice()),
            _ => None,
        })
        .expect("fixture に mdat がある");

    let mut found = false;
    for root in &file.boxes {
        let RootBox::Moov(moov) = root else {
            continue;
        };
        for trak in &moov.trak_boxes {
            let stbl = &trak.mdia_box.minf_box.stbl_box;
            for entry in &stbl.stsd_box.entries {
                let SampleEntry::Av01(av01) = entry else {
                    continue;
                };
                found = true;
                let obus = parse_obus(&av01.av1c_box.config_obus, Av1ObuParseContext::ConfigObus)
                    .expect("実データの configOBUs を解析できる");
                let sh_obu = obus
                    .iter()
                    .find(|o| o.obu_type == Av1ObuType::SequenceHeader)
                    .expect("実データの configOBUs に Sequence Header がある");
                let sh = parse_sequence_header(sh_obu.payload)
                    .expect("実データの Sequence Header を解析できる");
                assert_eq!(sh.max_frame_width, u32::from(av01.visual.width));
                assert_eq!(sh.max_frame_height, u32::from(av01.visual.height));
                assert_eq!(sh.seq_profile, av01.av1c_box.seq_profile.get());

                // stsz のサンプルサイズで mdat を切り、各サンプルを Binding §2.4 の
                // Temporal Unit (Sample コンテキスト) として解析する
                let chunk_count = match &stbl.stco_or_co64_box {
                    Either::A(stco) => stco.chunk_offsets.len(),
                    Either::B(co64) => co64.chunk_offsets.len(),
                };
                assert_eq!(chunk_count, 1, "fixture は単一チャンクでサンプルが連続する");
                let StszBox::Variable { entry_sizes } = &stbl.stsz_box else {
                    panic!("fixture の stsz はサンプルごとにサイズが異なる");
                };
                assert_eq!(entry_sizes.len(), 25, "fixture のサンプル数は 25");
                let total: u32 = entry_sizes.iter().sum();
                assert_eq!(
                    usize::try_from(total).expect("u32 は usize に収まる"),
                    mdat_payload.len(),
                    "stsz の合計は mdat payload 全体を覆う"
                );
                let mut offset = 0usize;
                for (index, size) in entry_sizes.iter().enumerate() {
                    let end = offset + usize::try_from(*size).expect("u32 は usize に収まる");
                    let sample = &mdat_payload[offset..end];
                    let sample_obus = parse_obus(sample, Av1ObuParseContext::Sample)
                        .expect("実データのサンプルは Temporal Unit として解析できる");
                    assert!(
                        !sample_obus.is_empty(),
                        "サンプル {index} は 1 個以上の OBU を含む"
                    );
                    offset = end;
                }
            }
        }
    }
    assert!(found, "fixture に Av01 サンプルエントリーがある");
}

/// `black-av1-video.mp4` から抽出した configOBUs 単体を解析し `Av01Box` を構築できる
#[test]
fn fixture_extracted_config_obus() {
    let config_obus = include_bytes!("testdata/black-av1-config-obus.bin");
    let obus = parse_obus(config_obus, Av1ObuParseContext::ConfigObus)
        .expect("抽出した configOBUs を解析できる");
    assert_eq!(obus[0].obu_type, Av1ObuType::SequenceHeader);
    let seq = parse_sequence_header(obus[0].payload).expect("抽出 SH を解析できる");
    let box_ = build_av01_box(
        &seq,
        config_obus,
        &Av1SampleEntryConfig {
            initial_presentation_delay_minus_one: None,
        },
    )
    .expect("抽出 SH から av01 を構築できる");
    assert_eq!(box_.av1c_box.seq_profile.get(), seq.seq_profile);
    assert_eq!(u32::from(box_.visual.width), seq.max_frame_width);
}
