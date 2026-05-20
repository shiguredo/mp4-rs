# examples/demux.rs の println! 出力を日本語に統一する

Created: 2026-05-20
Model: opencode mimo-v2.5-pro

## 概要

`examples/demux.rs` の `println!` 出力が英語のままとなっており、
AGENTS.md の「常に日本語を利用すること」および「サンプルはお手本なので性能と堅牢性を両立させること」に違反している。

## 根拠

`examples/demux.rs` の英語 `println!` 出力:

```rust
// 行 50
println!("Found {} track(s)\n", tracks.len());
// 行 54
println!("Track {}:", i + 1);
// 行 55
println!("  Track ID: {}", track.track_id);
// 行 57-58
println!("  Duration: {} (timescale: {})", track.duration, track.timescale);
// 行 65
println!("Samples:");
// 行 72
println!("  Sample {}:", sample_count);
// 行 74
println!("    Timestamp: {}", sample.timestamp);
// 行 75
println!("    Data offset: 0x{:x}", sample.data_offset);
// 行 76
println!("    Data size: {} bytes", sample.data_size);
// 行 80
println!("  ... (showing first 10 samples)");
```

対照的に `examples/fmp4.rs` は日本語で統一されている。

## 修正方針

`println!` 群を日本語に変更する。`eprintln!` (行 13, 19, 45, 86) の英語エラー文はそのままとする。

`examples/fmp4.rs` の出力スタイルに寄せた例:

```rust
println!("トラック数: {}\n", tracks.len());
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
