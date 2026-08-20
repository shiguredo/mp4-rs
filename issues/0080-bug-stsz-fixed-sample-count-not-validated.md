# StszBox::Fixed の sample_count が stts の合計と突き合わされていない

- Created: 2026-08-21
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-stsz-fixed-sample-count-validation
- Polished: {YYYY-MM-DD}

## 目的

`SampleTableAccessor::new`（`src/auxiliary.rs`）の `stsz` 整合性チェックは `StszBox::Variable` のときだけ `entry_sizes.len()` と `stts` 由来の `sample_count` を突き合わせる。`StszBox::Fixed { sample_size, sample_count }`（ワイヤ上の `sample_size` が非ゼロ）では `sample_count` が `stts` の合計と一致するか検証されない。破損入力に対する整合性検証の網を揃えるため、`Fixed` でも突き合わせるのが本 issue の目的である。

## 現状

`SampleTableAccessor::new` の `stsz` 検証は `if let StszBox::Variable { entry_sizes } = ...` の分岐でしか行われない。`Fixed` の `sample_count` は `SttsBox::decode` / `StszBox::decode`（`src/boxes_moov_tree.rs`）が値を検証せずに読むため、`stts` の合計と食い違う `Fixed.sample_count` を持つ `stbl` がそのまま `Ok` になる。

`Fixed.sample_count` は encode 以外の production コードから参照されない（`src/auxiliary.rs` の `data_size()` は `StszBox::Fixed { sample_size, .. }` として `sample_count` を破棄し、`SampleTableAccessor::new` は `stts` 由来の `sample_count` を使う）。そのため誤った `Fixed.sample_count` 自体による実害は小さいが、ファイルが破損している事実を検出できず素通りさせる点が問題である。

`StblBox` のフィールドはすべて `pub` なので、以下を組み立てて `SampleTableAccessor::new` に渡すだけで再現する。`stts` の合計が 100 万に対して `Fixed.sample_count` を 0 にしても、現状は `InconsistentSampleCount` にならず `Ok` を返し、`sample_count()` は 100 万を返す（検証プログラムで確認済み）。

- `stts_box`: `SttsEntry { sample_count: 1_000_000, sample_delta: 1 }` の 1 エントリ
- `stsc_box`: `StscEntry { first_chunk: 1, sample_per_chunk: 1_000_000, sample_description_index: 1 }` の 1 エントリ
- `stsz_box`: `StszBox::Fixed { sample_size: 1, sample_count: 0 }`（`stts` 合計 100 万と不一致）
- `stco_or_co64_box`: 1 チャンク

`issues/closed/0009-bug-sample-table-accessor-overflow.md` の「### スコープ外」に「`StszBox::Fixed { sample_count }` が `stts` の合計と突き合わされていないこと（`Variable` のときしか検証しない）」として明記され、「起票の要否とタイミングは担当者判断とする」とされている。open / pending には未起票である。

## 設計方針

`SampleTableAccessor::new` の `stsz` 検証を `Variable` と `Fixed` の両方に広げる。`Fixed` のとき `sample_count`（`stsz` 側）が `stts` 由来の `sample_count` と一致しなければ、既存の `SampleTableAccessorError::InconsistentSampleCount { stts_sample_count, other_box_type: StszBox::TYPE, other_sample_count }` を返す（`other_sample_count` に `Fixed.sample_count` を入れる）。新しいエラーバリアントの追加は不要である。

検査の位置は既存の `Variable` 版と同じ箇所（`stsc` の検査より前）に置くこと。位置によって `stsz` と `stsc` が同時に食い違ったときにどちらのエラーが表面化するかが変わるため、`Variable` と `Fixed` で表面化順序を揃える。

## 完了条件

- `SampleTableAccessor::new` が `StszBox::Fixed` でも `sample_count` を `stts` 合計と突き合わせ、不一致なら `InconsistentSampleCount { other_box_type: StszBox::TYPE }` を返すこと
- `Variable` の既存挙動が変わらないこと
- 正当な `Fixed`（`sample_count` が `stts` 合計と一致）の挙動が変わらないこと
- `tests/test_auxiliary.rs`（または `pbt/tests/prop_auxiliary.rs`）に、`Fixed.sample_count` を `stts` 合計と食い違わせた入力が `InconsistentSampleCount` を返すことを検証するテストを追加すること。既存の `inconsistent_sample_count_stts_vs_stsz`（`pbt/tests/prop_auxiliary.rs`）は `Variable` を使っており `Fixed` 経路を捕捉しない
- 既存テストが壊れないことを確認すること。`pbt/tests/prop_auxiliary.rs` の `fixed_sample_size`（`Fixed { sample_size: 256, sample_count: 5 }`・`stts` 合計 5）と、`tests/test_auxiliary.rs` の `sample_count_exactly_u32_max_returns_inconsistent_not_overflow`（`Fixed { sample_size: 1, sample_count: u32::MAX }`・`stts` 合計 `u32::MAX`）は、いずれも `Fixed.sample_count` が `stts` 合計と一致しているため新チェックを通過する。後者は `stts` と一致した後、空の `stsc` との不一致で従来どおり `StscBox` の `InconsistentSampleCount` が表面化する
- `make test` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check` が通ること
- `CHANGES.md` の `## develop` に `[FIX]` エントリと担当者行が追記されていること（既存の `InconsistentSampleCount` を流用しバリアントを追加しないため後方互換は保たれる。整合する正当な入力に対する挙動は不変で、`Fixed` で `stts` 合計と食い違う破損入力だけが新たに `Err` になる）

## 関連 issue

- `SampleTableAccessor::new` のメモリ増幅（別 issue）: 独立している。本 issue を修正しても、攻撃者は `stsz.Fixed.sample_count` を `stts` 合計に一致させればメモリ増幅の経路にそのまま到達できるため、本 issue はメモリ増幅の緩和策にはならない。両者は目的（整合性検証の網羅 / メモリ確保のオーダー抑制）が異なる別カテゴリの修正として分ける
