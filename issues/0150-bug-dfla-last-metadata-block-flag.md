# `DflaBox::encode` で最終ブロックフラグが末尾のブロックだけに立っていないメタデータ列を拒否する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-dfla-last-metadata-block-flag
- Polished: {YYYY-MM-DD}

## 目的

`DflaBox::encode` で、最終ブロックフラグ（`FlacMetadataBlock::last_metadata_block_flag`）が末尾のブロックだけに立っていないメタデータ列を拒否する。

[Xiph.Org の Encapsulation of FLAC in ISO Base Media File Format 0.0.4](https://raw.githubusercontent.com/xiph/flac/1.5.0/doc/isoflac.txt) の 3.3.2 は、`FLACMetadataBlock` の `LastMetadataBlockFlag` について「`FLACSpecificBox` の最後のメタデータブロックなら 1、それ以外は 0」と定めている（"The LastMetadataBlockFlag is set to 1 if this MetadataBlock is the last metadata block in the FLACSpecificBox. It is set to 0 otherwise."）。[RFC 9639](https://www.rfc-editor.org/rfc/rfc9639.html) の 8.1 Metadata Block Header も、このフラグを「後ろに他のメタデータブロックが続くときは 0、そうでなければ 1」と定めている（"It is 0 when other metadata blocks follow; otherwise, it is 1."）。

## 現状

`src/boxes_sample_entry.rs`:

- `FlacMetadataBlock` は最終ブロックフラグを公開フィールド `last_metadata_block_flag` として持つ。`FlacMetadataBlock::encode` はこの値をそのまま先頭バイトに書き、`DflaBox::encode` は各ブロックのフラグを検査しない。encode がフラグを位置に合わせて付け直すこともない
  - `DflaBox::metadata_blocks` の doc と `DflaBox::encode` のコメントには、フラグの位置を検査せず指定された値のまま書き出すことが明記されている（0131 で、対象外として残した）
- `DflaBox::decode` は先頭からブロックを読み、フラグが立ったブロックで読むのをやめる。その後ろにデータが残っていれば `ErrorKind::InvalidData` を返す。どのブロックにもフラグが立っていない場合は、ペイロードの末尾まで読んで受理する

`crates/wasm/src/boxes_flac.rs` の `validate_metadata_blocks` は、JSON の `metadataBlocks` を受け取る時点で、末尾のブロックだけにフラグが立っていることを検査している。コアの `DflaBox::encode` には同じ検査がない。

このため、仕様に合わない列が 2 通り書き出される。

1. 末尾以外のブロックにフラグが立った列（複数のブロックに立った列を含む）: encode は成功し、その出力を自身の `DflaBox::decode` が `InvalidData` で拒否する
2. どのブロックにもフラグが立っていない列: encode は成功し、`DflaBox::decode` も受理するが、3.3.2 の「最後なら 1」に反する

`FlacBox` / `SampleEntry::Flac` の encode も `DflaBox::encode` を経由するため、同じ `dfLa` を書き出す（`SampleEntry::Flac` で 1 の列を encode し、`SampleEntry::decode` が `InvalidData` を返すことを確認した）。

再現手順（develop で確認）:

```rust
let block = |last: u8, block_type: u8, len: usize| FlacMetadataBlock {
    last_metadata_block_flag: Uint::new(last),
    block_type: Uint::new(block_type),
    block_data: vec![0; len],
};

// 1. 先頭の STREAMINFO にフラグを立て、フラグのない PADDING を続ける
let dfla = DflaBox { metadata_blocks: vec![block(1, 0, 34), block(0, 1, 4)] };
let bytes = dfla.encode_to_vec().expect("現行のエンコーダーは書き出す");
let err = DflaBox::decode(&bytes).expect_err("自身の decode は拒否する");
assert_eq!(err.kind, ErrorKind::InvalidData);

// 2. どのブロックにもフラグを立てない
let dfla = DflaBox { metadata_blocks: vec![block(0, 0, 34), block(0, 1, 4)] };
let bytes = dfla.encode_to_vec().expect("現行のエンコーダーは書き出す");
let (decoded, _) = DflaBox::decode(&bytes).expect("decode も受理する");
assert_eq!(decoded, dfla);
```

確認結果:

```text
1: encode ok (58 bytes), decode: InvalidData "Unexpected data after last metadata block (8 bytes remaining)"
1 の両方のブロックにフラグを立てた列: encode ok, decode: InvalidData（同じ理由）
2: encode ok, decode ok（元の値と一致）。STREAMINFO 1 個だけでフラグを立てない列も同じ
```

既存のテストと生成器は、いずれも末尾のブロックだけにフラグを立てている（`pbt/tests/prop_additional_boxes.rs` の `arb_dfla_box`、`tests/test_boxes_sample_entry.rs` の `dfla_error_tests` など）。`tests/testdata/beep-flac-audio.mp4` の `dfLa` も、フラグを立てた STREAMINFO 1 個だけである。

## 設計方針

- `DflaBox::encode` は、既存の 2 つの検査（列が空でないこと、先頭が STREAMINFO であること）の後、ヘッダーを書き込む前に、末尾のブロックの `last_metadata_block_flag` が 1 で、それ以外のブロックが 0 であることを検査する。満たさない場合は `ErrorKind::InvalidInput` を返す。上の 1 と 2 の両方を拒否する
  - 既存の 2 つの検査の後に置き、それらに当たる列の拒否の理由は変えない
  - 既存の検査と同じく、バッファの長さによってエラー種別が変わらないよう、ヘッダーを書き込む前に行う
- フラグを位置に合わせて付け直すことはしない。公開フィールドの値を黙って書き換えず、既存の検査と同じく拒否する
- 2 の列は `DflaBox::decode` が受理するため、そうした `dfLa` を decode した値は、フラグを直さない限り encode し直せなくなる。decode の受理条件を厳しくすることは本 issue の対象外とする
- 仕様の根拠（Encapsulation of FLAC in ISO Base Media File Format 0.0.4 の 3.3.2）と、将来の改訂で変わる可能性があることをコードコメントに書く。フラグの位置を検査しないと書いている `DflaBox::metadata_blocks` の doc と `DflaBox::encode` のコメントは、新しい検査に合わせて書き直す
- 公開フィールドと API、`DflaBox::decode` の受理条件、`FlacMetadataBlock::encode` / `FlacMetadataBlock::decode` は変えない
- wasm は JSON を受け取る時点で同じ条件を検査し、`metadataBlocks` がない場合はフラグを立てた STREAMINFO 1 個の列を組み立てるため、挙動は変わらない

## 完了条件

- `tests/test_boxes_sample_entry.rs` の `dfla_error_tests` で、次の列について、`encode_to_vec` と空のバッファへの `encode` の両方が `ErrorKind::InvalidInput` を返す
  - 末尾以外のブロックにフラグが立った列
  - 複数のブロックにフラグが立った列
  - どのブロックにもフラグが立っていない列（STREAMINFO 1 個だけの場合と、後続のブロックがある場合）
- 既存の `dfla_box_encode_rejects_empty_metadata_blocks` / `dfla_box_encode_rejects_non_streaminfo_first_block` が引き続き成功する
- 正常系は既存の `dfla_box_roundtrip` / `flac_box_roundtrip` の PBT と、`tests/test_decode_encode.rs` の `decode_encode_beep_flac_audio_mp4` が引き続き成功する
- `cargo test --workspace` が成功する
