# `Mp4FileMuxer` が全サンプル非キーフレームのトラックでエントリー 0 個の `stss` を出力する

- Priority: Medium
- Created: 2026-07-27
- Completed: YYYY-MM-DD
- Model: Opus 5
- Branch: feature/fix-empty-stss-box
- Polished: 2026-07-31

## 目的

`Mp4FileMuxer` が、トラック内の全サンプルの `keyframe` が `false` のときにエントリー 0 個の `stss` を出力する問題を修正する。

ISO/IEC 14496-12 では `stss` の不在と `entry_count = 0` は意味が真逆であり、前者は「全サンプルが同期サンプル」、後者は「同期サンプルが 1 つも存在しない」を表す。現状の出力は後者を宣言してしまっており、当該トラックはランダムアクセス不能として扱われる。

## 優先度根拠

Medium。全サンプルを `keyframe = false` にすると空の `stss` が必ず出る機械的バグであり、自 crate のアクセサではそのトラックをランダムアクセス不能と解釈する。サンプルデータ自体は壊れず、実プレイヤーでの再生被害は未計測のため High とはしない。

音声の正しい典型は `keyframe = true` である（`crates/c-api/src/mux.rs` のコメント「音声では通常は常に true」、`examples/fmp4.rs` の音声投入）。`true` なら既存実装でも `stss` は既に省略される。空 `stss` の再現源は、主にテストが音声を `keyframe = false` で投入している書き癖と、同様の誤用である。

## 現状

`src/mux_mp4_file.rs` の `Mp4FileMuxer::build_stbl_box`:

```rust
let is_all_keyframe = chunks.iter().all(|c| c.samples.iter().all(|s| s.keyframe));
let stss_box = if is_all_keyframe {
    None
} else {
    Some(StssBox { sample_numbers: /* keyframe = true のものだけ集める */ })
};
```

全サンプルが `keyframe = false` の場合、`is_all_keyframe` が `false` になり、`sample_numbers` が空の `StssBox` が生成される。空の `stss` はフォーマット不正ではなく、「同期サンプルが 1 つも存在しない」という ISO 上有効な宣言である。音声・字幕では意図（全サンプル同期）と逆の意味になる。

自 crate のアクセサはこれを「同期サンプルなし」と解釈する。

- `src/auxiliary.rs` の `SampleAccessor::is_sync_sample()` — `stss` が無ければ全て同期サンプル扱いで `true` を返すが、空の `stss` があると `binary_search` が失敗して `false` を返す
- `src/auxiliary.rs` の `SampleAccessor::sync_sample()` — `Err(0)` 経路で `None` を返すため、前方の同期サンプルが永久に見つからない

### 既存テストが生成している MP4 でも発生している

以下はいずれも音声サンプルを `keyframe = false` で投入しており、生成される音声トラックが実際に 0 件 `stss` を持つ。どのテストも `stss` を検証していないため気付かれていない。正しい典型（`keyframe = true`）ではない。

- `src/mux_mp4_file.rs` の `test_audio_and_video_tracks`
- `pbt/tests/prop_mux_demux.rs` の `mux_demux_audio_only_roundtrip` / `mux_demux_video_audio_subtitle_roundtrip` ほか

### 字幕トラックでも踏みやすい

`docs/subtitle.md` は字幕サンプルを「前後に依存しない独立サンプルとして扱うのが通例」と説明したうえで `keyframe: true` を推奨しているが、守らないと「同期サンプルなし」を意味する空 `stss` が出る。

## 設計方針

API 契約を次で固定する。音声・字幕の正規入力は `keyframe = true`（C API / examples / `docs/subtitle.md` どおり）。全サンプル `false` は誤用パスであり、muxer は空 `stss` を出さないようにする。

いずれかを選ぶ。呼び出し元の `TrackEntry` は `track_kind` を持つため、`build_stbl_box` に種別を渡して分岐できる。

1. **種別分岐**: `TrackKind::Audio` / `TrackKind::Subtitle` で `sample_numbers` が空になる場合は `stss` 自体を省略する（= 全サンプル同期扱い）。`TrackKind::Video` で空になる場合は `MuxError` で拒否する。音声・字幕の誤用は意図どおりに救済し、映像の「同期なし」宣言を「全同期」へ黙って付け替えない
2. **一律拒否**: 空 `stss` になる入力を常に `MuxError` で拒否する。`CHANGES.md` の `append_sample` の `u32` 超過エラー化や 0032（`saturating_add` → 明示エラー）と同型の「暗黙の壊れた出力より明示エラー」方針と整合する。既に受け入れている入力（主にテストの誤用）をエラーにするため後方互換はない
3. **一律省略**: `sample_numbers` が空なら種別を問わず `stss` を省略する。影響範囲は小さいが、全サンプル非キーの映像トラックまで「全同期」と宣言する意味の逆転が起きる。採用するならその意図を完了条件に明示する

推奨は 1。2 は方針の一貫性が高いが既存の誤用入力を一括でエラー化する。3 は映像での意味逆転があるため、意図を明示しない限り採らない。旧案「Subtitle だけ常に `stss` 省略」は音声が残るため単独では不足する。

`Fmp4SegmentMuxer` 側は `stss` を持たない（`trun` の `SampleFlags` で表現する）ため、本 issue の対象は `Mp4FileMuxer` のみ。

## 完了条件

- 全サンプルが `keyframe = false` のトラックで、エントリー 0 個の `stss` が出力されなくなること（設計方針 1 なら音声・字幕は省略、映像はエラー。2 なら全種別エラー。3 なら全種別省略）
- 上記を検証するテストが追加されていること（音声トラックと字幕トラックの両方。設計方針 1 または 2 を採る場合は、映像トラックで空 `stss` 相当の入力がエラーになることも検証する）
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が通ること
- `Sample::keyframe` の doc に、指定した値が `stss` の生成にどう影響するか、および音声・字幕では `true` が正規であることが記載されていること
