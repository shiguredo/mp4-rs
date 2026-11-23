# `DflaBox::encode` で先頭に STREAMINFO がないメタデータ列を拒否する

- Created: 2026-11-22
- Completed: 2026-11-23
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

## 解決方法

- `src/boxes_sample_entry.rs` の `DflaBox::encode` の冒頭で、ヘッダーを書き込む前に `metadata_blocks` を検査するようにした
  - 列が空なら `ErrorKind::InvalidInput`（`"dfLa box must contain at least one metadata block (STREAMINFO)"`）を返す。理由の文言は `DflaBox::decode` と同じにした
  - 先頭の `block_type` が `FlacMetadataBlock::BLOCK_TYPE_STREAMINFO` でなければ `ErrorKind::InvalidInput`（`"First metadata block in dfLa must be STREAMINFO (block_type=0), got <値>"`）を返す
  - ヘッダーを書き込む前に検査するため、バッファの長さにかかわらず `InvalidInput` になる
  - 仕様の根拠（Encapsulation of FLAC in ISO Base Media File Format バージョン 0.0.4 の 3.3.2）と、将来の改訂で変わる可能性があることはコードコメントに残した。同節が定める最終ブロックフラグの位置は検査しないことも、コメントと `metadata_blocks` の doc に明記した
- `DflaBox::decode` の受理条件、各ブロックの内部データ、最終ブロックフラグの扱い、公開フィールドと API は変えていない
- テスト
  - `tests/test_boxes_sample_entry.rs` の `dfla_error_tests` に 2 件を追加した。空の列と、先頭の `block_type` が 1〜127 の列（単独の場合と、2 個目に STREAMINFO が続く場合）について、`encode_to_vec` と空のバッファへの `encode` の両方が `InvalidInput` を返すことを確認する
  - `pbt/tests/prop_additional_boxes.rs` の `arb_dfla_box` を、先頭の STREAMINFO の後に STREAMINFO 以外の種別のブロックを 0-3 個続け、最終ブロックフラグを末尾だけに立てる生成器にした。`dfla_box_roundtrip` と `flac_box_roundtrip` はデコード結果全体を比較する
  - FLAC を含む MP4 ファイルのラウンドトリップ（`tests/test_decode_encode.rs` の `decode_encode_beep_flac_audio_mp4`）が引き続き成功することを確認した
- `Mp4FileMuxer::finalize` / `Fmp4SegmentMuxer::init_segment_bytes` は、そうした `dfLa` を持つサンプルエントリーを渡されていた場合に `MuxError::EncodeError` を返すようになる。wasm は JSON の段階で同じ条件を検査しているため変わらない
- `CHANGES.md` の `## develop` に [FIX] を追記した
