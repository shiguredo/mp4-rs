# `MfraBox` の decode → encode で `mfro.size` が再エンコード後の実サイズと一致しない

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-mfra-mfro-size-stale
- Polished: {YYYY-MM-DD}

## 目的

`mfra` を decode して encode し直したとき、`mfro` の `mfra_size` が再エンコード後の `mfra` の実サイズと一致するようにする。

ISO/IEC 14496-12 の `MovieFragmentRandomAccessOffsetBox` の `mfra_size` は、それを含む `mfra` ボックスのサイズでなければならない。ここが食い違うと、ファイル末尾から `mfra` を逆算する読み手が壊れる。

## 現状

`src/boxes_fmp4.rs` の `MfraBox::decode` は `tfra` と `mfro` 以外の子ボックスを読み飛ばす。`MfraBox` には未知の子ボックスを保持する欄がないため、再エンコードするとその子ボックスは消える。

一方 `MfraBox::encode` は `self.mfro_box` をそのまま書くため、`mfro.size` は decode した時点の値のまま残る。

再現:

- `tfra` 35 バイト、`free` 16 バイト、`mfro` 16 バイト、全体 75 バイトの `mfra` を組み立てる
- `MfraBox::decode` は 75 バイトを消費し、`tfra` 1 件と `mfro.size == 75` を返す
- `encode_to_vec()` の出力は 59 バイト (`free` が消える) だが、`mfro.size` は 75 のまま

確認結果:

```text
decoded: consumed=75, tfra=1, mfro.size=75
encoded: len=59, mfro.size=75, mismatch=true
```

## 設計方針

- `MfraBox::encode` で `mfro.size` を再計算する。`mfra` 全体のサイズは子ボックスのエンコード結果から決まるため、`mfro.size` を仮の値で書いてサイズを確定してから、実際の値で書き直す 2 パスにする
- 未知の子ボックスを保持して往復させる対応は、この issue の必須条件にはしない。まず `mfro.size` と実サイズの一致を保証する
- `Fmp4SegmentMuxer::mfra_bytes` は既に 2 パスで `mfro.size` を確定させているため、同じ方針を `MfraBox::encode` に移す
- 不正な `mfro.size` を持つ入力を受け取った場合も、encode では実サイズを優先する

## 完了条件

- `tfra`、`free`、`mfro` を含む `mfra` を decode → encode したとき、出力の `mfro.size` が出力全体のバイト数と一致する
- 未知の子ボックスを含まない `mfra` でも、`mfro.size` が出力全体のバイト数と一致する
- 上記の 75 バイト → 59 バイトのケースを固定するテストが `tests/test_boxes_fmp4.rs` に追加されている
- `cargo test --workspace` が成功する
