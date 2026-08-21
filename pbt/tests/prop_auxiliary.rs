//! auxiliary.rs の SampleTableAccessor をテストする Property-Based Testing
//!
//! バグを発見することを目的として、エラーパスや境界値をテストする

use std::num::NonZeroU32;

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
    NonZeroU32::new(i).expect("不正な index である")
}

// ===== エラーケースのテスト =====

mod error_cases {
    use super::*;

    /// stts と stsz (Variable) でサンプル数が異なるケース
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
            "InconsistentSampleCount エラーを期待したが {:?} だった",
            result
        );
    }

    /// stts と stsz (Fixed) でサンプル数が異なるケース
    ///
    /// Variable 経路の `inconsistent_sample_count_stts_vs_stsz` では Fixed の
    /// `sample_count` 突き合わせを捕捉できないため、別テストで検証する。
    #[test]
    fn inconsistent_sample_count_stts_vs_stsz_fixed() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![dummy_sample_entry()],
            },
            stts_box: SttsBox {
                entries: vec![SttsEntry {
                    sample_count: 1_000_000,
                    sample_delta: 1,
                }],
            },
            stsc_box: StscBox {
                entries: vec![StscEntry {
                    first_chunk: nz(1),
                    sample_per_chunk: 1_000_000,
                    sample_description_index: nz(1),
                }],
            },
            // stts 合計 100 万に対して Fixed.sample_count を 0 にする
            stsz_box: StszBox::Fixed {
                sample_size: NonZeroU32::new(1).expect("1 は非ゼロなので失敗しない"),
                sample_count: 0,
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
        let Err(SampleTableAccessorError::InconsistentSampleCount {
            stts_sample_count,
            other_box_type,
            other_sample_count,
        }) = result
        else {
            panic!("InconsistentSampleCount が返るはずが、実際は {result:?} だった");
        };

        assert_eq!(
            stts_sample_count, 1_000_000,
            "stts 由来のサンプル数がそのまま報告されること"
        );
        assert_eq!(
            other_box_type,
            StszBox::TYPE,
            "食い違いが検出されたボックスが stsz であること"
        );
        assert_eq!(
            other_sample_count, 0,
            "Fixed.sample_count の値が other_sample_count に入ること"
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
            "InconsistentSampleCount エラーを期待したが {:?} だった",
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
            "ChunksExistButNoSamples エラーを期待したが {:?} だった",
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
            "FirstChunkIndexIsNotOne エラーを期待したが {:?} だった",
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
            "MissingSampleEntry エラーを期待したが {:?} だった",
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
            "ChunkIndicesNotMonotonicallyIncreasing エラーを期待したが {:?} だった",
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
            "LastChunkIndexIsTooLarge エラーを期待したが {:?} だった",
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
            "ctts で InconsistentSampleCount エラーを期待したが {:?} だった",
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
            stts_box: SttsBox::from_sample_deltas(sample_durations)
                .expect("短い正常系入力で sample_count が溢れることはない"),
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

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");

        // 各サンプルの開始タイムスタンプでテスト
        // sample 1: timestamp 0-9
        // sample 2: timestamp 10-29
        // sample 3: timestamp 30-59
        // sample 4: timestamp 60-99
        // sample 5: timestamp 100-149

        let sample = accessor
            .get_sample_by_timestamp(0)
            .expect("sample が見つからない");
        assert_eq!(sample.index().get(), 1);

        let sample = accessor
            .get_sample_by_timestamp(9)
            .expect("sample が見つからない");
        assert_eq!(sample.index().get(), 1);

        let sample = accessor
            .get_sample_by_timestamp(10)
            .expect("sample が見つからない");
        assert_eq!(sample.index().get(), 2);

        let sample = accessor
            .get_sample_by_timestamp(29)
            .expect("sample が見つからない");
        assert_eq!(sample.index().get(), 2);

        let sample = accessor
            .get_sample_by_timestamp(30)
            .expect("sample が見つからない");
        assert_eq!(sample.index().get(), 3);

        let sample = accessor
            .get_sample_by_timestamp(100)
            .expect("sample が見つからない");
        assert_eq!(sample.index().get(), 5);

        let sample = accessor
            .get_sample_by_timestamp(149)
            .expect("sample が見つからない");
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

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");
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

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");
        assert_eq!(accessor.sample_count(), 20);
        assert_eq!(accessor.chunk_count(), 4);
    }
}

mod composition_time_offset_tests {
    use super::*;

    /// このモジュールの PBT ケース数
    const CASES: usize = 200;

    #[test]
    fn composition_time_offset_matches_ctts_entries() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let n = noprop::sample_usize_in(ctx, 1..6);
            let mut entries: Vec<(u32, i32)> = Vec::new();
            for _ in 0..n {
                let sample_count = noprop::sample_u64_in(ctx, 1..5) as u32;
                let sample_offset = noprop::sample_u64_in(ctx, 0..201) as i32 - 100;
                entries.push((sample_count, sample_offset));
            }
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

            let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");

            for (i, expected_offset) in expected_offsets.iter().enumerate() {
                let sample = accessor
                    .get_sample(nz(i as u32 + 1))
                    .expect("sample が見つからない");
                assert_eq!(sample.composition_time_offset(), Some(*expected_offset));
            }
            Ok(())
        })?;
        Ok(())
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

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");
        let sample = accessor.get_sample(nz(1)).expect("sample が見つからない");
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

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");
        assert_eq!(accessor.sample_count(), 5);
        assert_eq!(accessor.chunk_count(), 1);

        let chunk = accessor.get_chunk(nz(1)).expect("chunk が見つからない");
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

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");

        // サンプル 1 は同期サンプル
        let sample1 = accessor.get_sample(nz(1)).expect("sample が見つからない");
        assert!(sample1.is_sync_sample());
        assert_eq!(
            sample1
                .sync_sample()
                .expect("sync sample である")
                .index()
                .get(),
            1
        );

        // サンプル 2 は非同期、同期サンプルは 1
        let sample2 = accessor.get_sample(nz(2)).expect("sample が見つからない");
        assert!(!sample2.is_sync_sample());
        assert_eq!(
            sample2
                .sync_sample()
                .expect("sync sample である")
                .index()
                .get(),
            1
        );

        // サンプル 4 は非同期、同期サンプルは 1
        let sample4 = accessor.get_sample(nz(4)).expect("sample が見つからない");
        assert!(!sample4.is_sync_sample());
        assert_eq!(
            sample4
                .sync_sample()
                .expect("sync sample である")
                .index()
                .get(),
            1
        );

        // サンプル 5 は同期サンプル
        let sample5 = accessor.get_sample(nz(5)).expect("sample が見つからない");
        assert!(sample5.is_sync_sample());
        assert_eq!(
            sample5
                .sync_sample()
                .expect("sync sample である")
                .index()
                .get(),
            5
        );

        // サンプル 6 は非同期、同期サンプルは 5
        let sample6 = accessor.get_sample(nz(6)).expect("sample が見つからない");
        assert!(!sample6.is_sync_sample());
        assert_eq!(
            sample6
                .sync_sample()
                .expect("sync sample である")
                .index()
                .get(),
            5
        );

        // サンプル 9 は同期サンプル
        let sample9 = accessor.get_sample(nz(9)).expect("sample が見つからない");
        assert!(sample9.is_sync_sample());
        assert_eq!(
            sample9
                .sync_sample()
                .expect("sync sample である")
                .index()
                .get(),
            9
        );

        // サンプル 10 は非同期、同期サンプルは 9
        let sample10 = accessor.get_sample(nz(10)).expect("sample が見つからない");
        assert!(!sample10.is_sync_sample());
        assert_eq!(
            sample10
                .sync_sample()
                .expect("sync sample である")
                .index()
                .get(),
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

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");

        for i in 1..=5 {
            let sample = accessor.get_sample(nz(i)).expect("sample が見つからない");
            assert!(sample.is_sync_sample());
            assert_eq!(
                sample
                    .sync_sample()
                    .expect("sync sample である")
                    .index()
                    .get(),
                i
            );
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
                sample_size: NonZeroU32::new(256).expect("256 は非ゼロなので失敗しない"),
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

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");
        assert_eq!(accessor.sample_count(), 5);

        for i in 1..=5 {
            let sample = accessor.get_sample(nz(i)).expect("sample が見つからない");
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

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");
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
            let sample = accessor
                .get_sample(nz(index))
                .expect("sample が見つからない");
            assert_eq!(sample.duration(), duration, "sample {} の duration", index);
            assert_eq!(
                sample.timestamp(),
                timestamp,
                "sample {} の timestamp",
                index
            );
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

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");
        assert_eq!(accessor.chunk_count(), 3);

        // chunk 1: 2 samples (sample 1-2)
        let chunk1 = accessor.get_chunk(nz(1)).expect("chunk が見つからない");
        assert_eq!(chunk1.offset(), 0);
        assert_eq!(chunk1.sample_count(), 2);

        // chunk 2: 2 samples (sample 3-4)
        let chunk2 = accessor.get_chunk(nz(2)).expect("chunk が見つからない");
        assert_eq!(chunk2.offset(), 200);
        assert_eq!(chunk2.sample_count(), 2);

        // chunk 3: 5 samples (sample 5-9)
        let chunk3 = accessor.get_chunk(nz(3)).expect("chunk が見つからない");
        assert_eq!(chunk3.offset(), 400);
        assert_eq!(chunk3.sample_count(), 5);

        // サンプルからチャンクを取得
        let sample1 = accessor.get_sample(nz(1)).expect("sample が見つからない");
        assert_eq!(sample1.chunk().index().get(), 1);

        let sample3 = accessor.get_sample(nz(3)).expect("sample が見つからない");
        assert_eq!(sample3.chunk().index().get(), 2);

        let sample5 = accessor.get_sample(nz(5)).expect("sample が見つからない");
        assert_eq!(sample5.chunk().index().get(), 3);
    }
}

// ===== Property-Based Testing =====

/// このファイルの主要 PBT ケース数（旧 `with_cases(100)` を維持）
const CASES: usize = 100;

/// ランダムなタイムスタンプで get_sample_by_timestamp が正しく動作することを確認
#[test]
fn get_sample_by_timestamp_pbt() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let sample_count = noprop::sample_u64_in(ctx, 1..50) as u32;
        let duration = noprop::sample_u64_in(ctx, 1..100) as u32;
        let timestamp_offset = noprop::sample_u64_in(ctx, 0..10000);

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

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");
        let total_duration = sample_count as u64 * duration as u64;

        // 有効なタイムスタンプ範囲内でテスト
        let timestamp = timestamp_offset % total_duration;
        let sample = accessor.get_sample_by_timestamp(timestamp);
        assert!(
            sample.is_some(),
            "timestamp {timestamp} に対応する sample が見つかる"
        );

        let sample = sample.expect("直前の assert! で Some であることを確認済み");
        let sample_start = sample.timestamp();
        let sample_end = sample_start + sample.duration() as u64;
        assert!(
            timestamp >= sample_start && timestamp < sample_end,
            "timestamp {timestamp} は範囲 [{sample_start}, {sample_end}) 内である",
        );

        // 範囲外のタイムスタンプ
        if total_duration < u64::MAX {
            assert!(accessor.get_sample_by_timestamp(total_duration).is_none());
        }
        Ok(())
    })?;
    Ok(())
}

/// サンプルとチャンクの関係が一貫していることを確認
#[test]
fn sample_chunk_consistency() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let samples_per_chunk = noprop::sample_u64_in(ctx, 1..10) as u32;
        let chunk_count = noprop::sample_u64_in(ctx, 1..10) as u32;
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
                chunk_offsets: (0..chunk_count)
                    .map(|i| i * samples_per_chunk * 100)
                    .collect(),
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let accessor = SampleTableAccessor::new(&stbl_box).expect("accessor の作成に失敗した");
        assert_eq!(accessor.sample_count(), sample_count);
        assert_eq!(accessor.chunk_count(), chunk_count);

        // 各サンプルが正しいチャンクに属していることを確認
        for i in 1..=sample_count {
            let sample = accessor.get_sample(nz(i)).expect("sample が見つからない");
            let expected_chunk = (i - 1) / samples_per_chunk + 1;
            assert_eq!(
                sample.chunk().index().get(),
                expected_chunk,
                "sample {i} は chunk {expected_chunk} に属する",
            );
        }

        // 各チャンクのサンプル数を確認
        for i in 1..=chunk_count {
            let chunk = accessor.get_chunk(nz(i)).expect("chunk が見つからない");
            assert_eq!(chunk.sample_count(), samples_per_chunk);
            assert_eq!(chunk.samples().count(), samples_per_chunk as usize);
        }
        Ok(())
    })?;
    Ok(())
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

// ===== Fixed / Variable 差分テスト =====

mod fixed_variable_differential_tests {
    use super::*;

    /// このモジュールの PBT ケース数
    const CASES: usize = 200;

    /// 同一の論理テーブルを `Fixed { sample_size: s }` と
    /// `Variable { entry_sizes: vec![s; n] }` の 2 通りで構築し、
    /// 全サンプルの `data_offset()` が一致することを検証する
    ///
    /// `Fixed` の `data_offset()` は `sample_index_offsets` からチャンク先頭
    /// インデックスを引いて `base + チャンク内序数 × s` で算出するため、
    /// 均一な 1 チャンクだけではチャンク境界のずれを検出できない。そこで
    /// `stsc` は `sample_per_chunk` が異なる複数エントリを、チャンクオフセットは
    /// 単調増加でない配置を含むよう生成する。どちらのクラスにも到達したことを
    /// カバレッジゲートで保証する。
    #[test]
    fn fixed_and_variable_data_offset_match() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        let multi_entry_cases = std::cell::Cell::new(0usize);
        let non_monotonic_cases = std::cell::Cell::new(0usize);
        let mut runner = noprop::Runner::new(seed);

        runner.run(CASES, |ctx| {
            let chunk_count = noprop::sample_usize_in(ctx, 1..=10);
            let sample_per_chunk: Vec<u32> = (0..chunk_count)
                .map(|_| noprop::sample_usize_in(ctx, 1..=10) as u32)
                .collect();
            let sample_count: u32 = sample_per_chunk.iter().sum();

            // チャンクオフセットは単調増加でない配置を含むよう非ソートで生成する
            let chunk_offsets: Vec<u32> = (0..chunk_count)
                .map(|_| noprop::sample_u64_in(ctx, 0..1 << 30) as u32)
                .collect();

            let sample_size = noprop::sample_usize_in(ctx, 1..=1000) as u32;

            let build = |stsz_box: StszBox| StblBox {
                stsd_box: StsdBox {
                    entries: vec![dummy_sample_entry()],
                },
                stts_box: SttsBox {
                    entries: vec![SttsEntry {
                        sample_count,
                        sample_delta: 1,
                    }],
                },
                stsc_box: StscBox {
                    entries: sample_per_chunk
                        .iter()
                        .enumerate()
                        .map(|(i, &spc)| StscEntry {
                            first_chunk: nz(i as u32 + 1),
                            sample_per_chunk: spc,
                            sample_description_index: nz(1),
                        })
                        .collect(),
                },
                stsz_box,
                stco_or_co64_box: Either::A(StcoBox {
                    chunk_offsets: chunk_offsets.clone(),
                }),
                stss_box: None,
                ctts_box: None,
                cslg_box: None,
                sdtp_box: None,
                unknown_boxes: Vec::new(),
            };

            let fixed_stbl = build(StszBox::Fixed {
                sample_size: NonZeroU32::new(sample_size).expect("sample_size は非ゼロ"),
                sample_count,
            });
            let fixed = SampleTableAccessor::new(&fixed_stbl)
                .expect("正当な入力なので Fixed 経路は成功する");
            let variable_stbl = build(StszBox::Variable {
                entry_sizes: vec![sample_size; sample_count as usize],
            });
            let variable = SampleTableAccessor::new(&variable_stbl)
                .expect("正当な入力なので Variable 経路は成功する");

            for i in 1..=sample_count {
                let fixed_sample = fixed.get_sample(nz(i)).expect("sample が見つかる");
                let variable_sample = variable.get_sample(nz(i)).expect("sample が見つかる");
                assert_eq!(
                    fixed_sample.data_offset(),
                    variable_sample.data_offset(),
                    "sample {i} の data_offset が Fixed と Variable で一致すること"
                );
            }

            if sample_per_chunk.windows(2).any(|w| w[0] != w[1]) {
                multi_entry_cases.set(multi_entry_cases.get() + 1);
            }
            if chunk_offsets.windows(2).any(|w| w[0] > w[1]) {
                non_monotonic_cases.set(non_monotonic_cases.get() + 1);
            }
            Ok(())
        })?;

        assert!(
            multi_entry_cases.get() > 0,
            "sample_per_chunk が異なる複数エントリのケースが 1 回も生成されなかった\n{runner}"
        );
        assert!(
            non_monotonic_cases.get() > 0,
            "チャンクオフセットが単調増加でないケースが 1 回も生成されなかった\n{runner}"
        );
        Ok(())
    }
}
