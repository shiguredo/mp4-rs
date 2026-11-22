# `MfraBox` の decode → encode で `mfro.size` が再エンコード後の実サイズと一致しない

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
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
