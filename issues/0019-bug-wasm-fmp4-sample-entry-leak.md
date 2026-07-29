# wasm の fmp4_segment_mux で sample entry の内部ポインタが mp4_sample_entry_free されずリークする

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-wasm-fmp4-sample-entry-leak
- Polished: 2026-07-29

## 目的

WebAssembly 版 `fmp4_segment_mux` の `write_segment_impl` が `parse_json_mp4_sample_entry` で確保した sample entry の内部ポインタ（SPS/PPS/NALU 等）を `mp4_sample_entry_free` せず、`Box<Mp4SampleEntry>` の Drop だけで解放してヒープリークする問題を修正する。

## 優先度根拠

ストリーミング用途で media segment を繰り返し生成するたびに sample entry の内部ポインタが `mp4_alloc` で確保され、解放されずに蓄積する。長時間実行で OOM に至る。`mux.rs` の `mp4_mux_sample_free` は `mp4_sample_entry_free` を呼んでいるのに `fmp4_segment_mux` は呼んでいない。

## 現状

```rust
// crates/wasm/src/fmp4_segment_mux.rs:181-192
let mut sample_entry_boxes: Vec<Option<Box<c_api::boxes::Mp4SampleEntry>>> = Vec::new();
// ...
    sample_entry_boxes.push(meta.sample_entry.map(Box::new));
```

`parse_json_mp4_sample_entry` が SPS/PPS/NALU 等を `allocate_and_copy_array_list` → `mp4_alloc` で確保する。`Box<Mp4SampleEntry>` の Drop は構造体本体のみ解放し、内部 raw ポインタは解放しない。`Mp4SampleEntry` / 各コーデック構造体に `impl Drop` はない。`SampleMeta.sample_entry` も `Option<Mp4SampleEntry>`（`fmp4_segment_mux.rs:276`）のまま通常 Drop される。

対照的に `crates/wasm/src/mux.rs:55-66` の `mp4_mux_sample_free` は、先に `Box::into_raw` で得た raw ポインタに対して `mp4_sample_entry_free` を呼ぶ（`mux.rs:97` で `into_raw`）。

```rust
// crates/wasm/src/boxes.rs:159-215
pub unsafe fn mp4_sample_entry_free(sample_entry: *mut Mp4SampleEntry) {
    // ... kind ごとに内部ポインタを解放 ...
    // 構造体自体を解放
    let _ = unsafe { Box::from_raw(sample_entry) };
}
```

`mp4_sample_entry_free` は内部ポインタ解放のあと `Box::from_raw` で構造体本体も消費する。したがって `Box` 所有のままポインタだけ渡して free し、続けて `Box` を Drop すると二重解放になる。

リーク条件: `sample_entry` 付きメタで `write_media_segment_metadata*_json` を呼ぶたび。ポインタフィールドあり（avc1 / hev1 / hvc1 / av01 / mp4a / flac / stpp / wvtt / tx3g）。ネスト確保なし（opus / vp08 / vp09）は実質リークなし（`mp4_sample_entry_free` の no-op arm のみ）。

早期 return でもリークする。サイズ不正の return（`fmp4_segment_mux.rs:186-191`）は `sample_entry_boxes.push` より前にあり、(1) 当該ループの未 push 分、(2) `for meta in sample_metas` の残余 `SampleMeta.sample_entry` がラッパを通らず通常 Drop される。C API エラー（`:264-266`）は push 済み分も `Box` Drop のみで内部ポインタが残る。

## 設計方針

`parse_json_sample_metas` の直後・サイズ検証ループに入る前に、全 `SampleMeta.sample_entry` を `take` して Drop ラッパの `Vec` に移す。その後の成功・早期 return いずれでも、この `Vec` の Drop だけで全エントリが解放される。関数末尾での明示解放は置かない（二重解放の温床になるため）。

ラッパの契約:

- 要素は `Option` を保持する（`sample_entry` 省略時は `None`）
- `Drop` では `Some(entry)` のときだけ `mp4_sample_entry_free(Box::into_raw(Box::new(entry)))`（または同等の `Box` 化 → `into_raw`）を呼ぶ。`None` では何もしない
- ラッパ側で構造体本体を再度 Drop しない（`mp4_sample_entry_free` が `Box::from_raw` する）

所有権の正しい流れ（`mux.rs` と同型）:

1. ループ前に全 entry をラッパ `Vec` へ移す。C API へ渡すポインタはラッパ内のエントリへの参照から取る
2. Drop 時に `Some` だけ `Box::into_raw` → `mp4_sample_entry_free`
3. `mp4_sample_entry_free` が内部ポインタと構造体本体の両方を解放する

## 完了条件

- `write_segment_impl` の成功・早期 return（サイズ不正・C API エラーを含む）いずれでも、確保済み `sample_entry` の内部ポインタと構造体本体が `mp4_sample_entry_free` 経由で解放されること（`Mp4SampleEntry` / `Box` の通常 Drop に頼らないこと）
- 上記の解放成立は、ラッパの `Drop` が `mp4_sample_entry_free` を呼ぶことと、ループ前 `take` で全 entry がラッパ `Vec` に載ることをコードレビューで確認する（`mp4_alloc` / `mp4_free` に公開カウンタが無いため、テストから「解放されたこと」を直接観測しない）
- ポインタフィールドありの kind（avc1 / hev1 / hvc1 / av01 / mp4a / flac / stpp / wvtt / tx3g）は `mp4_sample_entry_free` の既存 arm 経由で解放されること（ラッパは kind を分岐しない）
- `None` 要素に対して `mp4_sample_entry_free` を呼ばないこと、および `Some` を `Box` 所有のまま free して二重解放しないこと
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `fmp4_segment_mux.rs` に Drop ラッパ型を追加する。`Drop` は `Option::take` した `Some` だけを `Box::into_raw` → `mp4_sample_entry_free` する
2. `write_segment_impl` で `let mut sample_metas` とし、`parse_json_sample_metas` 直後に全 `sample_entry` をラッパ `Vec` へ `take` し、以降のループ・早期 return はこの `Vec` の寿命に載せる。ポインタはラッパ内エントリへの参照から取得する（C API 呼び出しが終わるまで `Vec` を drop しない）
3. ラッパの Drop 実装以外から `mp4_sample_entry_free` を呼ばない
4. `crates/wasm/src/fmp4_segment_mux.rs` の `#[cfg(test)]` に次を追加する（モック不可。これらは二重解放の有無と早期 return の健全性を見るものであり、「解放されたこと」自体の証明には使わない）
   - ラッパ単体: avc1（ポインタあり）を `Some` で包んで Drop してもパニックしないこと、`None` を Drop してもパニックしないこと
   - 統合: `sample_entry` 付きメタで `data_size` 不正による早期 return が null を返し、パニックしないこと
