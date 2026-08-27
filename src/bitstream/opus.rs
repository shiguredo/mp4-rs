//! Opus ビットストリーム処理ユーティリティ
//!
//! Opus の codec private 情報 (Ogg Opus identification header の一部) から、
//! ISOBMFF の固定値と `dOps` の対応関係を満たす `OpusBox` を構築する。
//!
//! 参照仕様は以下のとおり。
//!
//! - Encapsulation of Opus in ISO Base Media File Format
//!   (<https://www.opus-codec.org/docs/opus_in_isobmff.html>)
//!
//! # 対象外
//!
//! - `ChannelMappingFamily != 0` の multistream (`OutputChannelCount` が 3 以上)
//! - Ogg Opus identification header のパース (codec private 情報の解釈は利用側の責務)

use alloc::vec::Vec;

use crate::{
    Error, FixedPointNumber, Result,
    boxes::{AudioSampleEntryFields, DopsBox, OpusBox},
};

/// Audio Sample Entry の `samplerate` の固定値 (Hz)
///
/// 仕様は `samplerate` を `48000 << 16` と定める。Opus は内部で常に 48 kHz で
/// デコードするため、エンコード前の周波数 (`dOps` の `InputSampleRate`) とは
/// 別の意味であり混同しない
const SAMPLE_RATE_HZ: u16 = 48000;

/// `OpusBox` 構築時に呼び出し側が指定する値
///
/// 固定値 (data reference index / samplesize / samplerate = 48000 Hz /
/// `ChannelMappingFamily = 0` / 空 `unknown_boxes`) は含まない
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpusSampleEntryConfig {
    /// デコード後チャンネル数 (`dOps` の `OutputChannelCount` と
    /// `AudioSampleEntryFields::channelcount` に写る)
    ///
    /// 現行 `DopsBox` が固定する `ChannelMappingFamily = 0` は mono / stereo の
    /// family なので 1 または 2 だけを受理する
    pub output_channel_count: u8,
    /// 出力先頭で捨てるサンプル数 (48 kHz 基準。`dOps` の `PreSkip` に写る)
    pub pre_skip: u16,
    /// エンコード前のオリジナルのサンプリングレート (Hz。`dOps` の
    /// `InputSampleRate` に写る。参考値であり再生には影響しない)
    pub input_sample_rate: u32,
    /// 出力ゲイン (Q7.8 固定小数点の dB。`dOps` の `OutputGain` に写る)
    pub output_gain: i16,
}

/// codec private 情報と設定から [`OpusBox`] を 1 つ構築する
///
/// [`SampleEntry`](crate::boxes::SampleEntry) には包まない。`dOps` ボックスは
/// [`OpusBox::dops_box`] として中に置く。
///
/// # 固定値
///
/// - [`AudioSampleEntryFields::data_reference_index`] =
///   [`AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX`]
/// - [`AudioSampleEntryFields::samplesize`] =
///   [`AudioSampleEntryFields::DEFAULT_SAMPLESIZE`] (16)
/// - [`AudioSampleEntryFields::samplerate`] = 48000 Hz
/// - [`OpusBox::unknown_boxes`] = 空 `Vec`
/// - `DopsBox` の `ChannelMappingFamily` = 0 (mono / stereo)
///
/// # 呼び出し側指定値
///
/// [`OpusSampleEntryConfig`] の各フィールドを参照。`AudioSampleEntryFields::channelcount`
/// は `output_channel_count` と一致する。
///
/// # エラー条件
///
/// `config.output_channel_count` が 1 / 2 以外 (0、または 3 以上。対応していない
/// multistream の box は生成しない) で [`crate::Error`] を返す (panic はしない)
pub fn build_opus_box(config: &OpusSampleEntryConfig) -> Result<OpusBox> {
    // ChannelMappingFamily = 0 は mono / stereo の family なので 1 / 2 以外は拒否する
    if config.output_channel_count != 1 && config.output_channel_count != 2 {
        return Err(Error::invalid_input(
            "output_channel_count must be 1 or 2 (ChannelMappingFamily = 0)",
        ));
    }

    Ok(OpusBox {
        audio: AudioSampleEntryFields {
            data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            channelcount: u16::from(config.output_channel_count),
            samplesize: AudioSampleEntryFields::DEFAULT_SAMPLESIZE,
            samplerate: FixedPointNumber::new(SAMPLE_RATE_HZ, 0),
        },
        dops_box: DopsBox {
            output_channel_count: config.output_channel_count,
            pre_skip: config.pre_skip,
            input_sample_rate: config.input_sample_rate,
            output_gain: config.output_gain,
        },
        unknown_boxes: Vec::new(),
    })
}
