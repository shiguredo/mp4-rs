//! `shiguredo_mp4::codec_string` の Property-Based Testing
//!
//! - 任意の H.264 3 バイトが常に 6 桁小文字 hex として保存されること
//! - HEVC constraint の非ゼロ末尾バイトが文字列から失われないこと

use noprop::TestCaseContext;
use shiguredo_mp4::{
    Uint,
    boxes::{Avc1Box, AvccBox, Hev1Box, HvccBox, SampleEntry, VisualSampleEntryFields},
    codec_string,
};

/// このファイルの PBT ケース数
const CASES: usize = 500;

fn visual_fields() -> VisualSampleEntryFields {
    VisualSampleEntryFields {
        data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
        width: 16,
        height: 16,
        horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
        vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
        frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
        compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
        depth: VisualSampleEntryFields::DEFAULT_DEPTH,
    }
}

fn avc1_entry(profile: u8, compat: u8, level: u8) -> SampleEntry {
    SampleEntry::Avc1(Avc1Box {
        visual: visual_fields(),
        avcc_box: AvccBox {
            avc_profile_indication: profile,
            profile_compatibility: compat,
            avc_level_indication: level,
            length_size_minus_one: Uint::new(3),
            sps_list: vec![],
            pps_list: vec![],
            chroma_format: None,
            bit_depth_luma_minus8: None,
            bit_depth_chroma_minus8: None,
            sps_ext_list: vec![],
        },
        unknown_boxes: vec![],
    })
}

fn hev1_entry(constraint_flags: u64) -> SampleEntry {
    // Uint<u64, 48> に収まるよう下位 48 bit だけ使う
    let flags = constraint_flags & 0x0000_FFFF_FFFF_FFFF;
    SampleEntry::Hev1(Hev1Box {
        visual: visual_fields(),
        hvcc_box: HvccBox {
            general_profile_space: Uint::new(0),
            general_tier_flag: Uint::new(0),
            general_profile_idc: Uint::new(1),
            general_profile_compatibility_flags: 0,
            general_constraint_indicator_flags: Uint::new(flags),
            general_level_idc: 93,
            min_spatial_segmentation_idc: Uint::new(0),
            parallelism_type: Uint::new(0),
            chroma_format_idc: Uint::new(1),
            bit_depth_luma_minus8: Uint::new(0),
            bit_depth_chroma_minus8: Uint::new(0),
            avg_frame_rate: 0,
            constant_frame_rate: Uint::new(0),
            num_temporal_layers: Uint::new(1),
            temporal_id_nested: Uint::new(1),
            length_size_minus_one: Uint::new(3),
            nalu_arrays: vec![],
        },
        unknown_boxes: vec![],
    })
}

/// 任意の H.264 3 バイトが `avc1.` + 6 桁小文字 hex になること
#[test]
fn avc1_three_bytes_always_six_lowercase_hex() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx: &mut TestCaseContext| {
        let profile = noprop::sample_u8(ctx);
        let compat = noprop::sample_u8(ctx);
        let level = noprop::sample_u8(ctx);

        let s = codec_string::from_sample_entry(&avc1_entry(profile, compat, level))
            .expect("Avc1 のコーデック文字列生成は失敗しない");

        assert!(
            s.starts_with("avc1."),
            "プレフィックスが avc1. ではない: {s}"
        );
        let hex = &s[5..];
        assert_eq!(hex.len(), 6, "hex 部が 6 桁ではない: {s}");
        assert!(
            hex.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
            "hex 部が小文字 hex ではない: {s}"
        );
        assert_eq!(
            hex,
            format!("{profile:02x}{compat:02x}{level:02x}"),
            "3 バイトの保存に失敗: {s}"
        );
        Ok(())
    })?;
    Ok(())
}

/// HEVC constraint の非ゼロ末尾バイトが文字列から失われないこと
#[test]
fn hevc_nonzero_trailing_constraint_byte_is_preserved() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx: &mut TestCaseContext| {
        let flags = noprop::sample_u64(ctx);
        let s = codec_string::from_sample_entry(&hev1_entry(flags))
            .expect("Hev1 のコーデック文字列生成は失敗しない");

        let constraint_bytes = (flags & 0x0000_FFFF_FFFF_FFFF).to_be_bytes();
        let slice = &constraint_bytes[2..8];
        let last_nonzero = slice
            .iter()
            .rposition(|&b| b != 0)
            .map(|i| i + 1)
            .unwrap_or(1);

        // 形式: hev1.{space}{idc}.{compat}.{tier}{level}.{XX}...
        let parts: Vec<&str> = s.split('.').collect();
        assert!(parts.len() >= 5, "HEVC 文字列の部品数が足りない: {s}");
        let constraint_parts = &parts[4..];
        assert_eq!(
            constraint_parts.len(),
            last_nonzero,
            "非ゼロ末尾までのバイト数が一致しない: flags={flags:#x}, s={s}"
        );
        for (i, part) in constraint_parts.iter().enumerate() {
            assert_eq!(
                *part,
                format!("{:02X}", slice[i]),
                "constraint バイト {i} が失われたか壊れている: {s}"
            );
        }
        Ok(())
    })?;
    Ok(())
}
