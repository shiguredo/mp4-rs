# `DflaBox` が空のメタデータブロック列を encode でき、自身の decode で拒否される

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-dfla-empty-metadata-blocks-encode
- Polished: {YYYY-MM-DD}

## 目的

`DflaBox` の encode が、自身の decode で受理されるボックスだけを生成するようにする。

## 現状

`src/boxes_sample_entry.rs` の `DflaBox::decode` は、メタデータブロックが 1 個以上存在し、先頭のブロック種別が STREAMINFO であることを要求する。満たさない場合は `InvalidData` を返す。

一方 `DflaBox::encode` はブロック列の長さや先頭種別を検査せず、空の列でもボックスを書き出す。そのため `DflaBox { metadata_blocks: vec![] }` は 12 バイトの `dfLa` を生成し、その出力は自身の decode で拒否される。

再現:

```rust
let b = DflaBox { metadata_blocks: vec![] };
let bytes = b.encode_to_vec().unwrap();
assert!(DflaBox::decode(&bytes).is_err());
```

確認結果:

```text
encoded len=12, head=[00, 00, 00, 0c, 64, 66, 4c, 61, 00, 00, 00, 00]
decode result is_ok=false
```

## 設計方針

- `DflaBox::encode` の先頭で、decode と同じ不変条件を検査する。すなわち、ブロック列が空でないこと、先頭の `block_type` が `FlacMetadataBlock::BLOCK_TYPE_STREAMINFO` であることを確認し、違反なら `InvalidInput` を返す
- 構築時に不変条件を保証する `DflaBox::new` のような API を追加する場合は、この issue の範囲に含めてよい
- decode の受理条件は変えない
- 正常な `DflaBox` の roundtrip テストは維持する

## 完了条件

- `DflaBox { metadata_blocks: vec![] }.encode_to_vec()` が `InvalidInput` を返す
- 先頭が STREAMINFO でない `DflaBox` の encode が `InvalidInput` を返す
- STREAMINFO を先頭に持つ正常な `DflaBox` は従来どおり roundtrip する
- `tests/test_boxes_sample_entry.rs` に空の列と先頭種別のテストが追加されている
- `cargo test --workspace` が成功する
