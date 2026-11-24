# `pbt/tests/prop_basic_types.rs` でテスト関数の説明を `//` から `///` に書き換える

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-prop-basic-types-doc-comments
- Polished: {YYYY-MM-DD}

## 目的

`pbt/tests/prop_basic_types.rs` に、テスト関数の説明を `///` ではなく項目直前の `//` で書いた箇所が 39 件残っている。shiguredo-rust の規約「項目の説明を項目直前の `//` で書かないこと（doc コメントとして扱われず rustdoc に出力されないため）」に合わせて、`///` に書き換える。

## 現状

数え方: 項目（`fn` / `mod` / `struct` / `enum` / `const` / `static` / `impl` / `type` / `trait` / `use`）の直前にある `//` の行（`///` と `//!` を除く）を数える。`//` の行と項目の間にある属性の行（`#[test]` など）は飛ばし、空行を挟むものは数えない。

該当は 39 件で、すべて `#[test]` を付けたテスト関数の直前にある 1 行のコメントである。`///` と `//` が混在する説明はない。

- トップレベル（22 件）: `full_box_flags_roundtrip`、`full_box_flags_from_flags`、`full_box_flags_is_set_any_bit_position`、`full_box_flags_from_flags_any_bit_position`、`full_box_flags_from_flags_duplicate_positions_or_folded`、`full_box_header_roundtrip`、`fixed_point_u8_u8_roundtrip`、`fixed_point_i16_u16_roundtrip`、`box_type_normal_external_size`、`box_type_uuid_external_size`、`box_type_as_bytes_normal`、`box_type_as_bytes_uuid`、`box_size_u32_properties`、`box_size_u64_properties`、`utf8_string_roundtrip`、`utf8_string_rejects_null`、`mp4_file_time_roundtrip`、`uint_u8_4_0_from_bits`、`uint_u8_4_4_from_bits`、`uint_u16_12_0_from_bits`、`uint_to_bits_from_bits_symmetry`、`uint_1_as_bool`
- `error_cases` モジュール（17 件）: `full_box_flags_encode_insufficient_buffer`、`full_box_header_encode_insufficient_buffer`、`box_header_encode_insufficient_buffer`、`box_header_uuid_encode_insufficient_buffer`、`box_header_u64_encode_insufficient_buffer`、`full_box_flags_decode_truncated`、`full_box_header_decode_truncated`、`box_header_decode_truncated`、`fixed_point_u8_u8_decode_truncated`、`fixed_point_i16_u16_decode_truncated`、`utf8_string_decode_no_null_terminator`、`box_header_decode_invalid_size`、`box_header_decode_arbitrary_bytes`、`full_box_flags_decode_arbitrary_bytes`、`full_box_header_decode_arbitrary_bytes`、`utf8_string_decode_arbitrary_bytes`、`fixed_point_decode_arbitrary_bytes`

同じファイルのほかの項目（生成器、`box_size_with_payload_size`、`box_header_normal_u32_roundtrip` などの `BoxHeader` のラウンドトリップ、`error_cases` / `weird_values` モジュール自体と `weird_values` のテスト）は `///` で書かれている。同じ数え方で `pbt/tests/` と `tests/` の `*.rs` を調べた結果、該当はこのファイルだけだった。

## 設計方針

- 39 件の `//` を `///` に書き換える。文言は今のコメントを基に、そのテストが何を確かめるかを rustdoc の文として読める形にする（例: `// BoxType::as_bytes` のように対象が読み取りにくいものは、テストの中身に合わせて対象の variant を補う）
- テストの入力・断言・ケース数は変えない
- `error_cases` のうち、任意のバイト列でパニックしないことだけを確かめる PBT（`*_decode_arbitrary_bytes`）を fuzzing へ移すかどうかは、本 issue の対象外とする

## 完了条件

- 次のコマンドで、`pbt/tests/prop_basic_types.rs` に該当が 1 件も見つからない（今は 39 件）

  ```sh
  rg -U --count-matches '^[ \t]*//([^/!\n][^\n]*)?\n([ \t]*#\[[^\n]*\n)*[ \t]*(pub(\([^)]*\))? )?(fn|mod|struct|enum|const|static|impl|type|trait|use) ' pbt/tests/prop_basic_types.rs
  ```

- 39 件のテストの入力・断言・ケース数が変わっていない
- `cargo test -p pbt --test prop_basic_types` が成功する
