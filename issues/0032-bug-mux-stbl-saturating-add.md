# mux_mp4_file.rs の build_stbl_box で NonZeroU32::saturating_add がオーバーフロー時に暗黙のデータ破壊を引き起こす

- Priority: Medium
- Created: 2026-07-20
- Completed: 2026-07-31
- Model: qwen3.8-max-preview
- Branch: feature/fix-mux-stbl-saturating-add
- Polished: 2026-07-30
- Updated: 2026-07-27

## 目的

`Mp4FileMuxer::build_stbl_box()` 内の `stsc_box`・`stss_box` 構築で `NonZeroU32::MIN.saturating_add(i as u32)` を使用している。チャンク数やサンプル数が `u32::MAX` を超えると `saturating_add` により値が `u32::MAX` で飽和し、複数の `StscEntry` が同一の `first_chunk` 値を持つ。生成された MP4 は `SampleTableAccessor::new()` の検証（`ChunkIndicesNotMonotonicallyIncreasing`）で拒否されデコード不能になるが、当該箇所自体はエラーを返さない。

## 優先度根拠

現実的に `u32::MAX` 個のチャンクは発生しないが、暗黙のデータ破壊よりも明示的なエラーが望ましい。CHANGES.md の同種修正（`append_sample` の u32 オーバーフロー、映像トラック解像度の i16 オーバーフロー）も同様に「現実的には起きない」が防御的に修正しており、本 issue もその方針と整合する。

## 現状

`src/mux_mp4_file.rs` の `Mp4FileMuxer::build_stbl_box` 内に 3 箇所:

**1. `StscEntry::sample_description_index`**:

```rust
let sample_description_index = sample_entries
    .iter()
    .position(|entry| entry == &c.sample_entry)
    .map(|idx| NonZeroU32::MIN.saturating_add(idx as u32))
    .expect("sample_entry should exist in sample_entries");
```

`idx as u32` も `usize` から `u32` への truncation リスクがある。

**2. `StscEntry::first_chunk`**:

```rust
first_chunk: NonZeroU32::MIN.saturating_add(i as u32),
```

`i as u32` も同様の truncation リスクがある。

**3. `StssBox::sample_numbers`**:

```rust
s.keyframe
    .then_some(NonZeroU32::MIN.saturating_add(i as u32))
```

## 設計方針

`saturating_add` を `checked_add` に置き換える。`checked_add` は `Option<NonZeroU32>` を返す（`saturating_add` は `NonZeroU32` を返す）。`None` の場合に `MuxError::Overflow` を返す。同ファイルの既存先例（`Mp4FileMuxer::advance_position`・`Mp4FileMuxer::append_sample`・`build_ctts_box` 内の `checked_add` → `MuxError::Overflow`）と統一する。

`i as u32` / `idx as u32` は `u32::try_from()` に置き換え、失敗時に `MuxError::EncodeError(Error::invalid_data("..."))` を返す。これは同ファイルの既存先例（`Mp4FileMuxer::append_sample` の `sample.data_size`、`build_stbl_box` 内の `sample_per_chunk`、映像解像度の `i16::try_from`）と同じパターン。

`stss_box` の `filter_map` はそのまま維持する。クロージャが `Option<Result<NonZeroU32, MuxError>>` を返せば、`filter_map` が `Option` を剥がして非キーフレームを除外し、残った `Result` を `collect::<Result<Vec<_>, _>>()` で集約できる。`map` への変更は不要。

ただし現在の `sample_numbers` は `s.keyframe.then_some(...)` で eager 評価しているため、クロージャ内で `checked_add` / `u32::try_from` の失敗を扱うには `then(|| ...)` への変更が必要になる。

### 0051 との編集順序

`issues/0051-bug-empty-stss-box.md` は同じ `Mp4FileMuxer::build_stbl_box` 内の `stss_box` 構築ブロックを対象とする。扱う対象は異なる（0051 は `stss` を出力するか否か、本 issue は `sample_numbers` の要素値の計算）ため内容の重複は無いが、同じ範囲を書き換えるため後着側でコンフリクトする。

**着手順: 本 issue（0032）を先に着手・マージする。** 0051 は本 issue の変更を前提に、`collect::<Result<Vec<_>, _>>()?` の後で `sample_numbers.is_empty()` を判定して空なら `stss` を出さない形へ組み替える。

## 完了条件

- `Mp4FileMuxer::build_stbl_box` 内の `sample_description_index`・`first_chunk`・`StssBox::sample_numbers` 構築 3 箇所の `saturating_add` が `checked_add` に置き換えられ、オーバーフロー時に `MuxError::Overflow` が返ること
- 同箇所の `i as u32` / `idx as u32` が `u32::try_from()` に置き換えられ、変換失敗時に `MuxError::EncodeError` が返ること
- オーバーフロー境界値の新規テストが追加されること（`u32::MAX` 個のチャンク生成は非現実的なため、`NonZeroU32::MIN.checked_add(u32::MAX)` が `None` を返すことの直接検証、または `build_stbl_box` の小さな入力での正常系テストで対応）
- 既存のテストが通ること
- `cargo clippy` が通ること

## 解決方法

### 実装

`src/mux_mp4_file.rs` の `Mp4FileMuxer::build_stbl_box` で、次の 3 箇所の `NonZeroU32::MIN.saturating_add(... as u32)` を `u32::try_from` + `checked_add` に置き換えた。

- `StscEntry::sample_description_index`
- `StscEntry::first_chunk`
- `StssBox::sample_numbers`

`u32::try_from` 失敗時は `MuxError::EncodeError`、`checked_add` 失敗時は `MuxError::Overflow` を返す。

`stss` の構築は、clippy（`filter_map_bool_then`）に合わせて `filter` + `map` + `collect::<Result<Vec<_>, _>>()` にした。キーフレーム以外を落としたあとも `enumerate` のグローバル 0-based 番号は保持される。

### テスト

`src/mux_mp4_file.rs` の `#[cfg(test)]` に以下を追加した。

- `test_nonzero_u32_min_checked_add_overflows_at_u32_max`: 実装が依存する `NonZeroU32::MIN.checked_add` の算術境界を直接確認する（`u32::MAX` 個のチャンク生成は非現実的なため）
- `test_build_stbl_box_one_based_indices_for_chunks_and_keyframes`: `finalize` 経路で `first_chunk` が `[1, 2, 3]`、`stss.sample_numbers` が `[1, 3]` になることを検証する

### ドキュメント

- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加した

## 後方互換

オーバーフロー時にエラーを返すようになるが、これは暗黙のデータ破壊からのバグ修正であり、正当な入力に対する挙動は不変。API シグネチャの変更もない（`build_stbl_box` は既に `Result` を返している）。

## CHANGES.md

`[FIX]` で記載する。CHANGES.md の develop セクションの同種修正（`append_sample` の u32 オーバーフロー等）と整合する。
