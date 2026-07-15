# basic_types.rs の FullBoxFlags::from_flags / is_set で 1 << i が i >= 32 で panic する

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-fullboxflags-shift-overflow
- Polished: YYYY-MM-DD

## 目的

公開 API の `FullBoxFlags::from_flags` / `is_set` がビット位置 `i` の範囲を検査せず、`i >= 32` で `1 << i` がオーバーフローし debug ビルドで panic する問題を修正する。

## 優先度根拠

公開 API が `usize` を受け取るため、呼び出し側の誤入力でプロセスを落とせる。ISO BMFF の FullBox flags は 24 bit であり通常の呼び出しは 0..24 だが、API 境界としては未防御。release ビルドでは wrap して不正なフラグ値になる。panic は実装バグの表明であり、入力値の範囲エラーは `Result` で返すべき。

## 現状

```rust
// src/basic_types.rs:315
let flags = iter.into_iter().filter(|x| x.1).map(|x| 1 << x.0).sum();
```

```rust
// src/basic_types.rs:325-326
pub const fn is_set(self, i: usize) -> bool {
    (self.0 & (1 << i)) != 0
}
```

`1 << i` は `u32` に推論され、`i >= 32` で debug ビルドで `attempt to shift left with overflow` の panic、release で wrap する。`from_flags` は `pub fn`、`is_set` は `pub const fn` であり公開 API。

## 設計方針

- `is_set`: `i >= 24`（または `i >= 32`）のとき `false` を返すよう範囲チェックを追加する。`const fn` であることを維持する
- `from_flags`: `i >= 24` のビットを無視するか、`i >= 32` を saturating に扱う。flags は 24 bit が仕様上の有効範囲

## 完了条件

- `i >= 32` で panic せず安全な値を返すこと
- 既存の 0..24 の呼び出しで従来どおりの挙動を維持すること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `is_set` に `if i >= 24 { return false; }` のガードを追加する（`const fn` 互換）
2. `from_flags` の `1 << x.0` を `if x.0 < 24 { 1 << x.0 } else { 0 }` でガードする
3. 境界値テストを追加する
