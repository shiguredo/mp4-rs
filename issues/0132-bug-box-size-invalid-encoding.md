# `BoxSize::U32(1..=7)` と `with_payload_size(u64::MAX)` が不正なヘッダーを生成する

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-box-size-invalid-encoding
- Polished: {YYYY-MM-DD}

## 目的

`BoxSize` と `BoxHeader` の encode が、自身で decode できないヘッダーや panic を発生させないようにする。

## 現状

`BoxSize` は公開 enum で任意の値を作れる。`BoxHeader::encode` は `BoxSize::U32(v)` をそのまま 32 ビットの size フィールドに書く。

`BoxSize::U32(1)` は「size=1 で largesize が続く」形式を意味するが、`encode` は largesize を書かない。そのため出力は自身の `BoxHeader` でも解釈できない。`U32(2..=7)` もヘッダー幅未満のサイズとなり、decode の最小サイズ検査で拒否される。

また `BoxSize::with_payload_size(BoxType::Normal(*b"test"), u64::MAX)` は、`4 + external_size + payload` の加算が debug ビルドで panic する。

再現:

```rust
let b = UnknownBox {
    box_type: BoxType::Normal(*b"test"),
    box_size: BoxSize::U32(1),
    payload: vec![0; 8],
};
let bytes = b.encode_to_vec().unwrap();
assert!(UnknownBox::decode(&bytes).is_err());

let _ = BoxSize::with_payload_size(BoxType::Normal(*b"test"), u64::MAX);
```

確認結果:

```text
encoded len=16, head=[00, 00, 00, 01, 74, 65, 73, 74, 00, 00, 00, 00]
decode result is_ok=false

box size payload overflow:
      panicked: attempt to add with overflow
```

## 設計方針

- `BoxHeader::encode` で `BoxSize::U32(1..=7)` を拒否する。`U32(0)` は可変長として許容し、`U32(8..)` と `U64` は現状どおりとする
- `BoxSize::with_payload_size` の加算を `checked_add` にし、オーバーフロー時は `Result` を返すか、呼び出し側が検出できる形にする。関数のシグネチャ変更が大きい場合は、飽和させたうえで `U64` を返すなどの明示的な方針を決める
- `UnknownBox::encode` は保持している `box_size` をそのまま使わず、`finalize_box_size` などで実際のペイロードからサイズを再計算する
- 既存の正常な `BoxSize` の roundtrip は変えない

## 完了条件

- `BoxSize::U32(1..=7)` の `BoxHeader::encode` が `InvalidInput` を返す
- `BoxSize::with_payload_size` に `u64::MAX` を渡しても panic せず、エラーまたは明示的に定義した飽和値になる
- `UnknownBox` の `box_size` と `payload` が食い違う入力でも、encode が整合したヘッダーを書くかエラーを返す
- 正常な `U32(8..)` / `U32(0)` / `U64` の roundtrip テストが引き続き成功する
- `cargo test --workspace` が成功する
