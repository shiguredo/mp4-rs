//! Fragmented MP4 (fMP4) ボックスの Property-Based Testing
//!
//! MoofBox, MfhdBox, TrafBox, TfhdBox, TrunBox, TfdtBox, SidxBox,
//! MvexBox, TrexBox, MehdBox のテスト

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;
use shiguredo_mp4::{
    Decode, Encode, SampleFlags,
    boxes::{
        MehdBox, MfhdBox, MoofBox, MvexBox, SidxBox, SidxReference, TfdtBox, TfhdBox, TfraBox,
        TfraEntry, TrafBox, TrexBox, TrunBox, TrunSample,
    },
};

// ===== Strategy 定義 =====

/// SampleFlags を生成する Strategy
fn arb_sample_flags() -> impl Strategy<Value = SampleFlags> {
    any::<u32>().prop_map(SampleFlags::new)
}

/// TrexBox を生成する Strategy
fn arb_trex_box() -> impl Strategy<Value = TrexBox> {
    (
        any::<u32>(), // track_id
        any::<u32>(), // default_sample_description_index
        any::<u32>(), // default_sample_duration
        any::<u32>(), // default_sample_size
        arb_sample_flags(),
    )
        .prop_map(
            |(
                track_id,
                default_sample_description_index,
                default_sample_duration,
                default_sample_size,
                default_sample_flags,
            )| TrexBox {
                track_id,
                default_sample_description_index,
                default_sample_duration,
                default_sample_size,
                default_sample_flags,
            },
        )
}

/// MehdBox を生成する Strategy
fn arb_mehd_box() -> impl Strategy<Value = MehdBox> {
    any::<u64>().prop_map(|fragment_duration| MehdBox { fragment_duration })
}

/// MvexBox を生成する Strategy
fn arb_mvex_box() -> impl Strategy<Value = MvexBox> {
    (
        prop::option::of(arb_mehd_box()),
        prop::collection::vec(arb_trex_box(), 0..3),
    )
        .prop_map(|(mehd_box, trex_boxes)| MvexBox {
            mehd_box,
            trex_boxes,
            unknown_boxes: vec![],
        })
}

/// MfhdBox を生成する Strategy
fn arb_mfhd_box() -> impl Strategy<Value = MfhdBox> {
    any::<u32>().prop_map(|sequence_number| MfhdBox { sequence_number })
}

/// TfdtBox を生成する Strategy
fn arb_tfdt_box() -> impl Strategy<Value = TfdtBox> {
    (any::<u64>(), 0u8..=1u8).prop_map(|(base_media_decode_time, version)| {
        // 値が 32-bit に収まらない場合は version=1 が必須
        let version = if base_media_decode_time > u32::MAX as u64 {
            1
        } else {
            version
        };
        TfdtBox {
            version,
            base_media_decode_time,
        }
    })
}

/// TfhdBox を生成する Strategy
fn arb_tfhd_box() -> impl Strategy<Value = TfhdBox> {
    (
        any::<u32>(), // track_id
        prop::option::of(any::<u64>()),
        prop::option::of(any::<u32>()),
        prop::option::of(any::<u32>()),
        prop::option::of(any::<u32>()),
        prop::option::of(arb_sample_flags()),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(
                track_id,
                base_data_offset,
                sample_description_index,
                default_sample_duration,
                default_sample_size,
                default_sample_flags,
                duration_is_empty,
                default_base_is_moof,
            )| TfhdBox {
                track_id,
                base_data_offset,
                sample_description_index,
                default_sample_duration,
                default_sample_size,
                default_sample_flags,
                duration_is_empty,
                default_base_is_moof,
            },
        )
}

/// TrunBox を生成する Strategy (一貫性のあるサンプル)
fn arb_trun_box() -> impl Strategy<Value = TrunBox> {
    (
        prop::option::of(any::<i32>()),
        prop::option::of(arb_sample_flags()),
        // サンプルは一貫性を持たせる（全てのサンプルが同じオプションフィールドを持つ）
        (
            any::<bool>(), // has_duration
            any::<bool>(), // has_size
            any::<bool>(), // has_flags
            any::<bool>(), // has_composition_time_offset
            any::<bool>(), // cto の符号側: true = signed (version 1)、false = unsigned (version 0)
            0usize..10,    // sample_count
        ),
    )
        .prop_flat_map(
            |(
                data_offset,
                first_sample_flags,
                (has_duration, has_size, has_flags, has_cto, signed_side, count),
            )| {
                let duration_strategy: BoxedStrategy<Option<u32>> = if has_duration {
                    any::<u32>().prop_map(Some).boxed()
                } else {
                    Just(None).boxed()
                };
                let size_strategy: BoxedStrategy<Option<u32>> = if has_size {
                    any::<u32>().prop_map(Some).boxed()
                } else {
                    Just(None).boxed()
                };
                let flags_strategy: BoxedStrategy<Option<SampleFlags>> = if has_flags {
                    arb_sample_flags().prop_map(Some).boxed()
                } else {
                    Just(None).boxed()
                };
                // ISO/IEC 14496-12 8.8.8: composition_time_offset は version 0 で
                // `0..=u32::MAX`、version 1 で `i32::MIN..=i32::MAX` の範囲を許容する。
                // 負値と `> i32::MAX` は同一 TrunBox 内に混在させると encode 時に
                // どちらの版でも表現できず invalid_input になるため、TrunBox 単位で
                // 「符号あり側」か「符号なし側」のどちらか一方に統一して探索する。
                let cto_strategy: BoxedStrategy<Option<i64>> = if has_cto {
                    if signed_side {
                        // version 1 の許容範囲
                        ((i32::MIN as i64)..=(i32::MAX as i64))
                            .prop_map(Some)
                            .boxed()
                    } else {
                        // version 0 の許容範囲
                        (0i64..=(u32::MAX as i64)).prop_map(Some).boxed()
                    }
                } else {
                    Just(None).boxed()
                };

                let sample_strategy = (
                    duration_strategy,
                    size_strategy,
                    flags_strategy,
                    cto_strategy,
                )
                    .prop_map(
                        |(duration, size, flags, composition_time_offset)| TrunSample {
                            duration,
                            size,
                            flags,
                            composition_time_offset,
                        },
                    );

                prop::collection::vec(sample_strategy, count).prop_map(move |samples| TrunBox {
                    data_offset,
                    first_sample_flags,
                    samples,
                })
            },
        )
}

/// TrafBox を生成する Strategy
fn arb_traf_box() -> impl Strategy<Value = TrafBox> {
    (
        arb_tfhd_box(),
        prop::option::of(arb_tfdt_box()),
        prop::collection::vec(arb_trun_box(), 0..3),
    )
        .prop_map(|(tfhd_box, tfdt_box, trun_boxes)| TrafBox {
            tfhd_box,
            tfdt_box,
            trun_boxes,
            unknown_boxes: vec![],
        })
}

/// MoofBox を生成する Strategy
fn arb_moof_box() -> impl Strategy<Value = MoofBox> {
    (arb_mfhd_box(), prop::collection::vec(arb_traf_box(), 0..3)).prop_map(
        |(mfhd_box, traf_boxes)| MoofBox {
            mfhd_box,
            traf_boxes,
            unknown_boxes: vec![],
        },
    )
}

/// SidxReference を生成する Strategy
fn arb_sidx_reference() -> impl Strategy<Value = SidxReference> {
    (
        any::<bool>(),
        0u32..0x7FFFFFFF,
        any::<u32>(),
        any::<bool>(),
        0u8..8,
        0u32..0x0FFFFFFF,
    )
        .prop_map(
            |(
                reference_type,
                referenced_size,
                subsegment_duration,
                starts_with_sap,
                sap_type,
                sap_delta_time,
            )| SidxReference {
                reference_type,
                referenced_size,
                subsegment_duration,
                starts_with_sap,
                sap_type,
                sap_delta_time,
            },
        )
}

/// SidxBox を生成する Strategy
fn arb_sidx_box() -> impl Strategy<Value = SidxBox> {
    (
        any::<u32>(),
        any::<u32>(),
        any::<u64>(),
        any::<u64>(),
        prop::collection::vec(arb_sidx_reference(), 0..10),
    )
        .prop_map(
            |(reference_id, timescale, earliest_presentation_time, first_offset, references)| {
                SidxBox {
                    reference_id,
                    timescale,
                    earliest_presentation_time,
                    first_offset,
                    references,
                }
            },
        )
}

/// TfraEntry を生成する Strategy
///
/// 各フィールドの上限は呼び出し側から与える
/// （上限を絞る理由は `arb_tfra_box` の doc を参照）。
fn arb_tfra_entry(
    max_traf: u32,
    max_trun: u32,
    max_sample: u32,
    max_time: u64,
    max_moof_offset: u64,
) -> impl Strategy<Value = TfraEntry> {
    (
        0u64..=max_time,
        0u64..=max_moof_offset,
        0u32..=max_traf,
        0u32..=max_trun,
        0u32..=max_sample,
    )
        .prop_map(
            |(time, moof_offset, traf_number, trun_number, sample_number)| TfraEntry {
                time,
                moof_offset,
                traf_number,
                trun_number,
                sample_number,
            },
        )
}

/// TfraBox を生成する Strategy
///
/// version と `length_size_*` を先に決めたうえで `prop_flat_map` に入り、
/// 対応する上限に絞った `TfraEntry` を生成する。
///
/// - version は 0 / 1 のいずれか（ISO/IEC 14496-12 での有効値）
/// - version = 0 のとき `time` / `moof_offset` は `u32` 範囲、
///   version = 1 のとき `u64` 全域（`TfraBox::full_box_version` は
///   `time` / `moof_offset` の実値が `u32::MAX` を超えると自動で version=1 を返すため、
///   version=0 で 64-bit 値を混ぜるとラウンドトリップで元の version=0 が失われる）
/// - `traf_number` / `trun_number` / `sample_number` は対応する `length_size` に応じた
///   `byte_count` バイトに収まる範囲（`length_size = 0` なら上限 `0xFF`、
///   `length_size = 3` なら上限 `u32::MAX`）
fn arb_tfra_box() -> impl Strategy<Value = TfraBox> {
    (
        any::<u32>(), // track_id
        0u8..=1u8,    // version (0 または 1)
        0u8..=3u8,    // length_size_of_traf_num
        0u8..=3u8,    // length_size_of_trun_num
        0u8..=3u8,    // length_size_of_sample_num
    )
        .prop_flat_map(|(track_id, version, l_traf, l_trun, l_sample)| {
            // length_size に応じた u32 の上限
            // （length_size = 3 は u32::MAX、それ以外はシフトで算出）
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
            // time と moof_offset は同じ制約に従う（version = 0 のとき u32 範囲、
            // version = 1 のとき u64 全域）
            let max_time_and_moof_offset = if version == 0 {
                u32::MAX as u64
            } else {
                u64::MAX
            };
            prop::collection::vec(
                arb_tfra_entry(
                    max_traf,
                    max_trun,
                    max_sample,
                    max_time_and_moof_offset,
                    max_time_and_moof_offset,
                ),
                0..3,
            )
            .prop_map(move |entries| TfraBox {
                version,
                track_id,
                length_size_of_traf_num: l_traf,
                length_size_of_trun_num: l_trun,
                length_size_of_sample_num: l_sample,
                entries,
            })
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // ===== SampleFlags のテスト =====

    /// SampleFlags の encode/decode roundtrip
    #[test]
    fn sample_flags_roundtrip(flags in arb_sample_flags()) {
        let encoded = flags.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SampleFlags::decode(&encoded)
            .expect("直前にエンコードした有効な SampleFlags は必ずデコードできる");

        prop_assert_eq!(size, 4);
        prop_assert_eq!(decoded.get(), flags.get());
    }

    /// SampleFlags の各フィールドの取得テスト
    #[test]
    fn sample_flags_fields(
        is_leading in 0u8..4,
        sample_depends_on in 0u8..4,
        sample_is_depended_on in 0u8..4,
        sample_has_redundancy in 0u8..4,
        sample_padding_value in 0u8..8,
        sample_is_non_sync_sample in any::<bool>(),
        sample_degradation_priority in any::<u16>()
    ) {
        let flags = SampleFlags::from_fields(
            is_leading,
            sample_depends_on,
            sample_is_depended_on,
            sample_has_redundancy,
            sample_padding_value,
            sample_is_non_sync_sample,
            sample_degradation_priority,
        );

        prop_assert_eq!(flags.is_leading(), is_leading);
        prop_assert_eq!(flags.sample_depends_on(), sample_depends_on);
        prop_assert_eq!(flags.sample_is_depended_on(), sample_is_depended_on);
        prop_assert_eq!(flags.sample_has_redundancy(), sample_has_redundancy);
        prop_assert_eq!(flags.sample_padding_value(), sample_padding_value);
        prop_assert_eq!(flags.sample_is_non_sync_sample(), sample_is_non_sync_sample);
        prop_assert_eq!(flags.sample_degradation_priority(), sample_degradation_priority);
    }

    // ===== TrexBox のテスト =====

    /// TrexBox の encode/decode roundtrip
    #[test]
    fn trex_box_roundtrip(trex in arb_trex_box()) {
        let encoded = trex.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TrexBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrexBox は必ずデコードできる");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.track_id, trex.track_id);
        prop_assert_eq!(decoded.default_sample_description_index, trex.default_sample_description_index);
        prop_assert_eq!(decoded.default_sample_duration, trex.default_sample_duration);
        prop_assert_eq!(decoded.default_sample_size, trex.default_sample_size);
        prop_assert_eq!(decoded.default_sample_flags.get(), trex.default_sample_flags.get());
    }

    // ===== MehdBox のテスト =====

    /// MehdBox の encode/decode roundtrip
    #[test]
    fn mehd_box_roundtrip(mehd in arb_mehd_box()) {
        let encoded = mehd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MehdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MehdBox は必ずデコードできる");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.fragment_duration, mehd.fragment_duration);
    }

    // ===== MvexBox のテスト =====

    /// MvexBox の encode/decode roundtrip
    #[test]
    fn mvex_box_roundtrip(mvex in arb_mvex_box()) {
        let encoded = mvex.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MvexBox::decode(&encoded)
            .expect("直前にエンコードした有効な MvexBox は必ずデコードできる");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.mehd_box.is_some(), mvex.mehd_box.is_some());
        prop_assert_eq!(decoded.trex_boxes.len(), mvex.trex_boxes.len());
    }

    // ===== MfhdBox のテスト =====

    /// MfhdBox の encode/decode roundtrip
    #[test]
    fn mfhd_box_roundtrip(mfhd in arb_mfhd_box()) {
        let encoded = mfhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MfhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MfhdBox は必ずデコードできる");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.sequence_number, mfhd.sequence_number);
    }

    // ===== TfdtBox のテスト =====

    /// TfdtBox の encode/decode roundtrip
    #[test]
    fn tfdt_box_roundtrip(tfdt in arb_tfdt_box()) {
        let encoded = tfdt.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TfdtBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfdtBox は必ずデコードできる");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.base_media_decode_time, tfdt.base_media_decode_time);
    }

    // ===== TfhdBox のテスト =====

    /// TfhdBox の encode/decode roundtrip
    #[test]
    fn tfhd_box_roundtrip(tfhd in arb_tfhd_box()) {
        let encoded = tfhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TfhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfhdBox は必ずデコードできる");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.track_id, tfhd.track_id);
        prop_assert_eq!(decoded.base_data_offset, tfhd.base_data_offset);
        prop_assert_eq!(decoded.sample_description_index, tfhd.sample_description_index);
        prop_assert_eq!(decoded.default_sample_duration, tfhd.default_sample_duration);
        prop_assert_eq!(decoded.default_sample_size, tfhd.default_sample_size);
        prop_assert_eq!(decoded.duration_is_empty, tfhd.duration_is_empty);
        prop_assert_eq!(decoded.default_base_is_moof, tfhd.default_base_is_moof);
    }

    // ===== TrunBox のテスト =====

    /// TrunBox の encode/decode roundtrip
    #[test]
    fn trun_box_roundtrip(trun in arb_trun_box()) {
        let encoded = trun.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TrunBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrunBox は必ずデコードできる");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.data_offset, trun.data_offset);
        prop_assert_eq!(decoded.samples.len(), trun.samples.len());
    }

    // ===== TrafBox のテスト =====

    /// TrafBox の encode/decode roundtrip
    #[test]
    fn traf_box_roundtrip(traf in arb_traf_box()) {
        let encoded = traf.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TrafBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrafBox は必ずデコードできる");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.tfhd_box.track_id, traf.tfhd_box.track_id);
        prop_assert_eq!(decoded.tfdt_box.is_some(), traf.tfdt_box.is_some());
        prop_assert_eq!(decoded.trun_boxes.len(), traf.trun_boxes.len());
    }

    // ===== MoofBox のテスト =====

    /// MoofBox の encode/decode roundtrip
    #[test]
    fn moof_box_roundtrip(moof in arb_moof_box()) {
        let encoded = moof.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MoofBox::decode(&encoded)
            .expect("直前にエンコードした有効な MoofBox は必ずデコードできる");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.mfhd_box.sequence_number, moof.mfhd_box.sequence_number);
        prop_assert_eq!(decoded.traf_boxes.len(), moof.traf_boxes.len());
    }

    // ===== SidxBox のテスト =====

    /// SidxBox の encode/decode roundtrip
    #[test]
    fn sidx_box_roundtrip(sidx in arb_sidx_box()) {
        let encoded = sidx.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SidxBox::decode(&encoded)
            .expect("直前にエンコードした有効な SidxBox は必ずデコードできる");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.reference_id, sidx.reference_id);
        prop_assert_eq!(decoded.timescale, sidx.timescale);
        prop_assert_eq!(decoded.earliest_presentation_time, sidx.earliest_presentation_time);
        prop_assert_eq!(decoded.first_offset, sidx.first_offset);
        prop_assert_eq!(decoded.references.len(), sidx.references.len());
    }

    // ===== TfraBox のテスト =====

    /// TfraBox の encode/decode roundtrip
    #[test]
    fn tfra_box_roundtrip(tfra in arb_tfra_box()) {
        let encoded = tfra.encode_to_vec().unwrap();
        let (decoded, size) = TfraBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded, tfra);
    }
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
    ///
    /// ISO/IEC 14496-12 8.8.8 では version 0 は unsigned 32-bit 全域を許容する。
    /// 特に `> i32::MAX` の値が符号反転せずに保持されることを確認する。
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
        // 正値だが u32::MAX を超えるため version 0 でも書けない。
        // 負値でもないので version 1 も選ばれず、結果として version 0 が選ばれて範囲エラーになる。
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
        // 負値なので version 1 が選ばれるが、i32::MIN 未満は i32 に収まらないためエラー。
        let trun = trun_with_cto(i32::MIN as i64 - 1);
        assert_eq!(trun.full_box_version(), 1);
        assert!(
            trun.encode_to_vec().is_err(),
            "cto < i32::MIN は encode 時にエラーとなる"
        );
    }

    /// TrunBox: 負値と `> i32::MAX` の値が混在する場合は encode 時にエラーとなる
    ///
    /// 負値があるため version 1 が選ばれるが、`> i32::MAX` の値は version 1 の
    /// signed 32-bit に収まらないため、encode 時に invalid_input として弾かれる。
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
    ///
    /// `length_size_of_*` を全て 3（byte_count = 4）にして
    /// `traf_number` / `trun_number` / `sample_number` を `u32::MAX` まで、
    /// `time` / `moof_offset` を `u32::MAX` まで詰める。
    /// full_box_version は 0 を返し、self.version = 0 のまま保持される。
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

        let encoded = tfra.encode_to_vec().unwrap();
        let (decoded, _) = TfraBox::decode(&encoded).unwrap();
        assert_eq!(decoded, tfra);
    }

    /// TfraBox: self.version = 0 でも time > u32::MAX で自動的に version = 1 に昇格する
    ///
    /// `full_box_version` は entries のいずれかが `u32::MAX` を超えたら 1 を返す仕様
    /// （`src/boxes_fmp4.rs:1305-1318`）。この挙動により、encode 時のヘッダー版数は
    /// self.version より entries の値が優先される。decode 側は書かれた版数をそのまま
    /// self.version に戻すため、self.version = 0 → 1 への「意図的な化け」が起きる。
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

        let encoded = tfra.encode_to_vec().unwrap();
        let (decoded, _) = TfraBox::decode(&encoded).unwrap();
        assert_eq!(
            decoded.version, 1,
            "decode 側は書かれた版数 1 をそのまま self.version に戻す"
        );
        assert_eq!(decoded.entries, tfra.entries);
    }

    /// TfraBox: `length_size_of_*` = 0 の最小構成ラウンドトリップ
    ///
    /// 各可変長整数フィールドが 1 バイトで書かれ、上位バイトを持たない最小のケース。
    /// `encode_variable_uint` の 1 バイトアーム（`src/boxes_fmp4.rs:1409-1411`）と
    /// `decode_variable_uint` の 1 バイト分岐を通す。
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

        let encoded = tfra.encode_to_vec().unwrap();
        let (decoded, _) = TfraBox::decode(&encoded).unwrap();
        assert_eq!(decoded, tfra);
    }

    /// TfraBox: entries が空
    ///
    /// `number_of_entry` = 0 で entries ループを 1 度も回らない縮退ケース。
    /// `encode_variable_uint` は 1 度も呼ばれない。
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

        let encoded = tfra.encode_to_vec().unwrap();
        let (decoded, _) = TfraBox::decode(&encoded).unwrap();
        assert_eq!(decoded, tfra);
    }
}
