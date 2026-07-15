# wasm の fmp4_segment_mux で sample entry の内部ポインタが mp4_sample_entry_free されずリークする

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-wasm-fmp4-sample-entry-leak
- Polished: YYYY-MM-DD

## 目的

WebAssembly 版 `fmp4_segment_mux` の `write_segment_impl` が `parse_json_mp4_sample_entry` で確保した sample entry の内部ポインタ（SPS/PPS/NALU 等）を `mp4_sample_entry_free` せず、`Box<Mp4SampleEntry>` の Drop だけで解放してヒープリークする問題を修正する。

## 優先度根拠

ストリーミング用途で media segment を繰り返し生成するたびに sample entry の内部ポインタが `mp4_alloc` で確保され、解放されずに蓄積する。長時間実行で OOM に至る。`mux.rs` の `mp4_mux_sample_free` は `mp4_sample_entry_free` を呼んでいるのに `fmp4_segment_mux` は呼んでいない。

## 現状

```rust
// crates/wasm/src/fmp4_segment_mux.rs:178-189
let mut sample_entry_boxes: Vec<Option<Box<c_api::boxes::Mp4SampleEntry>>> = Vec::new();
// ...
    sample_entry_boxes.push(meta.sample_entry.map(Box::new));
```

`parse_json_mp4_sample_entry` が SPS/PPS/NALU 等を `allocate_and_copy_array_list` → `mp4_alloc` で確保する。`Box<Mp4SampleEntry>` の Drop は構造体本体のみ解放し、内部 raw ポインタは解放しない。`Mp4SampleEntry` / 各コーデック構造体に `impl Drop` はない。

対照的に `crates/wasm/src/mux.rs:61-64` の `mp4_mux_sample_free` は `mp4_sample_entry_free` を呼ぶ。

```rust
// crates/wasm/src/boxes.rs:126-170
pub unsafe fn mp4_sample_entry_free(sample_entry: *mut Mp4SampleEntry) {
    // ... kind ごとに内部ポインタを解放 ...
    let _ = unsafe { Box::from_raw(sample_entry) };
}
```

リーク条件: `sample_entry` 付きメタで `write_media_segment_metadata*_json` を呼ぶたび。avc1 / hev1 / hvc1 / av01 / mp4a / flac（ポインタフィールドあり）。opus / vp08 / vp09 はネスト確保なしのため実質リークなし。

## 設計方針

`sample_entry_boxes` の各要素を `Drop` する際に `mp4_sample_entry_free` を呼ぶ。関数終了時の `Vec` の Drop で各 `Box<Mp4SampleEntry>` が drop される前に、`mp4_sample_entry_free` で内部ポインタを解放する。

実装案:
- `sample_entry_boxes` を `Vec<Option<Box<Mp4SampleEntry>>>` ではなく、drop 時に `mp4_sample_entry_free` を呼ぶラッパ型で包む
- または関数の最後で明示的に `mp4_sample_entry_free` を呼んでから `Box` を消費する

## 完了条件

- `sample_entry` 付きの media segment を繰り返し生成してもメモリ使用量が単調増加しないこと
- avc1 / hev1 / hvc1 / av01 / mp4a / flac で内部ポインタが正しく解放されること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `write_segment_impl` の終了時に `sample_entry_boxes` の各要素に対して `mp4_sample_entry_free` を呼ぶ
2. 早期 return パス（エラー時）でもリークしないよう、`Vec` の drop で確実に解放される構造にする
3. リーク検証のテストを追加する（可能なら）
