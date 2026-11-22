# `DflaBox::encode` で先頭に STREAMINFO がないメタデータ列を拒否する

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-dfla-empty-metadata-blocks-encode
- Polished: 2026-11-22

## 目的

`DflaBox::encode` で、メタデータ列が空の場合と先頭が STREAMINFO でない場合を拒否する。

[Xiph.Org の Encapsulation of FLAC in ISO Base Media File Format 0.0.4](https://raw.githubusercontent.com/xiph/flac/1.5.0/doc/isoflac.txt) の 3.3.2 は、`dfLa` に最低 1 個のメタデータブロックが必要で、先頭は STREAMINFO であることを定めている。

## 現状

`src/boxes_sample_entry.rs` の `DflaBox::decode` は、メタデータブロックが 1 個以上存在し、先頭のブロック種別が STREAMINFO であることを要求する。満たさない場合は `InvalidData` を返す。

一方 `DflaBox::encode` はブロック列の長さや先頭種別を検査せず、空の列でもボックスを書き出す。そのため `DflaBox { metadata_blocks: vec![] }` は 12 バイトの `dfLa` を生成し、その出力は自身の decode で拒否される。

再現:

```rust
let b = DflaBox { metadata_blocks: vec![] };
let bytes = b.encode_to_vec().expect("現行のエンコーダーは空の列を書き出す");
assert!(DflaBox::decode(&bytes).is_err());
```

確認結果:

```text
encoded len=12, head=[00, 00, 00, 0c, 64, 66, 4c, 61, 00, 00, 00, 00]
decode result is_ok=false
```

## 設計方針

- `DflaBox::encode` は、ヘッダーを書き込む前に先頭ブロックを確認する。先頭が存在しない場合と、その `block_type` が `FlacMetadataBlock::BLOCK_TYPE_STREAMINFO` でない場合は `ErrorKind::InvalidInput` を返す
- 対象はこの 2 条件のエンコード時の検査に限る。公開フィールドと API は変更せず、`DflaBox::decode` の受理条件、各ブロックの内部データ、最終ブロックフラグの扱いは変えない

## 完了条件

- `DflaBox { metadata_blocks: vec![] }.encode_to_vec()` が `InvalidInput` を返す
- 先頭が STREAMINFO でない `DflaBox` の encode が `InvalidInput` を返す
- 正常系は既存の `dfla_box_roundtrip` PBT と、FLAC を含む MP4 ファイルのラウンドトリップが引き続き成功する
- `tests/test_boxes_sample_entry.rs` に空の列と先頭種別のテストが追加されている
- `cargo test --workspace` が成功する
