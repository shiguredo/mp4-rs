# wasm の fmp4_segment_demux.rs で sample.track の null チェックなしに参照外ししている

- Priority: Medium
- Created: 2026-07-20
- Completed: 2026-07-31
- Model: qwen3.8-max-preview
- Branch: feature/fix-wasm-fmp4-segment-demux-null-check
- Polished: 2026-07-20

## 目的

WASM の `crates/wasm/src/fmp4_segment_demux.rs` の `fmt_json_demux_sample()` 内で `sample.track` を null チェックなしで `&*sample.track` している。同じクレートの `crates/wasm/src/demux.rs` の `fmt_json_mp4_demux_sample()` では `sample.track` の null チェックを行っており一貫性がない。

## 優先度根拠

`Mp4DemuxSample::new()` の実装上は常に非 null だが、防御的プログラミングとクレート内の一貫性の観点から修正すべき。

## 現状

`crates/wasm/src/fmp4_segment_demux.rs` の `fmt_json_demux_sample()`:

```rust
let track = unsafe { &*sample.track }; // null チェックなし
```

対比: `crates/wasm/src/demux.rs` の `fmt_json_mp4_demux_sample()`:

```rust
if !sample.track.is_null() {
    let track = unsafe { &*sample.track };
```

## 設計方針

`fmt_json_mp4_demux_sample()` と同様に `sample.track.is_null()` のガードを追加する。null 時は `track_id` メンバーを省略する（同関数と同じ挙動）。

## 完了条件

- `sample.track.is_null()` ガードが追加されること
- null 時に `track_id` メンバーが省略されること
- 既存のテストが通ること
- `cargo clippy` が通ること

## 解決方法

コード変更なしで closed にした。

`fmt_json_demux_sample()` は private で、上流の `fmp4_segment_demuxer_handle_media_segment()` が track 未検出時にサンプルを一切返さずエラー終了するため、`sample.track` は非 null で到達し、`&*sample.track` の参照外しに UB は発生しない。

対比先の `fmt_json_mp4_demux_sample()` は public C API `mp4_demux_sample_to_json()` 経由で外部から任意サンプルを受け取るため、そちらの null ガードには独立した意義がある。両者は呼び出し境界の性質が異なり、単純な「一貫性欠如」ではないため、防御的コードを追加する動機は薄いと判断した。
