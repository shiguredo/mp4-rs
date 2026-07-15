# mux_fmp4_segment.rs の sidx earliest_presentation_time が composition_time_offset を無視して DTS を使っている

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-sidx-ept-cto
- Polished: YYYY-MM-DD

## 目的

`Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` で sidx の `earliest_presentation_time` に `track.decode_time`（DTS）をそのまま入れており、`composition_time_offset` を加算していない問題を修正する。

## 優先度根拠

ISO/IEC 14496-12 で `earliest_presentation_time` は presentation time（PTS）であり、`DTS + composition_time_offset` の最小値であるべき。B フレーム等で CTO ≠ 0 のとき DTS のみでは不正確で、DASH 等のセグメント境界・シーク指標がずれる。負 CTO では EPT が過大になる。

## 現状

```rust
// src/mux_fmp4_segment.rs:255-259
let earliest_presentation_time = self
    .tracks
    .iter()
    .find(|track| track.track_kind == first_track_kind)
    .map_or(0, |track| track.decode_time);
```

`track.decode_time` は累積 DTS（`base_media_decode_time` 相当）。一方同 muxer は `composition_time_offset` を `trun` に書く（`src/mux_fmp4_segment.rs:736-751`）。`PTS = DTS + composition_time_offset` だが、sidx 側は CTO を一切参照していない。

## 設計方針

当該トラックの各サンプルについて `decode_time + Σ prior_duration + composition_time_offset` の最小値を EPT にする。CTO が無指定（`None`）の場合は 0 扱いで `decode_time` を使う。

## 完了条件

- CTO ≠ 0 のセグメントで `earliest_presentation_time` が正しい PTS 最小値になること
- CTO が全て `None` / 0 の場合は従来どおり `decode_time` と一致すること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `create_media_segment_metadata_with_sidx` で EPT 計算時にサンプルの CTO を考慮する
2. 当該トラックの先頭サンプル群の `DTS + CTO` の最小値を計算する
3. CTO 付きの sidx テストを追加する
