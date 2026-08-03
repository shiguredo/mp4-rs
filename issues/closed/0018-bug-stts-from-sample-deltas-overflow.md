# boxes_moov_tree.rs の SttsBox::from_sample_deltas で sample_count += 1 が非 checked であり overflow 時に panic する

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-29
- Model: opencode-go glm-5.2
- Branch: feature/fix-stts-from-sample-deltas-overflow
- Polished: 2026-07-29

## 目的

`SttsBox::from_sample_deltas` で同一 `sample_delta` が連続するとき `last.sample_count += 1` が非 checked であり、`u32::MAX` を超えると debug で panic、release で wrap して不正な `stts` を生成する問題を修正する。

## 優先度根拠

mux 経路からも呼ばれる公開 API であり、境界入力で panic / 壊れたボックス出力になる。`mux_mp4_file` の `build_ctts_box` では `checked_add` しているのに `SttsBox` 側は未防御で一貫性が無い。`shiguredo-rust` 規約で panic は実装バグの表明であり、入力由来の overflow は `Result` で返すべき。現実のメディア長ではほぼ到達しないが、同種の防御的修正（例: `issues/0032-bug-mux-stbl-saturating-add.md`）と同じ方針で明示的なエラーにする。

## 現状

```rust
// src/boxes_moov_tree.rs:2021-2039（sample_count += 1 は 2030 行）
for sample_delta in sample_deltas {
    if let Some(last) = entries.last_mut()
        && last.sample_delta == sample_delta
    {
        last.sample_count += 1;
        continue;
```

`SttsEntry.sample_count` は `u32`。同一 `sample_delta` が `u32::MAX` 回を超えると `+= 1` で overflow する。入力は `IntoIterator<Item = u32>` であり、理論上到達可能。

対照的に `mux_mp4_file.rs:1228` の `build_ctts_box` では `last.sample_count.checked_add(1).ok_or(MuxError::Overflow)?` と防御済み。

## 設計方針

`from_sample_deltas` の戻り値を `Result<SttsBox, Error>` に変更し、overflow 時は `Err` を返す。`checked_add` を使用する。呼び出し側はエラーを伝播する。

`MuxError` には `From<Error>`（`EncodeError` への変換）があるため、`mux_mp4_file` / `mux_fmp4_segment` では `?` で伝播できる。`examples/transcode_wasm` はクレート独自の `Error` を使い `From<shiguredo_mp4::Error>` が無いため、同ファイルの他箇所と同様に `map_err(|e| Error::new(e.to_string()))` で変換する（`?` だけではコンパイルできない）。

`build_ctts_box` が返す `MuxError::Overflow` と、本 API 経由で mux に届く `MuxError::EncodeError(InvalidData)` はレイヤ差によるもので、boxes 公開 API が `MuxError` を返す必要はない。

## 完了条件

- 同一 `sample_delta` が `u32::MAX` 回を超えるとき panic せず `Err` を返すこと
- release ビルドでも wrap せず `Err` を返すこと
- 既存の正常系テストが通ること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

- `src/boxes_moov_tree.rs` の `SttsBox::from_sample_deltas` の戻り値を `SttsBox` から `Result<SttsBox, Error>` に変更した
- 集約ロジックを private helper `SttsBox::push_sample_delta(entries: &mut Vec<SttsEntry>, sample_delta: u32) -> Result<()>` に切り出し、`last.sample_count.checked_add(1).ok_or_else(|| Error::invalid_data("stts sample_count overflow"))?` で overflow を検出するようにした。切り出したのは、`u32::MAX` 近傍の境界を `entries` に事前状態を仕込む形で単体テストから叩けるようにするため
- 呼び出し側を追従した
  - `src/mux_mp4_file.rs` の `build_stbl_box`: `?` で伝播（`From<Error> for MuxError` 経由で `MuxError::EncodeError` になる）
  - `src/mux_fmp4_segment.rs` の `build_init_trak` 相当箇所: 元は空 iter を渡していたため `SttsBox { entries: Vec::new() }` の直接構築に置換して不要な `?` を除去した（周辺の空 box 直接構築の慣習に揃えた）
  - `examples/transcode_wasm/src/mp4.rs`: 同ファイルの他箇所と同じく `map_err(|e| Error::new(e.to_string()))` で独自 `Error` に変換
  - `src/auxiliary.rs` のテスト・`pbt/tests/prop_auxiliary.rs`・`pbt/tests/prop_boxes.rs`: 正常系入力なので `.expect(...)` で取り出す
- `src/boxes_moov_tree.rs` に `#[cfg(test)] mod stts_box_tests` を追加し、以下 3 テストを配置した
  - `from_sample_deltas_aggregates_identical_deltas`: 連続同一 `sample_delta` の run-length 集約と、非隣接に再登場した同一 `sample_delta` が別エントリーになることを検証
  - `push_sample_delta_accepts_u32_max_count`: 末尾エントリの `sample_count` が `u32::MAX - 1` の状態から 1 加算して `u32::MAX` に達するケースが成功することを検証
  - `push_sample_delta_rejects_overflow`: 末尾エントリの `sample_count` が `u32::MAX` の状態からさらに加算すると `Err(ErrorKind::InvalidData)` を返し、失敗時に `entries` を破壊しないことを検証
- `CHANGES.md` の `## develop` セクションに `[CHANGE]` エントリを追加した（`Result` 化と、これまで異常入力で panic または不正な `stts` を出力していた挙動を `InvalidData` に置き換えた旨を記載）
