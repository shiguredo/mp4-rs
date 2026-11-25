//! Fragmented MP4 (fMP4) ボックスの Property-Based Testing
//!
//! MoofBox, MfhdBox, TrafBox, TfhdBox, TrunBox, TfdtBox, SidxBox, StypBox,
//! MvexBox, TrexBox, MehdBox, TfraBox, MfraBox のテスト

use noprop::TestCaseContext;
use shiguredo_mp4::{
    Decode, Encode, ErrorKind, SampleFlags,
    boxes::{
        Brand, FreeBox, MehdBox, MfhdBox, MfraBox, MfroBox, MoofBox, MvexBox, SidxBox,
        SidxReference, StypBox, TfdtBox, TfhdBox, TfraBox, TfraEntry, TrafBox, TrexBox, TrunBox,
        TrunSample, TrunSamples,
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

/// 任意の 4 バイトのブランドを生成する
fn arb_brand(ctx: &mut TestCaseContext) -> Brand {
    Brand::new(noprop::sample_bytes::<4>(ctx))
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
///
/// バージョン 0 と 32 ビットに収まる値、バージョン 1 と 32 ビットに収まる値、
/// バージョン 1 と 64 ビットが必要な値の 3 通りを明示的に生成する
fn arb_tfdt_box(ctx: &mut TestCaseContext) -> TfdtBox {
    let (version, base_media_decode_time) = match noprop::sample_usize_in(ctx, 0..3) {
        0 => (0, noprop::sample_u64_in(ctx, 0..=u64::from(u32::MAX))),
        1 => (1, noprop::sample_u64_in(ctx, 0..=u64::from(u32::MAX))),
        _ => (
            1,
            noprop::sample_u64_in(ctx, u64::from(u32::MAX) + 1..=u64::MAX),
        ),
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

    // 0 件の場合と、各サンプル行にフィールドが 1 つもない場合は、件数だけの形式（`TrunSamples::CountOnly`）で生成する。
    // どちらもサンプルごとの値を持つ形式で表すと件数だけの形式と同じバイト列になり、
    // デコードでは件数だけの形式になるため
    let has_any_field = has_duration || has_size || has_flags || has_cto;
    let samples = if count == 0 || !has_any_field {
        TrunSamples::CountOnly(u32::try_from(count).expect("サンプル数は u32 に収まる"))
    } else {
        let mut samples = Vec::new();
        for _ in 0..count {
            samples.push(TrunSample {
                duration: sample_dur(ctx),
                size: sample_size(ctx),
                flags: sample_flg(ctx),
                composition_time_offset: sample_cto(ctx),
            });
        }
        TrunSamples::PerSample(samples)
    };
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
        samples: TrunSamples::PerSample(samples),
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
///
/// バージョン 0 と 32 ビットに収まる値、バージョン 1 と 32 ビットに収まる値、
/// バージョン 1 と 64 ビットまでの値の 3 通りを明示的に生成する
fn arb_tfra_box(ctx: &mut TestCaseContext) -> TfraBox {
    let track_id = noprop::sample_u32(ctx);
    let (version, max_time_and_moof_offset) = match noprop::sample_usize_in(ctx, 0..3) {
        0 => (0, u64::from(u32::MAX)),
        1 => (1, u64::from(u32::MAX)),
        _ => (1, u64::MAX),
    };
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

/// バージョンが 2〜255 の FullBox のバイト列を生成する
///
/// `tfdt` / `tfra` では、このライブラリが対応しないバージョンにあたる。
/// フラグと本文はランダムなバイト列にする。読み飛ばす子も `unknown_boxes` に保持する子も本文を解釈しないため、
/// 本文が構文に従う必要はない。
/// ヘッダーは 32 ビットの size と 64 ビットの largesize のどちらかをランダムに選ぶ。
/// largesize を混ぜるのは、読み飛ばす子では読み飛ばす長さに実際のヘッダー長（largesize では 16 バイト）が使われること、
/// `unknown_boxes` に保持する子では再エンコードしてもヘッダーの形式が変わらないことを確かめるため
fn arb_unsupported_version_full_box(ctx: &mut TestCaseContext, box_type: &[u8; 4]) -> Vec<u8> {
    let version = noprop::sample_u64_in(ctx, 2..=255) as u8;
    let flags = noprop::sample_bytes_vec(ctx, 3);
    let body_len = noprop::sample_usize_in(ctx, 0..16);
    let body = noprop::sample_bytes_vec(ctx, body_len);

    let mut payload = vec![version];
    payload.extend_from_slice(&flags);
    payload.extend_from_slice(&body);
    if noprop::sample_bool(ctx) {
        wrap_box(box_type, &[payload])
    } else {
        wrap_box_with_largesize(box_type, &payload)
    }
}

/// ペイロードを、size=1 と 64 ビットの largesize を持つヘッダーで包む
fn wrap_box_with_largesize(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let largesize =
        u64::try_from(16 + payload.len()).expect("テスト用のボックスは u64 に収まる大きさ");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(box_type);
    bytes.extend_from_slice(&largesize.to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

/// ペイロードのバイト列（子ボックス列など）を連結し、通常ヘッダー（32 ビットの size と種別）で包む
fn wrap_box(box_type: &[u8; 4], children: &[Vec<u8>]) -> Vec<u8> {
    let payload_len: usize = children.iter().map(Vec::len).sum();
    let size = u32::try_from(8 + payload_len).expect("テスト用のボックスは u32 に収まる大きさ");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&size.to_be_bytes());
    bytes.extend_from_slice(box_type);
    for child in children {
        bytes.extend_from_slice(child);
    }
    bytes
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
        // バージョンも含めて保持されることを確認する
        assert_eq!(decoded, tfdt);
        Ok(())
    })?;
    Ok(())
}

/// バージョン 0 を指定した TfdtBox で 64 ビットが必要な値を持つ場合、バージョン 1 として出力されること
#[test]
fn tfdt_box_version0_with_64bit_value_is_encoded_as_version1() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let tfdt = TfdtBox {
            version: 0,
            base_media_decode_time: noprop::sample_u64_in(ctx, u64::from(u32::MAX) + 1..=u64::MAX),
        };
        let encoded = tfdt.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfdtBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfdtBox は必ずデコードできる");

        let expected = TfdtBox {
            version: 1,
            base_media_decode_time: tfdt.base_media_decode_time,
        };
        assert_eq!(decoded, expected);
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
        assert_eq!(decoded, trun);
        Ok(())
    })?;
    Ok(())
}

/// サンプルごとのフィールドがない TrunBox は、件数だけの形式で論理件数を保つ
///
/// 件数だけの形式と、全フィールドが `None` のサンプル行を並べた形式の両方から始める。
/// どちらもサンプル行を書かずに同じバイト列になり、デコードすると件数だけの形式になること、
/// 論理件数と `data_offset`、`first_sample_flags` が再エンコードと再デコードを経ても変わらないことを確認する
#[test]
fn trun_box_without_per_sample_fields_keeps_count() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
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
        let count =
            u32::try_from(noprop::sample_usize_in(ctx, 0..10)).expect("サンプル数は u32 に収まる");

        let count_only = TrunBox {
            data_offset,
            first_sample_flags,
            samples: TrunSamples::CountOnly(count),
        };
        let empty_rows = TrunBox {
            data_offset,
            first_sample_flags,
            samples: TrunSamples::PerSample(
                (0..count)
                    .map(|_| TrunSample {
                        duration: None,
                        size: None,
                        flags: None,
                        composition_time_offset: None,
                    })
                    .collect(),
            ),
        };

        let encoded = count_only
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");
        // 全フィールドが None のサンプル行は 1 バイトも書かれないため、件数だけの形式と同じバイト列になる
        assert_eq!(
            empty_rows
                .encode_to_vec()
                .expect("Vec への書き込みは失敗しない"),
            encoded
        );
        // 通常ヘッダー 8 バイト、FullBox ヘッダー 4 バイト、sample_count 4 バイトと、
        // data_offset と first_sample_flags（各 4 バイト）だけからなり、サンプル行を含まない
        let expected_len = 16
            + if data_offset.is_some() { 4 } else { 0 }
            + if first_sample_flags.is_some() { 4 } else { 0 };
        assert_eq!(encoded.len(), expected_len);

        let (decoded, size) = TrunBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrunBox は必ずデコードできる");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded.samples, TrunSamples::CountOnly(count));
        assert_eq!(decoded.data_offset, data_offset);
        assert_eq!(decoded.first_sample_flags, first_sample_flags);

        // 再エンコードと再デコードを経ても変わらない
        let reencoded = decoded
            .encode_to_vec()
            .expect("デコードした TrunBox は必ずエンコードできる");
        assert_eq!(reencoded, encoded);
        let (redecoded, _) =
            TrunBox::decode(&reencoded).expect("再エンコードした TrunBox は必ずデコードできる");
        assert_eq!(redecoded, count_only);
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

/// バージョン 0 を指定した TfraBox で 64 ビットが必要な値を持つ場合、バージョン 1 として出力されること
///
/// 64 ビットが必要な time または moof_offset を持つエントリーを、任意の位置に 1 つ差し込む
#[test]
fn tfra_box_version0_with_64bit_value_is_encoded_as_version1() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let mut tfra = arb_tfra_box(ctx);
        tfra.version = 0;
        let large = noprop::sample_u64_in(ctx, u64::from(u32::MAX) + 1..=u64::MAX);
        let (time, moof_offset) = if noprop::sample_bool(ctx) {
            (large, 0)
        } else {
            (0, large)
        };
        let position = noprop::sample_usize_in(ctx, 0..=tfra.entries.len());
        tfra.entries.insert(
            position,
            TfraEntry {
                time,
                moof_offset,
                traf_number: 0,
                trun_number: 0,
                sample_number: 0,
            },
        );

        let encoded = tfra.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfraBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfraBox は必ずデコードできる");

        let mut expected = tfra.clone();
        expected.version = 1;
        assert_eq!(decoded, expected);
        Ok(())
    })?;
    Ok(())
}

// ===== 未対応バージョンの子ボックスの扱いのテスト =====

/// TrafBox の子に未対応バージョンの tfdt を差し込んでも、デコード結果が元の TrafBox と等しいこと
///
/// 未対応バージョンの tfdt は読み飛ばされ、`tfdt_box` にも `unknown_boxes` にも入らない。
/// 採用済みの tfdt がある場合はその直後に必ず 1 つ差し込み、さらに任意の位置に 0〜2 個差し込む。
/// 採用済みの tfdt がない場合は、任意の位置に 1〜2 個差し込む
#[test]
fn traf_box_skips_unsupported_version_tfdt() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let traf = arb_traf_box(ctx);

        let mut children = vec![
            traf.tfhd_box
                .encode_to_vec()
                .expect("Vec への書き込みは失敗しない"),
        ];
        if let Some(tfdt_box) = &traf.tfdt_box {
            children.push(
                tfdt_box
                    .encode_to_vec()
                    .expect("Vec への書き込みは失敗しない"),
            );
            // 採用済みの tfdt の直後に置いても、重複した tfdt として UnknownBox に残らないことを確かめる
            children.push(arb_unsupported_version_full_box(ctx, b"tfdt"));
        }
        for trun_box in &traf.trun_boxes {
            children.push(
                trun_box
                    .encode_to_vec()
                    .expect("Vec への書き込みは失敗しない"),
            );
        }
        // 採用済みの tfdt がない場合も、未対応バージョンの tfdt を少なくとも 1 つ差し込む
        let min_extra = if traf.tfdt_box.is_some() { 0 } else { 1 };
        for _ in 0..noprop::sample_usize_in(ctx, min_extra..3) {
            let position = noprop::sample_usize_in(ctx, 0..=children.len());
            children.insert(position, arb_unsupported_version_full_box(ctx, b"tfdt"));
        }
        let input = wrap_box(b"traf", &children);

        let (decoded, consumed) =
            TrafBox::decode(&input).expect("未対応バージョンの tfdt を含む traf はデコードできる");
        assert_eq!(consumed, input.len());
        assert_eq!(decoded, traf);
        Ok(())
    })?;
    Ok(())
}

/// tfdt 以外の子は、FullBox のバージョンが 2 以上でも読み飛ばさず、`unknown_boxes` として保持されること
///
/// traf の末尾（`TrafBox::encode` が `unknown_boxes` を書き出す位置）に、バージョンが 2 以上の FullBox 形式の
/// sgpd を 1〜2 個置く。未対応バージョンの判定を子の種別によらず行うと、sgpd が捨てられて再エンコードした出力が変わる
#[test]
fn traf_box_keeps_non_tfdt_child_with_unsupported_version() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let traf = arb_traf_box(ctx);
        let mut traf_bytes = traf.encode_to_vec().expect("Vec への書き込みは失敗しない");
        // traf のヘッダーを外してペイロードだけにし、末尾に sgpd を足してから包み直す
        let mut children = vec![traf_bytes.split_off(8)];
        let sgpd_count = noprop::sample_usize_in(ctx, 1..3);
        for _ in 0..sgpd_count {
            children.push(arb_unsupported_version_full_box(ctx, b"sgpd"));
        }
        let input = wrap_box(b"traf", &children);

        let (decoded, consumed) = TrafBox::decode(&input)
            .expect("バージョンが 2 以上の sgpd を含む traf はデコードできる");
        assert_eq!(consumed, input.len());
        assert_eq!(decoded.unknown_boxes.len(), sgpd_count);
        assert!(
            decoded
                .unknown_boxes
                .iter()
                .all(|b| b.box_type.as_bytes() == b"sgpd"),
            "unknown_boxes に sgpd 以外が入っている"
        );
        let encoded = decoded
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");
        assert_eq!(encoded, input, "再エンコードした出力が入力と一致しない");
        Ok(())
    })?;
    Ok(())
}

/// MfraBox の子に未対応バージョンの tfra を差し込んでも、`tfra_boxes` が元と等しいこと
///
/// 未対応バージョンの tfra は読み飛ばされ、`tfra_boxes` に入らない。tfra 列の先頭・間・末尾（mfro の直前）の
/// いずれかに 1〜2 個差し込む
#[test]
fn mfra_box_skips_unsupported_version_tfra() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let tfra_boxes = sample_vec(ctx, 0..3, arb_tfra_box);
        let mut children: Vec<Vec<u8>> = tfra_boxes
            .iter()
            .map(|b| b.encode_to_vec().expect("Vec への書き込みは失敗しない"))
            .collect();
        for _ in 0..noprop::sample_usize_in(ctx, 1..3) {
            let position = noprop::sample_usize_in(ctx, 0..=children.len());
            children.insert(position, arb_unsupported_version_full_box(ctx, b"tfra"));
        }

        // mfro は固定長 16 バイトで、size には入力の mfra 全体のバイト数を入れる
        let input_size = 8 + children.iter().map(Vec::len).sum::<usize>() + 16;
        let mfro_size = u32::try_from(input_size).expect("入力の mfra は u32 に収まる大きさ");
        children.push(
            MfroBox { size: mfro_size }
                .encode_to_vec()
                .expect("Vec への書き込みは失敗しない"),
        );
        let input = wrap_box(b"mfra", &children);
        assert_eq!(input.len(), input_size);

        let (decoded, consumed) =
            MfraBox::decode(&input).expect("未対応バージョンの tfra を含む mfra はデコードできる");
        assert_eq!(consumed, input.len());
        assert_eq!(decoded.tfra_boxes, tfra_boxes);
        assert_eq!(decoded.mfro_box.size, mfro_size);
        Ok(())
    })?;
    Ok(())
}

// ===== MfraBox のテスト =====

/// MfraBox の decode → encode で、mfro.size が再エンコード後の mfra 全体のバイト数になる
///
/// 入力の mfra は通常ヘッダー、tfra 列、任意の位置に挟んだ free（0 個または 1 個）、末尾の mfro からなる。
/// free は MfraBox が保持しない未知の子ボックスなので、decode で読み飛ばされ再エンコードで消える。
/// 入力の mfro.size には、再エンコード後の実サイズと必ず食い違う値を入れる。
#[test]
fn mfra_box_decode_encode_rewrites_mfro_size() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let tfra_boxes = sample_vec(ctx, 0..3, arb_tfra_box);
        let tfra_bytes: Vec<Vec<u8>> = tfra_boxes
            .iter()
            .map(|b| b.encode_to_vec().expect("Vec への書き込みは失敗しない"))
            .collect();

        // free を挟む場合は、tfra 列の先頭・間・末尾（mfro の直前）のいずれかに置く
        let mut children = tfra_bytes.clone();
        let free_size = if noprop::sample_bool(ctx) {
            let payload_len = noprop::sample_usize_in(ctx, 0..16);
            let free = FreeBox {
                payload: noprop::sample_bytes_vec(ctx, payload_len),
            };
            let free_bytes = free.encode_to_vec().expect("Vec への書き込みは失敗しない");
            let size = free_bytes.len();
            let position = noprop::sample_usize_in(ctx, 0..=children.len());
            children.insert(position, free_bytes);
            Some(size)
        } else {
            None
        };

        // mfro は通常ヘッダー（8 バイト）、FullBox ヘッダー（4 バイト）、32 ビットの size からなる
        // 固定長 16 バイトのボックス
        let mfro_box_size = 16;
        let input_size = 8 + children.iter().map(Vec::len).sum::<usize>() + mfro_box_size;
        let input_size_u32 = u32::try_from(input_size).expect("入力の mfra は u32 に収まる大きさ");
        let stale_mfro_size = match free_size {
            // 入力の mfra 全体のバイト数（入力の mfra としては正しい値）を入れる。
            // free が消えた後の実サイズとは free のバイト数だけ食い違う
            Some(_) => input_size_u32,
            // 未知の子ボックスがない場合、実サイズは入力と同じなので、それに 0 以外の値を足して食い違わせる
            None => input_size_u32
                .wrapping_add(noprop::sample_u64_in(ctx, 1..=u64::from(u32::MAX)) as u32),
        };

        // 通常ヘッダー（32 ビットの size + 種別）の後に子ボックス列と mfro を並べて入力を組み立てる
        let mut input = Vec::new();
        input.extend_from_slice(&input_size_u32.to_be_bytes());
        input.extend_from_slice(b"mfra");
        for child in &children {
            input.extend_from_slice(child);
        }
        let mfro_bytes = MfroBox {
            size: stale_mfro_size,
        }
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない");
        assert_eq!(mfro_bytes.len(), mfro_box_size, "mfro は固定長 16 バイト");
        input.extend_from_slice(&mfro_bytes);
        assert_eq!(input.len(), input_size);

        let (decoded, consumed) =
            MfraBox::decode(&input).expect("組み立てた有効な mfra は必ずデコードできる");
        assert_eq!(consumed, input.len());
        assert_eq!(decoded.tfra_boxes, tfra_boxes);
        // デコード時は入力の mfro.size をそのまま保持する
        assert_eq!(decoded.mfro_box.size, stale_mfro_size);

        let encoded = decoded
            .encode_to_vec()
            .expect("デコードした MfraBox は必ずエンコードできる");
        // free は再エンコードで消えるため、出力は free のバイト数だけ短くなる
        assert_eq!(encoded.len(), input.len() - free_size.unwrap_or(0));

        // 出力は、free を除いた入力の mfra ヘッダーの size と mfro.size の 2 か所だけを
        // 出力のバイト数に直したバイト列と完全に一致する
        let encoded_size_u32 =
            u32::try_from(encoded.len()).expect("出力の mfra は u32 に収まる大きさ");
        let mut expected = Vec::new();
        expected.extend_from_slice(&encoded_size_u32.to_be_bytes());
        expected.extend_from_slice(b"mfra");
        for tfra in &tfra_bytes {
            expected.extend_from_slice(tfra);
        }
        expected.extend_from_slice(
            &MfroBox {
                size: encoded_size_u32,
            }
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない"),
        );
        assert_eq!(encoded, expected);

        let (redecoded, reconsumed) =
            MfraBox::decode(&encoded).expect("エンコードした MfraBox は必ずデコードできる");
        assert_eq!(reconsumed, encoded.len());
        assert_eq!(redecoded.tfra_boxes, tfra_boxes);
        // mfro.size は入力の値ではなく、再エンコード後の mfra 全体のバイト数と一致する
        assert_eq!(
            redecoded.mfro_box.size, encoded_size_u32,
            "mfro.size が再エンコード後の mfra の実サイズと一致しない"
        );

        // 出力とちょうど同じ長さのバッファにも同じバイト列を書き込め、1 バイト足りないバッファでは
        // InsufficientBuffer になる
        let mut exact = vec![0; encoded.len()];
        let written = decoded
            .encode(&mut exact)
            .expect("出力と同じ長さのバッファには必ずエンコードできる");
        assert_eq!(written, encoded.len());
        assert_eq!(exact, encoded);
        let mut short = vec![0; encoded.len() - 1];
        let err = decoded
            .encode(&mut short)
            .expect_err("出力より 1 バイト短いバッファへのエンコードは必ず失敗する");
        assert_eq!(err.kind, ErrorKind::InsufficientBuffer);
        Ok(())
    })?;
    Ok(())
}

// ===== StypBox のテスト =====

/// StypBox の encode/decode roundtrip
///
/// ISO/IEC 14496-12:2022 8.16.2 の `styp` は `ftyp` と同じ構造
/// (major_brand / minor_version / compatible_brands) を持つ。
/// 任意の値でエンコードし、デコードし直すと元の値に戻ることを確認する。
/// あわせて、ヘッダーの size がボックス全体のバイト数と一致することも確認する。
#[test]
fn styp_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let major_brand = arb_brand(ctx);
        let minor_version = noprop::sample_u32(ctx);
        let compatible_brands_len = noprop::sample_usize_in(ctx, 0..5);
        let mut compatible_brands = Vec::new();
        for _ in 0..compatible_brands_len {
            compatible_brands.push(arb_brand(ctx));
        }
        let styp = StypBox {
            major_brand,
            minor_version,
            compatible_brands,
        };

        let encoded = styp.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let encoded_size = u32::try_from(encoded.len()).expect("StypBox のサイズは u32 に収まる");
        assert_eq!(
            &encoded[..4],
            &encoded_size.to_be_bytes(),
            "styp のヘッダーの size がボックス全体のバイト数と一致しない"
        );
        assert_eq!(
            &encoded[4..8],
            b"styp",
            "styp のヘッダーの box 種別が styp でない"
        );

        let (decoded, size) = StypBox::decode(&encoded)
            .expect("直前にエンコードした有効な StypBox は必ずデコードできる");
        assert_eq!(
            size,
            encoded.len(),
            "decode の消費バイト数がエンコード結果の長さと一致しない"
        );
        assert_eq!(
            decoded, styp,
            "デコードした StypBox がエンコード前の値と一致しない"
        );
        Ok(())
    })?;
    Ok(())
}
