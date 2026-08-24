//! `shiguredo_mp4::bitstream::aac` の決定的テスト
//!
//! 手動構築した ASC / ADTS のビット列に対してパーサーの受理・拒否条件を固定する。
//! 実 ffmpeg 出力による fixture テストは `tests/testdata/beep-aac-audio.aac` を用いた
//! 別テストで補う

use shiguredo_mp4::{
    Decode, Encode, ErrorKind,
    bitstream::aac::{
        AUDIO_OBJECT_TYPE_AAC_LC, AdtsEncodeConfig, AdtsMpegVersion, AudioSpecificConfig,
        Mp4aSampleEntryConfig, SamplingFrequency, build_mp4a_box, encode_audio_specific_config,
        parse_adts_frame, parse_audio_specific_config, wrap_raw_aac_in_adts,
    },
    boxes::{AudioSampleEntryFields, Mp4aBox},
};

/// ASC / ADTS の MSB-first ビット組み立て用
///
/// 実装側の `BitWriter` と対称なテスト用の bit writer。
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

/// ASC のビット列を組み立てる
///
/// `explicit` は `sampling_frequency_index == 0xF` のときに書き込む明示周波数 (Hz)。
/// `flags` は GASpecificConfig 必須 3 フラグ (`frameLengthFlag` / `dependsOnCoreCoder` /
/// `extensionFlag`) の順
fn build_asc(
    aot: u8,
    sampling_frequency_index: u8,
    explicit: Option<u32>,
    channel_configuration: u8,
    flags: [u8; 3],
) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.push_bits(u32::from(aot), 5);
    w.push_bits(u32::from(sampling_frequency_index), 4);
    if let Some(freq) = explicit {
        w.push_bits(freq, 24);
    }
    w.push_bits(u32::from(channel_configuration), 4);
    w.push_bit(flags[0]);
    w.push_bit(flags[1]);
    w.push_bit(flags[2]);
    w.into_bytes()
}

/// ADTS ヘッダーのビット列を組み立てる
///
/// `crc` は `protection_absent == 0` のときにヘッダー末尾に書き込む 16 ビット値
#[expect(clippy::too_many_arguments)]
fn build_adts_header(
    syncword: u32,
    id: u8,
    layer: u32,
    protection_absent: u8,
    profile: u32,
    sampling_frequency_index: u8,
    private_bit: u8,
    channel_configuration: u8,
    original_copy: u8,
    home: u8,
    copyright_bit: u8,
    copyright_start: u8,
    frame_length: u16,
    buffer_fullness: u32,
    nrdb: u32,
    crc: Option<u16>,
) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.push_bits(syncword, 12);
    w.push_bit(id);
    w.push_bits(layer, 2);
    w.push_bit(protection_absent);
    w.push_bits(profile, 2);
    w.push_bits(u32::from(sampling_frequency_index), 4);
    w.push_bit(private_bit);
    w.push_bits(u32::from(channel_configuration), 3);
    w.push_bit(original_copy);
    w.push_bit(home);
    w.push_bit(copyright_bit);
    w.push_bit(copyright_start);
    w.push_bits(u32::from(frame_length), 13);
    w.push_bits(buffer_fullness, 11);
    w.push_bits(nrdb, 2);
    let mut bytes = w.into_bytes();
    if let Some(crc) = crc {
        bytes.extend_from_slice(&crc.to_be_bytes());
    }
    bytes
}

/// 代表値 `0x11 0x90` (AAC-LC、48 kHz、stereo) の ASC
fn asc_48k_stereo() -> AudioSpecificConfig {
    AudioSpecificConfig {
        audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
        sampling_frequency: SamplingFrequency::Index { index: 3 }, // 48000
        channel_configuration: 2,
    }
}

/// `Mp4aSampleEntryConfig` の有効な既定値
fn default_mp4a_config() -> Mp4aSampleEntryConfig {
    Mp4aSampleEntryConfig {
        es_id: 1,
        buffer_size_db: 0,
        max_bitrate: 128000,
        avg_bitrate: 128000,
    }
}

/// 有効な ADTS 組み立て設定 (MPEG-4、original_copy / home なし)
fn default_adts_config() -> AdtsEncodeConfig {
    AdtsEncodeConfig {
        mpeg_version: AdtsMpegVersion::Mpeg4,
        original_copy: false,
        home: false,
    }
}

// ===== parse_audio_specific_config: 受理系 =====

/// 代表値 `0x11 0x90` (AAC-LC、48 kHz、stereo) を解析できる
#[test]
fn parse_asc_48k_stereo() {
    let config = parse_audio_specific_config(&[0x11, 0x90])
        .expect("0x11 0x90 (AAC-LC 48kHz stereo) は解析成功する");
    assert_eq!(config.audio_object_type, 2);
    assert_eq!(
        config.sampling_frequency,
        SamplingFrequency::Index { index: 3 }
    );
    assert_eq!(
        config.sampling_frequency.hz().expect("index 3 は有効"),
        48000
    );
    assert_eq!(config.channel_configuration, 2);
}

/// `0x12 0x08` (AAC-LC、44.1 kHz、mono) を解析できる
#[test]
fn parse_asc_44100_mono() {
    let config = parse_audio_specific_config(&[0x12, 0x08])
        .expect("0x12 0x08 (AAC-LC 44.1kHz mono) は解析成功する");
    assert_eq!(config.audio_object_type, 2);
    assert_eq!(
        config.sampling_frequency,
        SamplingFrequency::Index { index: 4 } // 44100
    );
    assert_eq!(
        config.sampling_frequency.hz().expect("index 4 は有効"),
        44100
    );
    assert_eq!(config.channel_configuration, 1);
}

/// `0x12 0x10` (AAC-LC、44.1 kHz、stereo) を解析できる
#[test]
fn parse_asc_44100_stereo() {
    let config = parse_audio_specific_config(&[0x12, 0x10])
        .expect("0x12 0x10 (AAC-LC 44.1kHz stereo) は解析成功する");
    assert_eq!(
        config.sampling_frequency,
        SamplingFrequency::Index { index: 4 } // 44100
    );
    assert_eq!(
        config.sampling_frequency.hz().expect("index 4 は有効"),
        44100
    );
    assert_eq!(config.channel_configuration, 2);
}

/// sampling_frequency_index 0 (96 kHz) と 12 (7350 Hz) を解析できる
#[test]
fn parse_asc_frequency_index_boundaries() {
    // index 0 (96000)
    let bytes = build_asc(2, 0, None, 2, [0, 0, 0]);
    let config = parse_audio_specific_config(&bytes).expect("index 0 は解析成功する");
    assert_eq!(
        config.sampling_frequency,
        SamplingFrequency::Index { index: 0 }
    );
    assert_eq!(
        config.sampling_frequency.hz().expect("index 0 は有効"),
        96000
    );

    // index 12 (7350)
    let bytes = build_asc(2, 12, None, 1, [0, 0, 0]);
    let config = parse_audio_specific_config(&bytes).expect("index 12 は解析成功する");
    assert_eq!(
        config.sampling_frequency,
        SamplingFrequency::Index { index: 12 }
    );
    assert_eq!(
        config.sampling_frequency.hz().expect("index 12 は有効"),
        7350
    );
}

/// channel_configuration 7 (8 チャンネル) を解析できる
#[test]
fn parse_asc_channel_configuration_7() {
    let bytes = build_asc(2, 3, None, 7, [0, 0, 0]);
    let config = parse_audio_specific_config(&bytes).expect("channel 7 は解析成功する");
    assert_eq!(config.channel_configuration, 7);
}

/// sampling_frequency_index 0xF の明示周波数 (24 ビット) を解析できる
#[test]
fn parse_asc_explicit_frequency() {
    // 明示周波数 44100 (0xAC44) を 24 ビットで書く
    let bytes = build_asc(2, 15, Some(44100), 2, [0, 0, 0]);
    let config = parse_audio_specific_config(&bytes).expect("明示周波数は解析成功する");
    assert_eq!(
        config.sampling_frequency,
        SamplingFrequency::Explicit { frequency: 44100 }
    );
    assert_eq!(
        config.sampling_frequency.hz().expect("明示周波数は有効"),
        44100
    );
    assert_eq!(config.channel_configuration, 2);
    // 24 ビット最大値 (16777215) も受理する
    let bytes = build_asc(2, 15, Some(0xFF_FFFF), 1, [0, 0, 0]);
    let config = parse_audio_specific_config(&bytes).expect("明示周波数最大値は解析成功する");
    assert_eq!(
        config.sampling_frequency,
        SamplingFrequency::Explicit {
            frequency: 0xFF_FFFF
        }
    );
}

// ===== parse_audio_specific_config: 拒否系 =====

/// 空入力は拒否する
#[test]
fn reject_asc_empty_input() {
    let err = parse_audio_specific_config(&[]).expect_err("空入力は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 1 バイトだけの短い入力は拒否する
#[test]
fn reject_asc_short_input() {
    let err = parse_audio_specific_config(&[0x11]).expect_err("1 バイトの入力は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// audio_object_type が 2 以外 (1 / 5 / 31 のエスケープ形式) は拒否する
#[test]
fn reject_asc_non_aac_lc_aot() {
    for aot in [1u8, 5, 31] {
        let bytes = build_asc(aot, 3, None, 2, [0, 0, 0]);
        let err =
            parse_audio_specific_config(&bytes).expect_err(&format!("AOT {aot} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// sampling_frequency_index 13 / 14 (reserved) は拒否する
#[test]
fn reject_asc_reserved_frequency_index() {
    for index in [13u8, 14] {
        let bytes = build_asc(2, index, None, 2, [0, 0, 0]);
        let err =
            parse_audio_specific_config(&bytes).expect_err(&format!("sfi {index} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// sampling_frequency_index 0xF の明示周波数が 0 は拒否する
#[test]
fn reject_asc_explicit_frequency_zero() {
    let bytes = build_asc(2, 15, Some(0), 2, [0, 0, 0]);
    let err = parse_audio_specific_config(&bytes).expect_err("明示周波数 0 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// channel_configuration 0 (PCE) と 8..=15 (reserved) は拒否する
#[test]
fn reject_asc_invalid_channel_configuration() {
    for channel in [0u8, 8, 15] {
        let bytes = build_asc(2, 3, None, channel, [0, 0, 0]);
        let err = parse_audio_specific_config(&bytes)
            .expect_err(&format!("channel {channel} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// GASpecificConfig 必須 3 フラグのいずれかが 1 なら拒否する
///
/// 後続の `coreCoderDelay` や PCE を読まずに入り口で拒否する
#[test]
fn reject_asc_nonzero_gaspecific_flags() {
    let flags_variants: [[u8; 3]; 3] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
    for flags in flags_variants {
        let bytes = build_asc(2, 3, None, 2, flags);
        let err = parse_audio_specific_config(&bytes)
            .expect_err(&format!("flags {flags:?} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// 読み切り位置以降に後続バイトがある入力は拒否する
///
/// fixture の 5 バイト ASC `12 08 56 e5 00` (先頭 16 ビットは AOT 2 / 44.1 kHz / mono だが、
/// 余りが SBR の `syncExtensionType` `0x2B7`) は AAC-LC として黙って読み替えない
#[test]
fn reject_asc_trailing_bytes() {
    let err = parse_audio_specific_config(&[0x12, 0x08, 0x56, 0xe5, 0x00])
        .expect_err("SBR 拡張付きの後続バイトは拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// ゼロ埋めの後続バイトも拒否する
#[test]
fn reject_asc_zero_padded_trailing_byte() {
    let err = parse_audio_specific_config(&[0x12, 0x10, 0x00])
        .expect_err("ゼロ埋めの後続バイトは拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

// ===== encode_audio_specific_config =====

/// 受理した ASC に対して `encode(parse(input))` が入力と一致する
#[test]
fn encode_asc_roundtrip() {
    let inputs: [&[u8]; 5] = [
        &[0x11, 0x90], // 48 kHz stereo
        &[0x12, 0x10], // 44.1 kHz stereo
        &[0x16, 0x08], // 7350 Hz mono
        &[0x10, 0x10], // 96 kHz stereo
        &[0x11, 0xb8], // 48 kHz 8ch
    ];
    for input in inputs {
        let config = parse_audio_specific_config(input).expect("入力は解析成功する");
        let encoded = encode_audio_specific_config(&config).expect("エンコード成功する");
        assert_eq!(
            encoded, input,
            "encode(parse({input:?})) は入力と一致するべき"
        );
    }
}

/// 明示周波数の ASC が 5 バイトの正規形に往復する
#[test]
fn encode_asc_explicit_frequency_roundtrip() {
    let bytes = build_asc(2, 15, Some(44100), 2, [0, 0, 0]);
    let config = parse_audio_specific_config(&bytes).expect("明示周波数は解析成功する");
    let encoded = encode_audio_specific_config(&config).expect("エンコード成功する");
    assert_eq!(encoded.len(), 5);
    assert_eq!(encoded, bytes);
    let reparsed = parse_audio_specific_config(&encoded).expect("再解析成功する");
    assert_eq!(reparsed, config);
}

/// 手組みの受理条件を満たす ASC が正規形にエンコードされる
#[test]
fn encode_asc_hand_built_valid() {
    let config = AudioSpecificConfig {
        audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
        sampling_frequency: SamplingFrequency::Index { index: 4 },
        channel_configuration: 1,
    };
    let encoded = encode_audio_specific_config(&config).expect("有効な ASC はエンコード成功する");
    assert_eq!(encoded, [0x12, 0x08]);
}

/// 手組みで範囲外の index (13..=15) を持つ ASC はエンコードで拒否する
///
/// `sampling_frequency` が enum になったことで「index と Hz の食い違い」は表現できず、
/// 残る手組みの不正は index の範囲外のみ
#[test]
fn reject_encode_asc_out_of_range_index() {
    for index in [13u8, 14, 15] {
        let config = AudioSpecificConfig {
            audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
            sampling_frequency: SamplingFrequency::Index { index },
            channel_configuration: 2,
        };
        let err = encode_audio_specific_config(&config)
            .expect_err(&format!("index {index} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// 手組みで audio_object_type が 2 以外の ASC はエンコードで拒否する
#[test]
fn reject_encode_asc_wrong_aot() {
    let config = AudioSpecificConfig {
        audio_object_type: 5,
        sampling_frequency: SamplingFrequency::Index { index: 3 },
        channel_configuration: 2,
    };
    let err = encode_audio_specific_config(&config).expect_err("AOT 5 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 手組みで channel_configuration が範囲外の ASC はエンコードで拒否する
#[test]
fn reject_encode_asc_invalid_channel_configuration() {
    for channel in [0u8, 8] {
        let config = AudioSpecificConfig {
            audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
            sampling_frequency: SamplingFrequency::Index { index: 3 },
            channel_configuration: channel,
        };
        let err = encode_audio_specific_config(&config)
            .expect_err(&format!("channel {channel} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// 手組みで明示周波数が 0 または 24 ビット超過の ASC はエンコードで拒否する
#[test]
fn reject_encode_asc_invalid_explicit_frequency() {
    for freq in [0u32, 0x0100_0000] {
        let config = AudioSpecificConfig {
            audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
            sampling_frequency: SamplingFrequency::Explicit { frequency: freq },
            channel_configuration: 2,
        };
        let err = encode_audio_specific_config(&config)
            .expect_err(&format!("明示周波数 {freq} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// `SamplingFrequency::hz()` が範囲外 index で Err を返す (panic しない)
#[test]
fn reject_sampling_frequency_hz_out_of_range_index() {
    for index in [13u8, 14, 15] {
        let frequency = SamplingFrequency::Index { index };
        let err = frequency
            .hz()
            .expect_err(&format!("index {index} は hz() で拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

// ===== SamplingFrequency::from_hz =====

/// 標準テーブルに一致する Hz は `Index` になり、正規形 (2 バイト) にエンコードされる
#[test]
fn sampling_frequency_from_hz_standard_frequency_is_index() {
    let cases: [(u32, u8); 3] = [(96000, 0), (48000, 3), (7350, 12)];
    for (hz, expected_index) in cases {
        let frequency = SamplingFrequency::from_hz(hz).expect("有効な Hz は生成成功する");
        assert_eq!(
            frequency,
            SamplingFrequency::Index {
                index: expected_index
            }
        );
        assert_eq!(frequency.hz().expect("生成値は有効"), hz);
    }

    // 代表値 48000 (index 3) が 2 バイトの正規形にエンコードされる
    let config = AudioSpecificConfig {
        audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
        sampling_frequency: SamplingFrequency::from_hz(48000).expect("48000 は生成成功する"),
        channel_configuration: 2,
    };
    let encoded = encode_audio_specific_config(&config).expect("エンコード成功する");
    assert_eq!(encoded, [0x11, 0x90]);
}

/// 標準テーブルに一致しない Hz は `Explicit` になり、5 バイトにエンコードされる
#[test]
fn sampling_frequency_from_hz_non_standard_frequency_is_explicit() {
    for hz in [44000u32, 65535, 0xFF_FFFF] {
        let frequency = SamplingFrequency::from_hz(hz).expect("有効な Hz は生成成功する");
        assert_eq!(frequency, SamplingFrequency::Explicit { frequency: hz });
        assert_eq!(frequency.hz().expect("生成値は有効"), hz);
    }

    let config = AudioSpecificConfig {
        audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
        sampling_frequency: SamplingFrequency::from_hz(44000).expect("44000 は生成成功する"),
        channel_configuration: 2,
    };
    let encoded = encode_audio_specific_config(&config).expect("エンコード成功する");
    assert_eq!(encoded.len(), 5);
}

/// `from_hz` が 0 または 24 ビット超過の Hz を拒否する
#[test]
fn reject_sampling_frequency_from_hz_invalid_range() {
    for hz in [0u32, 0x0100_0000] {
        let err =
            SamplingFrequency::from_hz(hz).expect_err(&format!("{hz} Hz は from_hz で拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// `AUDIO_OBJECT_TYPE_AAC_LC` 定数の値を固定する
#[test]
fn audio_object_type_constant_is_2() {
    assert_eq!(AUDIO_OBJECT_TYPE_AAC_LC, 2);
}

// ===== parse_adts_frame: 受理系 =====

/// 7 バイトヘッダー (protection_absent = 1、nrdb = 0) の ADTS フレームを解析できる
#[test]
fn parse_adts_7byte_header() {
    // 44.1 kHz / mono / MPEG-4 / frame_length = 7 + 3 (raw 3 バイト)
    let header = build_adts_header(0xFFF, 0, 0, 1, 1, 4, 0, 1, 0, 0, 0, 0, 10, 0x7FF, 0, None);
    assert_eq!(header.len(), 7);
    let raw = [0xAA, 0xBB, 0xCC];
    let mut frame = header.clone();
    frame.extend_from_slice(&raw);

    let (parsed, parsed_raw) = parse_adts_frame(&frame).expect("7 バイトヘッダーは解析成功する");
    assert_eq!(parsed.mpeg_version, AdtsMpegVersion::Mpeg4);
    assert!(parsed.protection_absent);
    assert_eq!(parsed.audio_object_type, 2);
    assert_eq!(parsed.sampling_frequency_index, 4);
    assert_eq!(parsed.channel_configuration, 1);
    assert_eq!(parsed.frame_length, 10);
    assert!(!parsed.original_copy);
    assert!(!parsed.home);
    assert_eq!(parsed_raw, &raw[..]);
}

/// MPEG-2 (ID = 1) の ADTS フレームを解析できる
#[test]
fn parse_adts_mpeg2() {
    let header = build_adts_header(0xFFF, 1, 0, 1, 1, 3, 0, 2, 0, 0, 0, 0, 9, 0x7FF, 0, None);
    let mut frame = header.clone();
    frame.extend_from_slice(&[0x01, 0x02]);
    let (parsed, _) = parse_adts_frame(&frame).expect("MPEG-2 は解析成功する");
    assert_eq!(parsed.mpeg_version, AdtsMpegVersion::Mpeg2);
    assert_eq!(parsed.sampling_frequency_index, 3);
    assert_eq!(parsed.channel_configuration, 2);
}

/// original_copy / home ビットを解析できる
#[test]
fn parse_adts_original_copy_and_home() {
    let header = build_adts_header(0xFFF, 0, 0, 1, 1, 3, 0, 2, 1, 1, 0, 0, 9, 0x7FF, 0, None);
    let mut frame = header.clone();
    frame.extend_from_slice(&[0x01, 0x02]);
    let (parsed, _) = parse_adts_frame(&frame).expect("original_copy / home 付きは解析成功する");
    assert!(parsed.original_copy);
    assert!(parsed.home);
}

/// private_bit / copyright ビットが 1 の ADTS フレームも解析できる (値を返さず読み飛ばす)
///
/// 本モジュールは private_bit / copyright ビットを 0 必須にしない契約なので、
/// 1 でも受理して残りのフィールドと raw を返す
#[test]
fn parse_adts_private_bit_and_copyright_accepted() {
    let header = build_adts_header(0xFFF, 0, 0, 1, 1, 3, 1, 2, 0, 0, 1, 1, 9, 0x7FF, 0, None);
    let mut frame = header.clone();
    frame.extend_from_slice(&[0x01, 0x02]);
    let (parsed, parsed_raw) =
        parse_adts_frame(&frame).expect("private_bit=1 / copyright=1 は読み飛ばされて受理される");
    assert_eq!(parsed.sampling_frequency_index, 3);
    assert_eq!(parsed.channel_configuration, 2);
    assert_eq!(parsed_raw, &[0x01, 0x02][..]);
}

/// 9 バイトヘッダー (protection_absent = 0、CRC 付き) を解析できる
///
/// CRC の値は検証せず読み飛ばす
#[test]
fn parse_adts_9byte_header_with_crc() {
    // frame_length = 9 (ヘッダー + CRC) + 2 (raw) = 11
    let header = build_adts_header(
        0xFFF,
        0,
        0,
        0,
        1,
        3,
        0,
        2,
        0,
        0,
        0,
        0,
        11,
        0x7FF,
        0,
        Some(0x1234),
    );
    assert_eq!(header.len(), 9);
    let raw = [0xDE, 0xAD];
    let mut frame = header.clone();
    frame.extend_from_slice(&raw);

    let (parsed, parsed_raw) = parse_adts_frame(&frame).expect("CRC 付きは解析成功する");
    assert!(!parsed.protection_absent);
    assert_eq!(parsed.frame_length, 11);
    assert_eq!(parsed_raw, &raw[..]);
}

/// frame_length が入力より短い場合、先頭から frame_length バイトだけをフレームとして解析する
#[test]
fn parse_adts_trailing_bytes_are_ignored() {
    let header = build_adts_header(0xFFF, 0, 0, 1, 1, 3, 0, 2, 0, 0, 0, 0, 8, 0x7FF, 0, None);
    let mut frame = header.clone();
    frame.extend_from_slice(&[0x01, 0x02]);
    // 余分な 1 バイトは frame_length (8) の範囲外
    frame.push(0xFF);

    let (parsed, parsed_raw) = parse_adts_frame(&frame).expect("末尾の余りは解析成功する");
    assert_eq!(parsed.frame_length, 8);
    assert_eq!(parsed_raw, &[0x01][..]);
}

// ===== parse_adts_frame: 拒否系 =====

/// syncword 不一致は拒否する
#[test]
fn reject_adts_wrong_syncword() {
    for syncword in [0u32, 0xFFE, 0x7FF] {
        let header =
            build_adts_header(syncword, 0, 0, 1, 1, 3, 0, 2, 0, 0, 0, 0, 9, 0x7FF, 0, None);
        let mut frame = header.clone();
        frame.extend_from_slice(&[0x01, 0x02]);
        let err =
            parse_adts_frame(&frame).expect_err(&format!("syncword {syncword:#x} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// layer != 0 は拒否する
#[test]
fn reject_adts_nonzero_layer() {
    for layer in [1u32, 3] {
        let header = build_adts_header(
            0xFFF, 0, layer, 1, 1, 3, 0, 2, 0, 0, 0, 0, 9, 0x7FF, 0, None,
        );
        let mut frame = header.clone();
        frame.extend_from_slice(&[0x01, 0x02]);
        let err = parse_adts_frame(&frame).expect_err(&format!("layer {layer} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// profile != 1 (AOT != 2) は拒否する
#[test]
fn reject_adts_profile_not_aac_lc() {
    for profile in [0u32, 2, 3] {
        let header = build_adts_header(
            0xFFF, 0, 0, 1, profile, 3, 0, 2, 0, 0, 0, 0, 9, 0x7FF, 0, None,
        );
        let mut frame = header.clone();
        frame.extend_from_slice(&[0x01, 0x02]);
        let err = parse_adts_frame(&frame).expect_err(&format!("profile {profile} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// sampling_frequency_index 13 / 14 / 15 は拒否する
#[test]
fn reject_adts_reserved_frequency_index() {
    for index in [13u8, 14, 15] {
        let header = build_adts_header(
            0xFFF, 0, 0, 1, 1, index, 0, 2, 0, 0, 0, 0, 9, 0x7FF, 0, None,
        );
        let mut frame = header.clone();
        frame.extend_from_slice(&[0x01, 0x02]);
        let err = parse_adts_frame(&frame).expect_err(&format!("sfi {index} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// channel_configuration 0 は拒否する
///
/// ADTS の channel_configuration は 3 ビットフィールドなので 8..=15 は表現できず、
/// 到達しうる不正値は 0 のみ
#[test]
fn reject_adts_invalid_channel_configuration() {
    let channel = 0u8;
    let header = build_adts_header(
        0xFFF, 0, 0, 1, 1, 3, 0, channel, 0, 0, 0, 0, 9, 0x7FF, 0, None,
    );
    let mut frame = header.clone();
    frame.extend_from_slice(&[0x01, 0x02]);
    let err = parse_adts_frame(&frame).expect_err(&format!("channel {channel} は拒否される"));
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// number_of_raw_data_blocks_in_frame != 0 は拒否する
#[test]
fn reject_adts_multiple_raw_data_blocks() {
    for nrdb in [1u32, 3] {
        let header =
            build_adts_header(0xFFF, 0, 0, 1, 1, 3, 0, 2, 0, 0, 0, 0, 9, 0x7FF, nrdb, None);
        let mut frame = header.clone();
        frame.extend_from_slice(&[0x01, 0x02]);
        let err = parse_adts_frame(&frame).expect_err(&format!("nrdb {nrdb} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}

/// frame_length がヘッダー長未満は拒否する
#[test]
fn reject_adts_frame_length_below_header() {
    // protection_absent = 1 でヘッダー 7 バイトなのに frame_length = 6
    let header = build_adts_header(0xFFF, 0, 0, 1, 1, 3, 0, 2, 0, 0, 0, 0, 6, 0x7FF, 0, None);
    let err = parse_adts_frame(&header).expect_err("frame_length がヘッダー未満は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// frame_length が入力末尾を超える場合は拒否する
#[test]
fn reject_adts_frame_length_exceeds_input() {
    // frame_length = 12 なのに入力はヘッダー 7 + raw 2 = 9 バイト
    let header = build_adts_header(0xFFF, 0, 0, 1, 1, 3, 0, 2, 0, 0, 0, 0, 12, 0x7FF, 0, None);
    let mut frame = header.clone();
    frame.extend_from_slice(&[0x01, 0x02]);
    let err = parse_adts_frame(&frame).expect_err("frame_length 超過は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// ヘッダーの途中で入力が切れる場合は拒否する
#[test]
fn reject_adts_truncated_header() {
    let header = build_adts_header(0xFFF, 0, 0, 1, 1, 3, 0, 2, 0, 0, 0, 0, 9, 0x7FF, 0, None);
    let truncated = &header[..5];
    let err = parse_adts_frame(truncated).expect_err("切り詰められたヘッダーは拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

// ===== wrap_raw_aac_in_adts =====

/// raw AAC が 7 バイトヘッダー付き ADTS フレームになり、解析で意味が往復する
#[test]
fn wrap_raw_aac_in_adts_roundtrip() {
    let asc = asc_48k_stereo();
    let raw = [0xAA, 0xBB, 0xCC, 0xDD];
    let config = AdtsEncodeConfig {
        mpeg_version: AdtsMpegVersion::Mpeg2,
        original_copy: true,
        home: true,
    };
    let frame = wrap_raw_aac_in_adts(&raw, &asc, &config).expect("組み立て成功する");
    assert_eq!(frame.len(), 7 + raw.len());

    let (header, parsed_raw) = parse_adts_frame(&frame).expect("組み立てたフレームは解析成功する");
    assert_eq!(header.mpeg_version, AdtsMpegVersion::Mpeg2);
    assert!(header.protection_absent);
    assert_eq!(header.audio_object_type, 2);
    assert_eq!(header.sampling_frequency_index, 3);
    assert_eq!(header.channel_configuration, 2);
    assert_eq!(header.frame_length, 11);
    assert!(header.original_copy);
    assert!(header.home);
    assert_eq!(parsed_raw, &raw[..]);
}

/// 明示周波数 (index 0xF) の ASC からの組み立ては拒否する
#[test]
fn reject_wrap_adts_explicit_frequency() {
    let asc = AudioSpecificConfig {
        audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
        sampling_frequency: SamplingFrequency::Explicit { frequency: 44100 },
        channel_configuration: 2,
    };
    let err = wrap_raw_aac_in_adts(&[0x01], &asc, &default_adts_config())
        .expect_err("明示周波数の ASC は ADTS 化で拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 受理条件を満たさない ASC からの組み立ては拒否する
#[test]
fn reject_wrap_adts_invalid_asc() {
    let asc = AudioSpecificConfig {
        audio_object_type: 5,
        sampling_frequency: SamplingFrequency::Index { index: 3 },
        channel_configuration: 2,
    };
    let err = wrap_raw_aac_in_adts(&[0x01], &asc, &default_adts_config())
        .expect_err("AOT 5 の ASC は ADTS 化で拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// ヘッダー + raw が 13 ビット (8191) に収まらない組み立ては拒否する
#[test]
fn reject_wrap_adts_frame_length_overflow() {
    let asc = asc_48k_stereo();
    // 8191 - 7 + 1 = 8185 バイトで 8192 を超える
    let raw = vec![0u8; 8185];
    let err = wrap_raw_aac_in_adts(&raw, &asc, &default_adts_config())
        .expect_err("frame_length が 13 ビットを超える組み立ては拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

// ===== build_mp4a_box =====

/// 48 kHz stereo の ASC から Mp4aBox を構築し、固定値 / 導出値 / 呼び出し側指定値が正しい
#[test]
fn build_mp4a_box_fixed_and_derived_values() {
    let asc = asc_48k_stereo();
    let config = Mp4aSampleEntryConfig {
        es_id: 7,
        buffer_size_db: 65536,
        max_bitrate: 256000,
        avg_bitrate: 128000,
    };
    let mp4a = build_mp4a_box(&asc, &config).expect("有効な ASC は構築成功する");

    // ストリーム導出値
    assert_eq!(mp4a.audio.channelcount, 2);
    assert_eq!(mp4a.audio.samplerate.integer, 48000);
    assert_eq!(mp4a.audio.samplerate.fraction, 0);

    // 固定値
    assert_eq!(
        mp4a.audio.data_reference_index,
        AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX
    );
    assert_eq!(
        mp4a.audio.samplesize,
        AudioSampleEntryFields::DEFAULT_SAMPLESIZE
    );
    assert!(mp4a.unknown_boxes.is_empty());
    assert_eq!(
        mp4a.esds_box.es.stream_priority,
        shiguredo_mp4::descriptors::EsDescriptor::LOWEST_STREAM_PRIORITY
    );

    // 呼び出し側指定値
    assert_eq!(mp4a.esds_box.es.es_id, 7);
    assert_eq!(
        mp4a.esds_box.es.dec_config_descr.buffer_size_db.get(),
        65536
    );
    assert_eq!(mp4a.esds_box.es.dec_config_descr.max_bitrate, 256000);
    assert_eq!(mp4a.esds_box.es.dec_config_descr.avg_bitrate, 128000);

    // ASC payload は正規形
    let payload = mp4a
        .esds_box
        .es
        .dec_config_descr
        .dec_specific_info
        .as_ref()
        .expect("dec_specific_info は存在する")
        .payload
        .clone();
    assert_eq!(payload, [0x11, 0x90]);
    let reparsed = parse_audio_specific_config(&payload).expect("payload は再解析成功する");
    assert_eq!(reparsed, asc);
}

/// channel_configuration 7 (8 チャンネル) が channelcount 8 に写る
#[test]
fn build_mp4a_box_channel_7_maps_to_8_channels() {
    let asc = AudioSpecificConfig {
        audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
        sampling_frequency: SamplingFrequency::Index { index: 3 },
        channel_configuration: 7,
    };
    let mp4a = build_mp4a_box(&asc, &default_mp4a_config()).expect("channel 7 は構築成功する");
    assert_eq!(mp4a.audio.channelcount, 8);
}

/// 96 kHz (index 0) の構築で samplerate は 0 になり、真値は ASC payload に残る
///
/// `samplerate` (u16 固定小数点) に 96000 は収まらないため切り捨てず 0 にする
#[test]
fn build_mp4a_box_96khz_samplerate_zero() {
    let asc = AudioSpecificConfig {
        audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
        sampling_frequency: SamplingFrequency::Index { index: 0 },
        channel_configuration: 2,
    };
    let mp4a = build_mp4a_box(&asc, &default_mp4a_config()).expect("96 kHz は構築成功する");
    assert_eq!(mp4a.audio.samplerate.integer, 0);
    assert_eq!(mp4a.audio.samplerate.fraction, 0);
    let payload = mp4a
        .esds_box
        .es
        .dec_config_descr
        .dec_specific_info
        .as_ref()
        .expect("dec_specific_info は存在する")
        .payload
        .clone();
    let reparsed = parse_audio_specific_config(&payload).expect("payload は再解析成功する");
    assert_eq!(
        reparsed.sampling_frequency.hz().expect("index 0 は有効"),
        96000
    );
}

/// 明示周波数が 65535 を超える ASC の構築で samplerate は 0 になる
#[test]
fn build_mp4a_box_explicit_frequency_over_u16() {
    let asc = AudioSpecificConfig {
        audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
        sampling_frequency: SamplingFrequency::Explicit { frequency: 70000 },
        channel_configuration: 2,
    };
    let mp4a =
        build_mp4a_box(&asc, &default_mp4a_config()).expect("明示周波数 70000 は構築成功する");
    assert_eq!(mp4a.audio.samplerate.integer, 0);
}

/// es_id が 0 (予約) は拒否する
#[test]
fn reject_build_mp4a_box_zero_es_id() {
    let asc = asc_48k_stereo();
    let config = Mp4aSampleEntryConfig {
        es_id: 0,
        ..default_mp4a_config()
    };
    let err = build_mp4a_box(&asc, &config).expect_err("es_id 0 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// buffer_size_db が 24 ビットを超える値は拒否する (黙って切り捨てない)
#[test]
fn reject_build_mp4a_box_buffer_size_db_overflow() {
    let asc = asc_48k_stereo();
    let config = Mp4aSampleEntryConfig {
        buffer_size_db: 0x0100_0000,
        ..default_mp4a_config()
    };
    let err = build_mp4a_box(&asc, &config).expect_err("buffer_size_db 24 ビット超過は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 受理条件を満たさない ASC (範囲外 index) は構築で拒否する
#[test]
fn reject_build_mp4a_box_invalid_asc() {
    let asc = AudioSpecificConfig {
        audio_object_type: AUDIO_OBJECT_TYPE_AAC_LC,
        sampling_frequency: SamplingFrequency::Index { index: 13 }, // 範囲外
        channel_configuration: 2,
    };
    let err = build_mp4a_box(&asc, &default_mp4a_config()).expect_err("範囲外 index は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// 構築した Mp4aBox が encode → decode でラウンドトリップする
#[test]
fn build_mp4a_box_roundtrip() {
    let asc = asc_48k_stereo();
    let mp4a = build_mp4a_box(&asc, &default_mp4a_config()).expect("構築成功する");
    let encoded = mp4a.encode_to_vec().expect("encode 成功");
    let (decoded, size) = Mp4aBox::decode(&encoded).expect("decode 成功");
    assert_eq!(size, encoded.len());
    assert_eq!(decoded, mp4a);
}

// ===== 実 ADTS fixture テスト =====

/// ffmpeg で `beep-aac-audio.mp4` から抽出した ADTS ストリームの先頭フレームを解析できる
///
/// 生成コマンド:
///
/// ```text
/// ffmpeg -y -i tests/testdata/beep-aac-audio.mp4 \
///     -vn -c copy tests/testdata/beep-aac-audio.aac
/// ```
///
/// (mp4 内の raw AAC サンプルを ADTS muxer で書き出す。音声は AAC-LC / 44.1 kHz / mono)
const REAL_ADTS: &[u8] = include_bytes!("testdata/beep-aac-audio.aac");

/// 実 ffmpeg 出力の ADTS ストリーム先頭フレームを解析できる
#[test]
fn real_adts_first_frame_parses() {
    let (header, raw) = parse_adts_frame(REAL_ADTS).expect("実 ADTS フレームは解析成功する");
    assert_eq!(header.mpeg_version, AdtsMpegVersion::Mpeg4);
    assert!(header.protection_absent);
    assert_eq!(header.audio_object_type, 2);
    // beep-aac-audio.mp4 の音声は 44.1 kHz / mono (AAC-LC)
    assert_eq!(header.sampling_frequency_index, 4);
    assert_eq!(header.channel_configuration, 1);
    assert_eq!(header.frame_length as usize, 7 + raw.len());
    assert!(!raw.is_empty(), "raw AAC ペイロードが空でないこと");
}

/// 実 ADTS ストリームの全フレームを先頭から順に解析できる (途中で止まらない)
#[test]
fn real_adts_all_frames_parse() {
    let mut offset = 0;
    let mut count = 0;
    while offset < REAL_ADTS.len() {
        let (header, raw) =
            parse_adts_frame(&REAL_ADTS[offset..]).expect("各 ADTS フレームは解析成功する");
        assert_eq!(header.sampling_frequency_index, 4);
        assert_eq!(header.channel_configuration, 1);
        offset += header.frame_length as usize;
        count += 1;
        assert!(!raw.is_empty(), "各フレームに raw AAC ペイロードがあること");
    }
    assert!(count >= 2, "fixture に複数フレームが含まれること");
}
