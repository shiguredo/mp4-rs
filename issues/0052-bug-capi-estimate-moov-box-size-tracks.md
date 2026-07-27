# C API の `mp4_estimate_maximum_moov_box_size()` が音声・映像の 2 トラック分しか見積もれない

- Priority: Medium
- Created: 2026-07-27
- Completed: YYYY-MM-DD
- Model: Opus 5
- Branch: feature/fix-capi-estimate-moov-box-size-tracks
- Polished: YYYY-MM-DD

## 目的

C API の `mp4_estimate_maximum_moov_box_size()` が音声・映像の 2 引数固定になっており、字幕トラックを含む 3 トラック構成の `moov` サイズを見積もれない問題を解消する。

`Mp4FileMuxer` が字幕トラックを受け入れるようになったため、C API / WASM の利用者は faststart 用の予約領域を正しく決められない状態にある。

## 優先度根拠

Medium。生成される MP4 が壊れるわけではなく、faststart が黙って無効になり `moov` がファイル末尾に回るだけの縮退で済む。ただし利用者がこの関数の戻り値をそのまま渡しても目的（faststart の有効化）を達成できず、しかも縮退したことを検知する手段が無い。

## 現状

### 見積もり関数が 2 トラック固定

`crates/c-api/src/mux.rs:306-317`:

```rust
pub extern "C" fn mp4_estimate_maximum_moov_box_size(
    audio_sample_count: u32,
    video_sample_count: u32,
) -> u32 {
    shiguredo_mp4::mux::estimate_maximum_moov_box_size(&[
        audio_sample_count as usize,
        video_sample_count as usize,
    ]) as u32
}
```

Rust 本体の `estimate_maximum_moov_box_size()`（`src/mux_mp4_file.rs:82`）は `&[usize]` で任意トラック数を受けられるため、制約は C API 側の引数だけにある。

### 実測

映像 / 音声 / 字幕を 1 本ずつ交互に `append_sample()` した構成で、2 トラック見積もりと 3 トラック見積もりを比較した結果、11 ケース中 6 ケースで faststart の有無が変わった。

```
v=  10 a=  10 s= 100 | 2track 見積= 2880 faststart=false 実 moov= 3583 | 3track 見積= 5504 faststart=true
v=  50 a=  50 s= 300 | 2track 見積= 4160 faststart=false 実 moov= 9603 | 3track 見積= 9984 faststart=true
v=   1 a=   1 s=1000 | 2track 見積= 2592 faststart=false 実 moov=16047 | 3track 見積=19616 faststart=true
```

「字幕分のサンプル数を `audio_sample_count` に足す」という回避策も、`v=50 a=50 s=300` のケースでは不足する（`512 + 1024 * 2 + 400 * 16 = 8960 < 9603`）。トラック数そのものが見積もり式の項（`PER_TRACK_OVERHEAD`）に効くためである。

### 縮退を検知する手段が無い

Rust 側の `FinalizedBoxes::is_faststart_enabled()`（`src/mux_mp4_file.rs:153`）は C API に公開されていない。`crates/c-api/src/mux.rs` の公開関数一覧にも faststart の成否を問い合わせるものは無いため、C API 利用者は見積もりが不足したことを知る方法がない。

## 設計方針

- 任意トラック数を受け取れる関数を追加する。C の呼び出し規約に合わせ、配列とその長さを取る形（例: `const uint32_t *sample_counts, uint32_t sample_counts_len`）を検討する
- 既存の `mp4_estimate_maximum_moov_box_size()` の扱いを決める。C API の後方互換を壊さないなら残したうえで doc に制約を明記する（制約の注記自体は 0046 の対応時に追記済み）
- faststart の成否を問い合わせる関数を C API に追加するかを判断する。追加する場合は `mp4_file_muxer_finalize()` の後に呼ぶ想定になる

なお `crates/c-api/include/mp4.h` は `crates/c-api/build.rs` の cbindgen が毎ビルド再生成するため、ヘッダーを手で編集する必要はない。

## 完了条件

- C API から字幕トラックを含む 3 トラック以上の構成の `moov` サイズを見積もれること
- 上記実測の 6 ケースで faststart が有効になること
- C API 経由で faststart の成否を判定できること（追加する方針を採る場合）
- `crates/c-api/tests/` に見積もり関数のテストが追加されていること
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `cargo test -p c-api --lib` が通ること
