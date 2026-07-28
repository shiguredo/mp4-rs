# basic_types.rs の FullBoxFlags::from_flags / is_set で 1 << i が i >= 32 で panic する

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-fullboxflags-shift-overflow
- Polished: 2026-07-28

## 目的

公開 API の `FullBoxFlags::from_flags` / `is_set` がビット位置 `i` の範囲を検査せず、`i >= 32` で `1 << i` がオーバーフローし debug ビルドで panic する問題を修正する。

## 優先度根拠

公開 API が `usize` を受け取るため、呼び出し側の誤入力でプロセスを落とせる。ISO BMFF の FullBox flags は 24 bit であり通常の呼び出しは 0..24 だが、API 境界としては未防御。release ビルドでは wrap して不正なフラグ値になる。パニックは実装バグの表明であり、公開 API の型 (`usize`) で受け付け得る入力に対してはパニックさせずに安全な値を返すべき。

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

- 範囲チェックの境界は `i >= 32`（`u32` の型幅）で統一する。以下の理由:
  - `FullBoxFlags::new(flags: u32)` (`src/basic_types.rs:306-308`) が 32 bit 全域を受け入れる既存 API のため、`new(0xFF00_0000).is_set(24) == true` のような 24..32 のビットに対する既存挙動を silently 破壊しない
  - ISO BMFF 仕様上の有効範囲 0..24 は Encode 実装 `self.0.to_be_bytes()[1..]` (`src/basic_types.rs:332`) で既に 24 bit に丸められており、`is_set` / `from_flags` 側で再度 24 bit にマスクする必要はない
  - `Result` 化は既存呼び出し側 (`src/boxes_moov_tree.rs:433-436` の `is_set(0)`〜`is_set(3)` など) の API を破壊するため採らない。本件はあくまで型幅を超えるビット位置へのシフトオーバーフロー回避に絞る
- `is_set`: `i >= 32` のとき `false` を返す。`const fn` を維持する
- `from_flags`: `x.0 >= 32` のビットを 0 として無視する（encode 段階でどのみち 24 bit に丸められるため、`x.0 in 24..32` を保持しても意味は無いが、上記の一貫性のため 32 で統一する）

## 完了条件

- `i >= 32` で panic せず、`is_set` は `false` を返し、`from_flags` はそのビット位置を 0 として無視すること（release ビルドでの wrap も発生しないこと）
- 既存の `i in 0..32` の範囲では従来どおりの挙動を維持すること（特に `is_set(24..32)` は `self.0` の該当ビットに応じた bool を返し続ける）
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `src/basic_types.rs` の `is_set` に `if i >= 32 { return false; }` のガードを追加する（`const fn` 互換）
2. `src/basic_types.rs` の `from_flags` の `1 << x.0` を `if x.0 < 32 { 1u32 << x.0 } else { 0 }` でガードする（型を明示して `sum` の推論を安定させる）
3. `pbt/tests/prop_basic_types.rs` に PBT を追加し、ビット位置 `i` の Strategy を型情報通り `any::<usize>()` として次のプロパティを検証する（shiguredo-rust の「PBT に『任意入力でパニックしないことだけを検証するテスト』を書かないこと（fuzzing の役割）」に従い、本 PBT の目的はプロパティ検証。パニック安全性は 4 の fuzz で担保する）:
   - 任意の `flags: u32` と `i: usize` について、`FullBoxFlags::new(flags).is_set(i)` の結果は「`i < 32` なら `(flags >> i) & 1 == 1` と等価、`i >= 32` なら `false`」
   - 任意の `i: usize` について、`FullBoxFlags::from_flags([(i, true)]).get()` の結果は「`i < 32` なら `1u32 << i`、`i >= 32` なら `0`」
4. `fuzz/fuzz_targets/fuzz_basic_types.rs` に、`FullBoxFlags::from_flags` と `FullBoxFlags::is_set` を任意入力で呼び出す fuzz ターゲットを追加する（任意入力に対するパニック安全性は shiguredo-rust の PBT / Fuzzing の役割分担に従い fuzz 側で担保する。既存の `Decode` パスの fuzz は `from_flags` / `is_set` を通らないため、本ターゲットの追加が必要）
5. `src/basic_types.rs` の `#[cfg(test)]` に、`is_set(31)` / `is_set(32)` / `is_set(usize::MAX)` の返り値と `from_flags([(31, true), (32, true), (usize::MAX, true)])` が panic せず 32 未満のビットのみ立つことを確認する境界値の単体テストを追加する（PBT の Strategy shrink 結果に依存しない具体値の回帰確認として置く）
