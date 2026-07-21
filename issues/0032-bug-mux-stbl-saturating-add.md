# mux_mp4_file.rs の build_stbl_box で NonZeroU32::saturating_add がオーバーフロー時に暗黙のデータ破壊を引き起こす

- Priority: Medium
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/fix-mux-stbl-saturating-add
- Polished: 2026-07-20

## 目的

`Mp4FileMuxer::build_stbl_box()` 内の `stsc_box`・`stss_box` 構築で `NonZeroU32::MIN.saturating_add(i as u32)` を使用している。チャンク数やサンプル数が `u32::MAX` を超えると `saturating_add` により値が `u32::MAX` で飽和し、複数の `StscEntry` が同一の `first_chunk` 値を持つ。生成された MP4 は `SampleTableAccessor::new()` の検証で拒否されデコード不能になるが、mux 側はエラーを返さない。

## 優先度根拠

現実的に u32::MAX 個のチャンクは発生しないが、暗黙のデータ破壊よりも明示的なエラーが望ましい。CHANGES.md の同種修正（`append_sample` の u32 オーバーフロー、`build_video_trak_box` の i16 オーバーフロー）も同様に「現実的には起きない」が防御的に修正しており、本 issue もその方針と整合する。

## 現状

`src/mux_mp4_file.rs` の `build_stbl_box` 内に 3 箇所:

**1. `sample_description_index`（994 行目）**:

```rust
let sample_description_index = sample_entries
    .iter()
    .position(|entry| entry == &c.sample_entry)
    .map(|idx| NonZeroU32::MIN.saturating_add(idx as u32))
    .expect("sample_entry should exist in sample_entries");
```

`idx as u32` も `usize` から `u32` への truncation リスクがある。

**2. `first_chunk`（997 行目）**:

```rust
first_chunk: NonZeroU32::MIN.saturating_add(i as u32),
```

`i as u32` も同様の truncation リスクがある。

**3. `stss_box` の `sample_numbers`（1037 行目）**:

```rust
s.keyframe
    .then_some(NonZeroU32::MIN.saturating_add(i as u32))
```

## 設計方針

`saturating_add` を `checked_add` に置き換える。`checked_add` は `Option<NonZeroU32>` を返す（`saturating_add` は `NonZeroU32` を返す）。`None` の場合に `MuxError::Overflow` を返す。同ファイルの既存の `checked_add` オーバーフロー（522, 613, 1129 行目）が `MuxError::Overflow` を使用しているため、これと統一する。

`i as u32` / `idx as u32` は `u32::try_from()` に置き換え、失敗時に `MuxError::EncodeError(Error::invalid_data("..."))` を返す。これは同ファイルの既存の `u32::try_from` 変換失敗（554, 859, 999 行目）と同じパターン。

`stss_box` の `filter_map` はそのまま維持する。クロージャが `Option<Result<NonZeroU32, MuxError>>` を返せば、`filter_map` が `Option` を剥がして非キーフレームを除外し、残った `Result` を `collect::<Result<Vec<_>, _>>()` で集約できる。`map` への変更は不要。

## 完了条件

- 994・997・1037 行目の `saturating_add` が `checked_add` に置き換えられ、オーバーフロー時に `MuxError::Overflow` が返ること
- `i as u32` / `idx as u32` が `u32::try_from()` に置き換えられ、変換失敗時に `MuxError::EncodeError` が返ること
- オーバーフロー境界値の新規テストが追加されること（`u32::MAX` 個のチャンク生成は非現実的なため、`NonZeroU32::MIN.checked_add(u32::MAX)` が `None` を返すことの直接検証、または `build_stbl_box` の小さな入力での正常系テストで対応）
- 既存のテストが通ること
- `cargo clippy` が通ること

## 解決方法

設計方針に従って `saturating_add` → `checked_add`、`as u32` → `u32::try_from()` に置き換える。

## 後方互換

オーバーフロー時にエラーを返すようになるが、これは暗黙のデータ破壊からのバグ修正であり、正当な入力に対する挙動は不変。API シグネチャの変更もない（`build_stbl_box` は既に `Result` を返している）。

## CHANGES.md

`[FIX]` で記載する。CHANGES.md の develop セクションの同種修正（`append_sample` の u32 オーバーフロー等）と整合する。
