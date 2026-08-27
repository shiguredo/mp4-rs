//! `shiguredo_mp4::bitstream::opus` の決定的テスト
//!
//! mono / stereo の `OpusBox` 構築結果の固定値・設定値の写り込みと、対応しない
//! チャンネル数の拒否を固定する

use shiguredo_mp4::{
    Decode, Encode, ErrorKind, FixedPointNumber,
    bitstream::opus::{OpusSampleEntryConfig, build_opus_box},
    boxes::{AudioSampleEntryFields, OpusBox},
};

/// mono 構築用の有効な設定
fn mono_config() -> OpusSampleEntryConfig {
    OpusSampleEntryConfig {
        output_channel_count: 1,
        pre_skip: 312,
        input_sample_rate: 48000,
        output_gain: 0,
    }
}

/// stereo 構築用の有効な設定
fn stereo_config() -> OpusSampleEntryConfig {
    OpusSampleEntryConfig {
        output_channel_count: 2,
        pre_skip: 312,
        input_sample_rate: 48000,
        output_gain: 0,
    }
}

// ===== build_opus_box: 受理系 =====

/// mono の構築結果が固定値と設定値の写り込みを満たす
#[test]
fn build_opus_box_mono_fixed_and_config_values() {
    let opus = build_opus_box(&mono_config()).expect("mono は構築成功する");

    // 固定値
    assert_eq!(
        opus.audio.data_reference_index,
        AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX
    );
    assert_eq!(
        opus.audio.samplesize,
        AudioSampleEntryFields::DEFAULT_SAMPLESIZE
    );
    assert_eq!(opus.audio.samplerate, FixedPointNumber::new(48000u16, 0));
    assert!(
        opus.unknown_boxes.is_empty(),
        "unknown_boxes は空であるべき"
    );

    // 設定値の写り込み
    assert_eq!(opus.audio.channelcount, 1);
    assert_eq!(opus.dops_box.output_channel_count, 1);
    assert_eq!(opus.dops_box.pre_skip, 312);
    assert_eq!(opus.dops_box.input_sample_rate, 48000);
    assert_eq!(opus.dops_box.output_gain, 0);
}

/// stereo の構築結果が固定値と設定値の写り込みを満たす
#[test]
fn build_opus_box_stereo_fixed_and_config_values() {
    let opus = build_opus_box(&stereo_config()).expect("stereo は構築成功する");

    // 固定値
    assert_eq!(
        opus.audio.data_reference_index,
        AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX
    );
    assert_eq!(
        opus.audio.samplesize,
        AudioSampleEntryFields::DEFAULT_SAMPLESIZE
    );
    assert_eq!(opus.audio.samplerate, FixedPointNumber::new(48000u16, 0));
    assert!(
        opus.unknown_boxes.is_empty(),
        "unknown_boxes は空であるべき"
    );

    // 設定値の写り込み
    assert_eq!(opus.audio.channelcount, 2);
    assert_eq!(opus.dops_box.output_channel_count, 2);
    assert_eq!(opus.dops_box.pre_skip, 312);
    assert_eq!(opus.dops_box.input_sample_rate, 48000);
    assert_eq!(opus.dops_box.output_gain, 0);
}

/// 非既定値の pre_skip / input_sample_rate / output_gain が dOps に写る
///
/// Hisui 固有の固定値 (stereo / 48 kHz input / gain 0) は本 API では固定しない
#[test]
fn build_opus_box_non_default_config_values_preserved() {
    let config = OpusSampleEntryConfig {
        output_channel_count: 2,
        pre_skip: 65535,
        input_sample_rate: 44100,
        output_gain: -512,
    };
    let opus = build_opus_box(&config).expect("有効な設定は構築成功する");
    assert_eq!(opus.audio.channelcount, 2);
    assert_eq!(opus.dops_box.output_channel_count, 2);
    assert_eq!(opus.dops_box.pre_skip, 65535);
    assert_eq!(opus.dops_box.input_sample_rate, 44100);
    assert_eq!(opus.dops_box.output_gain, -512);
}

/// mono (pre_skip = 312 / 48 kHz / gain 0) が仕様どおりのバイト列にエンコードされる
///
/// OpusSampleEntry (AudioSampleEntry 28 バイト + `dOps` 19 バイト) の構造を固定する。
/// `dOps` の `ChannelMappingFamily` は 0 で、mono は `OutputChannelCount = 1` になる
#[test]
fn build_opus_box_mono_encode_bytes() {
    let opus = build_opus_box(&mono_config()).expect("mono は構築成功する");
    let encoded = opus.encode_to_vec().expect("encode 成功");
    let expected = [
        0x00, 0x00, 0x00, 0x37, b'O', b'p', b'u', b's', // OpusBox ヘッダー (size 55)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // reserved(6) / data_reference_index
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // reserved(8)
        0x00, 0x01, 0x00, 0x10, 0x00, 0x00, 0x00,
        0x00, // channelcount / samplesize / pre_defined / reserved
        0xBB, 0x80, 0x00, 0x00, // samplerate 48000 (48000 << 16)
        0x00, 0x00, 0x00, 0x13, b'd', b'O', b'p', b's', // dOps ヘッダー (size 19)
        0x00, 0x01, 0x01, 0x38, 0x00, 0x00, 0xBB,
        0x80, // version / output / pre_skip / input_sample_rate
        0x00, 0x00, 0x00, // output_gain / channel_mapping_family
    ];
    assert_eq!(
        encoded, expected,
        "mono の OpusBox は仕様どおりのバイト列になるべき"
    );
}

/// 構築した OpusBox が encode → decode でラウンドトリップする
#[test]
fn build_opus_box_roundtrip() {
    let opus = build_opus_box(&stereo_config()).expect("構築成功する");
    let encoded = opus.encode_to_vec().expect("encode 成功");
    let (decoded, size) = OpusBox::decode(&encoded).expect("decode 成功");
    assert_eq!(size, encoded.len());
    assert_eq!(decoded, opus);
}

// ===== build_opus_box: 拒否系 =====

/// output_channel_count 0 は拒否する
#[test]
fn reject_build_opus_box_zero_channel_count() {
    let config = OpusSampleEntryConfig {
        output_channel_count: 0,
        ..mono_config()
    };
    let err = build_opus_box(&config).expect_err("チャンネル数 0 は拒否される");
    assert_eq!(err.kind, ErrorKind::InvalidInput);
}

/// output_channel_count 3 以上 (multistream) は拒否する
///
/// `ChannelMappingFamily = 0` は mono / stereo の family なので、対応していない
/// multistream の box を生成しない
#[test]
fn reject_build_opus_box_multistream_channel_count() {
    for count in [3u8, 4, 8, 255] {
        let config = OpusSampleEntryConfig {
            output_channel_count: count,
            ..mono_config()
        };
        let err = build_opus_box(&config).expect_err(&format!("チャンネル数 {count} は拒否される"));
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}
