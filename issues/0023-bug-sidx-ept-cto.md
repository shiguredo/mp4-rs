# mux_fmp4_segment.rs の sidx earliest_presentation_time が composition_time_offset を無視して DTS を使っている

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-sidx-ept-cto
- Polished: 2026-07-30

## 目的

`Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` で sidx の `earliest_presentation_time` に `track.decode_time`（セグメント先頭の累積 DTS）をそのまま入れており、各サンプルの `composition_time_offset` を反映していない問題を修正する。

## 優先度根拠

ISO/IEC 14496-12 で `earliest_presentation_time` は presentation time（PTS）であり、当該参照トラックの各サンプルの PTS（`DTS + composition_time_offset`）の最小値であるべき。B フレーム等で CTO ≠ 0 のとき DTS のみでは不正確で、DASH 等のセグメント境界・シーク指標がずれる。負 CTO では現行実装の EPT が真の PTS 最小値より過大になる。

## 現状

```rust
// Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx 内
let earliest_presentation_time = self
    .tracks
    .iter()
    .find(|track| track.track_kind == first_track_kind)
    .map_or(0, |track| track.decode_time);
```

`TrackEntry::decode_time` は累積 DTS（`TfdtBox::base_media_decode_time` 相当）。一方同 muxer は `Fmp4SegmentMuxer::build_moof` で `composition_time_offset` を `TrunSample` に書く。`PTS = DTS + composition_time_offset` だが、sidx 側は CTO を一切参照していない。

EPT の算出は `build_media_segment_bytes` より前に行われ、この時点の `decode_time` は当該セグメント先頭の DTS である（セグメント内での `decode_time` 更新前）。

## 設計方針

参照トラックは現行どおり `samples[0].track_kind`（`first_track_kind` / `SidxBox::reference_id` と同一）とする。

当該トラックの各サンプル `i` について次を計算し、その最小値を EPT にする。

- `DTS_i = decode_time + Σ_{k < i} duration_k`（`decode_time` は当該トラックのセグメント先頭累積 DTS。トラック未登録なら 0）
- `CTO_i = composition_time_offset.unwrap_or(0)`（サンプル単位。`None` は 0）
- `PTS_i = DTS_i + CTO_i`
- `EPT = min_i PTS_i`

`None` の扱いを「セグメント全体を `decode_time` にフォールバック」と読まないこと。完了条件の「全て `None` / 0 なら `decode_time` と一致」は、全サンプルで `CTO_i = 0` のとき `min DTS_i = decode_time` になる帰結である。

`SidxBox::earliest_presentation_time` は `u64` である。`PTS_i` は符号付きで計算し、いずれかの `PTS_i < 0`、または `u64` へ収まらない場合は同 muxer の他の時刻加算と同様に `MuxError`（オーバーフロー扱い）を返す。黙って飽和やラップはしない。

## 完了条件

- CTO ≠ 0 のセグメントで `earliest_presentation_time` が上記公式の PTS 最小値になること（最小値が先頭サンプル以外でも正しいこと）
- CTO が全て `None` / 0 の場合は従来どおりセグメント先頭の `decode_time` と一致すること
- 負 CTO により PTS 最小値がセグメント先頭 DTS より小さくなるケースで、現行より正しい（小さい）EPT になること。PTS が負になる入力では `MuxError` になること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` で EPT を計算するとき、`first_track_kind` に属する全サンプルを走査する
2. 各サンプルについて `DTS_i = decode_time + Σ prior_duration`、`PTS_i = DTS_i + CTO_i`（`None`→0）を求め、`min PTS_i` を EPT にする。負 PTS / `u64` 変換失敗は `MuxError`
3. CTO 付き（先頭以外が最小になるケースと負 CTO を含む）の sidx テストを `tests/test_mux_fmp4_segment.rs` に追加する
