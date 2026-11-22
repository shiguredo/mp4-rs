# `decode_tag_and_size` のディスクリプター size が 64 ビットでラップし、オーバーフロー検査をすり抜ける

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-descriptor-size-overflow
- Polished: {YYYY-MM-DD}

## 目的

ディスクリプターの size フィールドを 7 ビットずつ連結するときに桁溢れを検出し、巨大な size を小さな値として受理しないようにする。

## 現状

`src/descriptors.rs` の `decode_tag_and_size` は、size の更新に `size.checked_shl(7)` を使っている。`checked_shl` はシフト量がビット幅以上かどうかしか検査しないため、`usize` (64 ビット) を超える桁溢れは検出できず、上位ビットが黙って切り捨てられる。

再現入力:

```text
05 81 80 80 80 80 80 80 80 80 80 00
```

これは `tag=5`、size フィールド 11 バイトで、先頭の 7 ビットが 1、残りが 0 である。数学的な宣言値は `128^10 = 2^70` だが、64 ビットでラップして 0 になる。

確認結果:

```text
decoded ok: payload_len=0, consumed=12
```

本来は size の桁溢れとして `InvalidData` を返すべき入力が、空の `DecoderSpecificInfo` として受理される。

## 設計方針

- `checked_shl` ではなく、`checked_mul(128)` と `checked_add` で桁溢れを検査する
- size が入力の残りバイト数を超える場合は、その時点で `InvalidData` を返す
- 既存の正常なディスクリプターの decode 結果は変えない
- 10 バイト以上の size フィールドを持つ入力のテストを追加する

## 完了条件

- 上記の 11 バイトの size フィールドを持つ入力が `InvalidData` になる
- size が `usize` の範囲を超える入力で、ラップした値として受理されない
- 2 バイト以内の通常の size フィールドを持つ既存のテストが引き続き成功する
- `tests/test_descriptors.rs` に上記の再現テストが追加されている
- `cargo test --workspace` が成功する
