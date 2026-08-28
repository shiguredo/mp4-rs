//! `SampleEntry` から RFC 6381 系の `codecs` パラメーター文字列を生成する
//!
//! コーデック設定ボックスの構造化フィールドを解釈する。AAC の `audioObjectType` だけは
//! AudioSpecificConfig の先頭ビットから読む。書式は RFC 6381 および各 ISOBMFF binding に従う。

use alloc::{format, string::String};

use crate::{
    BoxType, Error, Result,
    bitstream::h264::H264ProfileLevelId,
    boxes::{
        Av01Box, Av1cBox, Avc1Box, AvccBox, FlacBox, Hev1Box, Hvc1Box, HvccBox, Mp4aBox, OpusBox,
        SampleEntry, StppBox, Tx3gBox, Vp08Box, Vp09Box, VpccBox, WvttBox,
    },
    descriptors::DecoderConfigDescriptor,
};

/// [`SampleEntry`] から `codecs` パラメーター文字列を生成する
///
/// 設定ボックスの構造化フィールドから必須形を組み立てる。H.264 なら `avcC` の 3 バイト、
/// HEVC なら `hvcC`、AV1 / VP なら各 binding の必須欄、AAC なら `esds` の OTI と AOT。
/// Opus / FLAC / 字幕は登録済み sample entry 4CC そのものになる。
///
/// # Examples
///
/// H.264 High Profile Level 4.0 は `avc1.640028` になる:
///
/// ```
/// use shiguredo_mp4::{
///     Uint,
///     boxes::{Avc1Box, AvccBox, SampleEntry, VisualSampleEntryFields},
///     codec_string,
/// };
///
/// # fn main() -> shiguredo_mp4::Result<()> {
/// let entry = SampleEntry::Avc1(Avc1Box {
///     visual: VisualSampleEntryFields {
///         data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
///         width: 1920,
///         height: 1080,
///         horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
///         vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
///         frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
///         compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
///         depth: VisualSampleEntryFields::DEFAULT_DEPTH,
///     },
///     avcc_box: AvccBox {
///         avc_profile_indication: 100,
///         profile_compatibility: 0x00,
///         avc_level_indication: 40,
///         length_size_minus_one: Uint::new(3),
///         sps_list: Vec::new(),
///         pps_list: Vec::new(),
///         chroma_format: None,
///         bit_depth_luma_minus8: None,
///         bit_depth_chroma_minus8: None,
///         sps_ext_list: Vec::new(),
///     },
///     unknown_boxes: Vec::new(),
/// });
///
/// assert_eq!(codec_string::from_sample_entry(&entry)?, "avc1.640028");
/// # Ok(())
/// # }
/// ```
///
/// # エラー条件
///
/// - [`SampleEntry::Unknown`]: 未知の sample entry は解釈できないため [`ErrorKind::Unsupported`][crate::ErrorKind::Unsupported]
/// - `mp4a` かつ OTI が `0x40` なのに `DecoderSpecificInfo` が欠落、または AOT ビット列が切り詰められている:
///   [`ErrorKind::InvalidData`][crate::ErrorKind::InvalidData]
pub fn from_sample_entry(entry: &SampleEntry) -> Result<String> {
    match entry {
        SampleEntry::Avc1(b) => Ok(avc1_codec_string(&b.avcc_box)),
        SampleEntry::Hev1(b) => Ok(hevc_codec_string(Hev1Box::TYPE, &b.hvcc_box)),
        SampleEntry::Hvc1(b) => Ok(hevc_codec_string(Hvc1Box::TYPE, &b.hvcc_box)),
        SampleEntry::Av01(b) => Ok(av01_codec_string(&b.av1c_box)),
        SampleEntry::Vp08(b) => Ok(vp_codec_string(Vp08Box::TYPE, &b.vpcc_box)),
        SampleEntry::Vp09(b) => Ok(vp_codec_string(Vp09Box::TYPE, &b.vpcc_box)),
        SampleEntry::Mp4a(b) => mp4a_codec_string(&b.esds_box.es.dec_config_descr),
        SampleEntry::Opus(_) => Ok(format!("{}", OpusBox::TYPE)),
        SampleEntry::Flac(_) => Ok(format!("{}", FlacBox::TYPE)),
        SampleEntry::Stpp(_) => Ok(format!("{}", StppBox::TYPE)),
        SampleEntry::Wvtt(_) => Ok(format!("{}", WvttBox::TYPE)),
        SampleEntry::Tx3g(_) => Ok(format!("{}", Tx3gBox::TYPE)),
        SampleEntry::Unknown(b) => Err(Error::unsupported(format!(
            "codec string is unsupported for unknown sample entry type `{}`",
            b.box_type
        ))),
    }
}

/// H.264: `avc1.` + 6 桁小文字 hex（RFC 6381）
fn avc1_codec_string(avcc: &AvccBox) -> String {
    let hex = H264ProfileLevelId {
        profile_idc: avcc.avc_profile_indication,
        profile_iop: avcc.profile_compatibility,
        level_idc: avcc.avc_level_indication,
    }
    .to_hex();
    format!("{}.{hex}", Avc1Box::TYPE)
}

/// HEVC: ISO/IEC 14496-15 Annex E の `codecs` パラメーター
fn hevc_codec_string(prefix: BoxType, hvcc: &HvccBox) -> String {
    let space = match hvcc.general_profile_space.get() {
        1 => "A",
        2 => "B",
        3 => "C",
        _ => "",
    };
    let profile_idc = hvcc.general_profile_idc.get();
    let tier = if hvcc.general_tier_flag.get() == 0 {
        'L'
    } else {
        'H'
    };
    let level = hvcc.general_level_idc;

    // general_profile_compatibility_flags を bit-reverse した値の大文字 hex（先頭ゼロ省略可）
    let compat = hvcc.general_profile_compatibility_flags.reverse_bits();
    let compat_hex = format!("{compat:X}");

    // 48 bit の constraint を 6 バイトとし、末尾ゼロを省略する（全ゼロでも最低 1 バイト）
    let constraint_bytes = hvcc.general_constraint_indicator_flags.get().to_be_bytes();
    let constraint_slice = &constraint_bytes[2..8];
    let last_nonzero = constraint_slice
        .iter()
        .rposition(|&b| b != 0)
        .map(|i| i + 1)
        .unwrap_or(1);
    let mut constraint_hex = String::new();
    for (i, byte) in constraint_slice[..last_nonzero].iter().enumerate() {
        if i > 0 {
            constraint_hex.push('.');
        }
        constraint_hex.push_str(&format!("{byte:02X}"));
    }

    format!("{prefix}.{space}{profile_idc}.{compat_hex}.{tier}{level}.{constraint_hex}")
}

/// AV1: Binding v1.3.0 Section 5 の必須形のみ
fn av01_codec_string(av1c: &Av1cBox) -> String {
    let profile = av1c.seq_profile.get();
    let level = av1c.seq_level_idx_0.get();
    let tier = if av1c.seq_tier_0.get() == 0 { 'M' } else { 'H' };
    let bit_depth = av1_bit_depth(profile, av1c.high_bitdepth.get(), av1c.twelve_bit.get());
    format!(
        "{}.{}.{level:02}{tier}.{bit_depth:02}",
        Av01Box::TYPE,
        profile
    )
}

/// AV1 Binding の `BitDepth` を `av1C` の構造化フラグから導出する
fn av1_bit_depth(seq_profile: u8, high_bitdepth: u8, twelve_bit: u8) -> u8 {
    // `seq_profile == 2` かつ `high_bitdepth` なら `twelve_bit` で 12 / 10。
    // それ以外は `high_bitdepth` なら 10、そうでなければ 8。
    // 合法な `seq_profile` 0..=2 では `bitstream::av1` の `read_color_config` と同じ分岐になる。
    if seq_profile == 2 && high_bitdepth != 0 {
        if twelve_bit != 0 { 12 } else { 10 }
    } else if high_bitdepth != 0 {
        10
    } else {
        8
    }
}

/// VP8 / VP9: Binding の必須形 `<4CC>.PP.LL.DD` のみ
fn vp_codec_string(prefix: BoxType, vpcc: &VpccBox) -> String {
    format!(
        "{}.{:02}.{:02}.{:02}",
        prefix,
        vpcc.profile,
        vpcc.level,
        vpcc.bit_depth.get(),
    )
}

/// AAC / MPEG-4 Audio: RFC 6381 の `mp4a.<OTI>[.<AOT>]`
fn mp4a_codec_string(dec_config: &DecoderConfigDescriptor) -> Result<String> {
    let oti = dec_config.object_type_indication;
    let mut s = format!("{}.{:02x}", Mp4aBox::TYPE, oti);

    if oti == DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3 {
        let Some(info) = &dec_config.dec_specific_info else {
            return Err(Error::invalid_data(
                "mp4a object type 0x40 requires DecoderSpecificInfo for codecs string",
            ));
        };
        let aot = audio_object_type_from_asc(&info.payload)?;
        s.push('.');
        s.push_str(&format!("{aot}"));
    }

    Ok(s)
}

/// AudioSpecificConfig 先頭から `audioObjectType` だけを読む
///
/// ASC 全体の妥当性（AOT 2 限定など）は検証しない。
fn audio_object_type_from_asc(payload: &[u8]) -> Result<u16> {
    if payload.is_empty() {
        return Err(Error::invalid_data(
            "AudioSpecificConfig is empty; cannot read audioObjectType",
        ));
    }

    let aot = payload[0] >> 3;
    if aot != 31 {
        return Ok(u16::from(aot));
    }

    // AOT 31 は 5 bit + 続き 6 bit のエスケープ形式（値は 32 + 拡張 6 bit）
    if payload.len() < 2 {
        return Err(Error::invalid_data(
            "AudioSpecificConfig is truncated at escaped audioObjectType",
        ));
    }
    let ext = ((payload[0] & 0x07) << 3) | (payload[1] >> 5);
    Ok(u16::from(ext) + 32)
}
