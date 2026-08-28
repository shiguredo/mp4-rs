//! Opus ビットストリーム処理ユーティリティ
//!
//! codec private 情報 (Ogg Opus identification header の一部) の各フィールドを
//! [`OpusSampleEntryConfig`] で指定して、ISOBMFF の固定値と `dOps` の対応関係を
//! 満たす `OpusBox` を構築する。
//!
//! 参照仕様は以下のとおり。
//!
//! - Encapsulation of Opus in ISO Base Media File Format
//!   (<https://www.opus-codec.org/docs/opus_in_isobmff.html>)
//!
//! # 対象外
//!
//! - `ChannelMappingFamily != 0` の multistream (3 チャンネル以上)
//! - Ogg Opus identification header のパース (codec private 情報の解釈は利用側の責務)

use alloc::vec::Vec;

use crate::{
    FixedPointNumber,
    boxes::{AudioSampleEntryFields, DopsBox, OpusBox},
};

/// Audio Sample Entry の `samplerate` の固定値 (Hz)
///
/// 仕様は `samplerate` を `48000 << 16` と定める。Opus は内部で常に 48 kHz で
/// デコードするため、エンコード前の周波数 (`dOps` の `InputSampleRate`) とは
/// 別の意味であり混同しない
const SAMPLE_RATE_HZ: u16 = 48000;

/// Opus のデコード後チャンネル数
///
/// 現行 `DopsBox` が固定する `ChannelMappingFamily = 0` は mono / stereo の
/// family なので 1 / 2 のみを表現する。対応していない multistream の box を
/// 生成できないよう、不正なチャンネル数は型として存在しない
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelCount {
    /// 1 チャンネル (mono)
    Mono,
    /// 2 チャンネル (stereo)
    Stereo,
}

impl ChannelCount {
    /// `dOps` の `OutputChannelCount` と `AudioSampleEntryFields::channelcount` に写る値
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }
}

/// `OpusBox` 構築時に呼び出し側が指定する値
///
/// 固定値 (data reference index / samplesize / samplerate = 48000 Hz /
/// `ChannelMappingFamily = 0` / 空 `unknown_boxes`) は含まない
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpusSampleEntryConfig {
    /// デコード後チャンネル数 (`dOps` の `OutputChannelCount` と
    /// `AudioSampleEntryFields::channelcount` に写る)
    pub channel_count: ChannelCount,
    /// 出力先頭で捨てるサンプル数 (48 kHz 基準。`dOps` の `PreSkip` に写る)
    pub pre_skip: u16,
    /// エンコード前のオリジナルのサンプリングレート (Hz。`dOps` の
    /// `InputSampleRate` に写る。参考値であり再生には影響しない)
    pub input_sample_rate: u32,
    /// 出力ゲイン (Q7.8 固定小数点の dB。`dOps` の `OutputGain` に写る)
    pub output_gain: i16,
}

/// codec private 相当の各フィールドを [`OpusSampleEntryConfig`] で指定して
/// [`OpusBox`] を 1 つ構築する
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
/// は `channel_count` と一致する。全フィールドが型で受理条件を保証されるため、
/// エラーを返すことはない
pub fn build_opus_box(config: &OpusSampleEntryConfig) -> OpusBox {
    OpusBox {
        audio: AudioSampleEntryFields {
            data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            channelcount: u16::from(config.channel_count.as_u8()),
            samplesize: AudioSampleEntryFields::DEFAULT_SAMPLESIZE,
            samplerate: FixedPointNumber::new(SAMPLE_RATE_HZ, 0),
        },
        dops_box: DopsBox {
            output_channel_count: config.channel_count.as_u8(),
            pre_skip: config.pre_skip,
            input_sample_rate: config.input_sample_rate,
            output_gain: config.output_gain,
        },
        unknown_boxes: Vec::new(),
    }
}
