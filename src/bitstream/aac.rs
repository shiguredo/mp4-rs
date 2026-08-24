//! AAC ビットストリーム処理ユーティリティ
//!
//! AAC-LC (AOT 2) の AudioSpecificConfig (以下 ASC) と ADTS ヘッダーを解析し、
//! `mp4a` / `esds` の構築、および ADTS と raw AAC の相互変換を提供する。
//!
//! 参照仕様は以下のとおり。
//!
//! - ISO/IEC 14496-3 (MPEG-4 Audio、ASC と GASpecificConfig)
//! - ISO/IEC 13818-7 (ADTS)
//!
//! # 対象外
//!
//! - SBR / PS (explicit / implicit) の解析と存在検出
//! - `dependsOnCoreCoder == 1` / `extensionFlag == 1` / PCE の構文解析
//! - ADTS の複数 raw data block と CRC の生成・値検証
//! - LATM / LOAS

use alloc::vec::Vec;

use crate::{
    Error, Result, Uint,
    boxes::{AudioSampleEntryFields, EsdsBox, Mp4aBox},
    descriptors::{DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor},
};

/// AAC の Audio Object Type (AOT)
///
/// 本モジュールは AOT 2 (AAC-LC) のみを受理するため、現状は単一の variant のみを持つ。
/// HE-AAC (SBR / PS) 対応を足す場合は variant を追加する
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioObjectType {
    /// AOT 2 (AAC-LC)
    AacLc,
}

impl AudioObjectType {
    /// ビットストリーム上の生の AOT 値
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::AacLc => 2,
        }
    }
}

/// ADTS の `profile` (2 ビット)。AOT は `profile + 1` なので AOT 2 では常に 1
const ADTS_PROFILE_AAC_LC: u32 = 1;

/// ADTS の `layer` の固定値 (常に 0)
const ADTS_LAYER_ZERO: u32 = 0;

/// ADTS の `adts_buffer_fullness` の固定値 (VBR 慣習値の `0x7FF`)
const ADTS_BUFFER_FULLNESS_VBR: u32 = 0x7FF;

/// ADTS ヘッダーの `protection_absent == 1` のときのヘッダー長 (CRC なし)
const ADTS_HEADER_SIZE_NO_CRC: usize = 7;

/// ADTS ヘッダーの `protection_absent == 0` のときのヘッダー長 (CRC 16 ビット付き)
const ADTS_HEADER_SIZE_WITH_CRC: usize = 9;

/// ADTS の `frame_length` (13 ビット) の最大値
const ADTS_FRAME_LENGTH_MAX: u32 = (1 << 13) - 1;

/// ASC の `samplingFrequencyIndex` が `0xF` のときに後続 24 ビットで示す明示周波数の最大値
const EXPLICIT_SAMPLING_FREQUENCY_MAX: u32 = 0xFF_FFFF;

/// `samplingFrequencyIndex` (0..=12) に対応する実効サンプリング周波数 (Hz)
///
/// ISO/IEC 14496-3 の sampling frequency の表に対応する
const SAMPLING_FREQUENCIES: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];

/// ASC のサンプリング周波数
///
/// 標準テーブル (index 0..=12) に対応する周波数はビットストリーム上
/// `samplingFrequencyIndex` で表され、それ以外の周波数は index `0xF` + 24 ビットの
/// 明示値で表される。この 2 形式は内部で保持し、利用者は [`Self::hz()`] で実効
/// 周波数だけを扱う。ビットストリーム上の形式 (index / 明示) は [`Self::from_hz()`]
/// と `parse_audio_specific_config` が正規形を自動選択する
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SamplingFrequency {
    /// 標準テーブルの index。`Some(0..=12)` なら index 形式、`None` なら明示形式
    index: Option<u8>,
    /// 実効周波数 (Hz)。`index` が `Some` のときは対応表の値と一致する (不変条件)
    frequency: u32,
}

impl SamplingFrequency {
    /// 実効サンプリング周波数 (Hz) から生成する
    ///
    /// 標準テーブル (index 0..=12) に一致する Hz は index 形式 (正規形 2 バイト)、
    /// それ以外の Hz は明示形式 (5 バイト) になる。利用者が index の対応表を
    /// 知らなくても正規形を選べるようにするためのコンストラクタ。
    ///
    /// # エラー条件
    ///
    /// `hz` が 0、または 24 ビット (1..=16777215) を超える場合に [`crate::Error`] を
    /// 返す (panic はしない)
    pub fn from_hz(hz: u32) -> Result<Self> {
        if hz == 0 || hz > EXPLICIT_SAMPLING_FREQUENCY_MAX {
            return Err(Error::invalid_input(
                "sampling frequency must be 1..=16777215",
            ));
        }
        if let Some(index) = SAMPLING_FREQUENCIES.iter().position(|f| *f == hz) {
            Ok(Self {
                index: Some(index as u8),
                frequency: hz,
            })
        } else {
            Ok(Self {
                index: None,
                frequency: hz,
            })
        }
    }

    /// 実効サンプリング周波数 (Hz)
    ///
    /// index 形式は対応表 (index 0..=12) の値、明示形式は保持値を返す。本構造体は
    /// [`Self::from_hz()`] / `parse_audio_specific_config` が不変条件を保証するため
    /// 常に有効な値を返す
    pub fn hz(self) -> u32 {
        self.frequency
    }

    /// ADTS 組み立て用にビットストリーム上の `samplingFrequencyIndex` を返す
    ///
    /// 明示形式 (ADTS に表現できない) のときは [`crate::Error`]
    pub(crate) fn sampling_frequency_index(self) -> Result<u8> {
        match self.index {
            Some(index) => Ok(index),
            None => Err(Error::invalid_input(
                "ADTS cannot represent an explicit sampling frequency (index 0xF)",
            )),
        }
    }
}

/// AAC のチャンネル構成
///
/// ISO/IEC 14496-3 の `channelConfiguration` (1..=7) に対応する。0 (PCE) と
/// 8..=15 (reserved) は本モジュールが受理しないため variant として存在しない
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelConfiguration {
    /// 1 チャンネル (mono)
    Mono,
    /// 2 チャンネル (stereo)
    Stereo,
    /// 3 チャンネル
    Channels3,
    /// 4 チャンネル
    Channels4,
    /// 5 チャンネル
    Channels5,
    /// 6 チャンネル (5.1)
    FivePointOne,
    /// 8 チャンネル (7.1)。`channelConfiguration` の 7 に対応する
    SevenPointOne,
}

impl ChannelConfiguration {
    /// ビットストリーム上の `channelConfiguration` 値
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Channels3 => 3,
            Self::Channels4 => 4,
            Self::Channels5 => 5,
            Self::FivePointOne => 6,
            Self::SevenPointOne => 7,
        }
    }

    /// チャンネル数
    ///
    /// [`Self::SevenPointOne`] だけ 8 で、`channelConfiguration` の値と一致しない
    pub const fn channel_count(self) -> u16 {
        match self {
            Self::SevenPointOne => 8,
            _ => self.as_u8() as u16,
        }
    }
}

/// 受理した AAC-LC の AudioSpecificConfig
///
/// `parse_audio_specific_config` が受理する構造化値で、`encode_audio_specific_config` が
/// 正規形バイト列へ戻す。全フィールドは型で受理条件が保証されるため、手組みでも
/// 不正な値を構築できない
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AudioSpecificConfig {
    /// 常に [`AudioObjectType::AacLc`]
    pub audio_object_type: AudioObjectType,
    /// サンプリング周波数
    pub sampling_frequency: SamplingFrequency,
    /// チャンネル構成
    pub channel_configuration: ChannelConfiguration,
}

/// ADTS ヘッダーの MPEG バージョン (ID ビット)
///
/// ISO/IEC 13818-7 の `ID` ビットに対応する。AAC の raw data block はどちらの
/// バージョンでも同一で、組み立て時に呼び出し側が選択する
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdtsMpegVersion {
    /// MPEG-4 (`ID = 0`)
    Mpeg4,
    /// MPEG-2 (`ID = 1`)
    Mpeg2,
}

/// 解析済みの ADTS ヘッダー
///
/// `parse_adts_frame` が返す解析結果全体を保持する。再組み立てに使うのは
/// `mpeg_version` / `original_copy` / `home` と、ASC 側の周波数 index・チャンネルで、
/// `wrap_raw_aac_in_adts` は本構造体を受け取らない
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdtsHeader {
    /// MPEG バージョン (ID ビット)
    pub mpeg_version: AdtsMpegVersion,
    /// `protection_absent`。`true` なら CRC なし (7 バイト)、`false` なら CRC 付き (9 バイト)
    pub protection_absent: bool,
    /// 常に [`AudioObjectType::AacLc`]
    pub audio_object_type: AudioObjectType,
    /// 0..=12
    pub sampling_frequency_index: u8,
    /// チャンネル構成
    pub channel_configuration: ChannelConfiguration,
    /// ヘッダー込みのフレーム長 (バイト)
    pub frame_length: u16,
    /// `original_copy` ビット
    pub original_copy: bool,
    /// `home` ビット
    pub home: bool,
}

/// ADTS フレーム組み立て時に呼び出し側が指定する値
///
/// 組み立て時に固定される値 (layer = 0 / private_bit = 0 / copyright ビット = 0 /
/// `adts_buffer_fullness` = `0x7FF` / `number_of_raw_data_blocks_in_frame` = 0 /
/// `protection_absent` = 1) は含まない
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdtsEncodeConfig {
    /// MPEG バージョン (ID ビット)
    pub mpeg_version: AdtsMpegVersion,
    /// `original_copy` ビット
    pub original_copy: bool,
    /// `home` ビット
    pub home: bool,
}

/// `Mp4aBox` 構築時に呼び出し側が指定する値
///
/// 固定値 (data reference index / samplesize / stream priority / 空 `unknown_boxes` /
/// デコーダー設定の object type・stream type など) と、ASC から写すストリーム導出値
/// (channelcount / samplerate / ASC payload) は含まない
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Mp4aSampleEntryConfig {
    /// `es_id` ([`EsDescriptor::MIN_ES_ID`] 以上。0 は予約なので拒否)
    pub es_id: u16,
    /// デコーダーバッファサイズ (バイト)。24 ビット (0..=16777215) に収まること
    pub buffer_size_db: u32,
    /// 最大ビットレート (bps)
    pub max_bitrate: u32,
    /// 平均ビットレート (bps。0 は「不明」を意味する)
    pub avg_bitrate: u32,
}

/// AAC-LC の AudioSpecificConfig を解析する
///
/// # 入力
///
/// `input` は `DecoderSpecificInfo::payload` などに入る ASC の生バイト列。
/// ビット列は MSB-first で、`audioObjectType` (5) / `samplingFrequencyIndex` (4、
/// `0xF` のときは後続 24 ビットの明示周波数) / `channelConfiguration` (4) /
/// GASpecificConfig 必須 3 フラグ (`frameLengthFlag` / `dependsOnCoreCoder` /
/// `extensionFlag`) の順に読む。
///
/// # 受理条件
///
/// - `audioObjectType` が 2 (AAC-LC)
/// - `samplingFrequencyIndex` が 0..=12、または `0xF` で明示周波数が 1..=16777215
/// - `channelConfiguration` が 1..=7
/// - GASpecificConfig 必須 3 フラグがすべて 0
/// - 上記を読み切った位置で入力が終端している
///
/// # エラー条件
///
/// 以下のいずれかで [`crate::Error`] を返す。
///
/// - 入力が ASC の途中で切れている
/// - `audioObjectType` が 2 以外 (31 のエスケープ形式を含む)
/// - `samplingFrequencyIndex` が 13 / 14 (reserved)、または `0xF` で明示周波数が 0
/// - `channelConfiguration` が 0 (PCE) または 8..=15 (reserved)
/// - GASpecificConfig 必須 3 フラグのいずれかが 1 (SBR / PS 等の拡張は AAC-LC として
///   黙って読み替えず拒否する)
/// - 読み切り位置以降に後続バイトがある (ゼロ埋めも含めて拒否。explicit SBR / PS 拡張は
///   この条件で拒否される)
pub fn parse_audio_specific_config(input: &[u8]) -> Result<AudioSpecificConfig> {
    let mut reader = BitReader::new(input);

    let audio_object_type = reader.read_bits(5)? as u8;
    // 対象は AOT 2 のみ。5 ビット値が 31 のエスケープ形式もここで拒否される
    if audio_object_type != AudioObjectType::AacLc.as_u8() {
        return Err(Error::invalid_input(
            "AAC audio object type must be 2 (AAC-LC)",
        ));
    }

    let sampling_frequency_index = reader.read_bits(4)? as u8;
    let sampling_frequency = match sampling_frequency_index {
        0..=12 => SamplingFrequency {
            index: Some(sampling_frequency_index),
            frequency: SAMPLING_FREQUENCIES[sampling_frequency_index as usize],
        },
        15 => {
            // index 0xF は後続 24 ビットの明示周波数 (Hz)
            let frequency = reader.read_bits(24)?;
            if frequency == 0 {
                return Err(Error::invalid_input(
                    "AAC explicit sampling frequency must not be 0",
                ));
            }
            SamplingFrequency {
                index: None,
                frequency,
            }
        }
        // index 13 / 14 は reserved
        _ => {
            return Err(Error::invalid_input(
                "AAC sampling frequency index 13 / 14 is reserved",
            ));
        }
    };

    let channel_configuration = reader.read_bits(4)? as u8;
    let channel_configuration =
        channel_configuration_from_raw(channel_configuration).ok_or_else(|| {
            // 0 は PCE、8..=15 は reserved。本モジュールでは受け入れない
            Error::invalid_input("AAC channel configuration must be 1..=7")
        })?;

    // GASpecificConfig の必須 3 フラグはすべて 0 のみ受理する。
    // 1 つでも 1 なら後続の coreCoderDelay や PCE を読まずに拒否する
    let frame_length_flag = reader.read_bit()?;
    let depends_on_core_coder = reader.read_bit()?;
    let extension_flag = reader.read_bit()?;
    if frame_length_flag != 0 || depends_on_core_coder != 0 || extension_flag != 0 {
        return Err(Error::invalid_input(
            "AAC GASpecificConfig flags (frameLengthFlag / dependsOnCoreCoder / extensionFlag) must be 0",
        ));
    }

    // 上記を読み切った位置で入力が終端していなければ、SBR / PS 等の後続拡張がある
    // 扱いで拒否する
    if !reader.is_at_end() {
        return Err(Error::invalid_input(
            "AAC AudioSpecificConfig has trailing bytes",
        ));
    }

    Ok(AudioSpecificConfig {
        audio_object_type: AudioObjectType::AacLc,
        sampling_frequency,
        channel_configuration,
    })
}

/// 受理条件を満たす [`AudioSpecificConfig`] を正規形バイト列へエンコードする
///
/// 出力は `audioObjectType` = 2 / 3 フラグ = 0 で、明示周波数 (index `0xF`) 以外は
/// 2 バイト、index `0xF` のときは 5 バイトになる。
///
/// 全フィールドが型で受理条件を保証されるため、エラーを返すことはない
/// (`AudioObjectType` / `SamplingFrequency` / `ChannelConfiguration` が不変条件を
/// 保持する)。受理した入力に対する `encode(parse(input))` は入力と一致する
pub fn encode_audio_specific_config(config: &AudioSpecificConfig) -> Vec<u8> {
    let mut writer = BitWriter::new();
    writer.push_bits(u32::from(config.audio_object_type.as_u8()), 5);
    match config.sampling_frequency.index {
        Some(index) => {
            writer.push_bits(u32::from(index), 4);
        }
        None => {
            writer.push_bits(15, 4);
            writer.push_bits(config.sampling_frequency.frequency, 24);
        }
    }
    writer.push_bits(u32::from(config.channel_configuration.as_u8()), 4);
    // GASpecificConfig 必須 3 フラグはすべて 0
    writer.push_bit(0);
    writer.push_bit(0);
    writer.push_bit(0);
    writer.into_bytes()
}

/// ADTS フレーム (ヘッダー + raw data block) を解析する
///
/// # 入力
///
/// `input` は ADTS フレーム 1 個分のバイト列 (ヘッダー 7 または 9 バイト + raw AAC)。
/// `frame_length` が `input` より短い場合は先頭から `frame_length` バイトまでを
/// フレームとして扱う。
///
/// # 受理条件
///
/// - syncword (`0xFFF`) が一致する
/// - `layer` が 0
/// - `profile` が 1 (AOT 2)
/// - `sampling_frequency_index` が 0..=12 (ADTS に 24 ビット明示周波数はない)
/// - `channel_configuration` が 1..=7
/// - `number_of_raw_data_blocks_in_frame` が 0 (raw data block 1 個)
/// - `frame_length` がヘッダー長以上かつ `input` の末尾を超えない
///
/// # エラー条件
///
/// 以下のいずれかで [`crate::Error`] を返す。
///
/// - 入力がヘッダーの途中で切れている
/// - syncword 不一致、`layer != 0`、`profile != 1`
/// - `sampling_frequency_index` が 13 / 14 / 15
/// - `channel_configuration` が 0、または 1..=7 の外 (3 ビットフィールドなので
///   到達しうるのは 0 のみ)
/// - `number_of_raw_data_blocks_in_frame` が 0 以外
/// - `frame_length` がヘッダー長未満、または `input` の末尾を超える
///
/// # CRC
///
/// `protection_absent == 0` (CRC 付き 9 バイトヘッダー) の場合、CRC 値の検証はせず
/// 読み飛ばす。CRC の生成は本モジュールでは行わない
pub fn parse_adts_frame(input: &[u8]) -> Result<(AdtsHeader, &[u8])> {
    let mut reader = BitReader::new(input);

    let syncword = reader.read_bits(12)?;
    if syncword != 0xFFF {
        return Err(Error::invalid_input("ADTS syncword must be 0xFFF"));
    }

    let mpeg_version = if reader.read_bit()? == 0 {
        AdtsMpegVersion::Mpeg4
    } else {
        AdtsMpegVersion::Mpeg2
    };

    let layer = reader.read_bits(2)?;
    if layer != ADTS_LAYER_ZERO {
        return Err(Error::invalid_input("ADTS layer must be 0"));
    }

    let protection_absent = reader.read_bit()? != 0;

    let profile = reader.read_bits(2)?;
    // AOT は profile + 1。本モジュールは AOT 2 のみなので profile 1 以外は拒否
    if profile != ADTS_PROFILE_AAC_LC {
        return Err(Error::invalid_input("ADTS profile must be 1 (AAC-LC)"));
    }

    let sampling_frequency_index = reader.read_bits(4)? as u8;
    // ADTS の sampling_frequency_index は 13 / 14 / 15 (reserved / 明示周波数なし) を拒否
    if sampling_frequency_index > 12 {
        return Err(Error::invalid_input(
            "ADTS sampling frequency index must be 0..=12",
        ));
    }

    // private_bit は組み立て側で固定 0 に書くが、解析では値を返さないため読み飛ばす
    let _private_bit = reader.read_bit()?;

    let channel_configuration = reader.read_bits(3)? as u8;
    let channel_configuration = channel_configuration_from_raw(channel_configuration)
        .ok_or_else(|| Error::invalid_input("ADTS channel configuration must be 1..=7"))?;

    let original_copy = reader.read_bit()? != 0;
    let home = reader.read_bit()? != 0;
    // copyright ビット 2 個は組み立て側で固定 0 に書くが、解析では返さない
    let _copyright_identification_bit = reader.read_bit()?;
    let _copyright_identification_start = reader.read_bit()?;

    let frame_length = reader.read_bits(13)? as u16;

    // adts_buffer_fullness は組み立て側で固定 0x7FF に書くが、解析では返さない
    let _adts_buffer_fullness = reader.read_bits(11)?;

    let number_of_raw_data_blocks_in_frame = reader.read_bits(2)?;
    // raw data block 1 個のみ受理する
    if number_of_raw_data_blocks_in_frame != 0 {
        return Err(Error::invalid_input(
            "ADTS number_of_raw_data_blocks_in_frame must be 0",
        ));
    }

    let header_size = if protection_absent {
        ADTS_HEADER_SIZE_NO_CRC
    } else {
        ADTS_HEADER_SIZE_WITH_CRC
    };
    if frame_length < header_size as u16 {
        return Err(Error::invalid_input(
            "ADTS frame_length must not be smaller than the header size",
        ));
    }
    let frame_end = frame_length as usize;
    if frame_end > input.len() {
        return Err(Error::invalid_input(
            "ADTS frame_length exceeds the input length",
        ));
    }

    Ok((
        AdtsHeader {
            mpeg_version,
            protection_absent,
            audio_object_type: AudioObjectType::AacLc,
            sampling_frequency_index,
            channel_configuration,
            frame_length,
            original_copy,
            home,
        },
        &input[header_size..frame_end],
    ))
}

/// raw AAC と [`AudioSpecificConfig`] から ADTS フレームを組み立てる
///
/// # 組み立て固定値
///
/// - `layer` = 0
/// - `protection_absent` = 1 (CRC なし。CRC 生成は対象外)
/// - `profile` = 1 (AOT 2)
/// - `private_bit` = 0
/// - copyright 識別ビット (2 ビット) = 0
/// - `adts_buffer_fullness` = `0x7FF` (VBR 慣習値)
/// - `number_of_raw_data_blocks_in_frame` = 0
///
/// ヘッダーは `ADTS_HEADER_SIZE_NO_CRC` (7) バイト + `raw` の長さになる。
///
/// # エラー条件
///
/// 以下のいずれかで [`crate::Error`] を返す。
///
/// - `asc.sampling_frequency` が明示形式 (ADTS に 24 ビット明示周波数はない)
/// - ヘッダー + `raw` の長さが ADTS の `frame_length` (13 ビット、最大 8191) に収まらない
pub fn wrap_raw_aac_in_adts(
    raw: &[u8],
    asc: &AudioSpecificConfig,
    config: &AdtsEncodeConfig,
) -> Result<Vec<u8>> {
    // ADTS に 24 ビット明示周波数は存在しないため明示形式からの変換は拒否する
    let sampling_frequency_index = asc.sampling_frequency.sampling_frequency_index()?;

    // 7 + raw の長さが 13 ビット (最大 8191) に収まることを検査する。
    // 加算は checked_add で 32-bit ターゲットでのオーバーフローを避け、
    // 比較も usize のまま行う (as u32 では 4 GiB 超で切り捨てにより検査をすり抜ける)
    let frame_length = ADTS_HEADER_SIZE_NO_CRC
        .checked_add(raw.len())
        .ok_or_else(|| Error::invalid_input("ADTS frame_length overflow"))?;
    if frame_length > ADTS_FRAME_LENGTH_MAX as usize {
        return Err(Error::invalid_input(
            "ADTS frame_length (header + raw) does not fit in 13 bits",
        ));
    }

    let mut writer = BitWriter::new();
    writer.push_bits(0xFFF, 12); // syncword
    let id = match config.mpeg_version {
        AdtsMpegVersion::Mpeg4 => 0,
        AdtsMpegVersion::Mpeg2 => 1,
    };
    writer.push_bit(id); // ID (MPEG バージョン)
    writer.push_bits(ADTS_LAYER_ZERO, 2);
    writer.push_bit(1); // protection_absent = 1 (CRC なし)
    writer.push_bits(ADTS_PROFILE_AAC_LC, 2);
    writer.push_bits(u32::from(sampling_frequency_index), 4);
    writer.push_bit(0); // private_bit
    writer.push_bits(u32::from(asc.channel_configuration.as_u8()), 3);
    writer.push_bit(u8::from(config.original_copy));
    writer.push_bit(u8::from(config.home));
    writer.push_bit(0); // copyright_identification_bit
    writer.push_bit(0); // copyright_identification_start
    writer.push_bits(frame_length as u32, 13);
    writer.push_bits(ADTS_BUFFER_FULLNESS_VBR, 11);
    writer.push_bits(0, 2); // number_of_raw_data_blocks_in_frame = 0

    let mut out = writer.into_bytes();
    out.extend_from_slice(raw);
    Ok(out)
}

/// 解析済み [`AudioSpecificConfig`] と呼び出し側設定から [`Mp4aBox`] を 1 つ構築する
///
/// [`SampleEntry`](crate::boxes::SampleEntry) には包まない。`esds` ボックスは
/// [`Mp4aBox::esds_box`] として中に置く。
///
/// # 固定値
///
/// - [`AudioSampleEntryFields::data_reference_index`] =
///   [`AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX`]
/// - [`AudioSampleEntryFields::samplesize`] =
///   [`AudioSampleEntryFields::DEFAULT_SAMPLESIZE`] (16)
/// - [`Mp4aBox::unknown_boxes`] = 空 `Vec`
/// - [`EsDescriptor::stream_priority`] = [`EsDescriptor::LOWEST_STREAM_PRIORITY`]
/// - [`EsDescriptor::depends_on_es_id`] / `url_string` / `ocr_es_id` = `None`
/// - [`EsDescriptor::sl_config_descr`] = [`SlConfigDescriptor`] (既存 encode が
///   `predefined = 2` を書く)
/// - [`DecoderConfigDescriptor::object_type_indication`] =
///   [`DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3`]
/// - [`DecoderConfigDescriptor::stream_type`] =
///   [`DecoderConfigDescriptor::STREAM_TYPE_AUDIO`]
/// - [`DecoderConfigDescriptor::up_stream`] =
///   [`DecoderConfigDescriptor::UP_STREAM_FALSE`]
/// - [`DecoderConfigDescriptor::dec_specific_info`] = `Some` (`payload` は
///   [`encode_audio_specific_config`] の正規形)
///
/// # ストリーム導出値 (ASC から写す)
///
/// - [`AudioSampleEntryFields::channelcount`] =
///   [`ChannelConfiguration::channel_count`] (ASC のチャンネル構成から導出)
/// - [`AudioSampleEntryFields::samplerate`]: 実効周波数が `u16` に収まる
///   (1..=65535) ときは `FixedPointNumber::new(hz, 0)`。収まらないとき
///   (96000 / 88200、および明示周波数が 65535 超) は切り捨てず
///   `FixedPointNumber::new(0, 0)` とし、真値は ASC payload 側に残す
///
/// # 呼び出し側指定値
///
/// [`Mp4aSampleEntryConfig`] の各フィールドを参照。
///
/// # エラー条件
///
/// - `config.es_id` が [`EsDescriptor::MIN_ES_ID`] 未満 (0 は予約)
/// - `config.buffer_size_db` が 24 ビット (0..=16777215) を超える
///   (黙って切り捨てず [`crate::Error`])
pub fn build_mp4a_box(
    asc: &AudioSpecificConfig,
    config: &Mp4aSampleEntryConfig,
) -> Result<Mp4aBox> {
    // 正規形 ASC の生成。全フィールドは型で受理条件が保証されるため Err はない
    let payload = encode_audio_specific_config(asc);
    let hz = asc.sampling_frequency.hz();

    if config.es_id < EsDescriptor::MIN_ES_ID {
        return Err(Error::invalid_input(
            "es_id must be 1 or greater (0 is reserved)",
        ));
    }
    if config.buffer_size_db > 0xFF_FFFF {
        // DecoderConfigDescriptor::encode は buffer_size_db の上位 1 バイトを捨てるため、
        // 収まらない値は黙って切り捨てず拒否する
        return Err(Error::invalid_input(
            "buffer_size_db must fit in 24 bits (0..=16777215)",
        ));
    }

    let audio = AudioSampleEntryFields {
        data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
        channelcount: asc.channel_configuration.channel_count(),
        samplesize: AudioSampleEntryFields::DEFAULT_SAMPLESIZE,
        samplerate: crate::FixedPointNumber::new(
            if hz <= u16::MAX as u32 { hz as u16 } else { 0 },
            0,
        ),
    };

    let esds_box = EsdsBox {
        es: EsDescriptor {
            es_id: config.es_id,
            stream_priority: EsDescriptor::LOWEST_STREAM_PRIORITY,
            depends_on_es_id: None,
            url_string: None,
            ocr_es_id: None,
            dec_config_descr: DecoderConfigDescriptor {
                object_type_indication:
                    DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3,
                stream_type: DecoderConfigDescriptor::STREAM_TYPE_AUDIO,
                up_stream: DecoderConfigDescriptor::UP_STREAM_FALSE,
                buffer_size_db: Uint::new(config.buffer_size_db),
                max_bitrate: config.max_bitrate,
                avg_bitrate: config.avg_bitrate,
                dec_specific_info: Some(DecoderSpecificInfo { payload }),
            },
            sl_config_descr: SlConfigDescriptor,
        },
    };

    Ok(Mp4aBox {
        audio,
        esds_box,
        unknown_boxes: Vec::new(),
    })
}

/// 生の `channelConfiguration` 値 (1..=7) から [`ChannelConfiguration`] を返す
///
/// 0 (PCE) と 8..=15 (reserved) は `None`。ASC (4 ビット) と ADTS (3 ビット) の
/// 両パーサーが使う
fn channel_configuration_from_raw(raw: u8) -> Option<ChannelConfiguration> {
    Some(match raw {
        1 => ChannelConfiguration::Mono,
        2 => ChannelConfiguration::Stereo,
        3 => ChannelConfiguration::Channels3,
        4 => ChannelConfiguration::Channels4,
        5 => ChannelConfiguration::Channels5,
        6 => ChannelConfiguration::FivePointOne,
        7 => ChannelConfiguration::SevenPointOne,
        _ => return None,
    })
}

/// MSB-first のビット読み取り
///
/// ISO/IEC 14496-3 / ISO/IEC 13818-7 の構文要素を読むために使う。入力が尽きた
/// 場合は `Err` を返す
struct BitReader<'a> {
    input: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bit(&mut self) -> Result<u8> {
        if self.byte_pos >= self.input.len() {
            return Err(Error::invalid_input("AAC input truncated"));
        }
        let bit = (self.input[self.byte_pos] >> (7 - self.bit_pos)) & 1;
        self.bit_pos += 1;
        if self.bit_pos == 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
        Ok(bit)
    }

    fn read_bits(&mut self, n: u32) -> Result<u32> {
        // 32 ビット超は本モジュールの構文にないので上限は 32
        let mut value: u32 = 0;
        for _ in 0..n {
            value = (value << 1) | u32::from(self.read_bit()?);
        }
        Ok(value)
    }

    /// 読み切り位置が入力の終端かどうか
    fn is_at_end(&self) -> bool {
        self.byte_pos >= self.input.len() && self.bit_pos == 0
    }
}

/// MSB-first のビット書き込み
///
/// 受理条件を満たす構造化値から正規形バイト列を組み立てるために使う
struct BitWriter {
    bytes: Vec<u8>,
    bit_pos: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit_pos: 0,
        }
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
