# Uint::new に宣言ビット幅の検証を追加する

- Created: 2026-08-20
- Completed: 2026-08-20
- Branch: feature/refactor-uint-new-bit-width-validation
- Polished: {YYYY-MM-DD}

## 目的

`src/basic_types.rs` の `Uint::<T, BITS, OFFSET>::new(v)` は現状 `Self(v)` を返すだけで、`v` が宣言ビット幅 `BITS` に収まっているかを検証していない。結果として、呼び出し側が `BITS` を超える値を渡すと、encode 経路で silent なビットオーバーフローを引き起こしうる。

現時点で crate 内の既存呼び出しはすべて範囲内の定数しか渡していないので実害は発生していないが、config 由来の可変値を将来 `Uint::new` に流し込むよう拡張したときに、この silent 挙動が仕様外バイト列を生む経路になりうる。

## 現状

`src/basic_types.rs` の `impl<T, const BITS: u32, const OFFSET: u32> Uint<T, BITS, OFFSET>` にある `pub const fn new(v: T) -> Self { Self(v) }` は `v` の範囲検証をしない。encode 側 (`src/boxes_sample_entry.rs` 内 `VpccBox` などの `to_bits()`) は `Uint` の中身をそのままシフト・OR して u8/u16/u32 に詰めるため、`BITS` を超える値が入っていた場合はビット位置がずれて隣接フィールドを侵食する形になる。

具体例:

- `VpccBox::bit_depth` は `Uint<u8, 4, 4>` (bit 7..4)。ここに `Uint::new(16u8)` を渡すと `to_bits()` が `16 << 4 = 256` を計算し、u8 では 0 にラップする。encode 結果の bit_depth に 0 が書かれるサイレント破壊になる
- `Vp8SampleEntryConfig::level` を将来 `Uint<u8, 8, 0>` 化するなど、config 経由で任意値を受けるルートを開いた瞬間に上記が現実の問題になる

## 背景

VP8 ビットストリーム処理ユーティリティ (`feature/add-vp8-bitstream-utilities`) のコードレビューでこの潜在リスクを検出した。当該コードは範囲内定数のみを渡す構造なので実害はなく、crate 全体の防衛策として本 issue で別途扱う。

## 設計方針

`Uint` は内部利用が主であり、`Uint::new` は const 文脈（`const BLOCK_TYPE_* = Uint::new(0)` 等）でも使われる型である。現在は可変値経路が存在しないため、新規 API 追加（`new_checked`）や挙動変更による防衛はコスト超過と判断し、**doc で前提条件を明示する** 方針を採る。

比較検討した案:

- 案 A: `Uint::new` に `debug_assert!(v <= max_for_bits(BITS))` を入れる
  - pros: 既存 API を保ちつつ debug で早期検出
  - cons: release では引き続き silent。`const fn` から外れる (`debug_assert!` は現状 const 不可)
- 案 B: `Uint::new_checked(v) -> Option<Self>` を追加し、既存 `Uint::new` は現状維持
  - pros: 既存呼び出しへの影響なし。呼び出し側が「不変条件を型で保証したい箇所」で選択できる
  - cons: 現在はこの API を使うユースケースが存在せず、未使用の公開 API が増える（YAGNI に反する）
- 案 C: `Uint::new` を `Option<Self>` を返すよう破壊的変更
  - pros: silent 経路を完全に塞ぐ
  - cons: 全既存呼び出しの書き換えが必要 (crate 内多数)。`const fn` 化ができない
- 案 D: `Uint::new` / `Uint::to_bits` / `Uint` 構造体の doc に「値が `BITS` ビットに収まる必要がある」前提と、範囲外の値の危険性を明記する
  - pros: API・挙動を変えず、const fn も維持。未使用 API を増やさない
  - cons: doc は強制力を持たない。ただし現時点で可変値経路が存在しないため実質的なリスク増はない

推奨: 案 D。将来 config 経由で可変値を受けるルートを開く際に、あらためて案 B（`new_checked` 等）の追加を検討する。

## 完了条件

- `Uint::new` / `Uint::to_bits` / `Uint` 構造体の doc に、保持値が `BITS` ビットに収まる必要がある旨と、範囲外の値の危険性を明記する
- 既存の挙動・API は変更しない（テスト変更なし）

## スコープ外

- `Uint::new_checked` などの検証付きコンストラクタの追加（将来可変値経路を開く際に別途検討）
- `Uint::to_bits` / `Uint::from_bits` の挙動変更
- crate 内 `Uint::new` 呼び出し全箇所の網羅的な監査。可変値経路で使う予定のない箇所まで一律置換する必要はない

## 補足

- 本 issue は feature/add-vp8-bitstream-utilities のコードレビューで検出したものであり、VP8 実装自体は範囲内定数のみを渡すので実害なし
- 参照: `src/basic_types.rs` の `Uint` 定義と `impl Uint`、および `src/boxes_sample_entry.rs::VpccBox` の `to_bits()` / `from_bits()`

## 解決方法

`src/basic_types.rs` の doc のみを更新して対応した。挙動・API は一切変更していない。

- `Uint` 構造体の doc に「保持値は常に `BITS` ビットで表現できる範囲に収まる」という不変条件と、`Uint::from_bits()` 経由では常に満たされるが `Uint::new()` は検証しない旨を追記
- `Uint::new` の doc に、`v` が `BITS` ビットに収まっている必要があること、範囲外の値は `Uint::to_bits()` が隣接フィールドのビットを侵食するなど不正なエンコード結果になるため呼び出し側が範囲を保証すること、を追記
- `Uint::to_bits` の doc に、不変条件が満たされていない場合の危険性を注意書きとして追記

既存の `Uint::new` 呼び出しはすべて定数か `is_some() as u8`（0/1、BITS=1 に構造上収まる）であり、現状はすべて安全であることを確認済み。将来 config 由来の可変値を `Uint::new` に流し込む拡張を検討する際は、本 issue の設計方針にある案 B（`Uint::new_checked` 等の検証付きコンストラクタ）をあらためて検討する。
