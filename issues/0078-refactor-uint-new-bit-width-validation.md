# Uint::new に宣言ビット幅の検証を追加する

- Created: 2026-08-20
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-uint-new-bit-width-validation
- Polished: {YYYY-MM-DD}

## 目的

`src/basic_types.rs` の `Uint::<T, BITS, OFFSET>::new(v)` は現状 `Self(v)` を返すだけで、`v` が宣言ビット幅 `BITS` に収まっているかを検証していない。結果として、呼び出し側が `BITS` を超える値を渡すと、encode 経路で silent なビットオーバーフローを引き起こしうる。

現時点で crate 内の既存呼び出しはすべて範囲内の定数しか渡していないので実害は発生していないが、config 由来の可変値を将来 `Uint::new` に流し込むよう拡張したときに、この silent 挙動が仕様外バイト列を生む経路になりうる。型側で防衛する。

## 現状

`src/basic_types.rs` の `impl<T, const BITS: u32, const OFFSET: u32> Uint<T, BITS, OFFSET>` にある `pub const fn new(v: T) -> Self { Self(v) }` は `v` の範囲検証をしない。encode 側 (`src/boxes_sample_entry.rs` 内 `VpccBox` などの `to_bits()`) は `Uint` の中身をそのままシフト・OR して u8/u16/u32 に詰めるため、`BITS` を超える値が入っていた場合はビット位置がずれて隣接フィールドを侵食する形になる。

具体例:

- `VpccBox::bit_depth` は `Uint<u8, 4, 4>` (bit 7..4)。ここに `Uint::new(16u8)` を渡すと `to_bits()` が `16 << 4 = 256` を計算し、u8 では 0 にラップする。encode 結果の bit_depth に 0 が書かれるサイレント破壊になる
- `Vp8SampleEntryConfig::level` を将来 `Uint<u8, 8, 0>` 化するなど、config 経由で任意値を受けるルートを開いた瞬間に上記が現実の問題になる

## 背景

VP8 ビットストリーム処理ユーティリティ (`feature/add-vp8-bitstream-utilities`) のコードレビューでこの潜在リスクを検出した。当該コードは範囲内定数のみを渡す構造なので実害はなく、crate 全体の防衛策として本 issue で別途扱う。

## 設計方針

以下の案を比較して選ぶ。

- 案 A: `Uint::new` に `debug_assert!(v <= max_for_bits(BITS))` を入れる
  - pros: 既存 API を保ちつつ debug で早期検出
  - cons: release では引き続き silent。`const fn` から外れる (`debug_assert!` は現状 const 不可)
- 案 B: `Uint::new_checked(v) -> Option<Self>` を追加し、既存 `Uint::new` は現状維持
  - pros: 既存呼び出しへの影響なし。呼び出し側が「不変条件を型で保証したい箇所」で選択できる
  - cons: `Uint::new` の silent 挙動は残る
- 案 C: `Uint::new` を `Option<Self>` を返すよう破壊的変更
  - pros: silent 経路を完全に塞ぐ
  - cons: 全既存呼び出しの書き換えが必要 (crate 内多数)。`const fn` 化ができない

推奨: 案 B。追加のみで安全、呼び出し側が判断できる。案 A は release で silent なので保護として不十分。案 C は crate 全体への波及が大きく、実害が出ていない現状ではコスト超過。

## 完了条件

- `Uint::new_checked` (または合意した別案) を `src/basic_types.rs` に追加する
- 新規追加 API に対するテスト (境界値 / 境界超過 / 境界内正常) を追加する
- crate 内の既存 `Uint::new` 呼び出しのうち、`BITS` の範囲を明示的に強制すべき箇所を洗い出し、必要に応じて `Uint::new_checked` へ切り替える (実データ経由で `Uint::new` に渡す可変値がある箇所が主対象)
- `CHANGES.md` の `## develop` に該当種別のエントリを追加する

## スコープ外

- `Uint::to_bits` / `Uint::from_bits` の挙動変更
- `Uint` 型全般の doc 整備 (必要なら別 issue)
- crate 内 `Uint::new` 呼び出し全箇所の網羅的な監査。可変値経路で使う予定のない箇所まで一律置換する必要はない

## 補足

- 本 issue は feature/add-vp8-bitstream-utilities のコードレビューで検出したものであり、VP8 実装自体は範囲内定数のみを渡すので実害なし
- 参照: `src/basic_types.rs` の `Uint` 定義と `impl Uint`、および `src/boxes_sample_entry.rs::VpccBox` の `to_bits()` / `from_bits()`
