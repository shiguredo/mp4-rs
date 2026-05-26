# Mp4FileMuxer の fuzz ターゲットを追加する

- Priority: Medium
- Created: 2026-05-26
- Model: Opus 4.7
- Branch: feature/add-fuzz-mp4-file-muxer

## 目的

`mux_mp4_file.rs` (`Mp4FileMuxer`) に対応する fuzz ターゲットが存在しない。
`fuzz_fmp4_segment_mux.rs` は fMP4 セグメントの mux を対象としているが、通常の MP4 ファイル mux は未カバー。

`Mp4FileMuxer` は `append_sample()` でのサンプル蓄積、`finalize()` での `MoovBox` 生成、
faststart 対応のオフセット調整など非自明な内部ロジックを持ち、任意入力に対するパニック安全性を検証すべき。

## 優先度根拠

公開 API であり、demux 側 (`fuzz_mp4_file_demux.rs`) や fMP4 mux 側 (`fuzz_fmp4_segment_mux.rs`) は
既にカバーされているのに対して、通常 MP4 の mux だけが抜けている。
バグではなくカバレッジの欠落のため Medium とする。

## 現状

| モジュール | fuzz ターゲット |
|---|---|
| `demux_mp4_file.rs` | `fuzz_mp4_file_demux.rs` |
| `demux_fmp4_file.rs` | `fuzz_fmp4_file_demux.rs` |
| `demux_fmp4_segment.rs` | `fuzz_fmp4_segment_demux.rs` |
| `mux_fmp4_segment.rs` | `fuzz_fmp4_segment_mux.rs` |
| **`mux_mp4_file.rs`** | **なし** |

## 設計方針

`fuzz_fmp4_segment_mux.rs` と同様のパターンを採用する:

1. 任意バイト列を `Mp4FileDemuxer` でデマルチプレクスする
2. 取得したサンプル情報を `Mp4FileMuxer` で再マルチプレクスする
3. `initial_boxes_bytes()` / `advance_position()` / `append_sample()` / `finalize()` の
   全メソッドチェーンがパニックしないことを確認する

## 完了条件

- `fuzz/fuzz_targets/fuzz_mp4_file_mux.rs` が追加されている
- `fuzz/Cargo.toml` に `[[bin]]` エントリが追加されている
- `cargo fuzz build fuzz_mp4_file_mux` が成功する
- 短時間の fuzzing 実行でパニックが発生しない
