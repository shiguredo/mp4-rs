# `MfraBox` の decode → encode で `mfro.size` が再エンコード後の実サイズと一致しない

- Created: 2026-11-22
- Completed: 2026-11-23
- Branch: feature/fix-mfra-mfro-size-stale
- Polished: 2026-11-22

## 目的

`mfra` を decode して encode し直したとき、出力の `mfro.size` が再エンコード後の `mfra` の実サイズと一致するようにする。

ISO/IEC 14496-12:2015 の 8.8.11.1〜8.8.11.3 では、`MovieFragmentRandomAccessOffsetBox` の `size` は親の `mfra` ボックス全体のバイト数を表す。値が実サイズと異なると、ファイル末尾の `mfro` を起点に `mfra` の先頭を逆算する読み手が正しい位置を見つけられない。

## 現状

`src/boxes_fmp4.rs` の `MfraBox::decode` は `tfra` と `mfro` 以外の子ボックスを読み飛ばす。`MfraBox` には未知の子ボックスを保持する欄がないため、再エンコードするとその子ボックスは消える。

一方 `MfraBox::encode` は `self.mfro_box` をそのまま書くため、`mfro.size` は decode した時点の値のまま残る。

再現:

- 通常の 8 バイトヘッダーに、35 バイトの `tfra`（version 0、番号の幅は各 1 バイト、エントリー 1 件）、16 バイトの `free`、`size` に 75 を入れた 16 バイトの `mfro` を順に付け、75 バイトの `mfra` を組み立てる
- `MfraBox::decode` は 75 バイトを消費し、`tfra` ボックス 1 個と `mfro.size == 75` を返す
- `encode_to_vec()` の出力は 59 バイト（`free` が消える）だが、出力を再デコードすると `mfro.size` は 75 のまま

確認結果:

```text
decoded: consumed=75, tfra=1, mfro.size=75
encoded: len=59, mfro.size=75, mismatch=true
```

## 設計方針

- `MfraBox::encode` は、通常ヘッダーと各 `tfra` を出力バッファへ書き、最後に固定長の `mfro` を `size = 0` で書く。従来どおり `BoxHeader::finalize_box_size` に書き込み済み範囲を渡し、`mfra` ヘッダーの `size` フィールドを確定する。その後、同じバッファの `mfro.size` フィールドだけを `mfra` 全体のバイト数で上書きする。`mfra` 全体や `tfra` は書き直さない
- `self.mfro_box.size` の値は出力に使わず、実サイズが `u32` に収まることを確認して書く。通常ヘッダーのサイズ上限を超える場合は、`BoxHeader::finalize_box_size` の `ErrorKind::InvalidInput` をそのまま返す。バッファ不足時の `ErrorKind::InsufficientBuffer` も現状どおりとする
- `Fmp4SegmentMuxer::mfra_bytes` は現状、`mfra` 全体を 2 回エンコードしている。仮エンコードとサイズの代入を削除し、`MfraBox::encode_to_vec` を 1 回呼ぶ。`tfra` エントリーの生成は変えない
- `MfraBox::decode` の挙動と単体の `MfroBox::encode` の挙動は変えない。未知の子ボックスは引き続き読み飛ばし、保持は対象外とする

## 完了条件

- ボックスの PBT に、通常ヘッダーと `tfra`、`free`、`mfro` からなる `mfra` の decode → encode を追加する。出力が `free` のバイト数だけ短くなり、出力を再デコードした `mfro.size` が出力のバイト数と一致することを確認する。上記の再現例なら 59 バイトとなる
- 同じ PBT で、未知の子ボックスがなく、入力の `mfro.size` に実サイズと異なる値を設定したケースも検証する。出力を再デコードした `mfro.size` がその出力のバイト数と一致することを確認する
- `Fmp4SegmentMuxer::mfra_bytes` の既存のラウンドトリップ PBT を含め、`cargo test --workspace` が成功する

## 解決方法

- `src/boxes_fmp4.rs` の `MfraBox::encode` を修正した
  - 通常ヘッダーと各 `tfra` を書いた後、`self.mfro_box` ではなく `MfroBox { size: 0 }` を書く
  - `BoxHeader::finalize_box_size` で `mfra` ヘッダーの `size` を確定した後、`mfra` の末尾 4 バイト（`mfro.size` フィールド）だけを `mfra` 全体のバイト数で上書きする
  - `finalize_box_size` が成功した時点で全体のサイズは `u32` に収まるため、`u32::try_from` は不変条件として `expect` で扱う。ヘッダー上限超過時の `InvalidInput` とバッファ不足時の `InsufficientBuffer` は従来どおり
  - `MfraBox::mfro_box` と `MfroBox::size` の doc に、エンコード時は `mfro_box.size` を使わず実サイズを書くこと、decode → encode → decode で値が変わりうることを追記した
- `src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::mfra_bytes` から仮エンコードとサイズの代入を削除し、`MfraBox::encode_to_vec` を 1 回だけ呼ぶようにした。出力のバイト列は変わらない
- `MfraBox::decode` と単体の `MfroBox::encode` は変更していない
- `pbt/tests/prop_fmp4_boxes.rs` に PBT `mfra_box_decode_encode_rewrites_mfro_size` を追加した
  - 通常ヘッダー、0〜2 個の `tfra`、任意の位置の `free`（0 個または 1 個）、`mfro` からなる `mfra` を組み立てて decode → encode する
  - `free` がある場合は入力の `mfro.size` に入力の `mfra` のバイト数を入れ、出力が `free` のバイト数だけ短くなることを確認する
  - `free` がない場合は入力の `mfro.size` に実サイズと必ず異なる値を入れる
  - どちらの場合も、出力が「`free` を除き、ヘッダーの `size` と `mfro.size` を出力のバイト数に直したバイト列」と完全に一致すること、再デコードした `mfro.size` が出力のバイト数と一致することを確認する
  - 出力と同じ長さのバッファへのエンコードが同じバイト列になり、1 バイト短いバッファでは `InsufficientBuffer` になることも確認する
  - 修正前の実装ではこの PBT が失敗することを確認した
- `CHANGES.md` の `## develop` に `[FIX]` を追加した
