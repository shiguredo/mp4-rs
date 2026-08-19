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

// ===== 境界値テスト =====

mod boundary_tests {
    use super::*;
    use shiguredo_mp4::{BaseBox, FullBox};

    /// MehdBox: version 0 (32-bit duration)
    #[test]
    fn mehd_box_version0() {
        let mehd = MehdBox {
            fragment_duration: u32::MAX as u64,
        };
        assert_eq!(mehd.full_box_version(), 0);

        let encoded = mehd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MehdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MehdBox は必ずデコードできる");
        assert_eq!(decoded.fragment_duration, u32::MAX as u64);
    }

    /// MehdBox: version 1 (64-bit duration)
    #[test]
    fn mehd_box_version1() {
        let mehd = MehdBox {
            fragment_duration: u32::MAX as u64 + 1,
        };
        assert_eq!(mehd.full_box_version(), 1);

        let encoded = mehd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MehdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MehdBox は必ずデコードできる");
        assert_eq!(decoded.fragment_duration, u32::MAX as u64 + 1);
    }

    /// TfdtBox: version 0 (32-bit time)
    #[test]
    fn tfdt_box_version0() {
        let tfdt = TfdtBox {
            version: 0,
            base_media_decode_time: u32::MAX as u64,
        };
        assert_eq!(tfdt.full_box_version(), 0);

        let encoded = tfdt.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfdtBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfdtBox は必ずデコードできる");
        assert_eq!(decoded.base_media_decode_time, u32::MAX as u64);
    }

    /// TfdtBox: version 1 (64-bit time)
    #[test]
    fn tfdt_box_version1() {
        let tfdt = TfdtBox {
            version: 1,
            base_media_decode_time: u32::MAX as u64 + 1,
        };
        assert_eq!(tfdt.full_box_version(), 1);

        let encoded = tfdt.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfdtBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfdtBox は必ずデコードできる");
        assert_eq!(decoded.base_media_decode_time, u32::MAX as u64 + 1);
    }

    /// TfhdBox: 全フラグなし
    #[test]
    fn tfhd_box_no_flags() {
        let tfhd = TfhdBox {
            track_id: 1,
            base_data_offset: None,
            sample_description_index: None,
            default_sample_duration: None,
            default_sample_size: None,
            default_sample_flags: None,
            duration_is_empty: false,
            default_base_is_moof: false,
        };
        assert_eq!(tfhd.full_box_flags().get(), 0);

        let encoded = tfhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfhdBox は必ずデコードできる");
        assert_eq!(decoded.track_id, 1);
        assert!(decoded.base_data_offset.is_none());
    }

    /// TfhdBox: 全フラグあり
    #[test]
    fn tfhd_box_all_flags() {
        let tfhd = TfhdBox {
            track_id: 1,
            base_data_offset: Some(100),
            sample_description_index: Some(1),
            default_sample_duration: Some(1024),
            default_sample_size: Some(512),
            default_sample_flags: Some(SampleFlags::new(0x01010000)),
            duration_is_empty: true,
            default_base_is_moof: true,
        };

        let flags = tfhd.full_box_flags().get();
        assert!(flags & TfhdBox::FLAG_BASE_DATA_OFFSET_PRESENT != 0);
        assert!(flags & TfhdBox::FLAG_SAMPLE_DESCRIPTION_INDEX_PRESENT != 0);
        assert!(flags & TfhdBox::FLAG_DEFAULT_SAMPLE_DURATION_PRESENT != 0);
        assert!(flags & TfhdBox::FLAG_DEFAULT_SAMPLE_SIZE_PRESENT != 0);
        assert!(flags & TfhdBox::FLAG_DEFAULT_SAMPLE_FLAGS_PRESENT != 0);
        assert!(flags & TfhdBox::FLAG_DURATION_IS_EMPTY != 0);
        assert!(flags & TfhdBox::FLAG_DEFAULT_BASE_IS_MOOF != 0);

        let encoded = tfhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfhdBox は必ずデコードできる");
        assert_eq!(decoded.base_data_offset, Some(100));
        assert_eq!(decoded.sample_description_index, Some(1));
        assert_eq!(decoded.default_sample_duration, Some(1024));
        assert_eq!(decoded.default_sample_size, Some(512));
        assert!(decoded.duration_is_empty);
        assert!(decoded.default_base_is_moof);
    }

    /// TrunBox: 空のサンプルリスト
    #[test]
    fn trun_box_empty_samples() {
        let trun = TrunBox {
            data_offset: Some(8),
            first_sample_flags: None,
            samples: vec![],
        };

        let encoded = trun.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TrunBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrunBox は必ずデコードできる");
        assert!(decoded.samples.is_empty());
        assert_eq!(decoded.data_offset, Some(8));
    }

    /// TrunBox: 複数のサンプル
    #[test]
    fn trun_box_multiple_samples() {
        let trun = TrunBox {
            data_offset: None,
            first_sample_flags: Some(SampleFlags::new(0x02000000)),
            samples: vec![
                TrunSample {
                    duration: Some(1024),
                    size: Some(512),
                    flags: Some(SampleFlags::new(0x01010000)),
                    composition_time_offset: Some(0),
                },
                TrunSample {
                    duration: Some(1024),
                    size: Some(256),
                    flags: Some(SampleFlags::new(0x01010000)),
                    composition_time_offset: Some(1024),
                },
            ],
        };

        let encoded = trun.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TrunBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrunBox は必ずデコードできる");
        assert_eq!(decoded.samples.len(), 2);
        assert_eq!(decoded.samples[0].duration, Some(1024));
        assert_eq!(decoded.samples[1].size, Some(256));
    }

    /// TrunBox: 負の composition_time_offset (version 1)
    #[test]
    fn trun_box_negative_cto() {
        let trun = TrunBox {
            data_offset: None,
            first_sample_flags: None,
            samples: vec![TrunSample {
                duration: Some(1024),
                size: Some(512),
                flags: None,
                composition_time_offset: Some(-100),
            }],
        };

        assert_eq!(trun.full_box_version(), 1);

        let encoded = trun.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TrunBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrunBox は必ずデコードできる");
        assert_eq!(decoded.samples[0].composition_time_offset, Some(-100));
    }

    /// 指定した composition_time_offset を持つ TrunBox を組み立てるヘルパー
    fn trun_with_cto(cto: i64) -> TrunBox {
        TrunBox {
            data_offset: None,
            first_sample_flags: None,
            samples: vec![TrunSample {
                duration: Some(1024),
                size: Some(512),
                flags: None,
                composition_time_offset: Some(cto),
            }],
        }
    }

    /// TrunBox: version 0 の composition_time_offset 境界値 roundtrip
    #[test]
    fn trun_box_cto_version0_boundaries() {
        for cto in [0i64, i32::MAX as i64, i32::MAX as i64 + 1, u32::MAX as i64] {
            let trun = trun_with_cto(cto);
            assert_eq!(
                trun.full_box_version(),
                0,
                "cto={cto} は version 0 で表現できる"
            );

            let encoded = trun
                .encode_to_vec()
                .unwrap_or_else(|e| panic!("cto={cto} の encode は成功する: {e:?}"));
            let (decoded, _) = TrunBox::decode(&encoded)
                .unwrap_or_else(|e| panic!("cto={cto} の decode は成功する: {e:?}"));
            assert_eq!(
                decoded.samples[0].composition_time_offset,
                Some(cto),
                "cto={cto} の roundtrip 値が一致する"
            );
        }
    }

    /// TrunBox: version 1 の負値 composition_time_offset 境界値 roundtrip
    #[test]
    fn trun_box_cto_version1_negative_boundaries() {
        for cto in [-1i64, i32::MIN as i64] {
            let trun = trun_with_cto(cto);
            assert_eq!(trun.full_box_version(), 1, "cto={cto} は version 1 が必要");

            let encoded = trun
                .encode_to_vec()
                .unwrap_or_else(|e| panic!("cto={cto} の encode は成功する: {e:?}"));
            let (decoded, _) = TrunBox::decode(&encoded)
                .unwrap_or_else(|e| panic!("cto={cto} の decode は成功する: {e:?}"));
            assert_eq!(
                decoded.samples[0].composition_time_offset,
                Some(cto),
                "cto={cto} の roundtrip 値が一致する"
            );
        }
    }

    /// TrunBox: `u32::MAX` を超える composition_time_offset は encode 時にエラーとなる
    #[test]
    fn trun_box_cto_above_u32_max_is_encode_error() {
        let trun = trun_with_cto(u32::MAX as i64 + 1);
        assert_eq!(trun.full_box_version(), 0);
        assert!(
            trun.encode_to_vec().is_err(),
            "cto > u32::MAX は encode 時にエラーとなる"
        );
    }

    /// TrunBox: `i32::MIN` を下回る composition_time_offset は encode 時にエラーとなる
    #[test]
    fn trun_box_cto_below_i32_min_is_encode_error() {
        let trun = trun_with_cto(i32::MIN as i64 - 1);
        assert_eq!(trun.full_box_version(), 1);
        assert!(
            trun.encode_to_vec().is_err(),
            "cto < i32::MIN は encode 時にエラーとなる"
        );
    }

    /// TrunBox: 負値と `> i32::MAX` の値が混在する場合は encode 時にエラーとなる
    #[test]
    fn trun_box_cto_mixed_negative_and_above_i32_max_is_encode_error() {
        let trun = TrunBox {
            data_offset: None,
            first_sample_flags: None,
            samples: vec![
                TrunSample {
                    duration: Some(1024),
                    size: Some(512),
                    flags: None,
                    composition_time_offset: Some(-1),
                },
                TrunSample {
                    duration: Some(1024),
                    size: Some(512),
                    flags: None,
                    composition_time_offset: Some(i32::MAX as i64 + 1),
                },
            ],
        };
        assert_eq!(trun.full_box_version(), 1);
        assert!(
            trun.encode_to_vec().is_err(),
            "負値と > i32::MAX の混在は version 1 でも書けないため encode 時にエラーとなる"
        );
    }

    /// SidxBox: version 0 (32-bit values)
    #[test]
    fn sidx_box_version0() {
        let sidx = SidxBox {
            reference_id: 1,
            timescale: 90000,
            earliest_presentation_time: u32::MAX as u64,
            first_offset: u32::MAX as u64,
            references: vec![],
        };
        assert_eq!(sidx.full_box_version(), 0);

        let encoded = sidx.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = SidxBox::decode(&encoded)
            .expect("直前にエンコードした有効な SidxBox は必ずデコードできる");
        assert_eq!(decoded.earliest_presentation_time, u32::MAX as u64);
    }

    /// SidxBox: version 1 (64-bit values)
    #[test]
    fn sidx_box_version1() {
        let sidx = SidxBox {
            reference_id: 1,
            timescale: 90000,
            earliest_presentation_time: u32::MAX as u64 + 1,
            first_offset: 0,
            references: vec![],
        };
        assert_eq!(sidx.full_box_version(), 1);

        let encoded = sidx.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = SidxBox::decode(&encoded)
            .expect("直前にエンコードした有効な SidxBox は必ずデコードできる");
        assert_eq!(decoded.earliest_presentation_time, u32::MAX as u64 + 1);
    }

    /// SidxBox: 複数の参照
    #[test]
    fn sidx_box_multiple_references() {
        let sidx = SidxBox {
            reference_id: 1,
            timescale: 90000,
            earliest_presentation_time: 0,
            first_offset: 0,
            references: vec![
                SidxReference {
                    reference_type: false,
                    referenced_size: 10000,
                    subsegment_duration: 90000,
                    starts_with_sap: true,
                    sap_type: 1,
                    sap_delta_time: 0,
                },
                SidxReference {
                    reference_type: true,
                    referenced_size: 5000,
                    subsegment_duration: 45000,
                    starts_with_sap: false,
                    sap_type: 0,
                    sap_delta_time: 1000,
                },
            ],
        };

        let encoded = sidx.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = SidxBox::decode(&encoded)
            .expect("直前にエンコードした有効な SidxBox は必ずデコードできる");
        assert_eq!(decoded.references.len(), 2);
        assert!(!decoded.references[0].reference_type);
        assert!(decoded.references[1].reference_type);
        assert_eq!(decoded.references[0].referenced_size, 10000);
        assert_eq!(decoded.references[1].sap_delta_time, 1000);
    }

    /// MoofBox: 最小構成
    #[test]
    fn moof_box_minimal() {
        let moof = MoofBox {
            mfhd_box: MfhdBox { sequence_number: 1 },
            traf_boxes: vec![],
            unknown_boxes: vec![],
        };

        let encoded = moof.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MoofBox::decode(&encoded)
            .expect("直前にエンコードした有効な MoofBox は必ずデコードできる");
        assert_eq!(decoded.mfhd_box.sequence_number, 1);
        assert!(decoded.traf_boxes.is_empty());
    }

    /// MoofBox: 複数の traf
    #[test]
    fn moof_box_multiple_traf() {
        let moof = MoofBox {
            mfhd_box: MfhdBox { sequence_number: 1 },
            traf_boxes: vec![
                TrafBox {
                    tfhd_box: TfhdBox {
                        track_id: 1,
                        base_data_offset: None,
                        sample_description_index: None,
                        default_sample_duration: None,
                        default_sample_size: None,
                        default_sample_flags: None,
                        duration_is_empty: false,
                        default_base_is_moof: true,
                    },
                    tfdt_box: Some(TfdtBox {
                        version: 0,
                        base_media_decode_time: 0,
                    }),
                    trun_boxes: vec![],
                    unknown_boxes: vec![],
                },
                TrafBox {
                    tfhd_box: TfhdBox {
                        track_id: 2,
                        base_data_offset: None,
                        sample_description_index: None,
                        default_sample_duration: None,
                        default_sample_size: None,
                        default_sample_flags: None,
                        duration_is_empty: false,
                        default_base_is_moof: true,
                    },
                    tfdt_box: None,
                    trun_boxes: vec![],
                    unknown_boxes: vec![],
                },
            ],
            unknown_boxes: vec![],
        };

        let encoded = moof.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MoofBox::decode(&encoded)
            .expect("直前にエンコードした有効な MoofBox は必ずデコードできる");
        assert_eq!(decoded.traf_boxes.len(), 2);
        assert_eq!(decoded.traf_boxes[0].tfhd_box.track_id, 1);
        assert_eq!(decoded.traf_boxes[1].tfhd_box.track_id, 2);
        assert!(decoded.traf_boxes[0].tfdt_box.is_some());
        assert!(decoded.traf_boxes[1].tfdt_box.is_none());
    }

    /// MvexBox: 最小構成
    #[test]
    fn mvex_box_minimal() {
        let mvex = MvexBox {
            mehd_box: None,
            trex_boxes: vec![],
            unknown_boxes: vec![],
        };

        let encoded = mvex.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MvexBox::decode(&encoded)
            .expect("直前にエンコードした有効な MvexBox は必ずデコードできる");
        assert!(decoded.mehd_box.is_none());
        assert!(decoded.trex_boxes.is_empty());
    }

    /// MvexBox: mehd と複数の trex
    #[test]
    fn mvex_box_full() {
        let mvex = MvexBox {
            mehd_box: Some(MehdBox {
                fragment_duration: 1000000,
            }),
            trex_boxes: vec![
                TrexBox {
                    track_id: 1,
                    default_sample_description_index: 1,
                    default_sample_duration: 1024,
                    default_sample_size: 0,
                    default_sample_flags: SampleFlags::new(0x01010000),
                },
                TrexBox {
                    track_id: 2,
                    default_sample_description_index: 1,
                    default_sample_duration: 1024,
                    default_sample_size: 0,
                    default_sample_flags: SampleFlags::new(0x02000000),
                },
            ],
            unknown_boxes: vec![],
        };

        let encoded = mvex.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MvexBox::decode(&encoded)
            .expect("直前にエンコードした有効な MvexBox は必ずデコードできる");
        assert!(decoded.mehd_box.is_some());
        assert_eq!(
            decoded
                .mehd_box
                .expect("直前の is_some 検証で Some であることを確認済み")
                .fragment_duration,
            1000000
        );
        assert_eq!(decoded.trex_boxes.len(), 2);
        assert_eq!(decoded.trex_boxes[0].track_id, 1);
        assert_eq!(decoded.trex_boxes[1].track_id, 2);
    }

    /// BaseBox::box_type テスト
    #[test]
    fn fmp4_box_types() {
        use shiguredo_mp4::BoxType;

        assert_eq!(MoofBox::TYPE, BoxType::Normal(*b"moof"));
        assert_eq!(MfhdBox::TYPE, BoxType::Normal(*b"mfhd"));
        assert_eq!(TrafBox::TYPE, BoxType::Normal(*b"traf"));
        assert_eq!(TfhdBox::TYPE, BoxType::Normal(*b"tfhd"));
        assert_eq!(TrunBox::TYPE, BoxType::Normal(*b"trun"));
        assert_eq!(TfdtBox::TYPE, BoxType::Normal(*b"tfdt"));
        assert_eq!(SidxBox::TYPE, BoxType::Normal(*b"sidx"));
        assert_eq!(MvexBox::TYPE, BoxType::Normal(*b"mvex"));
        assert_eq!(MehdBox::TYPE, BoxType::Normal(*b"mehd"));
        assert_eq!(TrexBox::TYPE, BoxType::Normal(*b"trex"));
    }

    /// MoofBox の children テスト
    #[test]
    fn moof_box_children() {
        let moof = MoofBox {
            mfhd_box: MfhdBox { sequence_number: 1 },
            traf_boxes: vec![TrafBox {
                tfhd_box: TfhdBox {
                    track_id: 1,
                    base_data_offset: None,
                    sample_description_index: None,
                    default_sample_duration: None,
                    default_sample_size: None,
                    default_sample_flags: None,
                    duration_is_empty: false,
                    default_base_is_moof: true,
                },
                tfdt_box: None,
                trun_boxes: vec![],
                unknown_boxes: vec![],
            }],
            unknown_boxes: vec![],
        };

        let children: Vec<_> = moof.children().collect();
        assert_eq!(children.len(), 2); // mfhd + 1 traf
    }

    /// TfraBox: version = 0 の上限値ラウンドトリップ
    #[test]
    fn tfra_box_version0_max_boundaries() {
        let tfra = TfraBox {
            version: 0,
            track_id: u32::MAX,
            length_size_of_traf_num: 3,
            length_size_of_trun_num: 3,
            length_size_of_sample_num: 3,
            entries: vec![TfraEntry {
                time: u32::MAX as u64,
                moof_offset: u32::MAX as u64,
                traf_number: u32::MAX,
                trun_number: u32::MAX,
                sample_number: u32::MAX,
            }],
        };
        assert_eq!(tfra.full_box_version(), 0);

        let encoded = tfra.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfraBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfraBox は必ずデコードできる");
        assert_eq!(decoded, tfra);
    }

    /// TfraBox: self.version = 0 でも time > u32::MAX で自動的に version = 1 に昇格する
    #[test]
    fn tfra_box_version_auto_promotion() {
        let tfra = TfraBox {
            version: 0,
            track_id: 1,
            length_size_of_traf_num: 0,
            length_size_of_trun_num: 0,
            length_size_of_sample_num: 0,
            entries: vec![TfraEntry {
                time: u32::MAX as u64 + 1,
                moof_offset: 0,
                traf_number: 1,
                trun_number: 1,
                sample_number: 1,
            }],
        };
        assert_eq!(
            tfra.full_box_version(),
            1,
            "time が u32::MAX を超えるので version は 1 に昇格する"
        );

        let encoded = tfra.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfraBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfraBox は必ずデコードできる");
        assert_eq!(
            decoded.version, 1,
            "decode 側は書かれた版数 1 をそのまま self.version に戻す"
        );
        assert_eq!(decoded.entries, tfra.entries);
    }

    /// TfraBox: `length_size_of_*` = 0 の最小構成ラウンドトリップ
    #[test]
    fn tfra_box_length_size_zero() {
        let tfra = TfraBox {
            version: 0,
            track_id: 1,
            length_size_of_traf_num: 0,
            length_size_of_trun_num: 0,
            length_size_of_sample_num: 0,
            entries: vec![TfraEntry {
                time: 0,
                moof_offset: 0,
                traf_number: 0xFF,
                trun_number: 0xFF,
                sample_number: 0xFF,
            }],
        };

        let encoded = tfra.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfraBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfraBox は必ずデコードできる");
        assert_eq!(decoded, tfra);
    }

    /// TfraBox: entries が空
    #[test]
    fn tfra_box_empty_entries() {
        let tfra = TfraBox {
            version: 0,
            track_id: 1,
            length_size_of_traf_num: 0,
            length_size_of_trun_num: 0,
            length_size_of_sample_num: 0,
            entries: vec![],
        };

        let encoded = tfra.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfraBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfraBox は必ずデコードできる");
        assert_eq!(decoded, tfra);
    }
}
