//! `SampleTableAccessor` の固定入力による単体テスト
//!
//! オーバーフロー検出の回帰、エラーパス、境界値、Display など、
//! PBT では安定して狙いにくいケースを公開 API のみで検証する。
//! 正常系のプロパティ検証は `pbt/tests/prop_auxiliary.rs` が担う。

use std::num::NonZeroU32;

use shiguredo_mp4::{
    BoxSize, BoxType, Either,
    aux::{SampleTableAccessor, SampleTableAccessorError},
    boxes::{
        Co64Box, CttsBox, CttsEntry, SampleEntry, StblBox, StcoBox, StscBox, StscEntry, StsdBox,
        StssBox, StszBox, SttsBox, SttsEntry, UnknownBox,
    },
};

/// テスト用のダミー `stsd` を作成する。
///
/// `SampleTableAccessor::new` からは `entries.len()` しか参照されないため、
/// 中身は任意の 1 エントリで足りる。
fn stsd_box() -> StsdBox {
    StsdBox {
        entries: vec![SampleEntry::Unknown(UnknownBox {
            box_type: BoxType::Normal(*b"test"),
            box_size: BoxSize::U32(8),
            payload: Vec::new(),
        })],
    }
}

/// stts のサンプル数累計が u32 を超えるケース
///
/// `0x8000_0000` を 2 エントリ足すと `0x1_0000_0000` になり u32 の範囲を超える。
/// `stts` のエントリ検証は他のどの整合性チェックよりも前にあるため、
/// `stts` 以外は空でよい。
#[test]
fn stts_sample_count_overflow_returns_error() {
    let stbl_box = StblBox {
        stsd_box: stsd_box(),
        stts_box: SttsBox {
            entries: vec![
                SttsEntry {
                    sample_count: 0x8000_0000,
                    sample_delta: 1,
                };
                2
            ],
        },
        ctts_box: None,
        cslg_box: None,
        stsc_box: StscBox {
            entries: Vec::new(),
        },
        stsz_box: StszBox::Variable {
            entry_sizes: Vec::new(),
        },
        stco_or_co64_box: Either::A(StcoBox {
            chunk_offsets: Vec::new(),
        }),
        stss_box: None,
        sdtp_box: None,
        unknown_boxes: Vec::new(),
    };

    let result = SampleTableAccessor::new(&stbl_box);
    let Err(SampleTableAccessorError::SampleCountOverflow {
        box_type,
        accumulated_sample_count,
        entry_sample_count,
    }) = result
    else {
        panic!("SampleCountOverflow が返るはずが、実際は {result:?} だった");
    };

    assert_eq!(
        box_type,
        SttsBox::TYPE,
        "オーバーフローしたボックス種別が stts であること"
    );
    assert_eq!(
        accumulated_sample_count, 0x8000_0000,
        "オーバーフロー直前の累計サンプル数が 0x8000_0000 であること"
    );
    assert_eq!(
        entry_sample_count, 0x8000_0000,
        "加算しようとしたエントリのサンプル数が 0x8000_0000 であること"
    );
}

/// ctts のサンプル数累計が u32 を超えるケース
///
/// `stts` の整合性チェック（stsz との突き合わせ）を通す必要があるので、
/// `stts` と `stsz` を 1 サンプル分だけ整合させておく。
#[test]
fn ctts_sample_count_overflow_returns_error() {
    let stbl_box = StblBox {
        stsd_box: stsd_box(),
        stts_box: SttsBox {
            entries: vec![SttsEntry {
                sample_count: 1,
                sample_delta: 1,
            }],
        },
        ctts_box: Some(CttsBox {
            version: 0,
            entries: vec![
                CttsEntry {
                    sample_count: 0x8000_0000,
                    sample_offset: 0,
                };
                2
            ],
        }),
        cslg_box: None,
        stsc_box: StscBox {
            entries: Vec::new(),
        },
        stsz_box: StszBox::Variable {
            entry_sizes: vec![1],
        },
        stco_or_co64_box: Either::A(StcoBox {
            chunk_offsets: Vec::new(),
        }),
        stss_box: None,
        sdtp_box: None,
        unknown_boxes: Vec::new(),
    };

    let result = SampleTableAccessor::new(&stbl_box);
    let Err(SampleTableAccessorError::SampleCountOverflow {
        box_type,
        accumulated_sample_count,
        entry_sample_count,
    }) = result
    else {
        panic!("SampleCountOverflow が返るはずが、実際は {result:?} だった");
    };

    assert_eq!(
        box_type,
        CttsBox::TYPE,
        "オーバーフローしたボックス種別が ctts であること"
    );
    assert_eq!(
        accumulated_sample_count, 0x8000_0000,
        "オーバーフロー直前の累計サンプル数が 0x8000_0000 であること"
    );
    assert_eq!(
        entry_sample_count, 0x8000_0000,
        "加算しようとしたエントリのサンプル数が 0x8000_0000 であること"
    );
}

/// サンプルデータのオフセット累計が u64 を超えるケース（1 チャンク 3 サンプル）
///
/// 3 サンプル目の直前で `chunk_offsets[0] + サイズ 1 + サイズ 1 = u64::MAX` となり、
/// そこにさらに 1 を足す 2 サンプル目末尾の加算がオーバーフローする。
/// このとき `sample_index` は 2 で、3 サンプル目に格納されるはずだった値が失われる。
#[test]
fn sample_data_offset_overflow_multi_sample_chunk() {
    let stbl_box = StblBox {
        stsd_box: stsd_box(),
        stts_box: SttsBox {
            entries: vec![SttsEntry {
                sample_count: 3,
                sample_delta: 1,
            }],
        },
        ctts_box: None,
        cslg_box: None,
        stsc_box: StscBox {
            entries: vec![StscEntry {
                first_chunk: NonZeroU32::MIN,
                sample_per_chunk: 3,
                sample_description_index: NonZeroU32::MIN,
            }],
        },
        stsz_box: StszBox::Variable {
            entry_sizes: vec![1, 1, 1],
        },
        stco_or_co64_box: Either::B(Co64Box {
            chunk_offsets: vec![u64::MAX - 1],
        }),
        stss_box: None,
        sdtp_box: None,
        unknown_boxes: Vec::new(),
    };

    let result = SampleTableAccessor::new(&stbl_box);
    let Err(SampleTableAccessorError::SampleDataOffsetOverflow {
        sample_index,
        accumulated_offset,
        sample_data_size,
    }) = result
    else {
        panic!("SampleDataOffsetOverflow が返るはずが、実際は {result:?} だった");
    };

    assert_eq!(
        sample_index,
        NonZeroU32::new(2).expect("bug"),
        "オーバーフロー時点のサンプルインデックスが 2 であること"
    );
    assert_eq!(
        accumulated_offset,
        u64::MAX,
        "オーバーフロー直前の累計バイト位置が u64::MAX であること"
    );
    assert_eq!(
        sample_data_size, 1,
        "加算しようとしたサンプルサイズが 1 であること"
    );
}

/// サンプルデータのオフセット累計が u64 を超えるケース（1 チャンク 1 サンプル）
///
/// 唯一のサンプルの値はすべて正常に算出され、`sample_data_offsets` に格納される
/// のは `u64::MAX` の 1 値のみ。その直後、次サンプル用の開始位置を求めるための
/// 末尾の加算がオーバーフローする。この「格納は正常だが末尾の加算だけオーバーフロー」の
/// ケースでも `Err` を返すのが仕様である。
#[test]
fn sample_data_offset_overflow_single_sample_chunk_tail() {
    let stbl_box = StblBox {
        stsd_box: stsd_box(),
        stts_box: SttsBox {
            entries: vec![SttsEntry {
                sample_count: 1,
                sample_delta: 1,
            }],
        },
        ctts_box: None,
        cslg_box: None,
        stsc_box: StscBox {
            entries: vec![StscEntry {
                first_chunk: NonZeroU32::MIN,
                sample_per_chunk: 1,
                sample_description_index: NonZeroU32::MIN,
            }],
        },
        stsz_box: StszBox::Variable {
            entry_sizes: vec![1],
        },
        stco_or_co64_box: Either::B(Co64Box {
            chunk_offsets: vec![u64::MAX],
        }),
        stss_box: None,
        sdtp_box: None,
        unknown_boxes: Vec::new(),
    };

    let result = SampleTableAccessor::new(&stbl_box);
    let Err(SampleTableAccessorError::SampleDataOffsetOverflow {
        sample_index,
        accumulated_offset,
        sample_data_size,
    }) = result
    else {
        panic!("SampleDataOffsetOverflow が返るはずが、実際は {result:?} だった");
    };

    assert_eq!(
        sample_index,
        NonZeroU32::new(1).expect("bug"),
        "捨てられる末尾加算でも sample_index は 1 になること"
    );
    assert_eq!(
        accumulated_offset,
        u64::MAX,
        "オーバーフロー直前の累計バイト位置が u64::MAX であること"
    );
    assert_eq!(
        sample_data_size, 1,
        "加算しようとしたサンプルサイズが 1 であること"
    );
}

/// サンプル数の合計がちょうど u32::MAX のとき、SampleCountOverflow ではなく
/// InconsistentSampleCount が返ること
///
/// `checked_add` は境界の取り違えを起こさない実装だが、将来これを手書きの上限
/// 比較に置き換えた際の回帰検出として境界ちょうどの入力を検証する。
/// `stsz` は `Fixed` にして `entry_sizes` の 17 GB 確保を避け、`stsc` / `stco`
/// を空にして `sample_data_offsets` の 34 GB 確保も避けている（`stts` サンプル数
/// と stsc/stco 空のミスマッチが `InconsistentSampleCount` として先に検出される）。
#[test]
fn sample_count_exactly_u32_max_returns_inconsistent_not_overflow() {
    let stbl_box = StblBox {
        stsd_box: stsd_box(),
        stts_box: SttsBox {
            entries: vec![
                SttsEntry {
                    sample_count: 0x8000_0000,
                    sample_delta: 1,
                },
                SttsEntry {
                    sample_count: 0x7fff_ffff,
                    sample_delta: 1,
                },
            ],
        },
        ctts_box: None,
        cslg_box: None,
        stsc_box: StscBox {
            entries: Vec::new(),
        },
        stsz_box: StszBox::Fixed {
            sample_size: NonZeroU32::MIN,
            sample_count: u32::MAX,
        },
        stco_or_co64_box: Either::A(StcoBox {
            chunk_offsets: Vec::new(),
        }),
        stss_box: None,
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
        stts_sample_count,
        u32::MAX,
        "stts の累計サンプル数がちょうど u32::MAX に一致すること"
    );
    assert_eq!(
        other_box_type,
        StscBox::TYPE,
        "ミスマッチが検出されたのは stsc（空）であること"
    );
    assert_eq!(other_sample_count, 0, "stsc のサンプル数が 0 であること");
}

/// 新規 2 バリアントの Display 出力が仕様通りであること
#[test]
fn display_of_new_variants_matches_spec() {
    let sample_count_overflow = SampleTableAccessorError::SampleCountOverflow {
        box_type: SttsBox::TYPE,
        accumulated_sample_count: 0x8000_0000,
        entry_sample_count: 0x8000_0000,
    };
    assert_eq!(
        sample_count_overflow.to_string(),
        "Total sample count in `stts` box overflows u32 (accumulated 2147483648, adding 2147483648)",
        "SampleCountOverflow の Display 出力が仕様と一致すること"
    );

    let offset_overflow = SampleTableAccessorError::SampleDataOffsetOverflow {
        sample_index: NonZeroU32::new(2).expect("bug"),
        accumulated_offset: u64::MAX,
        sample_data_size: 1,
    };
    assert_eq!(
        offset_overflow.to_string(),
        "Sample data offset overflows u64 at sample 2 (accumulated 18446744073709551615, adding 1)",
        "SampleDataOffsetOverflow の Display 出力が仕様と一致すること"
    );
}

/// `StszBox::Fixed` 経路でサンプルデータのオフセット累計が u64 を超えるケース
/// （1 チャンク 3 サンプル）
///
/// `chunk_offsets[0] = u64::MAX - 1` にサイズ 1 のサンプルを 2 つ足した
/// `u64::MAX` を累計として持ち、3 サンプル目の先頭位置を求める 2 サンプル目末尾の
/// 加算がオーバーフローする。`Fixed` ではテーブルを構築せずチャンク単位の
/// 判定だけで検出するため、`sample_data_offsets` を 3 要素作らない。
///
/// `stsz` が `Variable` の既存テスト（`sample_data_offset_overflow_multi_sample_chunk`）と
/// 同じ入力構成・同じ期待値で、`Fixed` 経路の回帰を捕捉する。
#[test]
fn fixed_sample_data_offset_overflow_multi_sample_chunk() {
    let stbl_box = StblBox {
        stsd_box: stsd_box(),
        stts_box: SttsBox {
            entries: vec![SttsEntry {
                sample_count: 3,
                sample_delta: 1,
            }],
        },
        ctts_box: None,
        cslg_box: None,
        stsc_box: StscBox {
            entries: vec![StscEntry {
                first_chunk: NonZeroU32::MIN,
                sample_per_chunk: 3,
                sample_description_index: NonZeroU32::MIN,
            }],
        },
        stsz_box: StszBox::Fixed {
            sample_size: NonZeroU32::new(1).expect("1 は非ゼロなので失敗しない"),
            sample_count: 3,
        },
        stco_or_co64_box: Either::B(Co64Box {
            chunk_offsets: vec![u64::MAX - 1],
        }),
        stss_box: None,
        sdtp_box: None,
        unknown_boxes: Vec::new(),
    };

    let result = SampleTableAccessor::new(&stbl_box);
    let Err(SampleTableAccessorError::SampleDataOffsetOverflow {
        sample_index,
        accumulated_offset,
        sample_data_size,
    }) = result
    else {
        panic!("SampleDataOffsetOverflow が返るはずが、実際は {result:?} だった");
    };

    assert_eq!(
        sample_index,
        NonZeroU32::new(2).expect("bug"),
        "オーバーフロー時点のサンプルインデックスが 2 であること"
    );
    assert_eq!(
        accumulated_offset,
        u64::MAX,
        "オーバーフロー直前の累計バイト位置が u64::MAX であること"
    );
    assert_eq!(
        sample_data_size, 1,
        "加算しようとしたサンプルサイズが 1 であること"
    );
}

/// `StszBox::Fixed` 経路でサンプルデータのオフセット累計が u64 を超えるケース
/// （1 チャンク 1 サンプル）
///
/// 唯一のサンプルの値はすべて正常に算出され、その直後に次サンプル用の開始位置を
/// 求めるための末尾の加算だけがオーバーフローする。「格納は正常だが末尾の加算だけ
/// オーバーフロー」のケースでも `Err` を返す仕様を、`Fixed` 経路でも満たすことの
/// 回帰テストである。
#[test]
fn fixed_sample_data_offset_overflow_single_sample_chunk_tail() {
    let stbl_box = StblBox {
        stsd_box: stsd_box(),
        stts_box: SttsBox {
            entries: vec![SttsEntry {
                sample_count: 1,
                sample_delta: 1,
            }],
        },
        ctts_box: None,
        cslg_box: None,
        stsc_box: StscBox {
            entries: vec![StscEntry {
                first_chunk: NonZeroU32::MIN,
                sample_per_chunk: 1,
                sample_description_index: NonZeroU32::MIN,
            }],
        },
        stsz_box: StszBox::Fixed {
            sample_size: NonZeroU32::new(1).expect("1 は非ゼロなので失敗しない"),
            sample_count: 1,
        },
        stco_or_co64_box: Either::B(Co64Box {
            chunk_offsets: vec![u64::MAX],
        }),
        stss_box: None,
        sdtp_box: None,
        unknown_boxes: Vec::new(),
    };

    let result = SampleTableAccessor::new(&stbl_box);
    let Err(SampleTableAccessorError::SampleDataOffsetOverflow {
        sample_index,
        accumulated_offset,
        sample_data_size,
    }) = result
    else {
        panic!("SampleDataOffsetOverflow が返るはずが、実際は {result:?} だった");
    };

    assert_eq!(
        sample_index,
        NonZeroU32::new(1).expect("bug"),
        "捨てられる末尾加算でも sample_index は 1 になること"
    );
    assert_eq!(
        accumulated_offset,
        u64::MAX,
        "オーバーフロー直前の累計バイト位置が u64::MAX であること"
    );
    assert_eq!(
        sample_data_size, 1,
        "加算しようとしたサンプルサイズが 1 であること"
    );
}

/// `StszBox::Fixed` で `sample_count = u32::MAX - 1` を宣言しても
/// `new` が即座に成功し、算術的に正しい `data_offset()` を返すこと
///
/// 修正前は `sample_data_offsets` を約 34 GB 確保しようとして abort（確保失敗）する。
/// 修正後はテーブルを構築しないため、サンプル数に比例した確保なしで成功する。
/// `data_offset()` が `base + チャンク内序数 × sample_size` で算出されることを
/// 先頭・途中・末尾のサンプルで照合する。
#[test]
fn fixed_max_sample_count_succeeds_without_allocation() {
    const MAX_COUNT: u32 = u32::MAX - 1;
    let stbl_box = StblBox {
        stsd_box: stsd_box(),
        stts_box: SttsBox {
            entries: vec![SttsEntry {
                sample_count: MAX_COUNT,
                sample_delta: 1,
            }],
        },
        ctts_box: None,
        cslg_box: None,
        stsc_box: StscBox {
            entries: vec![StscEntry {
                first_chunk: NonZeroU32::MIN,
                sample_per_chunk: MAX_COUNT,
                sample_description_index: NonZeroU32::MIN,
            }],
        },
        stsz_box: StszBox::Fixed {
            sample_size: NonZeroU32::new(1).expect("1 は非ゼロなので失敗しない"),
            sample_count: MAX_COUNT,
        },
        stco_or_co64_box: Either::A(StcoBox {
            chunk_offsets: vec![0],
        }),
        stss_box: None,
        sdtp_box: None,
        unknown_boxes: Vec::new(),
    };

    let accessor =
        SampleTableAccessor::new(&stbl_box).expect("Fixed なら確保なしでアクセサを作れる");

    assert_eq!(
        accessor.sample_count(),
        MAX_COUNT,
        "サンプル数が保持されること"
    );

    for sample_index in [1u32, 1000, MAX_COUNT] {
        let sample = accessor
            .get_sample(NonZeroU32::new(sample_index).expect("1 以上なので非ゼロ"))
            .expect("sample_count 以内のサンプルは取得できる");
        assert_eq!(
            sample.data_offset(),
            (sample_index - 1) as u64,
            "sample {sample_index} の data_offset が算術で算出されること"
        );
    }
}

/// `sample_per_chunk == 0` の空チャンクを挟んでも Fixed / Variable の
/// `data_offset()` が一致すること
///
/// 空チャンクがあると `sample_index_offsets` に同一値が連続する。
/// Fixed の `data_offset()` は `chunk()` 経由でチャンク先頭を引くため、
/// 重複時に空チャンク側を選ぶとオフセットがずれる。
/// `chunk()` が index 以下の最右を選ぶことで Variable（prefix-sum）と一致することを固定する。
#[test]
fn fixed_and_variable_data_offset_match_with_empty_chunk() {
    fn nz(v: u32) -> NonZeroU32 {
        NonZeroU32::new(v).expect("テスト入力のインデックスは非ゼロ")
    }

    let stsc_box = StscBox {
        entries: vec![
            StscEntry {
                first_chunk: nz(1),
                sample_per_chunk: 2,
                sample_description_index: nz(1),
            },
            StscEntry {
                first_chunk: nz(2),
                sample_per_chunk: 0,
                sample_description_index: nz(1),
            },
            StscEntry {
                first_chunk: nz(3),
                sample_per_chunk: 2,
                sample_description_index: nz(1),
            },
        ],
    };
    // 空チャンクのオフセットは実サンプル側と意図的にずらす
    let chunk_offsets = vec![100, 999, 200];
    let sample_count = 4u32;
    let sample_size = 10u32;

    let build = |stsz_box: StszBox| StblBox {
        stsd_box: stsd_box(),
        stts_box: SttsBox {
            entries: vec![SttsEntry {
                sample_count,
                sample_delta: 1,
            }],
        },
        ctts_box: None,
        cslg_box: None,
        stsc_box: stsc_box.clone(),
        stsz_box,
        stco_or_co64_box: Either::A(StcoBox {
            chunk_offsets: chunk_offsets.clone(),
        }),
        stss_box: None,
        sdtp_box: None,
        unknown_boxes: Vec::new(),
    };

    let fixed_stbl = build(StszBox::Fixed {
        sample_size: nz(sample_size),
        sample_count,
    });
    let variable_stbl = build(StszBox::Variable {
        entry_sizes: vec![sample_size; sample_count as usize],
    });
    let fixed =
        SampleTableAccessor::new(&fixed_stbl).expect("空チャンクを含む正当な Fixed は成功する");
    let variable = SampleTableAccessor::new(&variable_stbl)
        .expect("空チャンクを含む正当な Variable は成功する");

    for i in 1..=sample_count {
        let index = nz(i);
        let fixed_offset = fixed
            .get_sample(index)
            .expect("sample_count 以内なので取得できる")
            .data_offset();
        let variable_offset = variable
            .get_sample(index)
            .expect("sample_count 以内なので取得できる")
            .data_offset();
        assert_eq!(
            fixed_offset, variable_offset,
            "sample {i} の data_offset が Fixed と Variable で一致すること"
        );
        // 空チャンク (offset 999) を選んでいないこと
        assert_ne!(
            fixed_offset, 999,
            "sample {i} が空チャンクのオフセットを返していないこと"
        );
    }

    // 期待値そのものも固定する（チャンク1: 100,110 / チャンク3: 200,210）
    assert_eq!(
        fixed.get_sample(nz(1)).expect("bug").data_offset(),
        100,
        "先頭チャンクの 1 サンプル目"
    );
    assert_eq!(
        fixed.get_sample(nz(2)).expect("bug").data_offset(),
        110,
        "先頭チャンクの 2 サンプル目"
    );
    assert_eq!(
        fixed.get_sample(nz(3)).expect("bug").data_offset(),
        200,
        "空チャンクの次のチャンクの 1 サンプル目"
    );
    assert_eq!(
        fixed.get_sample(nz(4)).expect("bug").data_offset(),
        210,
        "空チャンクの次のチャンクの 2 サンプル目"
    );
}

// ===== pbt/tests/prop_auxiliary.rs から移した単体テスト =====

// ===== prop_auxiliary.rs から移した単体テスト用ヘルパ =====

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

mod composition_time_offset_unit_tests {
    use super::*;

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
