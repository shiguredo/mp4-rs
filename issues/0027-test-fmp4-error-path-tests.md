# fMP4 の主要エラーパス（EmptyTracks / EmptySamples / MixedSampleEntries / InvalidState）にテストが欠落している

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/add-fmp4-error-path-tests
- Polished: YYYY-MM-DD

## 目的

fMP4 の mux / demux で公開 API が返す主要なエラーバリアント（`MuxError::EmptyTracks` / `MuxError::EmptySamples` / `MuxError::MixedSampleEntries` / `DemuxError::InvalidState`）に、発生を期待するテストが一切なく、回帰検知できない状態を解消する。

## 優先度根拠

これらは公開 API のドキュメントに明記されたエラー契約であり、正常系 roundtrip テストだけでは検証されない。近傍エラー（`MissingSampleEntry` / `TimescaleMismatch` / `AlreadyFinalized`）は `Mp4FileMuxer` 側でテストがあるのに fMP4 側だけ抜けている。エラー経路の回帰を防ぐため。

## 現状

### EmptyTracks

```rust
// src/mux_fmp4_segment.rs:181-184
pub fn init_segment_bytes(&self) -> Result<Vec<u8>, MuxError> {
    if self.tracks.is_empty() {
        return Err(MuxError::EmptyTracks);
    }
```

`EmptyTracks` を期待するテストは `pbt/tests` / `tests/` / `src` 内 unit test のいずれにも 0 件。成功系の `init_segment_bytes().expect(...)` のみ。

### EmptySamples

```rust
// src/mux_fmp4_segment.rs:236-242, 307-309
if samples.is_empty() {
    return Err(MuxError::EmptySamples);
}
```

`EmptySamples` を期待するテストは 0 件。

### MixedSampleEntries

```rust
// src/mux_fmp4_segment.rs:857-860
if expected_index != sample_entry_index {
    return Err(MuxError::MixedSampleEntries { track_kind });
}
```

`MixedSampleEntries` を期待するテストは 0 件。セグメント跨ぎの sample entry 切替は検証済みだが、同一セグメント内混在の拒否は未検証。

### InvalidState

```rust
// src/demux_fmp4_segment.rs:95-97, 189-191, 288-290
return Err(DemuxError::InvalidState(
    "Init segment has already been processed",
));
```

`InvalidState` を期待するテストは 0 件。二重 `handle_init_segment` / init 前の `tracks()` / init 前の `handle_media_segment` の 3 経路すべて未検証。

## 設計方針

`pbt/tests/prop_fmp4_segment_mux_demux.rs` または `pbt/tests/prop_error_paths.rs` に各エラーバリアントを意図的に発生させて assert するテストを追加する。モック・スタブは使わず、実際の muxer / demuxer 操作で発生させる。

## 完了条件

- `MuxError::EmptyTracks` が返る経路のテストがあること
- `MuxError::EmptySamples` が返る経路のテストがあること
- `MuxError::MixedSampleEntries` が返る経路のテストがあること
- `DemuxError::InvalidState` が返る 3 経路すべてのテストがあること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `prop_fmp4_segment_mux_demux.rs` または `prop_error_paths.rs` に以下を追加:
   - `empty_tracks_on_init_segment_bytes`: サンプル未投入で `init_segment_bytes()` → `Err(MuxError::EmptyTracks)`
   - `empty_samples_on_create_media_segment`: 空 `&[]` で `create_media_segment_metadata()` → `Err(MuxError::EmptySamples)`
   - `mixed_sample_entries_in_segment`: 同一セグメント・同一トラックで異なる sample entry → `Err(MuxError::MixedSampleEntries)`
   - `invalid_state_double_init`: 二重 `handle_init_segment` → `Err(DemuxError::InvalidState)`
   - `invalid_state_tracks_before_init`: init 前に `tracks()` → `Err(DemuxError::InvalidState)`
   - `invalid_state_media_before_init`: init 前に `handle_media_segment()` → `Err(DemuxError::InvalidState)`
2. 各テストで `matches!(result, Err(variant))` でバリアントを assert する
