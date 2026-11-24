# `DecoderSpecificInfo::decode` で入力が宣言サイズに足りない場合に `InsufficientBuffer` を返す

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-decoder-specific-info-insufficient-buffer
- Polished: {YYYY-MM-DD}

## 目的

`DecoderSpecificInfo::decode` で、入力が宣言されたペイロードの終端に達しない場合のエラー種別を `ErrorKind::InsufficientBuffer` にする。`EsDescriptor` / `DecoderConfigDescriptor` / `SlConfigDescriptor` は、同じ条件ですでに `InsufficientBuffer` を返している。

今は同じ条件でも、`DecoderSpecificInfo` だけが `ErrorKind::InvalidData` を返す。`DecoderSpecificInfo` の中でも、ヘッダーの途中で入力が尽きた場合は `InsufficientBuffer` を返し、ペイロードの途中で尽きた場合は `InvalidData` を返すため、種別が食い違っている。

## 現状

`src/descriptors.rs` の `DecoderSpecificInfo::decode` は、共通関数の `decode_descriptor_payload` を使わない。`decode_tag_and_size` で読んだ宣言サイズを入力の残りと比べ、足りなければ `InvalidData`（理由 `DecoderSpecificInfo payload exceeds buffer boundary`）を返す。冒頭のコメントには次のように書かれている。

> `DecoderSpecificInfo` は、ほかのディスクリプターと異なり、入力が宣言されたペイロードの終端に達しない場合に `ErrorKind::InsufficientBuffer` ではなく `ErrorKind::InvalidData` を返す。この挙動を変えないため、`decode_descriptor_payload` は使わない

これは、0128 で既存の挙動を残した結果である。

ほかの 3 型は `decode_descriptor_payload` を使い、ヘッダーと宣言されたペイロードが入力に収まらなければ `InsufficientBuffer` を返す。ボックスの `BoxHeader::decode_header_and_payload` も、宣言サイズが入力を超える場合は `InsufficientBuffer` を返す。

正しいディスクリプターの先頭から、長さを変えて切り出した入力を単体で decode した。develop での結果は次のとおり。`SlConfigDescriptor` と `DecoderConfigDescriptor` は、表に挙げた長さだけを確かめた。

| 入力 | 切り出した長さ（バイト） | 結果 |
| --- | --- | --- |
| `DecoderSpecificInfo`（`05 02 11 90`） | 0、1（ヘッダーの途中まで） | `InsufficientBuffer` |
| 同上 | 2、3（ヘッダーの後、ペイロードの途中まで） | `InvalidData` |
| 同上 | 4 | 成功 |
| `SlConfigDescriptor`（`06 01 02`） | 0、1、2 | `InsufficientBuffer` |
| `DecoderConfigDescriptor`（`DecoderSpecificInfo` なし、15 バイト） | 0、1、2、3、14 | `InsufficientBuffer` |

入力を少しずつ足していくと、`DecoderSpecificInfo` だけ、ヘッダーがそろった時点で種別が `InsufficientBuffer` から `InvalidData` に変わる。

影響範囲:

- クレートの中で `DecoderSpecificInfo::decode` を呼ぶのは `DecoderConfigDescriptor::decode_payload` だけである。渡すのは、`DecoderConfigDescriptor` の宣言されたペイロードに収めたスライスである
  - この経路では、`DecoderSpecificInfo` が `InsufficientBuffer` を返しても、`within_declared_payload` が `InvalidData` に置き換える。そのため、種別は変わらない
- `EsdsBox::decode` は `esds` のペイロードを `EsDescriptor` に渡す。`DecoderSpecificInfo` を直接 decode することはない
- クレートの中には、ディスクリプターの decode が返す `InsufficientBuffer` を見て、入力を足してから decode し直す呼び出し側はない
- 影響を受けるのは、公開の `Decode` 実装で `DecoderSpecificInfo` を単体で decode する利用者である。そうした利用者が `InsufficientBuffer` を入力の不足として扱っていると、`DecoderSpecificInfo` のペイロードが途中で切れた入力だけを、壊れたデータとして扱うことになる

今の挙動を固定しているテスト:

- `src/descriptors.rs` の `mod tests` の `decoder_specific_info_payload_exceeds_boundary_with_usize_max_size`
  - 宣言サイズが `usize::MAX` の入力で、種別が `InvalidData` であることと、理由を確認している
- `tests/test_descriptors.rs` の `decoder_specific_info_payload_exceeds_boundary`
  - `is_err()` だけを確認している

## 設計方針

- `DecoderSpecificInfo::decode` を `decode_descriptor_payload` を使う形に書き直す
  - 入力が宣言されたペイロードの終端に達しない場合は、`InsufficientBuffer` を返す
  - タグ不一致の `InvalidData` と、サイズ欄の値が `usize` に収まらない場合の `InvalidData` は変えない
- 冒頭の、挙動を変えない理由のコメントは削除する
- `decode_descriptor_payload` は、ヘッダー長と宣言サイズの和がオーバーフローした場合も `InsufficientBuffer` として扱う。そのため、宣言サイズが `usize::MAX` の場合もパニックしない
  - この確認は、`src/descriptors.rs` の `mod tests` にある `descriptor_payload_end_overflow_is_insufficient_buffer` の対象に `DecoderSpecificInfo` を加えて行う
  - `decoder_specific_info_payload_exceeds_boundary_with_usize_max_size` は削除する
- `DecoderConfigDescriptor` の中の `DecoderSpecificInfo` が、親の宣言されたペイロードを超える場合は、これまでどおり `InvalidData` になる
  - ただし理由は、`within_declared_payload` が付ける `DecoderConfigDescriptor contents exceed declared descriptor size` に変わり、`DecoderSpecificInfo` の名前が消える
  - 理由に失敗したディスクリプターの型名を含める作業は、0143 で扱う

## 完了条件

- `tests/test_descriptors.rs` の `decoder_specific_info_payload_exceeds_boundary` を、エラー種別まで照合する形にする。ヘッダーの後、ペイロードの途中で切れた `DecoderSpecificInfo` を単体で decode すると、`InsufficientBuffer` を返すことを確認する
- 宣言サイズが `usize::MAX` の `DecoderSpecificInfo` が、パニックせずに `InsufficientBuffer` を返すことを、`src/descriptors.rs` の `mod tests` で確認する
- `DecoderConfigDescriptor` の宣言されたペイロードを超える子の `DecoderSpecificInfo` は、引き続き `InvalidData` になる。既存の `decoder_config_descriptor_child_exceeds_declared_payload` が変更なしで通ることで確認する
- `cargo test --workspace` が成功する
