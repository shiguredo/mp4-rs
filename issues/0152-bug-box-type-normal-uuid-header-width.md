# `BoxHeader::encode` で `BoxType::Normal(*b"uuid")` を拒否する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-box-type-normal-uuid
- Polished: {YYYY-MM-DD}

## 目的

`BoxType::Normal(*b"uuid")` を種別に持つ `BoxHeader` / `UnknownBox` を encode すると、usertype のない `uuid` ヘッダーが書き出される。読み直すと、続く 16 バイトを usertype とする `BoxType::Uuid` の別のボックスとして解釈されるか、エラーになる。encode でこの種別を拒否し、自身の decode で元の値に戻らないバイト列を書き出さないようにする。

ISO/IEC 14496-12 のボックスヘッダーの構文（0132 が引く 2015 年版の 4.2、0095 が引く 2022 年版の 4.2.2）では、type が `uuid` のときは必ず 16 バイトの usertype が続く。

## 現状

`src/basic_types.rs`:

- `BoxType` は公開 enum で、`Normal([u8; 4])` と `Uuid([u8; 16])` を持つ。`Normal` の 4 バイトの値は制限されておらず、`BoxType::Normal(*b"uuid")` も構築できる
- `BoxType::external_size` は `Normal` に 4、`Uuid` に 20 を返す。`BoxHeader::external_size` と `BoxSize::with_payload_size` もこれに従い、`Normal(*b"uuid")` のヘッダーの幅を、usertype を含まない 8 バイト（32-bit の size）/ 16 バイト（largesize）として扱う
- `impl Encode for BoxHeader` は、`Normal` なら 4 バイトの種別だけを書き、`Uuid` なら `uuid` と 16 バイトの usertype を書く。0132 で追加したサイズの下限の検査は `BoxHeader::external_size` を使うため、`Normal(*b"uuid")` は 8 バイト（largesize なら 16 バイト）以上のサイズで通る
- `impl Decode for BoxHeader` は、種別が `uuid` なら続く 16 バイトを usertype として読み、`BoxType::Uuid` を返す。decode が `BoxType::Normal(*b"uuid")` を返すことはない

`src/boxes.rs` の `UnknownBox::encode` は、`BoxHeader::external_size` と `payload.len()` の合計が宣言サイズと一致すれば、`BoxHeader::encode` でヘッダーを書いてからペイロードを書く。このため、`Normal(*b"uuid")` の `UnknownBox` も、`BoxSize::with_payload_size` で求めたサイズなら書き出される。

再現手順（develop で確認。32-bit の size の場合）:

```rust
// 1. ヘッダー単体
let header = BoxHeader { box_type: BoxType::Normal(*b"uuid"), box_size: BoxSize::U32(8) };
let bytes = header.encode_to_vec().expect("現行のエンコーダーは書き出す");
let err = BoxHeader::decode(&bytes).expect_err("usertype の 16 バイトが足りない");
assert_eq!(err.kind, ErrorKind::InsufficientBuffer);

// 2. 未知ボックス
let ty = BoxType::Normal(*b"uuid");
let payload: Vec<u8> = (0..20).collect();
let b = UnknownBox {
    box_type: ty,
    box_size: BoxSize::with_payload_size(ty, payload.len() as u64).expect("U32(28) になる"),
    payload,
};
let bytes = b.encode_to_vec().expect("現行のエンコーダーは書き出す");
let (decoded, _) = UnknownBox::decode(&bytes).expect("別のボックスとして読める");
assert_ne!(decoded, b);
```

確認結果:

```text
1: encode -> [0, 0, 0, 8, 117, 117, 105, 100] (8 bytes), decode -> InsufficientBuffer
2: encode -> 28 bytes, decode -> UnknownBox { box_type: Uuid([0, 1, ..., 15]), box_size: U32(28), payload: [16, 17, 18, 19] }
```

- 2 では、ペイロードの先頭 16 バイトが usertype として読まれ、種別が `BoxType::Uuid` に、ヘッダーの幅が 8 バイトから 24 バイトに、ペイロードが 20 バイトから 4 バイトに変わる
- ペイロードが 16 バイト未満なら、2 の decode は `InsufficientBuffer` になる（ペイロード 4 バイトで確認）
- `BoxSize::U64` でも usertype のない `uuid` ヘッダーが書き出される。読み直した結果は usertype と largesize を読む順序に依存し、その順序は 0095 で変わる（今の順序では、ペイロード 20 バイトの未知ボックスの decode が `InsufficientBuffer` になる）
- `BoxSize::VARIABLE_SIZE` でも、`[0, 0, 0, 0, 117, 117, 105, 100]` が書き出される

`Normal(*b"uuid")` は利用者が値を組み立てたときにだけ現れる。wasm では、`crates/wasm/src/boxes.rs` の `parse_unknown_boxes` が JSON の 4 バイトの種別 `uuid` を拒否するため現れない。

PBT の生成器のうち、`pbt/tests/prop_basic_types.rs` の `arb_box_type_normal` と、`pbt/tests/prop_additional_boxes.rs` の `arb_unknown_box` / `unknown_box_roundtrip` の種別の生成は、任意の 4 バイトを `BoxType::Normal` にしており、`uuid` を除外していない（引く確率は 2^-32）。`pbt/tests/prop_fmp4_segment_mux_demux.rs` の `arb_skippable_box` は `uuid` を棄却している。

## 設計方針

- `impl Encode for BoxHeader` で、書き込む前に `BoxType::Normal(*b"uuid")` を `ErrorKind::InvalidInput` として拒否する。サイズの下限の検査と同じく、バッファの長さによってエラー種別が変わらないよう、書き込む前に検査する。`BoxSize::VARIABLE_SIZE` / `BoxSize::LARGE_VARIABLE_SIZE` を含め、サイズによらず拒否する
- `UnknownBox::encode` などヘッダーを書くボックスの encode は、ペイロードより先に `BoxHeader::encode` を呼ぶため、同じ検査で拒否される
- エラーの理由と `BoxType::Normal` の doc に、`uuid` の種別は `BoxType::Uuid` で表すことを書く。仕様の根拠（ISO/IEC 14496-12 の 4.2）と、将来の改訂で変わる可能性があることをコードコメントに書く
- decode は `Normal(*b"uuid")` を返さないため、decode した値を encode し直す経路には影響しない。wasm の挙動も変わらない
- `BoxType::external_size` / `BoxHeader::external_size` / `BoxSize::with_payload_size` は変えない
- 上記の PBT の生成器 3 箇所で、`uuid` を生成しないようにする（`arb_skippable_box` と同じく棄却する）
- usertype と largesize を読み書きする順序は 0095 の対象であり、本 issue では変えない。本 issue の変更は `BoxType::Uuid` のヘッダーのバイト列を変えないため、0095 とどちらを先に直しても他方の内容は変わらない

## 完了条件

- `tests/test_basic_types.rs` で、`BoxType::Normal(*b"uuid")` の `BoxHeader` が、`BoxSize::U32` / `BoxSize::U64` / `BoxSize::VARIABLE_SIZE` / `BoxSize::LARGE_VARIABLE_SIZE` のいずれでも、`encode_to_vec` と空のバッファへの `encode` の両方で `ErrorKind::InvalidInput` を返す
- `tests/test_boxes.rs` で、`BoxSize::with_payload_size` で求めたサイズを持つ `BoxType::Normal(*b"uuid")` の `UnknownBox` の encode が `ErrorKind::InvalidInput` を返す
- `arb_box_type_normal` / `arb_unknown_box` / `unknown_box_roundtrip` の生成器が `uuid` を生成しない
- 既存の `BoxHeader` / `UnknownBox` のラウンドトリップの PBT が引き続き成功する
- `cargo test --workspace` が成功する
