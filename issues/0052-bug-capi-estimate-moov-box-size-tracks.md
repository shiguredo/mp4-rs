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

以下の方針で実装する（3 つの主要な設計判断は決定済み）。

### 見積もり関数のシグネチャ

`mp4_estimate_maximum_moov_box_size()` を、任意トラック数を受け取れる配列 + 長さのシグネチャに変更する。

- 変更後: `mp4_estimate_maximum_moov_box_size(const uint32_t *sample_counts, uint32_t sample_counts_len) -> uint32_t`
- Rust 側 `shiguredo_mp4::mux::estimate_maximum_moov_box_size(&[usize])` と 1:1 で対応する形。トラック種別は Rust 側の見積もり式が使わないため C 側でも受けない。
- `sample_counts` が NULL の場合の扱いは実装時に決める（他の C API 関数と揃えて、NULL には 0 を返す、または NULL の場合は 0 として扱うなど、安全側で処理する）。

### 既存 2 引数版の扱い

既存の `(audio_sample_count, video_sample_count)` 版は破壊的に置き換える（deprecated として残さない）。

- `c-api` クレートは 0.1.0 のためメジャーバージョン到達前で破壊的変更は許容範囲。
- CHANGES.md に破壊的変更として記録する（`shiguredo-changelog` に従う）。
- 既存の呼び出し箇所も新シグネチャに書き換える:
    - `crates/c-api/tests/simple_mux_demux.c` の `main` 内 `mp4_estimate_maximum_moov_box_size` 呼び出し
    - `crates/wasm/examples/mux.js` の `mp4_estimate_maximum_moov_box_size` 呼び出し

### faststart 成否の問い合わせ関数

今回は追加しない。

- Rust 側の `FinalizedBoxes::is_faststart_enabled()` を C API に公開する対応は行わない。
- 見積もり関数が任意トラック数に対応することで、上記実測 6 ケースを含む通常構成では faststart が有効になることが期待できる。
- faststart を確実に有効にしたい利用者は、余裕を持たせて `mp4_file_muxer_set_reserved_moov_box_size()` に直接指定する従来の運用で対応可能。
- 「## 現状」の「縮退を検知する手段が無い」は今回のスコープ外として残す（必要になった時点で別 issue を起票する）。

### その他

`crates/c-api/include/mp4.h` は `crates/c-api/build.rs` の cbindgen が毎ビルド再生成するため、ヘッダーを手で編集する必要はない。

## 完了条件

- `mp4_estimate_maximum_moov_box_size()` が任意トラック数を受け取れる（配列 + 長さ）シグネチャに置き換わっていること
- C API から字幕トラックを含む 3 トラック以上の構成の `moov` サイズを見積もれること
- 上記実測の 6 ケースで faststart が有効になること
- 既存の呼び出し箇所（`crates/c-api/tests/simple_mux_demux.c`、`crates/wasm/examples/mux.js`）が新シグネチャに追従していること
- `crates/c-api/tests/` に見積もり関数のテストが追加されていること
- CHANGES.md に破壊的変更として追記されていること
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `cargo test -p c-api --lib` が通ること
