# `Mp4FileMuxer` が全サンプル非キーフレームのトラックでエントリー 0 個の `stss` を出力する

- Priority: Medium
- Created: 2026-07-27
- Completed: YYYY-MM-DD
- Model: Opus 5
- Branch: feature/fix-empty-stss-box
- Polished: YYYY-MM-DD

## 目的

`Mp4FileMuxer` が、トラック内の全サンプルの `keyframe` が `false` のときにエントリー 0 個の `stss` を出力する問題を修正する。

ISO/IEC 14496-12 では `stss` の不在と `entry_count = 0` は意味が真逆であり、前者は「全サンプルが同期サンプル」、後者は「同期サンプルが 1 つも存在しない」を表す。現状の出力は後者を宣言してしまっており、当該トラックはランダムアクセス不能として扱われる。

## 優先度根拠

Medium。音声トラックのサンプルを `keyframe = false` で投入するのは典型的な使い方であり、その場合に常に発生する。生成物が仕様上不正な状態になるが、サンプルデータ自体は壊れず、実プレイヤーでの再生被害は未計測のため High とはしない。

なお同種の「暗黙のデータ破壊よりも明示的な正しさを優先する」修正方針は、`CHANGES.md` の `append_sample` の `u32` オーバーフロー対応や 0032 と整合する。

## 現状

`src/mux_mp4_file.rs:1067-1082` の `build_stbl_box()`:

```rust
let is_all_keyframe = chunks.iter().all(|c| c.samples.iter().all(|s| s.keyframe));
let stss_box = if is_all_keyframe {
    None
} else {
    Some(StssBox { sample_numbers: /* keyframe = true のものだけ集める */ })
};
```

全サンプルが `keyframe = false` の場合、`is_all_keyframe` が `false` になり、`sample_numbers` が空の `StssBox` が生成される。

自 crate のアクセサはこれを「同期サンプルなし」と解釈する。

- `src/auxiliary.rs:400-407` `SampleAccessor::is_sync_sample()` — `stss` が無ければ全て同期サンプル扱いで `true` を返すが、空の `stss` があると `binary_search` が失敗して `false` を返す
- `src/auxiliary.rs:413-427` `SampleAccessor::sync_sample()` — `Err(0)` 経路で `None` を返すため、前方の同期サンプルが永久に見つからない

### 既存テストが生成している MP4 でも発生している

以下はいずれも音声サンプルを `keyframe = false` で投入しており、生成される音声トラックが実際に 0 件 `stss` を持つ。どのテストも `stss` を検証していないため気付かれていない。

- `src/mux_mp4_file.rs` の `test_audio_and_video_tracks`
- `pbt/tests/prop_mux_demux.rs` の `mux_demux_audio_only_roundtrip` / `mux_demux_video_audio_subtitle_roundtrip` ほか

### 字幕トラックでも踏みやすい

`docs/subtitle.md` は字幕サンプルを「前後に依存しない独立サンプルとして扱うのが通例」と説明したうえで `keyframe: true` を推奨しているが、これは推奨ではなく守らないと不正な `stss` が出る必須条件になっている。

## 設計方針

いずれかを選ぶ。

1. `sample_numbers` が空になる場合は `stss` 自体を出力しない（= 全サンプル同期扱い）。音声・字幕にとってはこれが正しい
2. `TrackKind::Subtitle` では `sample.keyframe` を無視して常に `stss` を省略する
3. 空 `stss` になる入力を `MuxError` で拒否する

1 が最も影響範囲が小さく、既存の出力が壊れる方向にも働かない。2 は字幕以外の音声トラックで問題が残るため単独では不足する。3 は既に受け入れている入力をエラーにするため後方互換のない変更になる。

`Fmp4SegmentMuxer` 側は `stss` を持たない（`trun` の `SampleFlags` で表現する）ため、本 issue の対象は `Mp4FileMuxer` のみ。

## 完了条件

- 全サンプルが `keyframe = false` のトラックで、エントリー 0 個の `stss` が出力されなくなること
- 上記を検証するテストが追加されていること（音声トラックと字幕トラックの両方）
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が通ること
- `Sample::keyframe` の doc に、指定した値が `stss` の生成にどう影響するかが記載されていること
