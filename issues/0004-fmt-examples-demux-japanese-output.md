# examples/demux.rs の println! 出力を日本語に統一する

Created: 2026-05-20
Model: opencode mimo-v2.5-pro

## 概要

`examples/demux.rs` の `println!` 出力が英語のままとなっており、
CLAUDE.md の「常に日本語を利用すること」に違反している。

## 根拠

`examples/demux.rs` の英語 `println!` 出力 (行 50, 54, 55, 57, 65, 72, 74, 75, 76, 80)。

対照的に `examples/fmp4.rs` は日本語で統一されている
(例: `println!("トラック数: {}", ...)`, `println!("サンプル情報:")`)。

`eprintln!` (行 13, 19, 45, 86) の英語エラー文は CLAUDE.md の「エラーメッセージは全て英語」に従いそのままとする。

## 修正方針

`println!` 群を日本語に変更する。`examples/fmp4.rs` のラベルスタイルに合わせるが、
demux.rs はファイル情報を表示するため、`key=value` 形式ではなく `ラベル: 値` 形式を採用する。

```rust
println!("トラック数: {}", tracks.len());
println!("トラック {}:", i + 1);
println!("  トラック ID: {}", track.track_id);
println!("  再生時間: {} (タイムスケール: {})", track.duration, track.timescale);
println!("サンプル情報:");
println!("  サンプル {}:", sample_count);
println!("    タイムスタンプ: {}", sample.timestamp);
println!("    データオフセット: 0x{:x}", sample.data_offset);
println!("    データサイズ: {} バイト", sample.data_size);
println!("  ... (最初の 10 サンプルのみ表示)");
```
