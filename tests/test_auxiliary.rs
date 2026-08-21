//! `SampleTableAccessor::new()` のオーバーフロー検出に関する回帰テスト
//!
//! `SampleTableAccessorError` の `SampleCountOverflow` / `SampleDataOffsetOverflow`
//! バリアントが期待通りの値で返ることを、公開 API のみを使って検証する。
//!
//! 固定入力による回帰テストであり PBT で代替できないため、
//! `pbt/tests/prop_auxiliary.rs` ではなく `tests/test_<module>.rs` 規約に従って
//! integration test として配置している。

use std::num::NonZeroU32;

use shiguredo_mp4::{
    BoxSize, BoxType, Either,
    aux::{SampleTableAccessor, SampleTableAccessorError},
    boxes::{
        Co64Box, CttsBox, CttsEntry, SampleEntry, StblBox, StcoBox, StscBox, StscEntry, StsdBox,
        StszBox, SttsBox, SttsEntry, UnknownBox,
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
