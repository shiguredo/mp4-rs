# examples と doc コメント内の `println!` 出力を英語に統一する

- Priority: Low
- Created: 2026-05-20
- Completed: YYYY-MM-DD
- Model: opencode mimo-v2.5-pro

## 目的

`examples/fmp4.rs` および `src/*.rs` の doc コメント内サンプルの `println!` 出力文字列が日本語のままで、`examples/demux.rs` の `println!` (英語) と表記が分かれている。以下の理由から `println!` の出力文字列を英語に統一する。

- `examples/demux.rs` の `eprintln!` によるエラーメッセージは既に英語であり、`println!` を英語に揃えると同一ファイル・同一プロジェクト内での日英混在が解消される。
- `crates.io` 経由で公開する OSS ライブラリのサンプルコードとして、英語表記のほうが幅広い読者に伝わる。
- `println!` は `log`/`tracing` 経由のプロダクションログではないが、実行時に stdout に出す出力である以上、性質としてはログメッセージに準じるものと解釈し、CLAUDE.md「ログメッセージは全て英語にすること」を適用する。

## 優先度根拠

機能への影響はない表記統一のため Low。ただし放置すると `examples/demux.rs` と `examples/fmp4.rs` のスタイル分裂が続き、新しい example や doc コメント内サンプルを追加するときに毎回どちらに合わせるかの判断コストがかかる。

## 対象範囲

### 英語化対象 (現状は日本語)

- `examples/fmp4.rs` の `println!` 全 13 箇所 (行 152-254)
- `src/demux_mp4_file.rs:26,28,34` の doc コメント内 `println!` サンプル
- `src/demux_fmp4_file.rs:33,36` の doc コメント内 `println!` サンプル
- `src/demux.rs:24,26` の doc コメント内 `println!` サンプル
- `src/demux_fmp4_segment.rs:26` の doc コメント内 `println!` サンプル

### そのまま維持 (既に英語)

- `examples/demux.rs` の `println!` 全 13 箇所 (行 50-80)
- `examples/demux.rs` の `eprintln!` 全 4 箇所 (行 13, 19, 45, 86)
- `src/demux_mp4_file_kind_detector.rs:32-33` の doc コメント内 `println!` サンプル

### 対象外 (別カテゴリの規約に従うので触らない)

- コメント本文 (`//`、`///`、`//!` の説明文): CLAUDE.md「コメントは全て日本語にすること」に従い日本語のまま維持する。
- テストの assertion メッセージ (`.expect()` / `assert!` / `prop_assert!`): CLAUDE.md「テストのログメッセージは全て日本語にすること」に従い日本語のまま維持する (issue 0037 で別途対応)。

## 完了条件

- `examples/fmp4.rs` の `println!` すべてが英語になっていること。
- `src/demux_mp4_file.rs` / `src/demux_fmp4_file.rs` / `src/demux.rs` / `src/demux_fmp4_segment.rs` の doc コメント内 `println!` サンプルが英語になっていること。
- コメント本文 (`//`、`///`、`//!` の説明文) は日本語のまま維持されていること。
- `cargo run --example demux <file>` と `cargo run --example fmp4` が正常に完了すること。
- `cargo test --doc` が通ること。
- `grep -rEn 'println!' examples/ src/` の出力を目視し、日本語文字を含む `println!` が残っていないことを確認すること。

## 解決方法

`examples/demux.rs` のラベルスタイル (`Track {}:`、`Track ID: {}`、`Timestamp: {}`、`Data size: {} bytes` など) を参考に英語化する。

例:

```rust
// examples/fmp4.rs
println!("Init segment: {} bytes", init_segment.len());
println!("Media segment {}: {} bytes", seg_idx + 1, segment.len());
println!("\nTracks: {}", tracks.len());
println!(
    "  track_id={}, kind={:?}, timescale={}",
    track.track_id, track.kind, track.timescale
);
println!("\nSamples:");
println!("  Segment {}:", i + 1);
println!(
    "    track_id={}, timestamp={}, duration={}, keyframe={}, size={}",
    sample.track.track_id,
    sample.timestamp,
    sample.duration,
    sample.keyframe,
    sample.data_size
);
println!("  Segment with sidx:");
println!("\nmfra box: {} bytes", mfra.len());
println!("mfro.size = {mfro_size} (matches total mfra size)");
println!("\nOK: fMP4 mux/demux completed successfully");
```

```rust
// src/demux_mp4_file.rs などの doc コメント内サンプル
//! println!("Found {} track(s)", tracks.len());
//!     println!(
//!         "Track ID: {}, kind: {:?}, duration: {}, timescale: {}",
//!         track.track_id, track.kind, track.duration, track.timescale
//!     );
//!     println!(
//!         "Sample - Track ID: {}, timestamp: {}, size: {} bytes",
//!         sample.track.track_id, sample.timestamp, sample.data_size
//!     );
```
