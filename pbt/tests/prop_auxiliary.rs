//! auxiliary.rs の SampleTableAccessor の Property-Based Testing
//!
//! ランダム入力に対する不変条件（タイムスタンプ検索・チャンク整合・Fixed/Variable 差分）を検証する。
//! 固定入力のエラーパス・境界値は `tests/test_auxiliary.rs` が担う。

use std::num::NonZeroU32;

use shiguredo_mp4::{
    BoxSize, BoxType, Either,
    aux::SampleTableAccessor,
    boxes::{
        CttsBox, CttsEntry, SampleEntry, StblBox, StcoBox, StscBox, StscEntry, StsdBox, StszBox,
        SttsBox, SttsEntry, UnknownBox,
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
