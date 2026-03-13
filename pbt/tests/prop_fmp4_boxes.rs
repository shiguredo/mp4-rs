//! Fragmented MP4 (fMP4) ボックスの Property-Based Testing
//!
//! MoofBox, MfhdBox, TrafBox, TfhdBox, TrunBox, TfdtBox, SidxBox,
//! MvexBox, TrexBox, MehdBox のテスト

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;
use shiguredo_mp4::{
    Decode, Encode, SampleFlags,
    boxes::{
        MehdBox, MfhdBox, MoofBox, MvexBox, SidxBox, SidxReference, TfdtBox, TfhdBox, TrafBox,
        TrexBox, TrunBox, TrunSample,
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
            0usize..10,    // sample_count
        ),
    )
        .prop_flat_map(
            |(
                data_offset,
                first_sample_flags,
                (has_duration, has_size, has_flags, has_cto, count),
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
                let cto_strategy: BoxedStrategy<Option<i32>> = if has_cto {
                    any::<i32>().prop_map(Some).boxed()
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // ===== SampleFlags のテスト =====

    /// SampleFlags の encode/decode roundtrip
    #[test]
    fn sample_flags_roundtrip(flags in arb_sample_flags()) {
        let encoded = flags.encode_to_vec().unwrap();
        let (decoded, size) = SampleFlags::decode(&encoded).unwrap();

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
        let encoded = trex.encode_to_vec().unwrap();
        let (decoded, size) = TrexBox::decode(&encoded).unwrap();

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
        let encoded = mehd.encode_to_vec().unwrap();
        let (decoded, size) = MehdBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.fragment_duration, mehd.fragment_duration);
    }

    // ===== MvexBox のテスト =====

    /// MvexBox の encode/decode roundtrip
    #[test]
    fn mvex_box_roundtrip(mvex in arb_mvex_box()) {
        let encoded = mvex.encode_to_vec().unwrap();
        let (decoded, size) = MvexBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.mehd_box.is_some(), mvex.mehd_box.is_some());
        prop_assert_eq!(decoded.trex_boxes.len(), mvex.trex_boxes.len());
    }

    // ===== MfhdBox のテスト =====

    /// MfhdBox の encode/decode roundtrip
    #[test]
    fn mfhd_box_roundtrip(mfhd in arb_mfhd_box()) {
        let encoded = mfhd.encode_to_vec().unwrap();
        let (decoded, size) = MfhdBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.sequence_number, mfhd.sequence_number);
    }

    // ===== TfdtBox のテスト =====

    /// TfdtBox の encode/decode roundtrip
    #[test]
    fn tfdt_box_roundtrip(tfdt in arb_tfdt_box()) {
        let encoded = tfdt.encode_to_vec().unwrap();
        let (decoded, size) = TfdtBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.base_media_decode_time, tfdt.base_media_decode_time);
    }

    // ===== TfhdBox のテスト =====

    /// TfhdBox の encode/decode roundtrip
    #[test]
    fn tfhd_box_roundtrip(tfhd in arb_tfhd_box()) {
        let encoded = tfhd.encode_to_vec().unwrap();
        let (decoded, size) = TfhdBox::decode(&encoded).unwrap();

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
        let encoded = trun.encode_to_vec().unwrap();
        let (decoded, size) = TrunBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.data_offset, trun.data_offset);
        prop_assert_eq!(decoded.samples.len(), trun.samples.len());
    }

    // ===== TrafBox のテスト =====

    /// TrafBox の encode/decode roundtrip
    #[test]
    fn traf_box_roundtrip(traf in arb_traf_box()) {
        let encoded = traf.encode_to_vec().unwrap();
        let (decoded, size) = TrafBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.tfhd_box.track_id, traf.tfhd_box.track_id);
        prop_assert_eq!(decoded.tfdt_box.is_some(), traf.tfdt_box.is_some());
        prop_assert_eq!(decoded.trun_boxes.len(), traf.trun_boxes.len());
    }

    // ===== MoofBox のテスト =====

    /// MoofBox の encode/decode roundtrip
    #[test]
    fn moof_box_roundtrip(moof in arb_moof_box()) {
        let encoded = moof.encode_to_vec().unwrap();
        let (decoded, size) = MoofBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.mfhd_box.sequence_number, moof.mfhd_box.sequence_number);
        prop_assert_eq!(decoded.traf_boxes.len(), moof.traf_boxes.len());
    }

    // ===== SidxBox のテスト =====

    /// SidxBox の encode/decode roundtrip
    #[test]
    fn sidx_box_roundtrip(sidx in arb_sidx_box()) {
        let encoded = sidx.encode_to_vec().unwrap();
        let (decoded, size) = SidxBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.reference_id, sidx.reference_id);
        prop_assert_eq!(decoded.timescale, sidx.timescale);
        prop_assert_eq!(decoded.earliest_presentation_time, sidx.earliest_presentation_time);
        prop_assert_eq!(decoded.first_offset, sidx.first_offset);
        prop_assert_eq!(decoded.references.len(), sidx.references.len());
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

        let encoded = mehd.encode_to_vec().unwrap();
        let (decoded, _) = MehdBox::decode(&encoded).unwrap();
        assert_eq!(decoded.fragment_duration, u32::MAX as u64);
    }

    /// MehdBox: version 1 (64-bit duration)
    #[test]
    fn mehd_box_version1() {
        let mehd = MehdBox {
            fragment_duration: u32::MAX as u64 + 1,
        };
        assert_eq!(mehd.full_box_version(), 1);

        let encoded = mehd.encode_to_vec().unwrap();
        let (decoded, _) = MehdBox::decode(&encoded).unwrap();
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

        let encoded = tfdt.encode_to_vec().unwrap();
        let (decoded, _) = TfdtBox::decode(&encoded).unwrap();
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

        let encoded = tfdt.encode_to_vec().unwrap();
        let (decoded, _) = TfdtBox::decode(&encoded).unwrap();
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

        let encoded = tfhd.encode_to_vec().unwrap();
        let (decoded, _) = TfhdBox::decode(&encoded).unwrap();
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

        let encoded = tfhd.encode_to_vec().unwrap();
        let (decoded, _) = TfhdBox::decode(&encoded).unwrap();
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

        let encoded = trun.encode_to_vec().unwrap();
        let (decoded, _) = TrunBox::decode(&encoded).unwrap();
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

        let encoded = trun.encode_to_vec().unwrap();
        let (decoded, _) = TrunBox::decode(&encoded).unwrap();
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

        let encoded = trun.encode_to_vec().unwrap();
        let (decoded, _) = TrunBox::decode(&encoded).unwrap();
        assert_eq!(decoded.samples[0].composition_time_offset, Some(-100));
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

        let encoded = sidx.encode_to_vec().unwrap();
        let (decoded, _) = SidxBox::decode(&encoded).unwrap();
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

        let encoded = sidx.encode_to_vec().unwrap();
        let (decoded, _) = SidxBox::decode(&encoded).unwrap();
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

        let encoded = sidx.encode_to_vec().unwrap();
        let (decoded, _) = SidxBox::decode(&encoded).unwrap();
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

        let encoded = moof.encode_to_vec().unwrap();
        let (decoded, _) = MoofBox::decode(&encoded).unwrap();
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

        let encoded = moof.encode_to_vec().unwrap();
        let (decoded, _) = MoofBox::decode(&encoded).unwrap();
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

        let encoded = mvex.encode_to_vec().unwrap();
        let (decoded, _) = MvexBox::decode(&encoded).unwrap();
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

        let encoded = mvex.encode_to_vec().unwrap();
        let (decoded, _) = MvexBox::decode(&encoded).unwrap();
        assert!(decoded.mehd_box.is_some());
        assert_eq!(decoded.mehd_box.unwrap().fragment_duration, 1000000);
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
}
