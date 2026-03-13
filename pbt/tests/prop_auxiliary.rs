//! auxiliary.rs の SampleTableAccessor をテストする Property-Based Testing
//!
//! バグを発見することを目的として、エラーパスや境界値をテストする

use std::num::NonZeroU32;

use proptest::prelude::*;
use shiguredo_mp4::{
    BoxSize, BoxType, Either,
    aux::{SampleTableAccessor, SampleTableAccessorError},
    boxes::{
        Co64Box, CttsBox, CttsEntry, SampleEntry, StblBox, StcoBox, StscBox, StscEntry, StsdBox,
        StssBox, StszBox, SttsBox, SttsEntry, UnknownBox,
    },
};

/// テスト用のダミー SampleEntry を作成
fn dummy_sample_entry() -> SampleEntry {
    SampleEntry::Unknown(UnknownBox {
        box_type: BoxType::Normal(*b"test"),
        box_size: BoxSize::U32(8),
        payload: Vec::new(),
    })
}

/// NonZeroU32 を作成するヘルパー
fn nz(i: u32) -> NonZeroU32 {
    NonZeroU32::new(i).expect("invalid index")
}

// ===== エラーケースのテスト =====

mod error_cases {
    use super::*;

    /// stts と stsz でサンプル数が異なるケース
    #[test]
    fn inconsistent_sample_count_stts_vs_stsz() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 10,
                    sample_delta: 1,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 10,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 5], // 5 サンプル (stts は 10 サンプル)
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let result = SampleTableAccessor::new(&stbl_box);
        assert!(
            matches!(
                result,
                Err(SampleTableAccessorError::InconsistentSampleCount { .. })
            ),
            "Expected InconsistentSampleCount error, got {:?}",
            result
        );
    }

    /// stts と stsc でサンプル数が異なるケース
    #[test]
    fn inconsistent_sample_count_stts_vs_stsc() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 10,
                    sample_delta: 1,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 5, // 5 サンプル (stts は 10 サンプル)
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 10],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let result = SampleTableAccessor::new(&stbl_box);
        assert!(
            matches!(
                result,
                Err(SampleTableAccessorError::InconsistentSampleCount { .. })
            ),
            "Expected InconsistentSampleCount error, got {:?}",
            result
        );
    }

    /// チャンクが存在するが stsc が空のケース
    #[test]
    fn chunks_exist_but_no_samples() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox { entries: vec![] },
            stsc_box: StscBox { entries: vec![] }, // 空の stsc
            stsz_box: StszBox::Variable {
                entry_sizes: vec![],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![100], // 1 つのチャンク
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let result = SampleTableAccessor::new(&stbl_box);
        assert!(
            matches!(
                result,
                Err(SampleTableAccessorError::ChunksExistButNoSamples { .. })
            ),
            "Expected ChunksExistButNoSamples error, got {:?}",
            result
        );
    }

    /// stsc の最初のエントリのチャンクインデックスが 1 ではないケース
    #[test]
    fn first_chunk_index_is_not_one() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 5,
                    sample_delta: 1,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(2), // 1 ではない
                    sample_per_chunk: 5,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 5],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0, 100],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let result = SampleTableAccessor::new(&stbl_box);
        assert!(
            matches!(
                result,
                Err(SampleTableAccessorError::FirstChunkIndexIsNotOne { .. })
            ),
            "Expected FirstChunkIndexIsNotOne error, got {:?}",
            result
        );
    }

    /// 存在しないサンプルエントリーを参照するケース
    #[test]
    fn missing_sample_entry() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()], // 1 つのサンプルエントリー
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 5,
                    sample_delta: 1,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 5,
                    sample_description_index: nz(2), // 存在しない
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 5],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let result = SampleTableAccessor::new(&stbl_box);
        assert!(
            matches!(
                result,
                Err(SampleTableAccessorError::MissingSampleEntry { .. })
            ),
            "Expected MissingSampleEntry error, got {:?}",
            result
        );
    }

    /// stsc のチャンクインデックスが単調増加していないケース
    #[test]
    fn chunk_indices_not_monotonically_increasing() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 10,
                    sample_delta: 1,
                }],
            },
            stsc_box: StscBox {
                entries: vec![
                    StscEntry {
                        first_chunk: nz(1),
                        sample_per_chunk: 5,
                        sample_description_index: nz(1),
                    },
                    StscEntry {
                        first_chunk: nz(1), // 同じか前のインデックス
                        sample_per_chunk: 5,
                        sample_description_index: nz(1),
                    },
                ],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 10],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0, 500],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let result = SampleTableAccessor::new(&stbl_box);
        assert!(
            matches!(
                result,
                Err(SampleTableAccessorError::ChunkIndicesNotMonotonicallyIncreasing)
            ),
            "Expected ChunkIndicesNotMonotonicallyIncreasing error, got {:?}",
            result
        );
    }

    /// stsc の最後のエントリのチャンクインデックスが大きすぎるケース
    #[test]
    fn last_chunk_index_is_too_large() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 10,
                    sample_delta: 1,
                }],
            },
            stsc_box: StscBox {
                entries: vec![
                    StscEntry {
                        first_chunk: nz(1),
                        sample_per_chunk: 5,
                        sample_description_index: nz(1),
                    },
                    StscEntry {
                        first_chunk: nz(10), // 存在しないチャンク
                        sample_per_chunk: 5,
                        sample_description_index: nz(1),
                    },
                ],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 10],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0, 500], // 2 チャンクのみ
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let result = SampleTableAccessor::new(&stbl_box);
        assert!(
            matches!(
                result,
                Err(SampleTableAccessorError::LastChunkIndexIsTooLarge { .. })
            ),
            "Expected LastChunkIndexIsTooLarge error, got {:?}",
            result
        );
    }

    /// stts と ctts でサンプル数が異なるケース
    #[test]
    fn inconsistent_sample_count_stts_vs_ctts() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 5,
                    sample_delta: 1,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 5,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 5],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0],
            }),
            stss_box: None,
            ctts_box: Some(CttsBox {
                version: 0,
                entries: vec![CttsEntry {
                    sample_count: 4,
                    sample_offset: 10,
                }],
            }),
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let result = SampleTableAccessor::new(&stbl_box);
        assert!(
            matches!(
                result,
                Err(SampleTableAccessorError::InconsistentSampleCount {
                    other_box_type,
                    ..
                }) if other_box_type == CttsBox::TYPE
            ),
            "Expected InconsistentSampleCount error for ctts, got {:?}",
            result
        );
    }
}

// ===== get_sample_by_timestamp のテスト =====

mod timestamp_tests {
    use super::*;

    /// 正常系: タイムスタンプでサンプルを取得できる
    #[test]
    fn get_sample_by_timestamp_basic() {
        let sample_durations = [10u32, 20, 30, 40, 50];
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox::from_sample_deltas(sample_durations),
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 5,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 5],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");

        // 各サンプルの開始タイムスタンプでテスト
        // sample 1: timestamp 0-9
        // sample 2: timestamp 10-29
        // sample 3: timestamp 30-59
        // sample 4: timestamp 60-99
        // sample 5: timestamp 100-149

        let sample = accessor
            .get_sample_by_timestamp(0)
            .expect("sample not found");
        assert_eq!(sample.index().get(), 1);

        let sample = accessor
            .get_sample_by_timestamp(9)
            .expect("sample not found");
        assert_eq!(sample.index().get(), 1);

        let sample = accessor
            .get_sample_by_timestamp(10)
            .expect("sample not found");
        assert_eq!(sample.index().get(), 2);

        let sample = accessor
            .get_sample_by_timestamp(29)
            .expect("sample not found");
        assert_eq!(sample.index().get(), 2);

        let sample = accessor
            .get_sample_by_timestamp(30)
            .expect("sample not found");
        assert_eq!(sample.index().get(), 3);

        let sample = accessor
            .get_sample_by_timestamp(100)
            .expect("sample not found");
        assert_eq!(sample.index().get(), 5);

        let sample = accessor
            .get_sample_by_timestamp(149)
            .expect("sample not found");
        assert_eq!(sample.index().get(), 5);

        // 範囲外のタイムスタンプ
        assert!(accessor.get_sample_by_timestamp(150).is_none());
        assert!(accessor.get_sample_by_timestamp(1000).is_none());
    }

    /// samples() イテレーターのテスト
    #[test]
    fn samples_iterator() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 5,
                    sample_delta: 10,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 5,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100, 200, 300, 400, 500],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");
        let mut count = 0;
        for (i, sample) in accessor.samples().enumerate() {
            assert_eq!(sample.index().get(), i as u32 + 1);
            assert_eq!(sample.duration(), 10);
            assert_eq!(sample.timestamp(), i as u64 * 10);
            assert_eq!(sample.data_size(), (i as u32 + 1) * 100);
            count += 1;
        }
        assert_eq!(count, 5);
    }

    /// sample_count() と chunk_count() のテスト
    #[test]
    fn sample_and_chunk_count() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 20,
                    sample_delta: 10,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 5,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 20],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0, 500, 1000, 1500], // 4 チャンク
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");
        assert_eq!(accessor.sample_count(), 20);
        assert_eq!(accessor.chunk_count(), 4);
    }
}

mod composition_time_offset_tests {
    use super::*;

    proptest! {
        #[test]
        fn composition_time_offset_matches_ctts_entries(
            entries in prop::collection::vec((1u32..5, -100i32..101), 1..6)
        ) {
            let sample_count: u32 = entries.iter().map(|(sample_count, _)| *sample_count).sum();
            let expected_offsets: Vec<i64> = entries
                .iter()
                .flat_map(|(sample_count, sample_offset)| {
                    core::iter::repeat_n(*sample_offset as i64, *sample_count as usize)
                })
                .collect();
            let ctts_version = if entries.iter().any(|(_, sample_offset)| *sample_offset < 0) {
                1
            } else {
                0
            };

            let stbl_box = StblBox {
                stsd_box: StsdBox {
                    entries: vec![dummy_sample_entry()],
                },
                stts_box: SttsBox {
                    entries: vec![SttsEntry {
                        sample_count,
                        sample_delta: 10,
                    }],
                },
                stsc_box: StscBox {
                    entries: vec![StscEntry {
                        first_chunk: nz(1),
                        sample_per_chunk: sample_count,
                        sample_description_index: nz(1),
                    }],
                },
                stsz_box: StszBox::Variable {
                    entry_sizes: vec![100; sample_count as usize],
                },
                stco_or_co64_box: Either::A(StcoBox {
                    chunk_offsets: vec![0],
                }),
                stss_box: None,
                ctts_box: Some(CttsBox {
                    version: ctts_version,
                    entries: entries
                        .iter()
                        .map(|(sample_count, sample_offset)| CttsEntry {
                            sample_count: *sample_count,
                            sample_offset: *sample_offset as i64,
                        })
                        .collect(),
                }),
                cslg_box: None,
                sdtp_box: None,
                unknown_boxes: Vec::new(),
            };

            let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");

            for (i, expected_offset) in expected_offsets.iter().enumerate() {
                let sample = accessor
                    .get_sample(nz(i as u32 + 1))
                    .expect("sample not found");
                prop_assert_eq!(sample.composition_time_offset(), Some(*expected_offset));
            }
        }
    }

    #[test]
    fn composition_time_offset_returns_none_without_ctts() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 3,
                    sample_delta: 10,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 3,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 3],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");
        let sample = accessor.get_sample(nz(1)).expect("sample not found");
        assert_eq!(sample.composition_time_offset(), None);
    }
}

// ===== Co64Box を使うケース =====

mod co64_tests {
    use super::*;

    /// Co64Box を使用するケースで正しく動作することを確認
    #[test]
    fn sample_accessor_with_co64() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 5,
                    sample_delta: 10,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 5,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 5],
            },
            stco_or_co64_box: Either::B(Co64Box {
                chunk_offsets: vec![0x100000000], // u32 を超える値
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");
        assert_eq!(accessor.sample_count(), 5);
        assert_eq!(accessor.chunk_count(), 1);

        let chunk = accessor.get_chunk(nz(1)).expect("chunk not found");
        assert_eq!(chunk.offset(), 0x100000000);
    }
}

// ===== 同期サンプルのテスト =====

mod sync_sample_tests {
    use super::*;

    /// 同期サンプル検索のテスト
    #[test]
    fn sync_sample_search() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 10,
                    sample_delta: 10,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 10,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 10],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0],
            }),
            stss_box: Some(StssBox {
                sample_numbers: vec![nz(1), nz(5), nz(9)],
            }),
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");

        // サンプル 1 は同期サンプル
        let sample1 = accessor.get_sample(nz(1)).expect("sample not found");
        assert!(sample1.is_sync_sample());
        assert_eq!(sample1.sync_sample().expect("sync sample").index().get(), 1);

        // サンプル 2 は非同期、同期サンプルは 1
        let sample2 = accessor.get_sample(nz(2)).expect("sample not found");
        assert!(!sample2.is_sync_sample());
        assert_eq!(sample2.sync_sample().expect("sync sample").index().get(), 1);

        // サンプル 4 は非同期、同期サンプルは 1
        let sample4 = accessor.get_sample(nz(4)).expect("sample not found");
        assert!(!sample4.is_sync_sample());
        assert_eq!(sample4.sync_sample().expect("sync sample").index().get(), 1);

        // サンプル 5 は同期サンプル
        let sample5 = accessor.get_sample(nz(5)).expect("sample not found");
        assert!(sample5.is_sync_sample());
        assert_eq!(sample5.sync_sample().expect("sync sample").index().get(), 5);

        // サンプル 6 は非同期、同期サンプルは 5
        let sample6 = accessor.get_sample(nz(6)).expect("sample not found");
        assert!(!sample6.is_sync_sample());
        assert_eq!(sample6.sync_sample().expect("sync sample").index().get(), 5);

        // サンプル 9 は同期サンプル
        let sample9 = accessor.get_sample(nz(9)).expect("sample not found");
        assert!(sample9.is_sync_sample());
        assert_eq!(sample9.sync_sample().expect("sync sample").index().get(), 9);

        // サンプル 10 は非同期、同期サンプルは 9
        let sample10 = accessor.get_sample(nz(10)).expect("sample not found");
        assert!(!sample10.is_sync_sample());
        assert_eq!(
            sample10.sync_sample().expect("sync sample").index().get(),
            9
        );
    }

    /// stss がない場合は全てのサンプルが同期サンプル
    #[test]
    fn no_stss_all_sync() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 5,
                    sample_delta: 10,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 5,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 5],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0],
            }),
            stss_box: None, // stss なし
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");

        for i in 1..=5 {
            let sample = accessor.get_sample(nz(i)).expect("sample not found");
            assert!(sample.is_sync_sample());
            assert_eq!(sample.sync_sample().expect("sync sample").index().get(), i);
        }
    }
}

// ===== Fixed size stsz のテスト =====

mod fixed_stsz_tests {
    use super::*;

    /// StszBox::Fixed の場合のテスト
    #[test]
    fn fixed_sample_size() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 5,
                    sample_delta: 10,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 5,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Fixed {
                sample_size: NonZeroU32::new(256).expect("invalid"),
                sample_count: 5,
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![1000],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");
        assert_eq!(accessor.sample_count(), 5);

        for i in 1..=5 {
            let sample = accessor.get_sample(nz(i)).expect("sample not found");
            assert_eq!(sample.data_size(), 256);
            assert_eq!(sample.data_offset(), 1000 + (i as u64 - 1) * 256);
        }
    }
}

// ===== 複数 stts エントリーのテスト =====

mod multiple_stts_tests {
    use super::*;

    /// 複数の stts エントリーがある場合のテスト
    #[test]
    fn multiple_stts_entries() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![
                    SttsEntry {
                        sample_count: 3,
                        sample_delta: 10,
                    },
                    SttsEntry {
                        sample_count: 2,
                        sample_delta: 20,
                    },
                    SttsEntry {
                        sample_count: 2,
                        sample_delta: 5,
                    },
                ],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 7,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 7],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");
        assert_eq!(accessor.sample_count(), 7);

        // sample 1: duration=10, timestamp=0
        // sample 2: duration=10, timestamp=10
        // sample 3: duration=10, timestamp=20
        // sample 4: duration=20, timestamp=30
        // sample 5: duration=20, timestamp=50
        // sample 6: duration=5, timestamp=70
        // sample 7: duration=5, timestamp=75

        let expected = [
            (1, 10, 0),
            (2, 10, 10),
            (3, 10, 20),
            (4, 20, 30),
            (5, 20, 50),
            (6, 5, 70),
            (7, 5, 75),
        ];

        for (index, duration, timestamp) in expected {
            let sample = accessor.get_sample(nz(index)).expect("sample not found");
            assert_eq!(sample.duration(), duration, "sample {} duration", index);
            assert_eq!(sample.timestamp(), timestamp, "sample {} timestamp", index);
        }
    }
}

// ===== 複数チャンクのテスト =====

mod multiple_chunks_tests {
    use super::*;

    /// 複数チャンクがある場合のテスト
    #[test]
    fn multiple_chunks() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 9,
                    sample_delta: 10,
                }],
            },
            stsc_box: StscBox {
                entries: vec![
                    StscEntry {
                        first_chunk: nz(1),
                        sample_per_chunk: 2,
                        sample_description_index: nz(1),
                    },
                    StscEntry {
                        first_chunk: nz(3),
                        sample_per_chunk: 5,
                        sample_description_index: nz(1),
                    },
                ],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; 9],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0, 200, 400],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");
        assert_eq!(accessor.chunk_count(), 3);

        // chunk 1: 2 samples (sample 1-2)
        let chunk1 = accessor.get_chunk(nz(1)).expect("chunk not found");
        assert_eq!(chunk1.offset(), 0);
        assert_eq!(chunk1.sample_count(), 2);

        // chunk 2: 2 samples (sample 3-4)
        let chunk2 = accessor.get_chunk(nz(2)).expect("chunk not found");
        assert_eq!(chunk2.offset(), 200);
        assert_eq!(chunk2.sample_count(), 2);

        // chunk 3: 5 samples (sample 5-9)
        let chunk3 = accessor.get_chunk(nz(3)).expect("chunk not found");
        assert_eq!(chunk3.offset(), 400);
        assert_eq!(chunk3.sample_count(), 5);

        // サンプルからチャンクを取得
        let sample1 = accessor.get_sample(nz(1)).expect("sample not found");
        assert_eq!(sample1.chunk().index().get(), 1);

        let sample3 = accessor.get_sample(nz(3)).expect("sample not found");
        assert_eq!(sample3.chunk().index().get(), 2);

        let sample5 = accessor.get_sample(nz(5)).expect("sample not found");
        assert_eq!(sample5.chunk().index().get(), 3);
    }
}

// ===== Property-Based Testing =====

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// ランダムなタイムスタンプで get_sample_by_timestamp が正しく動作することを確認
    #[test]
    fn get_sample_by_timestamp_pbt(
        sample_count in 1u32..50,
        duration in 1u32..100,
        timestamp_offset in 0u64..10000
    ) {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count,
                    sample_delta: duration,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: sample_count,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; sample_count as usize],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![0],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");
        let total_duration = sample_count as u64 * duration as u64;

        // 有効なタイムスタンプ範囲内でテスト
        let timestamp = timestamp_offset % total_duration;
        let sample = accessor.get_sample_by_timestamp(timestamp);
        prop_assert!(sample.is_some(), "sample should be found for timestamp {}", timestamp);

        let sample = sample.unwrap();
        let sample_start = sample.timestamp();
        let sample_end = sample_start + sample.duration() as u64;
        prop_assert!(
            timestamp >= sample_start && timestamp < sample_end,
            "timestamp {} should be in range [{}, {})",
            timestamp, sample_start, sample_end
        );

        // 範囲外のタイムスタンプ
        if total_duration < u64::MAX {
            prop_assert!(accessor.get_sample_by_timestamp(total_duration).is_none());
        }
    }

    /// サンプルとチャンクの関係が一貫していることを確認
    #[test]
    fn sample_chunk_consistency(
        samples_per_chunk in 1u32..10,
        chunk_count in 1u32..10
    ) {
        let sample_count = samples_per_chunk * chunk_count;

        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count,
                    sample_delta: 10,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: samples_per_chunk,
                    sample_description_index: nz(1),
                }],
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![100; sample_count as usize],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: (0..chunk_count).map(|i| i * samples_per_chunk * 100).collect(),
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("failed to create accessor");
        prop_assert_eq!(accessor.sample_count(), sample_count);
        prop_assert_eq!(accessor.chunk_count(), chunk_count);

        // 各サンプルが正しいチャンクに属していることを確認
        for i in 1..=sample_count {
            let sample = accessor.get_sample(nz(i)).expect("sample not found");
            let expected_chunk = (i - 1) / samples_per_chunk + 1;
            prop_assert_eq!(
                sample.chunk().index().get(),
                expected_chunk,
                "sample {} should be in chunk {}",
                i, expected_chunk
            );
        }

        // 各チャンクのサンプル数を確認
        for i in 1..=chunk_count {
            let chunk = accessor.get_chunk(nz(i)).expect("chunk not found");
            prop_assert_eq!(chunk.sample_count(), samples_per_chunk);
            prop_assert_eq!(chunk.samples().count(), samples_per_chunk as usize);
        }
    }
}

// ===== Display トレイトのテスト =====

mod error_display_tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn error_display_inconsistent_sample_count() {
        let err = SampleTableAccessorError::InconsistentSampleCount {
            stts_sample_count: 10,
            other_box_type: BoxType::Normal(*b"stsz"),
            other_sample_count: 5,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("10"));
        assert!(msg.contains("5"));
        assert!(err.source().is_none());
    }

    #[test]
    fn error_display_first_chunk_index_is_not_one() {
        let err = SampleTableAccessorError::FirstChunkIndexIsNotOne {
            actual_chunk_index: nz(5),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("5"));
    }

    #[test]
    fn error_display_last_chunk_index_too_large() {
        let err = SampleTableAccessorError::LastChunkIndexIsTooLarge {
            max_chunk_index: nz(3),
            last_chunk_index: nz(10),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("3"));
        assert!(msg.contains("10"));
    }

    #[test]
    fn error_display_missing_sample_entry() {
        let err = SampleTableAccessorError::MissingSampleEntry {
            stsc_entry_index: 0,
            sample_description_index: nz(5),
            sample_entry_count: 1,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("5"));
        assert!(msg.contains("1"));
    }

    #[test]
    fn error_display_chunk_indices_not_monotonic() {
        let err = SampleTableAccessorError::ChunkIndicesNotMonotonicallyIncreasing;
        let msg = format!("{}", err);
        assert!(msg.contains("monotonically"));
    }

    #[test]
    fn error_display_chunks_exist_but_no_samples() {
        let err = SampleTableAccessorError::ChunksExistButNoSamples { chunk_count: 5 };
        let msg = format!("{}", err);
        assert!(msg.contains("5"));
    }
}
