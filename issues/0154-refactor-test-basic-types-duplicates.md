# `tests/test_basic_types.rs` の重複テストと参照先を失った目印コメントを整理する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-test-basic-types-duplicates
- Polished: {YYYY-MM-DD}
- Refreshed: 2026-11-24

## 目的

`tests/test_basic_types.rs` の `boundary_tests_from_prop_basic_types` モジュールに、同じモジュールの別のテストや PBT と重複するテストが 3 件ある。また、このモジュールの目印コメントとモジュール名は、すでに存在しない `pbt/tests/prop_basic_types.rs` の `boundary_tests` を参照している。shiguredo-rust の「PBT でカバーできるものを単体テストで書かないこと」と shiguredo-test の「無駄なテストを絶対に追加しないこと」に合わせて重複を削除し、モジュールを移動元ではなく中身で名付け直す。

## 現状

### 目印コメントとモジュール名

`tests/test_basic_types.rs` の `boundary_tests_from_prop_basic_types` モジュールの直前に、`// ===== pbt/tests/prop_basic_types.rs の boundary_tests 固定入力から移動 =====` というコメントがある。0081 で `pbt/tests/prop_basic_types.rs` の `boundary_tests` から固定入力の単体テストを移したときの目印で、モジュール名も移動元を表している。

`pbt/tests/prop_basic_types.rs` の `boundary_tests` は、0081 の後も PBT の `box_size_u64_boundary` / `box_size_large_payload` を持っていた。0132 でこの 2 件を `box_size_with_payload_size` の PBT に含めて削除し、モジュールごとなくなった。今の `pbt/tests/prop_basic_types.rs` に `boundary_tests` はない。

ファイル先頭の `//!` は、このモジュールが固定している内容（`FullBoxFlags`、`BoxSize` の定数、`Utf8String`、`Mp4FileTime`、`BoxHeader` の定数と largesize のデコード）に触れていない。

### 重複しているテスト

重複は「同じ API に同じ入力を与え、断言が別のテストの断言に含まれるもの」とする。PBT との重複は、PBT がその入力を固定の候補として持ち、各ケースで候補から一様に選ぶものに限る。

| 削除するテスト | 重複先 | 根拠 |
| --- | --- | --- |
| `box_header_size_zero_means_variable` | 同じモジュールの `box_size_variable` | `box_size` に `BoxSize::VARIABLE_SIZE` を入れた `BoxHeader` の `box_size.get()` が 0 であることだけを確かめる。`box_size_variable` の `BoxSize::VARIABLE_SIZE.get() == 0` と同じ |
| `utf8_string_only_null` | 同じモジュールの `utf8_string_empty` | `[0]` を `Utf8String::decode` して、長さ 1 と空文字列を確かめる。`utf8_string_empty` は `Utf8String::new("")` のエンコード結果が `[0]` であることを確かめた後、同じバイト列を同じ断言でデコードしている |
| `mp4_file_time_max` | `pbt/tests/prop_basic_types.rs` の `weird_values::mp4_file_time_extreme_values` | `Mp4FileTime::from_secs(u64::MAX).as_secs() == u64::MAX` を確かめる。PBT は `u64::MAX` を含む 8 個の候補から 500 ケース一様に選び、同じ断言をする（任意の `u64` を使う `mp4_file_time_roundtrip` もある） |

次のものは PBT と重なる部分があるが、上の定義では重複に当たらない（固有の入力か断言がある）ため残す。

- `full_box_flags_overflow_ignored`: `weird_values::full_box_flags_boundary_bits` は `0xFFFFFFFF` を候補に持ちデコード結果を確かめるが、エンコード結果が 3 バイトであることは確かめない
- `box_size_variable` / `box_size_variable_external_sizes`: `box_size_u32_properties` / `box_size_u64_properties` は 0 を境界値として生成するが、定数の `BoxSize::VARIABLE_SIZE` / `BoxSize::LARGE_VARIABLE_SIZE` を使わない
- `utf8_string_empty`: `utf8_string_roundtrip` は長さを 0..=100 から一様に引くため、空文字列を毎回は引かない。`weird_values::utf8_string_weird_chars` は空文字列を候補に持つが、エンコード結果とデコードしたバイト数を確かめない
- `utf8_string_invalid_utf8`: `weird_values::utf8_string_invalid_sequences` はエラーになることだけを確かめ、`ErrorKind::InvalidInput` であることは確かめない
- `box_header_decode_extended_size`: 手で組み立てたバイト列をデコードする。`box_header_normal_u64_roundtrip` はエンコード結果をデコードし、`weird_values::box_header_extended_size_edge_cases` はパニックしないことだけを確かめる

このモジュールのほかのテスト（`full_box_flags_zero` / `full_box_flags_max` / `mp4_file_time_unix_epoch` / `box_header_min_size` / `box_header_max_size`）と、ほかのモジュールのテストには、重複に当たるものはない。

## 設計方針

- 上の表の 3 件を削除する
- 目印コメントを削除し、`boundary_tests_from_prop_basic_types` を中身を表す名前（例: `basic_type_boundaries`）に改める。モジュールは分割しない
- ファイル先頭の `//!` に、このモジュールが固定する内容を加える
- 残すテストの入力と断言は変えない。`pbt/tests/prop_basic_types.rs` は変更しない

## 完了条件

- 上の表の 3 件のテストが削除されている
- `rg -n 'boundary_tests|から移動' tests/test_basic_types.rs` が何も返さない
- ファイル先頭の `//!` が、改名したモジュールの内容に触れている
- `cargo test --test test_basic_types` が成功する
