//! `shiguredo_mp4::bitstream::h264` の決定的テスト
//!
//! 手動構築した Annex B / length-prefixed のバイト列と SPS のビット列に対して
//! パーサーの受理・拒否条件を固定する。実エンコーダー出力による fixture テストは
//! `tests/testdata/h264-sps-pps-annexb.bin` を用いた別テストで補う。

use shiguredo_mp4::{
    Decode, Encode, ErrorKind, Uint,
    bitstream::h264::{
        H264SampleEntryConfig, build_avc1_box, build_avc1_box_from_annexb, collect_nal_units,
        parse_annexb_nal_units, parse_length_prefixed_nal_units, parse_sps,
    },
    boxes::{Avc1Box, VisualSampleEntryFields},
};

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
        // codeNum + 1 を 2 進数で表したときのビット数を leadingZeroBits から導出する
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

/// `seq_parameter_set_rbsp` の構築パラメタ
///
/// バリデーションはせず、渡した値をそのままビット位置に詰める
#[derive(Debug, Clone, Copy)]
struct SpsParams {
    profile_idc: u8,
    constraint_set_flags: u8,
    level_idc: u8,
    chroma_format_idc: u8,
    separate_colour_plane_flag: bool,
    bit_depth_luma_minus8: u8,
    bit_depth_chroma_minus8: u8,
    seq_scaling_matrix_present_flag: bool,
    /// scaling list のうち先頭から何個を present にするか (0 = 全て absent)
    scaling_list_present_count: usize,
    /// scaling list の先頭要素の delta_scale (0 以外で nextScale の挙動を変える)
    scaling_list_first_delta: i64,
    pic_order_cnt_type: u8,
    /// pic_order_cnt_type == 1 のときの offset_for_non_ref_pic
    offset_for_non_ref_pic: i64,
    /// pic_order_cnt_type == 1 のときの num_ref_frames_in_pic_order_cnt_cycle
    num_ref_frames_in_pic_order_cnt_cycle: u32,
    pic_width_in_mbs_minus1: u32,
    pic_height_in_map_units_minus1: u32,
    frame_mbs_only_flag: bool,
    frame_cropping_flag: bool,
    frame_crop_left_offset: u32,
    frame_crop_right_offset: u32,
    frame_crop_top_offset: u32,
    frame_crop_bottom_offset: u32,
}

impl SpsParams {
    /// 有効値で全フィールドを初期化した最小構成 (Baseline / 320x240)
    fn valid() -> Self {
        Self {
            profile_idc: 66,
            constraint_set_flags: 0x00,
            level_idc: 30,
            chroma_format_idc: 1,
            separate_colour_plane_flag: false,
            bit_depth_luma_minus8: 0,
            bit_depth_chroma_minus8: 0,
            seq_scaling_matrix_present_flag: false,
            scaling_list_present_count: usize::MAX,
            scaling_list_first_delta: 0,
            pic_order_cnt_type: 0,
            offset_for_non_ref_pic: 0,
            num_ref_frames_in_pic_order_cnt_cycle: 0,
            pic_width_in_mbs_minus1: 19,
            pic_height_in_map_units_minus1: 14,
            frame_mbs_only_flag: true,
            frame_cropping_flag: false,
            frame_crop_left_offset: 0,
            frame_crop_right_offset: 0,
            frame_crop_top_offset: 0,
            frame_crop_bottom_offset: 0,
        }
    }
}

/// NAL ヘッダー (type 7) 付きの SPS EBSP を組み立てる
fn build_sps(p: &SpsParams) -> Vec<u8> {
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
        w.push_bit(u8::from(p.seq_scaling_matrix_present_flag));
        if p.seq_scaling_matrix_present_flag {
            // scaling list は spec どおり nextScale が 0 になるまでだけ書き込む
            let list_count = if p.chroma_format_idc != 3 { 8 } else { 12 };
            for i in 0..list_count {
                let present = i < p.scaling_list_present_count.min(list_count);
                w.push_bit(u8::from(present)); // seq_scaling_list_present_flag
                if present {
                    let size = if i < 6 { 16 } else { 64 };
                    let mut last_scale: i64 = 8;
                    let mut next_scale: i64 = 8;
                    for j in 0..size {
                        if next_scale != 0 {
                            let delta = if j == 0 {
                                p.scaling_list_first_delta
                            } else {
                                0
                            };
                            w.push_se(delta);
                            next_scale = (last_scale + delta + 256) % 256;
                        }
                        last_scale = if next_scale == 0 {
                            last_scale
                        } else {
                            next_scale
                        };
                    }
                }
            }
        }
    }
    w.push_ue(0); // log2_max_frame_num_minus4
    w.push_ue(u32::from(p.pic_order_cnt_type));
    if p.pic_order_cnt_type == 0 {
        w.push_ue(0); // log2_max_pic_order_cnt_lsb_minus4
    } else if p.pic_order_cnt_type == 1 {
        w.push_bit(0); // delta_pic_order_always_zero_flag
        w.push_se(p.offset_for_non_ref_pic);
        w.push_se(0); // offset_for_top_to_bottom_field
        w.push_ue(p.num_ref_frames_in_pic_order_cnt_cycle);
        for _ in 0..p.num_ref_frames_in_pic_order_cnt_cycle {
            w.push_se(1); // offset_for_ref_frame (正値で se の奇数経路を踏む)
        }
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
    w.push_bit(u8::from(p.frame_cropping_flag));
    if p.frame_cropping_flag {
        w.push_ue(p.frame_crop_left_offset);
        w.push_ue(p.frame_crop_right_offset);
        w.push_ue(p.frame_crop_top_offset);
        w.push_ue(p.frame_crop_bottom_offset);
    }
    w.into_bytes()
}

/// 3 バイト開始コードで NAL を連結した Annex B バイト列を作る
fn annexb_with_3(nals: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for nal in nals {
        out.extend_from_slice(&[0x00, 0x00, 0x01]);
        out.extend_from_slice(nal);
    }
    out
}

/// 4 バイト開始コードで NAL を連結した Annex B バイト列を作る
fn annexb_with_4(nals: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for nal in nals {
        out.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        out.extend_from_slice(nal);
    }
    out
}

/// 大端序の長さフィールドを付けて length-prefixed バイト列を作る
fn length_prefixed(nals: &[&[u8]], length_size: u8) -> Vec<u8> {
    let mut out = Vec::new();
    for nal in nals {
        match length_size {
            1 => out.push(nal.len() as u8),
            2 => out.extend_from_slice(&(nal.len() as u16).to_be_bytes()),
            4 => out.extend_from_slice(&(nal.len() as u32).to_be_bytes()),
            _ => unreachable!(),
        }
        out.extend_from_slice(nal);
    }
    out
}

/// 有効な PPS (type 8 の非空 NAL)
fn valid_pps() -> Vec<u8> {
    vec![0x68, 0xEB, 0xE3, 0xCB, 0x22, 0xC0]
}

fn default_config() -> H264SampleEntryConfig {
    H264SampleEntryConfig { length_size: 4 }
}

// ===== parse_annexb_nal_units: 受理系 =====

/// 4 バイト開始コードの単一 NAL を解析できる
#[test]
fn parse_annexb_single_nal_with_4byte_start_code() {
    let nal = [0x65, 0x88, 0x84];
    let input = annexb_with_4(&[&nal]);
    let nals = parse_annexb_nal_units(&input).expect("4 バイト開始コードの NAL は解析成功する");
    assert_eq!(nals.len(), 1);
    assert_eq!(nals[0].nal_unit_type, 5);
    assert_eq!(nals[0].data, nal);
}

/// 3 バイト開始コードの単一 NAL を解析できる
#[test]
fn parse_annexb_single_nal_with_3byte_start_code() {
    let nal = [0x41, 0x9A];
    let input = annexb_with_3(&[&nal]);
    let nals = parse_annexb_nal_units(&input).expect("3 バイト開始コードの NAL は解析成功する");
    assert_eq!(nals.len(), 1);
    assert_eq!(nals[0].nal_unit_type, 1);
    assert_eq!(nals[0].data, nal);
}

/// 3 バイトと 4 バイトの開始コードが混在しても NAL 境界を正しく走査できる
#[test]
fn parse_annexb_mixed_start_code_lengths() {
    let nal1 = [0x67, 0x42, 0xC0];
    let nal2 = [0x65, 0x88];
    let nal3 = [0x41, 0x9A];
    let mut input = Vec::new();
    input.extend_from_slice(&[0x00, 0x00, 0x01]);
    input.extend_from_slice(&nal1);
    input.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
    input.extend_from_slice(&nal2);
    input.extend_from_slice(&[0x00, 0x00, 0x01]);
    input.extend_from_slice(&nal3);
    let nals = parse_annexb_nal_units(&input).expect("混在開始コードは解析成功する");
    assert_eq!(nals.len(), 3);
    assert_eq!(nals[0].data, nal1);
    assert_eq!(nals[1].data, nal2);
    assert_eq!(nals[2].data, nal3);
}

/// 4 バイト開始コードを 3 バイト開始コード + 先行ゼロに誤分割しない
#[test]
fn parse_annexb_4byte_start_code_not_split() {
    let nal = [0x65, 0x88];
    let input = annexb_with_4(&[&nal]);
    let nals =
        parse_annexb_nal_units(&input).expect("4 バイト開始コードは 1 個の NAL に解釈される");
    assert_eq!(nals.len(), 1);
    assert_eq!(nals[0].data, nal);
}

/// 空入力は NAL ユニット 0 個の成功 (開始コード欠落とは区別する)
#[test]
fn parse_annexb_empty_input_is_success() {
    let nals = parse_annexb_nal_units(&[]).expect("空入力は 0 個の成功");
    assert!(nals.is_empty());
}

/// 先頭のゼロ詰め (leading_zero_8bits) が NAL 本体に混ざらない
#[test]
fn parse_annexb_strips_leading_zero_padding() {
    let nal = [0x65, 0x88];
    let mut input = vec![0x00, 0x00];
    input.extend_from_slice(&annexb_with_3(&[&nal]));
    let nals = parse_annexb_nal_units(&input).expect("先頭ゼロ詰めは解析成功する");
    assert_eq!(nals.len(), 1);
    assert_eq!(nals[0].data, nal);
}

/// 末尾のゼロ詰め (trailing_zero_8bits) が NAL 本体に混ざらない
#[test]
fn parse_annexb_strips_trailing_zero_padding() {
    let nal = [0x65, 0x88];
    let mut input = annexb_with_3(&[&nal]);
    input.extend_from_slice(&[0x00, 0x00, 0x00]);
    let nals = parse_annexb_nal_units(&input).expect("末尾ゼロ詰めは解析成功する");
    assert_eq!(nals.len(), 1);
    assert_eq!(nals[0].data, nal);
}

/// NAL 間のゼロ詰めが直前の NAL 本体に混ざらない
///
/// 仕様 (ITU-T H.264 Annex B B.2) では NAL 本体は後続のバイトアラインされた
/// `0x000000` / `0x000001` の直前まで。`65 88 00 00 00` は次の 3 バイト開始
/// コード (`00 00 01`) の前置きゼロなので、1 個目は `[65, 88]` になる
#[test]
fn parse_annexb_strips_trailing_zeros_between_nals() {
    let input = [
        0x00, 0x00, 0x01, 0x65, 0x88, 0x00, 0x00, 0x00, 0x00, 0x01, 0x41, 0x9A,
    ];
    let nals = parse_annexb_nal_units(&input).expect("NAL 間ゼロ詰めは解析成功する");
    assert_eq!(nals.len(), 2);
    assert_eq!(nals[0].data, [0x65, 0x88]);
    assert_eq!(nals[1].data, [0x41, 0x9A]);
}

/// NAL 間の 1 バイトだけのゼロ詰めも直前の NAL 本体に混ざらない
///
/// 4 バイト開始コード (`00 00 00 01`) の前置きゼロ 1 バイトを NAL 本体に
/// 含めない
#[test]
fn parse_annexb_single_zero_before_4byte_start_code() {
    let input = [0x00, 0x00, 0x01, 0x65, 0x00, 0x00, 0x00, 0x01, 0x41];
    let nals = parse_annexb_nal_units(&input).expect("前置きゼロ 1 バイトは解析成功する");
    assert_eq!(nals.len(), 2);
    assert_eq!(nals[0].data, [0x65]);
    assert_eq!(nals[1].data, [0x41]);
}

/// NAL 間の詰め物が全てゼロだと空 NAL として Error
#[test]
fn parse_annexb_rejects_all_zero_span_between_start_codes() {
    // 1 個目の開始コードの後にゼロ 1 バイト、続けて 4 バイト開始コード。
    // ゼロ除去後は空になるため Error (黙って読み飛ばさない)
    let input = [0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x41];
    let err = parse_annexb_nal_units(&input).expect_err("ゼロのみの NAL 間は空 NAL のため Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 予約・未指定の nal_unit_type はエラーにせず不透明な NAL として通す
#[test]
fn parse_annexb_reserved_nal_unit_type_is_opaque() {
    // nal_unit_type = 0 (未指定)
    let nal = [0x00, 0x01, 0x02];
    let input = annexb_with_3(&[&nal]);
    let nals = parse_annexb_nal_units(&input).expect("未指定 nal_unit_type は通す");
    assert_eq!(nals[0].nal_unit_type, 0);
    assert_eq!(nals[0].data, nal);
}

// ===== parse_annexb_nal_units: 拒否系 =====

/// 非空入力に開始コードが 1 つも無い場合は Error
#[test]
fn parse_annexb_rejects_no_start_code() {
    let input = [0x67, 0x42, 0xC0];
    let err = parse_annexb_nal_units(&input).expect_err("開始コード無しは Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 開始コードの直後に別の開始コードが来る空 NAL は Error
#[test]
fn parse_annexb_rejects_empty_nal_between_start_codes() {
    let input = annexb_with_4(&[&[0x65, 0x88], &[]]);
    let err = parse_annexb_nal_units(&input).expect_err("空 NAL は Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 開始コードが連続する (間に本体が一切無い) 空 NAL は Error
#[test]
fn parse_annexb_rejects_consecutive_start_codes() {
    let input = [0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x65, 0x88];
    let err = parse_annexb_nal_units(&input).expect_err("連続する開始コードの空 NAL は Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 開始コードの直後に入力終端が来る空 NAL は Error
#[test]
fn parse_annexb_rejects_empty_nal_at_end() {
    let input = [0x00, 0x00, 0x00, 0x01];
    let err = parse_annexb_nal_units(&input).expect_err("末尾の空 NAL は Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 開始コードの直後にゼロ詰めだけが続いて終端する場合は空 NAL として Error
#[test]
fn parse_annexb_rejects_empty_nal_with_only_zero_padding() {
    let input = [0x00, 0x00, 0x00, 0x01, 0x00, 0x00];
    let err = parse_annexb_nal_units(&input).expect_err("ゼロ詰めだけの空 NAL は Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 最初の開始コードより前に非ゼロバイトがある場合は Error
///
/// 詰め物 (ゼロ) でも NAL 本体でもないデータを黙って捨てない
#[test]
fn parse_annexb_rejects_non_zero_leading_bytes() {
    let nal = [0x65, 0x88];
    let mut input = vec![0x41];
    input.extend_from_slice(&annexb_with_3(&[&nal]));
    let err = parse_annexb_nal_units(&input).expect_err("非ゼロの先行バイトは Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// forbidden_zero_bit が 1 の NAL は Error
#[test]
fn parse_annexb_rejects_forbidden_zero_bit() {
    // 0xE5 = 1110_0101: forbidden_zero_bit = 1
    let nal = [0xE5, 0x88];
    let input = annexb_with_3(&[&nal]);
    let err = parse_annexb_nal_units(&input).expect_err("forbidden_zero_bit=1 は Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

// ===== parse_length_prefixed_nal_units: 受理系 =====

/// 幅 1 / 2 / 4 の length-prefixed 列を解析できる
#[test]
fn parse_length_prefixed_widths_1_2_4() {
    let nal = [0x65, 0x88, 0x84];
    for length_size in [1u8, 2, 4] {
        let input = length_prefixed(&[&nal], length_size);
        let nals = parse_length_prefixed_nal_units(&input, length_size)
            .unwrap_or_else(|_| panic!("幅 {length_size} は解析成功する"));
        assert_eq!(nals.len(), 1);
        assert_eq!(nals[0].nal_unit_type, 5);
        assert_eq!(nals[0].data, nal);
    }
}

/// 複数 NAL を入力順で返す
#[test]
fn parse_length_prefixed_multiple_nals() {
    let nal1 = [0x67, 0x42, 0xC0];
    let nal2 = [0x65, 0x88];
    let input = length_prefixed(&[&nal1, &nal2], 4);
    let nals = parse_length_prefixed_nal_units(&input, 4).expect("複数 NAL は解析成功する");
    assert_eq!(nals.len(), 2);
    assert_eq!(nals[0].nal_unit_type, 7);
    assert_eq!(nals[0].data, nal1);
    assert_eq!(nals[1].nal_unit_type, 5);
    assert_eq!(nals[1].data, nal2);
}

/// 空入力は NAL ユニット 0 個の成功
#[test]
fn parse_length_prefixed_empty_input_is_success() {
    let nals = parse_length_prefixed_nal_units(&[], 4).expect("空入力は 0 個の成功");
    assert!(nals.is_empty());
}

// ===== parse_length_prefixed_nal_units: 拒否系 =====

/// 幅 3 (lengthSizeMinusOne == 2、reserved) は Error
///
/// length-prefixed を扱う公開 API は全て幅 3 を拒否する。各 API が幅 3 を
/// 受理する実装に差し替えた場合に入力は成功するよう、幅 3 で正当な入力を
/// 使う (幅 3 拒否契約だけが Error の理由になる)
#[test]
fn parse_length_prefixed_rejects_width_3() {
    // 幅 3 の長さフィールド (0x000003) と NAL 本体で正しい length-prefixed 入力
    let lp = [0x00, 0x00, 0x03, 0x65, 0x88, 0x84];
    // 幅 3 で変換できる正当な Annex B 入力
    let annexb = [0x00, 0x00, 0x01, 0x65, 0x88];
    for err in [
        parse_length_prefixed_nal_units(&lp, 3).expect_err("幅 3 は reserved のため Error"),
        shiguredo_mp4::bitstream::h264::annexb_to_length_prefixed(&annexb, 3)
            .expect_err("幅 3 は reserved のため Error"),
        shiguredo_mp4::bitstream::h264::length_prefixed_to_annexb(&lp, 3)
            .expect_err("幅 3 は reserved のため Error"),
    ] {
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// 長さフィールドが入力末尾を超える場合は Error
#[test]
fn parse_length_prefixed_rejects_length_field_exceeding_end() {
    // 幅 4 の長さフィールドを読むのに 3 バイトしか残っていない
    let input = [0x00, 0x00, 0x01];
    let err = parse_length_prefixed_nal_units(&input, 4).expect_err("長さフィールド超過は Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 宣言長が残バイトを超える (切り詰め) 場合は Error
#[test]
fn parse_length_prefixed_rejects_declared_length_exceeding_remaining() {
    // 宣言長 5 に対して実データが 2 バイトしか無い
    let mut input = Vec::new();
    input.extend_from_slice(&5u32.to_be_bytes());
    input.extend_from_slice(&[0x65, 0x88]);
    let err = parse_length_prefixed_nal_units(&input, 4)
        .expect_err("宣言長超過は Error (黙って打ち切らない)");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 宣言長が 0 の NAL は Error
#[test]
fn parse_length_prefixed_rejects_zero_length() {
    let input = [0x00, 0x00, 0x00, 0x00];
    let err = parse_length_prefixed_nal_units(&input, 4).expect_err("長さ 0 の NAL は Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// forbidden_zero_bit が 1 の NAL は Error
#[test]
fn parse_length_prefixed_rejects_forbidden_zero_bit() {
    let nal = [0xE5, 0x88];
    let input = length_prefixed(&[&nal], 1);
    let err =
        parse_length_prefixed_nal_units(&input, 1).expect_err("forbidden_zero_bit=1 は Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

// ===== 相互変換 =====

/// Annex B → length-prefixed が幅どおりの長さフィールドを付けて変換する
#[test]
fn annexb_to_length_prefixed_adds_length_fields() {
    let nal1 = [0x67, 0x42, 0xC0];
    let nal2 = [0x65, 0x88];
    let input = annexb_with_3(&[&nal1, &nal2]);
    let out = shiguredo_mp4::bitstream::h264::annexb_to_length_prefixed(&input, 2)
        .expect("Annex B → length-prefixed は成功する");
    let expected = length_prefixed(&[&nal1, &nal2], 2);
    assert_eq!(out, expected);
}

/// length-prefixed → Annex B が 4 バイト開始コードで変換する
#[test]
fn length_prefixed_to_annexb_adds_4byte_start_codes() {
    let nal1 = [0x67, 0x42, 0xC0];
    let nal2 = [0x65, 0x88];
    let input = length_prefixed(&[&nal1, &nal2], 1);
    let out = shiguredo_mp4::bitstream::h264::length_prefixed_to_annexb(&input, 1)
        .expect("length-prefixed → Annex B は成功する");
    let expected = annexb_with_4(&[&nal1, &nal2]);
    assert_eq!(out, expected);
}

/// Annex B → length-prefixed → Annex B がラウンドトリップする
#[test]
fn annexb_length_prefixed_annexb_roundtrip() {
    let nal1 = [0x67, 0x42, 0xC0];
    let nal2 = [0x65, 0x88];
    let original = annexb_with_4(&[&nal1, &nal2]);
    let lp = shiguredo_mp4::bitstream::h264::annexb_to_length_prefixed(&original, 4)
        .expect("Annex B → length-prefixed は成功する");
    let back = shiguredo_mp4::bitstream::h264::length_prefixed_to_annexb(&lp, 4)
        .expect("length-prefixed → Annex B は成功する");
    assert_eq!(back, original);
}

/// NAL 本体が長さフィールド幅に収まらない場合は Error (黙った切り詰めをしない)
#[test]
fn annexb_to_length_prefixed_rejects_nal_too_long_for_width_1() {
    // 幅 1 は最大 255 バイト。256 バイトの NAL は収まらない
    let nal = vec![0x65; 256];
    let input = annexb_with_3(&[&nal]);
    let err = shiguredo_mp4::bitstream::h264::annexb_to_length_prefixed(&input, 1)
        .expect_err("幅 1 に収まらない NAL は Error");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

// ===== collect_nal_units =====

/// 指定した nal_unit_type の NAL だけを入力順で集める
#[test]
fn collect_nal_units_filters_by_type() {
    let input = annexb_with_3(&[&[0x67, 0x42], &[0x68, 0xEB], &[0x65, 0x88], &[0x67, 0x4D]]);
    let nals = parse_annexb_nal_units(&input).expect("Annex B は解析成功する");
    let sps = collect_nal_units(nals.iter().copied(), 7);
    assert_eq!(sps.len(), 2);
    assert_eq!(sps[0], [0x67, 0x42]);
    assert_eq!(sps[1], [0x67, 0x4D]);
    let pps = collect_nal_units(nals.iter().copied(), 8);
    assert_eq!(pps.len(), 1);
    assert_eq!(pps[0], [0x68, 0xEB]);
    // 一致が無ければ空 Vec
    let sei = collect_nal_units(nals.iter().copied(), 6);
    assert!(sei.is_empty());
}

// ===== parse_sps: 受理系 =====

/// Baseline (66) は SPS 追加構文が無く、推論値 (4:2:0 / 8-bit) になる
#[test]
fn parse_sps_baseline_profile() {
    let sps = parse_sps(&build_sps(&SpsParams::valid())).expect("Baseline SPS は解析成功する");
    assert_eq!(sps.profile_idc, 66);
    assert_eq!(sps.constraint_set_flags, 0x00);
    assert_eq!(sps.level_idc, 30);
    assert_eq!(sps.chroma_format_idc, 1);
    assert_eq!(sps.bit_depth_luma_minus8, 0);
    assert_eq!(sps.bit_depth_chroma_minus8, 0);
    assert_eq!(sps.width, 320);
    assert_eq!(sps.height, 240);
}

/// Main (77) は SPS 追加構文が無く、推論値になる
#[test]
fn parse_sps_main_profile() {
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 77,
        ..SpsParams::valid()
    }))
    .expect("Main SPS は解析成功する");
    assert_eq!(sps.profile_idc, 77);
    assert_eq!(sps.chroma_format_idc, 1);
    assert_eq!(sps.bit_depth_luma_minus8, 0);
    assert_eq!(sps.bit_depth_chroma_minus8, 0);
}

/// High (100) は SPS 追加構文を読み、chroma / bit depth が反映される
#[test]
fn parse_sps_high_profile_with_chroma_syntax() {
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 1,
        ..SpsParams::valid()
    }))
    .expect("High SPS は解析成功する");
    assert_eq!(sps.profile_idc, 100);
    assert_eq!(sps.chroma_format_idc, 1);
    assert_eq!(sps.bit_depth_luma_minus8, 0);
    assert_eq!(sps.bit_depth_chroma_minus8, 0);
}

/// High 10 (110) は bit_depth_luma_minus8 = 2 (10-bit) が反映される
#[test]
fn parse_sps_high10_profile() {
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 110,
        chroma_format_idc: 1,
        bit_depth_luma_minus8: 2,
        bit_depth_chroma_minus8: 2,
        ..SpsParams::valid()
    }))
    .expect("High 10 SPS は解析成功する");
    assert_eq!(sps.profile_idc, 110);
    assert_eq!(sps.bit_depth_luma_minus8, 2);
    assert_eq!(sps.bit_depth_chroma_minus8, 2);
}

/// 4:4:4 (chroma_format_idc = 3) かつ separate_colour_plane_flag = 1 を解析できる
#[test]
fn parse_sps_chroma_444_separate_colour_plane() {
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 3,
        separate_colour_plane_flag: true,
        ..SpsParams::valid()
    }))
    .expect("4:4:4 separate colour plane の SPS は解析成功する");
    assert_eq!(sps.chroma_format_idc, 3);
    assert_eq!(sps.width, 320);
    assert_eq!(sps.height, 240);
}

/// seq_scaling_matrix_present_flag 配下の scaling list を読み飛ばして解析できる
#[test]
fn parse_sps_with_scaling_matrix() {
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 1,
        seq_scaling_matrix_present_flag: true,
        ..SpsParams::valid()
    }))
    .expect("scaling list 入りの SPS は解析成功する");
    assert_eq!(sps.profile_idc, 100);
    assert_eq!(sps.width, 320);
    assert_eq!(sps.height, 240);
}

/// 一部の scaling list が absent (present flag = 0) でも読み飛ばして解析できる
#[test]
fn parse_sps_with_partial_scaling_matrix() {
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 1,
        seq_scaling_matrix_present_flag: true,
        scaling_list_present_count: 1,
        ..SpsParams::valid()
    }))
    .expect("present flag 0 の scaling list 入り SPS は解析成功する");
    assert_eq!(sps.width, 320);
    assert_eq!(sps.height, 240);
}

/// scaling list の delta_scale で nextScale が 0 になると以降の要素が読まれない
///
/// 先頭の delta_scale = -8 で `nextScale = (8 + (-8) + 256) % 256 = 0` になり、
/// それ以降は se(v) が書かれない (ITU-T H.264 7.3.2.1.1)
#[test]
fn parse_sps_with_scaling_matrix_next_scale_zero() {
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 1,
        seq_scaling_matrix_present_flag: true,
        scaling_list_present_count: 1,
        scaling_list_first_delta: -8,
        ..SpsParams::valid()
    }))
    .expect("nextScale が 0 になる scaling list 入りの SPS は解析成功する");
    assert_eq!(sps.width, 320);
    assert_eq!(sps.height, 240);
}

/// 4:4:4 (chroma_format_idc = 3) では scaling list が 12 本になる
///
/// 7.3.2.1.1 の `i < ((chroma_format_idc != 3) ? 8 : 12)` どおり、chroma 3 では
/// 12 本 (i < 6 は 16 要素、それ以外は 64 要素) を読み飛ばして解析できる。
/// 読み飛ばし数が 8 本に退行すると後続フィールドがずれて 320x240 にならない
#[test]
fn parse_sps_chroma_444_with_scaling_matrix() {
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 3,
        seq_scaling_matrix_present_flag: true,
        ..SpsParams::valid()
    }))
    .expect("chroma 3 で scaling list 12 本入りの SPS は解析成功する");
    assert_eq!(sps.chroma_format_idc, 3);
    assert_eq!(sps.width, 320);
    assert_eq!(sps.height, 240);
}

/// 4:4:4 で一部の scaling list が absent (present flag = 0) でも解析できる
#[test]
fn parse_sps_chroma_444_with_partial_scaling_matrix() {
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 3,
        seq_scaling_matrix_present_flag: true,
        scaling_list_present_count: 1,
        ..SpsParams::valid()
    }))
    .expect("chroma 3 で present flag 0 の scaling list 入り SPS は解析成功する");
    assert_eq!(sps.width, 320);
    assert_eq!(sps.height, 240);
}

/// pic_order_cnt_type = 1 の追加構文 (offset 列を含む) を読み飛ばして解析できる
#[test]
fn parse_sps_pic_order_cnt_type_1() {
    let sps = parse_sps(&build_sps(&SpsParams {
        pic_order_cnt_type: 1,
        offset_for_non_ref_pic: 1,
        num_ref_frames_in_pic_order_cnt_cycle: 2,
        ..SpsParams::valid()
    }))
    .expect("pic_order_cnt_type=1 の SPS は解析成功する");
    assert_eq!(sps.width, 320);
    assert_eq!(sps.height, 240);
}

/// クロップ無しで符号化寸法がそのまま出る
#[test]
fn parse_sps_no_crop_dimensions() {
    let sps = parse_sps(&build_sps(&SpsParams {
        pic_width_in_mbs_minus1: 119,
        pic_height_in_map_units_minus1: 67,
        ..SpsParams::valid()
    }))
    .expect("クロップ無し SPS は解析成功する");
    assert_eq!(sps.width, 1920);
    assert_eq!(sps.height, 1088);
}

/// クロップ後 1920x1080 になる
///
/// 符号化高さ 1088 (68 マクロブロック行) から縦 8 ピクセルをクロップする。
/// 4:2:0 の CropUnitY = 2 なので top / bottom 各 2 が縦 8 ピクセルに相当する
#[test]
fn parse_sps_crop_to_1920x1080() {
    let sps = parse_sps(&build_sps(&SpsParams {
        pic_width_in_mbs_minus1: 119,
        pic_height_in_map_units_minus1: 67,
        frame_cropping_flag: true,
        frame_crop_top_offset: 2,
        frame_crop_bottom_offset: 2,
        ..SpsParams::valid()
    }))
    .expect("クロップ後 1920x1080 の SPS は解析成功する");
    assert_eq!(sps.width, 1920);
    assert_eq!(sps.height, 1080);
}

/// frame_mbs_only_flag = 0 では高さがマクロブロック行の 2 倍になる
#[test]
fn parse_sps_frame_mbs_only_flag_zero() {
    // 15 マップ単位 × 2 = 30 行 → 480 ピクセル
    let sps = parse_sps(&build_sps(&SpsParams {
        pic_height_in_map_units_minus1: 14,
        frame_mbs_only_flag: false,
        ..SpsParams::valid()
    }))
    .expect("frame_mbs_only_flag=0 の SPS は解析成功する");
    assert_eq!(sps.height, 480);
}

/// frame_mbs_only_flag = 0 のときのクロップ (CropUnitY = 4) が正しく適用される
#[test]
fn parse_sps_frame_mbs_only_flag_zero_with_crop() {
    // 480 - 4 * (1 + 1) = 472
    let sps = parse_sps(&build_sps(&SpsParams {
        pic_height_in_map_units_minus1: 14,
        frame_mbs_only_flag: false,
        frame_cropping_flag: true,
        frame_crop_top_offset: 1,
        frame_crop_bottom_offset: 1,
        ..SpsParams::valid()
    }))
    .expect("frame_mbs_only_flag=0 のクロップ SPS は解析成功する");
    assert_eq!(sps.height, 472);
}

/// モノクロ (chroma_format_idc = 0) のクロップ (CropUnitX = 1 / CropUnitY = 1)
///
/// 7.4.2.1.1 の `ChromaArrayType == 0` で `CropUnitX = 1`、`CropUnitY = 2 - frame_mbs_only_flag`
/// (fmbf=1 で 1)。4:2:0 の 2 / 2 と取り違えると期待値がずれる
#[test]
fn parse_sps_monochrome_crop() {
    // 320 - 1 * (1 + 1) = 318、240 - 1 * (1 + 1) = 238
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 0,
        frame_cropping_flag: true,
        frame_crop_left_offset: 1,
        frame_crop_right_offset: 1,
        frame_crop_top_offset: 1,
        frame_crop_bottom_offset: 1,
        ..SpsParams::valid()
    }))
    .expect("モノクロのクロップ SPS は解析成功する");
    assert_eq!(sps.chroma_format_idc, 0);
    assert_eq!(sps.width, 318);
    assert_eq!(sps.height, 238);
}

/// 4:2:2 (chroma_format_idc = 2) のクロップ (CropUnitX = 2 / CropUnitY = 1)
///
/// Table 6-1 の `SubWidthC = 2` / `SubHeightC = 1` が反映される。
/// SubHeightC を 4:2:0 の 2 と取り違えると高さがずれる
#[test]
fn parse_sps_chroma_422_crop() {
    // 320 - 2 * (1 + 1) = 316、240 - 1 * (1 + 1) = 238
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 2,
        frame_cropping_flag: true,
        frame_crop_left_offset: 1,
        frame_crop_right_offset: 1,
        frame_crop_top_offset: 1,
        frame_crop_bottom_offset: 1,
        ..SpsParams::valid()
    }))
    .expect("4:2:2 のクロップ SPS は解析成功する");
    assert_eq!(sps.chroma_format_idc, 2);
    assert_eq!(sps.width, 316);
    assert_eq!(sps.height, 238);
}

/// 4:4:4 (chroma_format_idc = 3、separate 無し) のクロップ (CropUnitX = 1 / CropUnitY = 1)
///
/// Table 6-1 の `SubWidthC = 1` / `SubHeightC = 1` が反映される
#[test]
fn parse_sps_chroma_444_crop() {
    // 320 - 1 * (1 + 1) = 318、240 - 1 * (1 + 1) = 238
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 3,
        frame_cropping_flag: true,
        frame_crop_left_offset: 1,
        frame_crop_right_offset: 1,
        frame_crop_top_offset: 1,
        frame_crop_bottom_offset: 1,
        ..SpsParams::valid()
    }))
    .expect("4:4:4 のクロップ SPS は解析成功する");
    assert_eq!(sps.chroma_format_idc, 3);
    assert_eq!(sps.width, 318);
    assert_eq!(sps.height, 238);
}

/// separate_colour_plane_flag = 1 (ChromaArrayType = 0) のクロップ
///
/// 7.4.2.1.1 の `ChromaArrayType == 0` で `CropUnitX = 1`、
/// `CropUnitY = 2 - frame_mbs_only_flag`。fmbf=0 で CropUnitY = 2 になる
#[test]
fn parse_sps_chroma_444_separate_colour_plane_crop() {
    // 高さ 480 (15 マップ単位 × 2)、320 - 1 * (1 + 1) = 318、
    // 480 - 2 * (1 + 1) = 476
    let sps = parse_sps(&build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 3,
        separate_colour_plane_flag: true,
        frame_mbs_only_flag: false,
        frame_cropping_flag: true,
        frame_crop_left_offset: 1,
        frame_crop_right_offset: 1,
        frame_crop_top_offset: 1,
        frame_crop_bottom_offset: 1,
        ..SpsParams::valid()
    }))
    .expect("separate colour plane のクロップ SPS は解析成功する");
    assert_eq!(sps.chroma_format_idc, 3);
    assert_eq!(sps.width, 318);
    assert_eq!(sps.height, 476);
}

/// 実エンコーダー由来の EBSP (emulation prevention byte を含む) が RBSP 化後に読める
///
/// この SPS のバイト列は `00 00 03` を 2 箇所含む (black-h264-video.mp4 から抽出)
#[test]
fn parse_sps_real_ebsp_with_emulation_prevention_bytes() {
    let sps_ebsp = [
        0x64, 0x00, 0x1E, 0xAC, 0xD9, 0x40, 0xA0, 0x3D, 0xB0, 0x11, 0x00, 0x00, 0x03, 0x00, 0x01,
        0x00, 0x00, 0x03, 0x00, 0x32, 0x0F, 0x16, 0x2D, 0x96,
    ];
    // NAL ヘッダー (type 7) を付けて解析する
    let mut nal = Vec::new();
    nal.push(0x67);
    nal.extend_from_slice(&sps_ebsp);
    let sps = parse_sps(&nal).expect("emulation prevention byte 入りの実 SPS は解析成功する");
    assert_eq!(sps.profile_idc, 100);
    assert_eq!(sps.constraint_set_flags, 0x00);
    assert_eq!(sps.level_idc, 30);
    assert_eq!(sps.chroma_format_idc, 1);
    assert_eq!(sps.bit_depth_luma_minus8, 0);
    assert_eq!(sps.bit_depth_chroma_minus8, 0);
    assert_eq!(sps.width, 640);
    assert_eq!(sps.height, 480);
}

// ===== parse_sps: 拒否系 =====

/// NAL type が 7 以外の SPS は拒否する
#[test]
fn parse_sps_rejects_non_sps_nal_type() {
    // ヘッダーを 0x65 (type 5) に書き換える
    let mut nal = build_sps(&SpsParams::valid());
    nal[0] = 0x65;
    let err = parse_sps(&nal).expect_err("type 5 の NAL は SPS として拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 空入力 (NAL ヘッダー 1 バイト未満) の SPS は拒否する
#[test]
fn parse_sps_rejects_empty_input() {
    let err = parse_sps(&[]).expect_err("空入力は SPS として拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 32 ビットを超える Exp-Golomb コード (0 の 32 連続) は拒否する
#[test]
fn parse_sps_rejects_exp_golomb_code_too_long() {
    // ヘッダー + profile / constraint / level の後、seq_parameter_set_id の ue(v) が
    // 0 を 32 個並べる (leadingZeroBits = 32 で値域外)
    let nal = [0x67, 0x64, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x00, 0xFF];
    let err = parse_sps(&nal).expect_err("32 ビット超の Exp-Golomb は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// ヘッダーの forbidden_zero_bit が 1 の SPS は拒否する
#[test]
fn parse_sps_rejects_forbidden_zero_bit() {
    let mut nal = build_sps(&SpsParams::valid());
    nal[0] = 0xE7;
    let err = parse_sps(&nal).expect_err("forbidden_zero_bit=1 の SPS は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 切り詰められた SPS は拒否する
#[test]
fn parse_sps_rejects_truncated() {
    let full = build_sps(&SpsParams::valid());
    let truncated = &full[..full.len() - 3];
    let err = parse_sps(truncated).expect_err("切り詰められた SPS は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// chroma_format_idc > 3 は値域外として拒否する
#[test]
fn parse_sps_rejects_chroma_format_idc_out_of_range() {
    let nal = build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 4,
        ..SpsParams::valid()
    });
    let err = parse_sps(&nal).expect_err("chroma_format_idc=4 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// bit_depth_luma_minus8 > 6 は値域外として拒否する
#[test]
fn parse_sps_rejects_bit_depth_luma_out_of_range() {
    let nal = build_sps(&SpsParams {
        profile_idc: 100,
        bit_depth_luma_minus8: 7,
        ..SpsParams::valid()
    });
    let err = parse_sps(&nal).expect_err("bit_depth_luma_minus8=7 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// bit_depth_chroma_minus8 > 6 は値域外として拒否する
#[test]
fn parse_sps_rejects_bit_depth_chroma_out_of_range() {
    let nal = build_sps(&SpsParams {
        profile_idc: 100,
        bit_depth_chroma_minus8: 7,
        ..SpsParams::valid()
    });
    let err = parse_sps(&nal).expect_err("bit_depth_chroma_minus8=7 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// クロップが符号化幅を食いつぶす場合は拒否する
#[test]
fn parse_sps_rejects_crop_eating_coded_width() {
    // 幅 320 (20 マクロブロック)。4:2:0 の CropUnitX = 2 で 2 * (81 + 80) = 322 をクロップする
    let nal = build_sps(&SpsParams {
        pic_width_in_mbs_minus1: 19,
        frame_cropping_flag: true,
        frame_crop_left_offset: 81,
        frame_crop_right_offset: 80,
        ..SpsParams::valid()
    });
    let err = parse_sps(&nal).expect_err("幅を食いつぶすクロップは拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// クロップが符号化高さを食いつぶす場合は拒否する
#[test]
fn parse_sps_rejects_crop_eating_coded_height() {
    // 高さ 240 (15 マップ単位、frame_mbs_only_flag=1)。CropUnitY = 2 で 2 * (61 + 60) = 242 をクロップする
    let nal = build_sps(&SpsParams {
        pic_height_in_map_units_minus1: 14,
        frame_cropping_flag: true,
        frame_crop_top_offset: 61,
        frame_crop_bottom_offset: 60,
        ..SpsParams::valid()
    });
    let err = parse_sps(&nal).expect_err("高さを食いつぶすクロップは拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// クロップ後に幅 0 になる場合は拒否する
#[test]
fn parse_sps_rejects_zero_width_after_crop() {
    // 幅 16 (1 マクロブロック)。4:2:0 の CropUnitX = 2 で左右各 4 = 16 をクロップする
    let nal = build_sps(&SpsParams {
        pic_width_in_mbs_minus1: 0,
        frame_cropping_flag: true,
        frame_crop_left_offset: 4,
        frame_crop_right_offset: 4,
        ..SpsParams::valid()
    });
    let err = parse_sps(&nal).expect_err("クロップ後幅 0 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// クロップ後に高さ 0 になる場合は拒否する
#[test]
fn parse_sps_rejects_zero_height_after_crop() {
    // 高さ 16 (1 マップ単位、frame_mbs_only_flag=1)。CropUnitY = 2 で上下各 4 = 16 をクロップする
    let nal = build_sps(&SpsParams {
        pic_height_in_map_units_minus1: 0,
        frame_cropping_flag: true,
        frame_crop_top_offset: 4,
        frame_crop_bottom_offset: 4,
        ..SpsParams::valid()
    });
    let err = parse_sps(&nal).expect_err("クロップ後高さ 0 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// クロップ後に u16::MAX を超える幅は飽和せず拒否する
#[test]
fn parse_sps_rejects_width_exceeding_u16_max() {
    // (4095 + 1) * 16 = 65536 > u16::MAX
    let nal = build_sps(&SpsParams {
        pic_width_in_mbs_minus1: 4095,
        ..SpsParams::valid()
    });
    let err = parse_sps(&nal).expect_err("u16::MAX 超過の幅は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// クロップ後に u16::MAX を超える高さは飽和せず拒否する
#[test]
fn parse_sps_rejects_height_exceeding_u16_max() {
    // (4095 + 1) * 16 = 65536 > u16::MAX
    let nal = build_sps(&SpsParams {
        pic_height_in_map_units_minus1: 4095,
        ..SpsParams::valid()
    });
    let err = parse_sps(&nal).expect_err("u16::MAX 超過の高さは拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

// ===== build_avc1_box =====

/// 66 / 77 / 88 では chroma_format 等が None、それ以外では Some になる
#[test]
fn build_avc1_box_chroma_fields_by_profile() {
    for (profile_idc, expected_chroma) in [
        (66u8, None),
        (77, None),
        (88, None),
        (100, Some(1)),
        (110, Some(1)),
    ] {
        let sps = build_sps(&SpsParams {
            profile_idc,
            chroma_format_idc: 1,
            bit_depth_luma_minus8: if profile_idc == 110 { 2 } else { 0 },
            bit_depth_chroma_minus8: if profile_idc == 110 { 2 } else { 0 },
            ..SpsParams::valid()
        });
        let pps = valid_pps();
        let avc1 = build_avc1_box(&[sps], &[pps], &default_config())
            .expect("有効な SPS / PPS は構築成功する");
        assert_eq!(
            avc1.avcc_box.chroma_format.map(|c| c.get()),
            expected_chroma,
            "profile {profile_idc} の chroma_format"
        );
        if matches!(profile_idc, 66 | 77 | 88) {
            assert!(avc1.avcc_box.bit_depth_luma_minus8.is_none());
            assert!(avc1.avcc_box.bit_depth_chroma_minus8.is_none());
        } else {
            assert_eq!(
                avc1.avcc_box.bit_depth_luma_minus8.map(|b| b.get()),
                Some(if profile_idc == 110 { 2 } else { 0 })
            );
            assert_eq!(
                avc1.avcc_box.bit_depth_chroma_minus8.map(|b| b.get()),
                Some(if profile_idc == 110 { 2 } else { 0 })
            );
        }
    }
}

/// 構築した Avc1Box の固定値 / ストリーム導出値 / 呼び出し側指定値を検証する
#[test]
fn build_avc1_box_fixed_and_derived_values() {
    let sps = build_sps(&SpsParams {
        profile_idc: 100,
        constraint_set_flags: 0x40,
        level_idc: 31,
        chroma_format_idc: 1,
        pic_width_in_mbs_minus1: 119,
        pic_height_in_map_units_minus1: 67,
        frame_cropping_flag: true,
        frame_crop_top_offset: 2,
        frame_crop_bottom_offset: 2,
        ..SpsParams::valid()
    });
    let pps = valid_pps();
    let config = H264SampleEntryConfig { length_size: 2 };
    let avc1 = build_avc1_box(
        core::slice::from_ref(&sps),
        core::slice::from_ref(&pps),
        &config,
    )
    .expect("有効な SPS / PPS は構築成功する");

    // ストリーム導出値
    assert_eq!(avc1.avcc_box.avc_profile_indication, 100);
    assert_eq!(avc1.avcc_box.profile_compatibility, 0x40);
    assert_eq!(avc1.avcc_box.avc_level_indication, 31);
    assert_eq!(avc1.avcc_box.sps_list, vec![sps]);
    assert_eq!(avc1.avcc_box.pps_list, vec![pps]);
    assert_eq!(avc1.avcc_box.chroma_format, Some(Uint::new(1)));
    assert_eq!(avc1.avcc_box.bit_depth_luma_minus8, Some(Uint::new(0)));
    assert_eq!(avc1.avcc_box.bit_depth_chroma_minus8, Some(Uint::new(0)));
    assert_eq!(avc1.visual.width, 1920);
    assert_eq!(avc1.visual.height, 1080);

    // 固定値
    assert_eq!(
        avc1.visual.data_reference_index,
        VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX
    );
    assert_eq!(
        avc1.visual.horizresolution,
        VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION
    );
    assert_eq!(
        avc1.visual.vertresolution,
        VisualSampleEntryFields::DEFAULT_VERTRESOLUTION
    );
    assert_eq!(
        avc1.visual.frame_count,
        VisualSampleEntryFields::DEFAULT_FRAME_COUNT
    );
    assert_eq!(
        avc1.visual.compressorname,
        VisualSampleEntryFields::NULL_COMPRESSORNAME
    );
    assert_eq!(avc1.visual.depth, VisualSampleEntryFields::DEFAULT_DEPTH);
    assert!(avc1.unknown_boxes.is_empty());
    assert!(avc1.avcc_box.sps_ext_list.is_empty());

    // 呼び出し側指定値 (長さ幅 2 → length_size_minus_one = 1)
    assert_eq!(avc1.avcc_box.length_size_minus_one, Uint::new(1));
}

/// SPS リストが空なら拒否する
#[test]
fn build_avc1_box_rejects_empty_sps_list() {
    let err = build_avc1_box(&[], &[valid_pps()], &default_config())
        .expect_err("SPS 空リストは拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// PPS リストが空なら拒否する
#[test]
fn build_avc1_box_rejects_empty_pps_list() {
    let sps = build_sps(&SpsParams::valid());
    let err = build_avc1_box(&[sps], &[], &default_config()).expect_err("PPS 空リストは拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 長さ幅 3 は reserved のため拒否する
#[test]
fn build_avc1_box_rejects_length_size_3() {
    let sps = build_sps(&SpsParams::valid());
    let err = build_avc1_box(
        &[sps],
        &[valid_pps()],
        &H264SampleEntryConfig { length_size: 3 },
    )
    .expect_err("長さ幅 3 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// PPS が NAL type 8 以外なら拒否する
#[test]
fn build_avc1_box_rejects_wrong_pps_type() {
    let sps = build_sps(&SpsParams::valid());
    // ヘッダーを type 5 に書き換えた PPS
    let pps = vec![0x65, 0xEB, 0xE3];
    let err = build_avc1_box(&[sps], &[pps], &default_config())
        .expect_err("type 8 以外の PPS は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// PPS が空なら拒否する
#[test]
fn build_avc1_box_rejects_empty_pps() {
    let sps = build_sps(&SpsParams::valid());
    let err =
        build_avc1_box(&[sps], &[Vec::new()], &default_config()).expect_err("空 PPS は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// SPS が 31 個超なら拒否する
#[test]
fn build_avc1_box_rejects_too_many_sps() {
    let sps = build_sps(&SpsParams::valid());
    let sps_list = vec![sps.clone(); 32];
    let err = build_avc1_box(&sps_list, &[valid_pps()], &default_config())
        .expect_err("SPS 32 個は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// PPS が 255 個超なら拒否する
#[test]
fn build_avc1_box_rejects_too_many_pps() {
    let sps = build_sps(&SpsParams::valid());
    let pps_list = vec![valid_pps(); 256];
    let err =
        build_avc1_box(&[sps], &pps_list, &default_config()).expect_err("PPS 256 個は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// SPS が u16::MAX バイト超なら拒否する
#[test]
fn build_avc1_box_rejects_sps_too_long() {
    // 長さ検証は先頭 SPS の解析より前に行われるため、中身は不正でもよい
    let too_long_sps = vec![0x67; u16::MAX as usize + 1];
    let err = build_avc1_box(&[too_long_sps], &[valid_pps()], &default_config())
        .expect_err("u16::MAX 超過の SPS は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// PPS が u16::MAX バイト超なら拒否する
#[test]
fn build_avc1_box_rejects_pps_too_long() {
    let sps = build_sps(&SpsParams::valid());
    let too_long_pps = vec![0x68; u16::MAX as usize + 1];
    let err = build_avc1_box(&[sps], &[too_long_pps], &default_config())
        .expect_err("u16::MAX 超過の PPS は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// PPS の forbidden_zero_bit が 1 なら拒否する
#[test]
fn build_avc1_box_rejects_pps_forbidden_zero_bit() {
    let sps = build_sps(&SpsParams::valid());
    // 0x88 = 1000_1000: forbidden_zero_bit = 1 / nal_unit_type = 8
    let pps = vec![0x88, 0xEB, 0xE3];
    let err = build_avc1_box(&[sps], &[pps], &default_config())
        .expect_err("forbidden_zero_bit=1 の PPS は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 構築した Avc1Box が encode → decode でラウンドトリップする
#[test]
fn build_avc1_box_roundtrip() {
    let sps = build_sps(&SpsParams {
        profile_idc: 100,
        chroma_format_idc: 1,
        ..SpsParams::valid()
    });
    let avc1 = build_avc1_box(&[sps], &[valid_pps()], &default_config())
        .expect("有効な SPS / PPS は構築成功する");
    let encoded = avc1.encode_to_vec().expect("encode 成功");
    let (decoded, size) = Avc1Box::decode(&encoded).expect("decode 成功");
    assert_eq!(size, encoded.len());
    assert_eq!(decoded, avc1);
}

// ===== build_avc1_box_from_annexb =====

/// Annex B から SPS / PPS を抽出して Avc1Box を構築する
///
/// SEI やスライス等の他種別 NAL は無視される
#[test]
fn build_avc1_box_from_annexb_ignores_non_sps_pps() {
    let sps = build_sps(&SpsParams {
        profile_idc: 100,
        constraint_set_flags: 0x00,
        level_idc: 30,
        chroma_format_idc: 1,
        pic_width_in_mbs_minus1: 39,
        pic_height_in_map_units_minus1: 29,
        ..SpsParams::valid()
    });
    let pps = valid_pps();
    let input = annexb_with_3(&[&[0x06, 0x01, 0x02], &sps, &pps, &[0x65, 0x88]]);
    let avc1 =
        build_avc1_box_from_annexb(&input, &default_config()).expect("Annex B から構築成功する");
    assert_eq!(avc1.avcc_box.avc_profile_indication, 100);
    assert_eq!(avc1.avcc_box.avc_level_indication, 30);
    assert_eq!(avc1.avcc_box.sps_list, vec![sps]);
    assert_eq!(avc1.avcc_box.pps_list, vec![pps]);
    assert_eq!(avc1.visual.width, 640);
    assert_eq!(avc1.visual.height, 480);
}

/// Annex B に SPS が無ければ拒否する
#[test]
fn build_avc1_box_from_annexb_rejects_without_sps() {
    let pps = valid_pps();
    let input = annexb_with_3(&[&pps]);
    let err = build_avc1_box_from_annexb(&input, &default_config())
        .expect_err("SPS が無い Annex B は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

// ===== 実データ fixture =====

/// 実エンコーダー (ffmpeg) が生成した MP4 から抽出した SPS / PPS の Annex B 列
///
/// 抽出元: `tests/testdata/black-h264-video.mp4` の `avcC` ボックス。
/// SPS は profile 100 / level 30 / 640x480 で、emulation prevention byte を 2 箇所含む。
/// 生成環境: ffmpeg + x264
const REAL_H264_SPS_PPS_ANNEXB: &[u8] = include_bytes!("testdata/h264-sps-pps-annexb.bin");

/// 実データの Annex B を解析して SPS / PPS の境界と種別を確認する
#[test]
fn real_h264_annexb_parses() {
    let nals = parse_annexb_nal_units(REAL_H264_SPS_PPS_ANNEXB)
        .expect("実 SPS / PPS の Annex B は解析成功する");
    assert_eq!(nals.len(), 2);
    assert_eq!(nals[0].nal_unit_type, 7);
    assert_eq!(nals[1].nal_unit_type, 8);
}

/// 実データから build_avc1_box_from_annexb で Avc1Box を構築できる
#[test]
fn real_h264_build_avc1_box_from_annexb() {
    let avc1 = build_avc1_box_from_annexb(REAL_H264_SPS_PPS_ANNEXB, &default_config())
        .expect("実データから構築成功する");
    assert_eq!(avc1.avcc_box.avc_profile_indication, 100);
    assert_eq!(avc1.avcc_box.profile_compatibility, 0x00);
    assert_eq!(avc1.avcc_box.avc_level_indication, 30);
    assert_eq!(avc1.avcc_box.chroma_format, Some(Uint::new(1)));
    assert_eq!(avc1.avcc_box.bit_depth_luma_minus8, Some(Uint::new(0)));
    assert_eq!(avc1.avcc_box.bit_depth_chroma_minus8, Some(Uint::new(0)));
    assert_eq!(avc1.visual.width, 640);
    assert_eq!(avc1.visual.height, 480);

    // 格納された SPS / PPS は emulation prevention byte を残した EBSP のまま
    assert_eq!(avc1.avcc_box.sps_list.len(), 1);
    assert!(
        avc1.avcc_box.sps_list[0]
            .windows(3)
            .any(|w| w == [0x00, 0x00, 0x03])
    );
    assert_eq!(avc1.avcc_box.pps_list.len(), 1);

    // 実データ由来の avc1 が encode → decode でラウンドトリップする
    let encoded = avc1.encode_to_vec().expect("encode 成功");
    let (decoded, size) = Avc1Box::decode(&encoded).expect("decode 成功");
    assert_eq!(size, encoded.len());
    assert_eq!(decoded, avc1);
}
