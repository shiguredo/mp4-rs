# basic_types.rs の FullBoxFlags::from_flags / is_set で 1 << i が i >= 32 で panic する

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-28
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

`feature/fix-fullboxflags-shift-overflow` ブランチで対応した。

### 実施内容

- `src/basic_types.rs` の `FullBoxFlags` を修正した
  - `is_set` の先頭に `if i >= u32::BITS as usize { return false; }` のガードを追加し、`const fn` を維持した
  - `from_flags` を `filter(x.1 && x.0 < u32::BITS as usize).map(1u32 << x.0).fold(0u32, |acc,b| acc | b)` に組み直した
    - 32 以上のビット位置は `filter` で除外して無視する
    - 内部の畳み込みを `.sum()` から OR (`.fold`) に変更し、同一ビット位置の重複入力による u32 加算オーバーフローも防ぐようにした
  - 32 境界のマジックナンバーは `u32::BITS as usize` に統一し、`1u32 << i` の型を明示して `is_set` / `from_flags` の表記を揃えた
  - doc コメントに「32 以上を無視する」「重複ビット位置は OR で合成される（冪等）」「素朴に書いた場合に起きるパニック / ラップを回避する」旨を明記した
- `pbt/tests/prop_basic_types.rs` に PBT を追加・整理した
  - `arb_bit_position()` Strategy を新設し、`0 / 31 / 32 / 33 / usize::MAX` を `Just` で毎回踏みつつ `any::<usize>()` も混ぜる構成にした
  - `full_box_flags_is_set_any_bit_position`: 任意の `flags` × ビット位置に対する `is_set` の挙動を検証する
  - `full_box_flags_from_flags_any_bit_position`: `from_flags([(i, true)])` の結果と `is_set(i) == (i < 32)` のクロス検証で 32 境界の対称性を確認する
  - `full_box_flags_from_flags_duplicate_positions_or_folded`: `(usize, bool)` 混在の任意リストで、`from_flags` の結果が「有効ビットの OR」と一致することを検証する（重複入力の冪等性と `filter(x.1)` の false 分岐を同時にカバー）
  - 既存の `full_box_flags_bit_operations` は新 PBT に完全包摂されるため削除した
- `fuzz/fuzz_targets/fuzz_basic_types.rs` に `from_flags` / `is_set` を任意入力で叩くパスを追加した
  - 先頭 4 バイトを `u32` の flags 値、続く 8 バイトを `u64` から `usize` に落としたビット位置 `i` として両関数を直接叩く
  - 残りバイトは 4 バイト単位で `(usize, true)` のリストにして `from_flags` に流し、重複を含む任意入力に対するパニック安全性を検証する
- `CHANGES.md` の `## develop` に `[FIX]` エントリを 2 件追記した（32 以上のガードと、重複ビット位置の加算オーバーフロー修正）

### 計画から外れた点

- **単体テストは追加せず PBT の Strategy に境界値を注入する形にした。** 事前計画では `src/basic_types.rs` の `#[cfg(test)]` に境界値の単体テスト（`is_set(31)` / `is_set(32)` / `is_set(usize::MAX)` など）を置く想定だったが、shiguredo-rust 規約「PBT でカバーできるものを単体テストで書かない」「`src/<module>.rs` 内の `#[cfg(test)]` は private 対象専用」に反するため、`arb_bit_position()` Strategy で `Just(境界値)` を確実に踏む構成に切り替えた。issue 起票時の目的（shrink 結果に依存しない具体値の回帰）は Strategy 側で達成できる
- **同一ビット位置の重複入力による u32 加算オーバーフロー修正も併せて実施した。** 事前計画は「シフトオーバーフロー」のみを対象としていたが、レビュー中に `.sum()` による加算オーバーフローで公開 API が debug ビルドでパニックすること（例: `from_flags([(31, true), (31, true)])`）が判明した。本 PR の doc / fuzz が「オーバーフローを防ぐ」「任意入力に対するパニック安全性を検証する」と宣言している以上、同じ関数に別クラスのパニック経路を残すのは自己矛盾するため、修正を追加した

### レビューを受けて追加で対応した内容

- doc コメントの因果表現を「素朴に書いた場合に起きる〜を回避する」に整理し、`is_set` の doc にも release ビルドの wrap（ラップ）による誤判定への言及を追加した
- fuzz のビット位置生成を `u32::from_be_bytes` から `u64::from_be_bytes` に変更し、64bit プラットフォームで `usize::MAX` まで到達できるようにした
- fuzz 追加パスのコメントで「Decode パスは 24 bit マスクを通るため」と書いていた誤った因果関係を、「既存 fuzz 本体は `from_flags` / `is_set` を呼ばないため」に修正した
- CHANGES.md と doc / PBT コメントに残っていた英語の "panic" / "wrap" を「パニック」「ラップ」に統一した
