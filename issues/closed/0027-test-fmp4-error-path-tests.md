# fMP4 の主要エラーパス（EmptyTracks / EmptySamples / MixedSampleEntries / InvalidState）にテストが欠落している

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-30
- Model: opencode-go glm-5.2
- Branch: feature/add-fmp4-error-path-tests
- Polished: 2026-07-30

## 目的

fMP4 の mux / demux で公開 API が返す主要なエラーバリアント（`MuxError::EmptyTracks` / `MuxError::EmptySamples` / `MuxError::MixedSampleEntries` / `DemuxError::InvalidState`）に、発生を期待するテストが一切なく、回帰検知できない状態を解消する。

## 優先度根拠

これらは公開 API のドキュメントに明記されたエラー契約であり、正常系 roundtrip テストだけでは検証されない。意図的なエラーパスは `shiguredo-rust` の単体テスト役割であり、同種の先例として `tests/test_boxes_fmp4.rs` がある。`MuxError::MissingSampleEntry` は `pbt/tests/prop_fmp4_segment_mux_demux.rs` の `sidx_rejects_missing_sample_entry_on_first_sample` で fMP4 側も検証済みだが、本 issue 対象の 4 バリアントは未検証のまま残っている。エラー経路の回帰を防ぐため。

## 現状

### EmptyTracks

`Fmp4SegmentMuxer::init_segment_bytes` は `self.tracks` が空のとき `MuxError::EmptyTracks` を返す。

```rust
pub fn init_segment_bytes(&self) -> Result<Vec<u8>, MuxError> {
    if self.tracks.is_empty() {
        return Err(MuxError::EmptyTracks);
    }
```

`EmptyTracks` を期待するテストは `pbt/tests` / `tests/` / `src` 内 unit test のいずれにも 0 件。成功系の `init_segment_bytes().expect(...)` のみ。

### EmptySamples

`Fmp4SegmentMuxer::create_media_segment_metadata` は内部の `build_media_segment_bytes` 経由で、`Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` は自身の先頭で、`samples` が空のとき `MuxError::EmptySamples` を返す。

```rust
// create_media_segment_metadata_with_sidx / build_media_segment_bytes の双方
if samples.is_empty() {
    return Err(MuxError::EmptySamples);
}
```

`EmptySamples` を期待するテストは 0 件。

### MixedSampleEntries

`resolve_segment_tracks` は、同一セグメント・同一トラックで異なる sample entry index が混在すると `MuxError::MixedSampleEntries` を返す。

```rust
if let Some(expected_index) = segment_sample_entry_index {
    if expected_index != sample_entry_index {
        return Err(MuxError::MixedSampleEntries { track_kind });
    }
}
```

`MixedSampleEntries` を期待するテストは 0 件。セグメント跨ぎの sample entry 切替は検証済みだが、同一セグメント内混在の拒否は未検証。

### InvalidState

`Fmp4SegmentDemuxer` は次の 3 経路で `DemuxError::InvalidState` を返す。いずれも期待テストは 0 件。

1. `handle_init_segment`: 二重呼び出し（`"Init segment has already been processed"`）
2. `tracks`: init 前（`"Init segment has not been processed yet"`）
3. `handle_media_segment`: init 前（`"Init segment has not been processed yet"`）

なお 3 は `moof` + `mdat` の構文解析が成功したあとに初めて返る。空バイト列では `DecodeError("empty media segment")` になり、`InvalidState` には到達しない。

## 設計方針

- 追加先は次の単体テストファイル（いずれも新規）とする。意図的なエラーパスは `shiguredo-rust` の「単体テスト」役割であり、固定入力でエラー契約を検証するため PBT には置かない
  - mux 側（`EmptyTracks` / `EmptySamples` / `MixedSampleEntries`）: `tests/test_mux_fmp4_segment.rs`
  - demux 側（`InvalidState` 3 経路）: `tests/test_demux_fmp4_segment.rs`
- `pbt/tests/prop_error_paths.rs` は `issues/closed/0003-refactor-split-prop-error-paths.md` で削除済みであり、再作成しない
- モック・スタブは使わず、実際の `Fmp4SegmentMuxer` / `Fmp4SegmentDemuxer` 操作で発生させる
- `handle_media_segment` の init 前 `InvalidState` は、`moof` + `mdat` の構文解析が成功したあとでしか返らない。空バイト列や不正バイト列では `DecodeError` になるため、別の `Fmp4SegmentMuxer` で正当なメディアセグメントを組み立ててから、未初期化の demuxer に渡す
- 二重 `handle_init_segment` も、正当な init セグメント（別 muxer でサンプル投入後に `init_segment_bytes()`）を用意してから検証する
- 各テストで `matches!(result, Err(...))` によりバリアントを assert する

## 完了条件

- `MuxError::EmptyTracks` が返る経路のテストがあること
- `MuxError::EmptySamples` が返る経路のテストがあること
- `MuxError::EmptySamples` は `create_media_segment_metadata(&[])` で検証すること（`build_media_segment_bytes` 経由の公開経路）
- `MuxError::MixedSampleEntries` が返る経路のテストがあること
- `DemuxError::InvalidState` が返る 3 経路すべてのテストがあること
- 追加先が `tests/test_mux_fmp4_segment.rs` と `tests/test_demux_fmp4_segment.rs` であること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

以下のテストを追加した。

- `tests/test_mux_fmp4_segment.rs`
  - `empty_tracks_on_init_segment_bytes`: サンプル未投入の muxer で `init_segment_bytes()` を呼び `Err(MuxError::EmptyTracks)` を検証
  - `empty_samples_on_create_media_segment`: 空 `&[]` で `create_media_segment_metadata()` を呼び `Err(MuxError::EmptySamples)` を検証（`build_media_segment_bytes` 経由の公開経路）
  - `empty_samples_on_create_media_segment_with_sidx`: 空 `&[]` で `create_media_segment_metadata_with_sidx()` を呼び `Err(MuxError::EmptySamples)` を検証。`_with_sidx` は自身の入口に独立した早期リターンを持つため、`create_media_segment_metadata` とは別経路として個別に検証する
  - `mixed_sample_entries_in_segment`: 同一セグメント・同一 Video トラックへ幅違いの `Avc1` を並べて `Err(MuxError::MixedSampleEntries { track_kind: TrackKind::Video })` を検証
- `tests/test_demux_fmp4_segment.rs`
  - `invalid_state_double_init`: 別 muxer で得た正当な init を `handle_init_segment` に 2 回渡し `Err(DemuxError::InvalidState)` を検証
  - `invalid_state_tracks_before_init`: 未初期化 demuxer で `tracks()` を呼び `Err(DemuxError::InvalidState)` を検証
  - `invalid_state_media_before_init`: 別 muxer で得た正当な `moof` + `mdat` メディアセグメントを、未初期化 demuxer の `handle_media_segment` に渡し `Err(DemuxError::InvalidState)` を検証（空・不正バイト列では `DecodeError` になるため、`moof` + `mdat` 構文解析が成功した後の `InvalidState` 経路に到達させる必要がある）

各テストは `matches!(result, Err(variant))` でバリアントを assert する。`MixedSampleEntries` は `..` を使わず `track_kind: TrackKind::Video` まで含めて完全一致で検証する。モック・スタブは使わず、実際の `Fmp4SegmentMuxer` / `Fmp4SegmentDemuxer` の公開 API のみで発生させる。
