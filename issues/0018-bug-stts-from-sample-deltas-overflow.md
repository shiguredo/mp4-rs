# boxes_moov_tree.rs の SttsBox::from_sample_deltas で sample_count += 1 が非 checked であり overflow 時に panic する

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-stts-from-sample-deltas-overflow
- Polished: YYYY-MM-DD

## 目的

`SttsBox::from_sample_deltas` で同一 `sample_delta` が連続するとき `last.sample_count += 1` が非 checked であり、`u32::MAX` を超えると debug で panic、release で wrap して不正な `stts` を生成する問題を修正する。

## 優先度根拠

mux 経路からも呼ばれる公開 API であり、境界入力で panic / 壊れたボックス出力になる。`mux_mp4_file` の `build_ctts_box` では `checked_add` しているのに `SttsBox` 側は未防御で一貫性が無い。`shiguredo-rust` 規約で panic は実装バグの表明であり、入力由来の overflow は `Result` で返すべき。

## 現状

```rust
// src/boxes_moov_tree.rs:1718-1723
for sample_delta in sample_deltas {
    if let Some(last) = entries.last_mut()
        && last.sample_delta == sample_delta
    {
        last.sample_count += 1;
        continue;
```

`SttsEntry.sample_count` は `u32`。同一 `sample_delta` が `u32::MAX` 回を超えると `+= 1` で overflow する。入力は `IntoIterator<Item = u32>` であり、理論上到達可能。

対照的に `mux_mp4_file.rs:1129` の `build_ctts_box` では `last.sample_count.checked_add(1).ok_or(MuxError::Overflow)?` と防御済み。

## 設計方針

`from_sample_deltas` の戻り値を `Result<SttsBox, Error>` に変更し、overflow 時は `Err` を返す。`checked_add` を使用する。呼び出し側（`mux_mp4_file` 等）も併せて `?` で伝播する。

## 完了条件

- 同一 `sample_delta` が `u32::MAX` 回を超えるとき panic せず `Err` を返すこと
- release ビルドでも wrap せず `Err` を返すこと
- 既存の正常系テストが通ること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `SttsBox::from_sample_deltas` のシグネチャを `Result<SttsBox, Error>` に変更する
2. `last.sample_count += 1` を `last.sample_count = last.sample_count.checked_add(1).ok_or_else(|| Error::invalid_data("stts sample_count overflow"))?` に置き換える
3. 呼び出し側を `?` で伝播するよう修正する
4. 境界値テストを追加する
