# auxiliary.rs の SampleTableAccessor::new で stts/ctts 加算が非 checked であり overflow 時に panic / wrap する

- Priority: High
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-sample-table-accessor-overflow
- Polished: YYYY-MM-DD

## 目的

`SampleTableAccessor::new` が `Result` を返す API でありながら、破損した `stts` / `ctts` ボックスの加算処理が `checked_add` を使わず、debug ビルドで panic、release ビルドで wrap して誤ったサンプル数・タイムスタンプを返す問題を修正する。

## 優先度根拠

本ライブラリは入力バイナリをデコードする性質上、破損ファイル・攻撃的入力への堅牢性が重要である。panic は `shiguredo-rust` 規約で「実装バグの表明」であり、入力由来の異常は `Err` にすべき。他のデコード経路（mux/demux）は `checked_add` を使っているのにここだけ未防御であり、一貫性も欠いている。debug ビルドで確実にクラッシュするため High。

## 現状

```rust
// src/auxiliary.rs:29-32
for entry in &stbl_box_ref.stts_box.entries {
    sample_durations.push((sample_count, entry.sample_delta, acc_duration));
    sample_count += entry.sample_count;
    acc_duration += entry.sample_delta as u64 * entry.sample_count as u64;
}
```

```rust
// src/auxiliary.rs:49-51
for entry in &ctts_box.entries {
    sample_composition_offsets.push((ctts_sample_count, entry.sample_offset));
    ctts_sample_count += entry.sample_count;
}
```

`sample_count`（`u32`）と `acc_duration`（`u64`）の加算が非 checked。`SttsEntry.sample_count` は `u32` であり、複数エントリの合計が `u32::MAX` を超える破損 `stts` で debug panic する。`SampleTableAccessorError` に overflow 系のバリアントがなく、overflow が `Err` にならない。

対照的に `mux_mp4_file.rs:1129` の `build_ctts_box` では `checked_add` を使っている。

## 設計方針

`checked_add` を使用し、overflow 時は `Err` を返す。`SampleTableAccessorError` に overflow 用バリアントを追加するか、既存の `InconsistentSampleCount` と使い分ける。`acc_duration` の `u64` 乗算 `entry.sample_delta as u64 * entry.sample_count as u64` 自体は `u64` に収まるが、累積加算で `u64` の overflow が起き得るため `checked_add` で防御する。

## 完了条件

- 破損 `stts` / `ctts` で加算 overflow が起きても panic せず `Err` を返すこと
- release ビルドでも wrap せず `Err` を返すこと
- 既存の正常系テストが通ること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `src/auxiliary.rs:31-32, 51` の `+=` を `checked_add` に置き換え、overflow 時は `Err(SampleTableAccessorError::...)` を返す
2. `SampleTableAccessorError` に overflow 用バリアントを追加する（例: `Overflow`）
3. `prop_auxiliary` に破損 `stts` / `ctts` で overflow を起こすテストを追加する
