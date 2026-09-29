# `Fmp4SegmentMuxer` の `tfra.time` が同期サンプルの presentation time ではない

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-tfra-sync-sample-presentation-time
- Polished: {YYYY-MM-DD}

## 目的

`mfra` の `tfra` に、各セグメントの最初の同期サンプルの presentation time と、そのサンプルの位置を書く。同期サンプルが無いセグメントはエントリにしない。

## 現状

ISO/IEC 14496-12:2022 の 8.8.10.1 は、各エントリが同期サンプルの位置と presentation time だとしている。すべての同期サンプルを載せる必要はない。8.8.10.3 の `time` は、その同期サンプルの presentation time で、単位は該当トラックの media timescale である（通常の presentation time は movie timescale だが、`tfra` だけ media timescale）。`sample_delta` は、対象サンプル番号からその `trun` の先頭サンプル番号を引いて 1 を足した値である。

`src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::build_media_segment_bytes` は、セグメント先頭サンプルの累積 DTS（`TrackEntry::decode_time`）を `TfraSegmentEntry.time` に入れる。`keyframe` は見ない。`composition_time_offset` も足さない。`mfra_bytes` はその値を `TfraEntry.time` に写し、`sample_number` を常に 1 にする。

先頭が非キーフレームのセグメントも 1 件入る。先頭の CTO が 0 でないと、DTS が PTS として書かれる。同期サンプルが 2 個目以降にあるとき、`sample_number` の 1 はそのサンプルを指さない。

`TfraEntry::time` の doc は presentation time と書いている。内部で入れている値はデコード時刻である。

`moof_offset` を init セグメントのサイズ起点にする前提は、`mfra_bytes` の doc にある。この issue ではオフセットの起点は変えない。

## 設計方針

- 各トラックについて、そのセグメントのデコード順で最初の `keyframe == true` のサンプルを 1 エントリにする
- `time` は、そのサンプルの PTS（DTS + composition offset。offset が無いときは 0）とする。単位は今どおり media timescale とする
- `sample_number` は、この muxer が書く 1 個の `trun` の中での、そのサンプルの 1 始まり番号とする
- 同期サンプルが無いセグメントは、そのトラックのエントリを足さない
- PTS が負、または `u64` に収まらないときは、今の時刻計算と同じくエラーにする

## 完了条件

- 先頭が非キーフレームで、2 個目がキーフレームのセグメントでは、`tfra.time` が 2 個目の PTS になり、`sample_number` が 2 になる
- 先頭がキーフレームで CTO が 0 でないとき、`tfra.time` が先頭の PTS になる
- 同期サンプルが無いセグメントは、そのトラックの `tfra` エントリを増やさない
- `moof_offset` が init セグメントサイズからの相対であることは変わらない
