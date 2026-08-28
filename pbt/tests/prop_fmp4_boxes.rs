//! Fragmented MP4 (fMP4) ボックスの Property-Based Testing
//!
//! MoofBox, MfhdBox, TrafBox, TfhdBox, TrunBox, TfdtBox, SidxBox,
//! MvexBox, TrexBox, MehdBox のテスト

use noprop::TestCaseContext;
use shiguredo_mp4::{
    Decode, Encode, ErrorKind, SampleFlags,
    boxes::{
        MehdBox, MfhdBox, MoofBox, MvexBox, SidxBox, SidxReference, TfdtBox, TfhdBox, TfraBox,
        TfraEntry, TrafBox, TrexBox, TrunBox, TrunSample,
    },
};

/// このファイルの主要 PBT ケース数（旧 `with_cases(100)` を維持）
const CASES: usize = 100;

/// noprop の `sample_usize_in` で長さを引いてから要素を生成するベクタサンプラー
fn sample_vec<T>(
    ctx: &mut TestCaseContext,
    range: std::ops::Range<usize>,
    mut elem: impl FnMut(&mut TestCaseContext) -> T,
) -> Vec<T> {
    let len = noprop::sample_usize_in(ctx, range);
    let mut result = Vec::new();
    for _ in 0..len {
        result.push(elem(ctx));
    }
    result
}

// ===== サンプラー定義 =====

/// SampleFlags を生成する
fn arb_sample_flags(ctx: &mut TestCaseContext) -> SampleFlags {
    SampleFlags::new(noprop::sample_u32(ctx))
}

/// TrexBox を生成する
fn arb_trex_box(ctx: &mut TestCaseContext) -> TrexBox {
    TrexBox {
        track_id: noprop::sample_u32(ctx),
        default_sample_description_index: noprop::sample_u32(ctx),
        default_sample_duration: noprop::sample_u32(ctx),
        default_sample_size: noprop::sample_u32(ctx),
        default_sample_flags: arb_sample_flags(ctx),
    }
}

/// MehdBox を生成する
fn arb_mehd_box(ctx: &mut TestCaseContext) -> MehdBox {
    MehdBox {
        fragment_duration: noprop::sample_u64(ctx),
    }
}

/// MvexBox を生成する
fn arb_mvex_box(ctx: &mut TestCaseContext) -> MvexBox {
    let mehd_box = if noprop::sample_bool(ctx) {
        Some(arb_mehd_box(ctx))
    } else {
        None
    };
    let trex_boxes = sample_vec(ctx, 0..3, arb_trex_box);
    MvexBox {
        mehd_box,
        trex_boxes,
        unknown_boxes: vec![],
    }
}

/// MfhdBox を生成する
fn arb_mfhd_box(ctx: &mut TestCaseContext) -> MfhdBox {
    MfhdBox {
        sequence_number: noprop::sample_u32(ctx),
    }
}

/// TfdtBox を生成する
fn arb_tfdt_box(ctx: &mut TestCaseContext) -> TfdtBox {
    let base_media_decode_time = noprop::sample_u64(ctx);
    let version_hint = noprop::sample_u64_in(ctx, 0..=1) as u8;
    // 値が 32-bit に収まらない場合は version=1 が必須
    let version = if base_media_decode_time > u32::MAX as u64 {
        1
    } else {
        version_hint
    };
    TfdtBox {
        version,
        base_media_decode_time,
    }
}

/// TfhdBox を生成する
fn arb_tfhd_box(ctx: &mut TestCaseContext) -> TfhdBox {
    let track_id = noprop::sample_u32(ctx);
    let base_data_offset = if noprop::sample_bool(ctx) {
        Some(noprop::sample_u64(ctx))
    } else {
        None
    };
    let sample_description_index = if noprop::sample_bool(ctx) {
        Some(noprop::sample_u32(ctx))
    } else {
        None
    };
    let default_sample_duration = if noprop::sample_bool(ctx) {
        Some(noprop::sample_u32(ctx))
    } else {
        None
    };
    let default_sample_size = if noprop::sample_bool(ctx) {
        Some(noprop::sample_u32(ctx))
    } else {
        None
    };
    let default_sample_flags = if noprop::sample_bool(ctx) {
        Some(arb_sample_flags(ctx))
    } else {
        None
    };
    let duration_is_empty = noprop::sample_bool(ctx);
    let default_base_is_moof = noprop::sample_bool(ctx);
    TfhdBox {
        track_id,
        base_data_offset,
        sample_description_index,
        default_sample_duration,
        default_sample_size,
        default_sample_flags,
        duration_is_empty,
        default_base_is_moof,
    }
}

/// TrunBox を生成する (一貫性のあるサンプル)
fn arb_trun_box(ctx: &mut TestCaseContext) -> TrunBox {
    let data_offset = if noprop::sample_bool(ctx) {
        Some(noprop::sample_i32(ctx))
    } else {
        None
    };
    let first_sample_flags = if noprop::sample_bool(ctx) {
        Some(arb_sample_flags(ctx))
    } else {
        None
    };
    let has_duration = noprop::sample_bool(ctx);
    let has_size = noprop::sample_bool(ctx);
    let has_flags = noprop::sample_bool(ctx);
    let has_cto = noprop::sample_bool(ctx);
    // cto の符号側: true = signed (version 1)、false = unsigned (version 0)
    let signed_side = noprop::sample_bool(ctx);
    let count = noprop::sample_usize_in(ctx, 0..10);

    // ISO/IEC 14496-12 8.8.8: composition_time_offset は version 0 で
    // `0..=u32::MAX`、version 1 で `i32::MIN..=i32::MAX` の範囲を許容する。
    // 負値と `> i32::MAX` は同一 TrunBox 内に混在させると encode 時に
    // どちらの版でも表現できず invalid_input になるため、TrunBox 単位で
    // 「符号あり側」か「符号なし側」のどちらか一方に統一して探索する。
    let sample_cto = |ctx: &mut TestCaseContext| -> Option<i64> {
        if has_cto {
            if signed_side {
                // version 1 の許容範囲
                Some(noprop::sample_i32(ctx) as i64)
            } else {
                // version 0 の許容範囲
                Some(noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64)) as i64)
            }
        } else {
            None
        }
    };
    let sample_dur = |ctx: &mut TestCaseContext| -> Option<u32> {
        if has_duration {
            Some(noprop::sample_u32(ctx))
        } else {
            None
        }
    };
    let sample_size = |ctx: &mut TestCaseContext| -> Option<u32> {
        if has_size {
            Some(noprop::sample_u32(ctx))
        } else {
            None
        }
    };
    let sample_flg = |ctx: &mut TestCaseContext| -> Option<SampleFlags> {
        if has_flags {
            Some(arb_sample_flags(ctx))
        } else {
            None
        }
    };

    let mut samples = Vec::new();
    for _ in 0..count {
        samples.push(TrunSample {
            duration: sample_dur(ctx),
            size: sample_size(ctx),
            flags: sample_flg(ctx),
            composition_time_offset: sample_cto(ctx),
        });
    }
    TrunBox {
        data_offset,
        first_sample_flags,
        samples,
    }
}

/// per-sample `Option` 有無を意図的に不整合にした `TrunBox` を生成する
///
/// `arb_trun_box` は run 全体で Option 有無を揃えるので、`TrunBox::encode` に追加された
/// `validate_sample_option_consistency` 経路（ISO/IEC 14496-12 8.8.8 の `tr_flags` が
/// run 全体共通であることに由来する不整合検出）を PBT で踏めない。本サンプラーは
/// 「サンプル数 2〜5、4 フィールド (duration / size / flags / composition_time_offset) の
/// どれか 1 つで先頭サンプルと後続いずれか 1 サンプルの Option 有無を食い違わせる」入力を作る。
fn arb_trun_box_inconsistent(ctx: &mut TestCaseContext) -> TrunBox {
    let count = noprop::sample_usize_in(ctx, 2..=5);
    let target_field = noprop::sample_usize_in(ctx, 0..4);
    let base_has = noprop::sample_bool(ctx);
    let flip_index = noprop::sample_usize_in(ctx, 1..count);
    let dur = noprop::sample_u32(ctx);
    let sz = noprop::sample_u32(ctx);
    let flg = arb_sample_flags(ctx);
    // composition_time_offset は version 0 の範囲に絞って cto 側の別 encode エラーを避ける
    let cto = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64)) as i64;

    // 対象フィールドを Some/None どちらか付けたベースサンプルを作るヘルパー
    let mk_sample = |has_target: bool| -> TrunSample {
        let mut s = TrunSample {
            duration: None,
            size: None,
            flags: None,
            composition_time_offset: None,
        };
        if has_target {
            match target_field {
                0 => s.duration = Some(dur),
                1 => s.size = Some(sz),
                2 => s.flags = Some(flg),
                _ => s.composition_time_offset = Some(cto),
            }
        }
        s
    };
    // flip_index のサンプルだけ Option 有無を反転させて不整合を作る
    let samples: Vec<TrunSample> = (0..count)
        .map(|i| mk_sample(if i == flip_index { !base_has } else { base_has }))
        .collect();
    TrunBox {
        data_offset: None,
        first_sample_flags: None,
        samples,
    }
}

/// TrafBox を生成する
fn arb_traf_box(ctx: &mut TestCaseContext) -> TrafBox {
    let tfhd_box = arb_tfhd_box(ctx);
    let tfdt_box = if noprop::sample_bool(ctx) {
        Some(arb_tfdt_box(ctx))
    } else {
        None
    };
    let trun_boxes = sample_vec(ctx, 0..3, arb_trun_box);
    TrafBox {
        tfhd_box,
        tfdt_box,
        trun_boxes,
        unknown_boxes: vec![],
    }
}

/// MoofBox を生成する
fn arb_moof_box(ctx: &mut TestCaseContext) -> MoofBox {
    let mfhd_box = arb_mfhd_box(ctx);
    let traf_boxes = sample_vec(ctx, 0..3, arb_traf_box);
    MoofBox {
        mfhd_box,
        traf_boxes,
        unknown_boxes: vec![],
    }
}

/// SidxReference を生成する
fn arb_sidx_reference(ctx: &mut TestCaseContext) -> SidxReference {
    SidxReference {
        reference_type: noprop::sample_bool(ctx),
        referenced_size: noprop::sample_u64_in(ctx, 0..0x7FFF_FFFF) as u32,
        subsegment_duration: noprop::sample_u32(ctx),
        starts_with_sap: noprop::sample_bool(ctx),
        sap_type: noprop::sample_u64_in(ctx, 0..8) as u8,
        sap_delta_time: noprop::sample_u64_in(ctx, 0..0x0FFF_FFFF) as u32,
    }
}

/// SidxBox を生成する
fn arb_sidx_box(ctx: &mut TestCaseContext) -> SidxBox {
    let reference_id = noprop::sample_u32(ctx);
    let timescale = noprop::sample_u32(ctx);
    let earliest_presentation_time = noprop::sample_u64(ctx);
    let first_offset = noprop::sample_u64(ctx);
    let references = sample_vec(ctx, 0..10, arb_sidx_reference);
    SidxBox {
        reference_id,
        timescale,
        earliest_presentation_time,
        first_offset,
        references,
    }
}

/// TfraEntry を生成する
///
/// 各フィールドの上限は呼び出し側から与える
fn arb_tfra_entry(
    ctx: &mut TestCaseContext,
    max_traf: u32,
    max_trun: u32,
    max_sample: u32,
    max_time: u64,
    max_moof_offset: u64,
) -> TfraEntry {
    TfraEntry {
        time: noprop::sample_u64_in(ctx, 0..=max_time),
        moof_offset: noprop::sample_u64_in(ctx, 0..=max_moof_offset),
        traf_number: noprop::sample_u64_in(ctx, 0..=max_traf as u64) as u32,
        trun_number: noprop::sample_u64_in(ctx, 0..=max_trun as u64) as u32,
        sample_number: noprop::sample_u64_in(ctx, 0..=max_sample as u64) as u32,
    }
}

/// TfraBox を生成する
///
/// version と `length_size_*` を先に決めたうえで、対応する上限に絞った `TfraEntry` を生成する。
fn arb_tfra_box(ctx: &mut TestCaseContext) -> TfraBox {
    let track_id = noprop::sample_u32(ctx);
    let version = noprop::sample_u64_in(ctx, 0..=1) as u8;
    let l_traf = noprop::sample_u64_in(ctx, 0..=3) as u8;
    let l_trun = noprop::sample_u64_in(ctx, 0..=3) as u8;
    let l_sample = noprop::sample_u64_in(ctx, 0..=3) as u8;

    // length_size に応じた u32 の上限
    let max_value_for_length_size = |length_size: u8| -> u32 {
        if length_size >= 3 {
            u32::MAX
        } else {
            (1u32 << (8 * (length_size as u32 + 1))) - 1
        }
    };
    let max_traf = max_value_for_length_size(l_traf);
    let max_trun = max_value_for_length_size(l_trun);
    let max_sample = max_value_for_length_size(l_sample);
    // time と moof_offset は同じ制約に従う
    let max_time_and_moof_offset = if version == 0 {
        u32::MAX as u64
    } else {
        u64::MAX
    };

    let n = noprop::sample_usize_in(ctx, 0..3);
    let mut entries = Vec::new();
    for _ in 0..n {
        entries.push(arb_tfra_entry(
            ctx,
            max_traf,
            max_trun,
            max_sample,
            max_time_and_moof_offset,
            max_time_and_moof_offset,
        ));
    }

    TfraBox {
        version,
        track_id,
        length_size_of_traf_num: l_traf,
        length_size_of_trun_num: l_trun,
        length_size_of_sample_num: l_sample,
        entries,
    }
}

// ===== SampleFlags のテスト =====

/// SampleFlags の encode/decode roundtrip
#[test]
fn sample_flags_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let flags = arb_sample_flags(ctx);
        let encoded = flags.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SampleFlags::decode(&encoded)
            .expect("直前にエンコードした有効な SampleFlags は必ずデコードできる");

        assert_eq!(size, 4);
        assert_eq!(decoded.get(), flags.get());
        Ok(())
    })?;
    Ok(())
}

/// SampleFlags の各フィールドの取得テスト
#[test]
fn sample_flags_fields() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let is_leading = noprop::sample_u64_in(ctx, 0..4) as u8;
        let sample_depends_on = noprop::sample_u64_in(ctx, 0..4) as u8;
        let sample_is_depended_on = noprop::sample_u64_in(ctx, 0..4) as u8;
        let sample_has_redundancy = noprop::sample_u64_in(ctx, 0..4) as u8;
        let sample_padding_value = noprop::sample_u64_in(ctx, 0..8) as u8;
        let sample_is_non_sync_sample = noprop::sample_bool(ctx);
        let sample_degradation_priority = noprop::sample_u16(ctx);

        let flags = SampleFlags::from_fields(
            is_leading,
            sample_depends_on,
            sample_is_depended_on,
            sample_has_redundancy,
            sample_padding_value,
            sample_is_non_sync_sample,
            sample_degradation_priority,
        );

        assert_eq!(flags.is_leading(), is_leading);
        assert_eq!(flags.sample_depends_on(), sample_depends_on);
        assert_eq!(flags.sample_is_depended_on(), sample_is_depended_on);
        assert_eq!(flags.sample_has_redundancy(), sample_has_redundancy);
        assert_eq!(flags.sample_padding_value(), sample_padding_value);
        assert_eq!(flags.sample_is_non_sync_sample(), sample_is_non_sync_sample);
        assert_eq!(
            flags.sample_degradation_priority(),
            sample_degradation_priority
        );
        Ok(())
    })?;
    Ok(())
}

// ===== TrexBox のテスト =====

/// TrexBox の encode/decode roundtrip
#[test]
fn trex_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let trex = arb_trex_box(ctx);
        let encoded = trex.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TrexBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrexBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.track_id, trex.track_id);
        assert_eq!(
            decoded.default_sample_description_index,
            trex.default_sample_description_index
        );
        assert_eq!(
            decoded.default_sample_duration,
            trex.default_sample_duration
        );
        assert_eq!(decoded.default_sample_size, trex.default_sample_size);
        assert_eq!(
            decoded.default_sample_flags.get(),
            trex.default_sample_flags.get()
        );
        Ok(())
    })?;
    Ok(())
}

// ===== MehdBox のテスト =====

/// MehdBox の encode/decode roundtrip
#[test]
fn mehd_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let mehd = arb_mehd_box(ctx);
        let encoded = mehd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MehdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MehdBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.fragment_duration, mehd.fragment_duration);
        Ok(())
    })?;
    Ok(())
}

// ===== MvexBox のテスト =====

/// MvexBox の encode/decode roundtrip
#[test]
fn mvex_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let mvex = arb_mvex_box(ctx);
        let encoded = mvex.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MvexBox::decode(&encoded)
            .expect("直前にエンコードした有効な MvexBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.mehd_box.is_some(), mvex.mehd_box.is_some());
        assert_eq!(decoded.trex_boxes.len(), mvex.trex_boxes.len());
        Ok(())
    })?;
    Ok(())
}

// ===== MfhdBox のテスト =====

/// MfhdBox の encode/decode roundtrip
#[test]
fn mfhd_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let mfhd = arb_mfhd_box(ctx);
        let encoded = mfhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MfhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MfhdBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.sequence_number, mfhd.sequence_number);
        Ok(())
    })?;
    Ok(())
}

// ===== TfdtBox のテスト =====

/// TfdtBox の encode/decode roundtrip
#[test]
fn tfdt_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let tfdt = arb_tfdt_box(ctx);
        let encoded = tfdt.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TfdtBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfdtBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.base_media_decode_time, tfdt.base_media_decode_time);
        Ok(())
    })?;
    Ok(())
}

// ===== TfhdBox のテスト =====

/// TfhdBox の encode/decode roundtrip
#[test]
fn tfhd_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let tfhd = arb_tfhd_box(ctx);
        let encoded = tfhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TfhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfhdBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.track_id, tfhd.track_id);
        assert_eq!(decoded.base_data_offset, tfhd.base_data_offset);
        assert_eq!(
            decoded.sample_description_index,
            tfhd.sample_description_index
        );
        assert_eq!(
            decoded.default_sample_duration,
            tfhd.default_sample_duration
        );
        assert_eq!(decoded.default_sample_size, tfhd.default_sample_size);
        assert_eq!(decoded.duration_is_empty, tfhd.duration_is_empty);
        assert_eq!(decoded.default_base_is_moof, tfhd.default_base_is_moof);
        Ok(())
    })?;
    Ok(())
}

// ===== TrunBox のテスト =====

/// TrunBox の encode/decode roundtrip
#[test]
fn trun_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let trun = arb_trun_box(ctx);
        let encoded = trun.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TrunBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrunBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.data_offset, trun.data_offset);
        assert_eq!(decoded.samples.len(), trun.samples.len());
        Ok(())
    })?;
    Ok(())
}

/// TrunBox の per-sample Option 不整合入力は encode で InvalidInput になる
#[test]
fn trun_box_inconsistent_option_is_invalid_input() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let trun = arb_trun_box_inconsistent(ctx);
        let err = trun
            .encode_to_vec()
            .expect_err("per-sample Option 不整合な TrunBox は encode で必ず失敗する");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別は InvalidInput のはず (実際は {:?}, reason={})",
            err.kind,
            err.reason,
        );
        // TrunBox::encode には別の InvalidInput 経路 (cto 範囲外など) もあるため、
        // validate_sample_option_consistency の特徴的な文言で経路を絞り込む
        assert!(
            err.reason.contains("inconsistent Option presence"),
            "エラーが per-sample Option 整合性チェックの経路から出ていない (reason={})",
            err.reason,
        );
        Ok(())
    })?;
    Ok(())
}

// ===== TrafBox のテスト =====

/// TrafBox の encode/decode roundtrip
#[test]
fn traf_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let traf = arb_traf_box(ctx);
        let encoded = traf.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TrafBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrafBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.tfhd_box.track_id, traf.tfhd_box.track_id);
        assert_eq!(decoded.tfdt_box.is_some(), traf.tfdt_box.is_some());
        assert_eq!(decoded.trun_boxes.len(), traf.trun_boxes.len());
        Ok(())
    })?;
    Ok(())
}

// ===== MoofBox のテスト =====

/// MoofBox の encode/decode roundtrip
#[test]
fn moof_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let moof = arb_moof_box(ctx);
        let encoded = moof.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MoofBox::decode(&encoded)
            .expect("直前にエンコードした有効な MoofBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(
            decoded.mfhd_box.sequence_number,
            moof.mfhd_box.sequence_number
        );
        assert_eq!(decoded.traf_boxes.len(), moof.traf_boxes.len());
        Ok(())
    })?;
    Ok(())
}

// ===== SidxBox のテスト =====

/// SidxBox の encode/decode roundtrip
#[test]
fn sidx_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let sidx = arb_sidx_box(ctx);
        let encoded = sidx.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SidxBox::decode(&encoded)
            .expect("直前にエンコードした有効な SidxBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.reference_id, sidx.reference_id);
        assert_eq!(decoded.timescale, sidx.timescale);
        assert_eq!(
            decoded.earliest_presentation_time,
            sidx.earliest_presentation_time
        );
        assert_eq!(decoded.first_offset, sidx.first_offset);
        assert_eq!(decoded.references.len(), sidx.references.len());
        Ok(())
    })?;
    Ok(())
}

// ===== TfraBox のテスト =====

/// TfraBox の encode/decode roundtrip
#[test]
fn tfra_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let tfra = arb_tfra_box(ctx);
        let encoded = tfra.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TfraBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfraBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded, tfra);
        Ok(())
    })?;
    Ok(())
}
