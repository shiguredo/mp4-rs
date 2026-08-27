//! `SampleEntry` から RFC 6381 系の `codecs` パラメーター文字列を生成する
//!
//! 構築済みのコーデック設定ボックスを解釈する機能であり、ビットストリーム解析は行わない。
//! 書式の根拠は各コーデックの ISOBMFF binding / RFC 6381 であり、将来の仕様改訂で
//! 変わる可能性がある。

use alloc::{format, string::String};

use crate::{
    Error, Result,
    bitstream::h264::H264ProfileLevelId,
    boxes::{Av1cBox, AvccBox, HvccBox, SampleEntry, VpccBox},
    descriptors::DecoderConfigDescriptor,
};

/// [`SampleEntry`] から `codecs` パラメーター文字列を生成する
///
/// # エラー条件
///
/// - [`SampleEntry::Unknown`]: 未知の sample entry は解釈できないため [`ErrorKind::Unsupported`][crate::ErrorKind::Unsupported]
/// - `mp4a` かつ OTI が `0x40` なのに `DecoderSpecificInfo` が欠落、または AOT ビット列が切り詰められている:
///   [`ErrorKind::InvalidData`][crate::ErrorKind::InvalidData]
pub fn from_sample_entry(entry: &SampleEntry) -> Result<String> {
    match entry {
        SampleEntry::Avc1(b) => Ok(avc1_codec_string(&b.avcc_box)),
        SampleEntry::Hev1(b) => Ok(hevc_codec_string("hev1", &b.hvcc_box)),
        SampleEntry::Hvc1(b) => Ok(hevc_codec_string("hvc1", &b.hvcc_box)),
        SampleEntry::Av01(b) => Ok(av01_codec_string(&b.av1c_box)),
        SampleEntry::Vp08(b) => Ok(vp_codec_string("vp08", &b.vpcc_box)),
        SampleEntry::Vp09(b) => Ok(vp_codec_string("vp09", &b.vpcc_box)),
        SampleEntry::Mp4a(b) => mp4a_codec_string(&b.esds_box.es.dec_config_descr),
        SampleEntry::Opus(_) => Ok(String::from("Opus")),
        SampleEntry::Flac(_) => Ok(String::from("fLaC")),
        SampleEntry::Stpp(_) => Ok(String::from("stpp")),
        SampleEntry::Wvtt(_) => Ok(String::from("wvtt")),
        SampleEntry::Tx3g(_) => Ok(String::from("tx3g")),
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
    format!("avc1.{hex}")
}

/// HEVC: ISO/IEC 14496-15 Annex E の `codecs` パラメーター
fn hevc_codec_string(prefix: &str, hvcc: &HvccBox) -> String {
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
    format!("av01.{profile}.{level:02}{tier}.{bit_depth:02}")
}

/// AV1 の `BitDepth` 導出（`bitstream::av1` の `color_config` 解釈と一致）
fn av1_bit_depth(seq_profile: u8, high_bitdepth: u8, twelve_bit: u8) -> u8 {
    if seq_profile == 2 && high_bitdepth != 0 {
        if twelve_bit != 0 { 12 } else { 10 }
    } else if high_bitdepth != 0 {
        10
    } else {
        8
    }
}

/// VP8 / VP9: Binding の必須形 `<4CC>.PP.LL.DD` のみ
fn vp_codec_string(prefix: &str, vpcc: &VpccBox) -> String {
    format!(
        "{prefix}.{:02}.{:02}.{:02}",
        vpcc.profile,
        vpcc.level,
        vpcc.bit_depth.get(),
    )
}

/// AAC / MPEG-4 Audio: RFC 6381 の `mp4a.<OTI>[.<AOT>]`
fn mp4a_codec_string(dec_config: &DecoderConfigDescriptor) -> Result<String> {
    let oti = dec_config.object_type_indication;
    let mut s = format!("mp4a.{oti:02x}");

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
