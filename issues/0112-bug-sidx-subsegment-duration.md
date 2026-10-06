# `Fmp4SegmentMuxer` の `sidx.subsegment_duration` がサンプル duration の和になっている

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-sidx-subsegment-duration
- Polished: 2026-10-06

## 目的

メディアを参照する `sidx` の `subsegment_duration` を、このセグメントの直後の DTS から、このサブセグメントの earliest presentation time を引いた値にする。EPT が先頭 DTS より後ろなら、今の duration の和より短くなる。

## 現状

ISO/IEC 14496-12:2022 の 8.16.3.3 では、メディアへの参照の `subsegment_duration` は、次に挙げる時刻から、このサブセグメントの earliest presentation time を引いた値である。次に挙げる時刻は、次サブセグメントの earliest presentation time、このセグメントの最後のサブセグメントなら次セグメントの先頭サブセグメントの earliest presentation time、ストリームの最後なら参照ストリームの終端の presentation time、である。終端の presentation time 自体が差の左に来る。それを earliest presentation time と呼び替えてはいない。

`src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` は、参照トラックの `Sample::duration` を `u64` で合計した値を `subsegment_duration` に書く。`earliest_presentation_time` は `compute_earliest_presentation_time` で PTS の最小値になっている。

このセグメントの CTO がすべて 0 なら、この EPT は先頭 DTS になる。次の EPT が直後の DTS と一致するとき、和と差は一致する。次セグメントの先頭サンプルの CTO が 0 で、かつそのサブセグメント内にそれより早い PTS が無ければ、その一致が起きる。一致しない例（参照トラックだけ、セグメント先頭の累積 DTS は 0）:

| サンプル | duration | DTS | CTO | PTS |
| --- | --- | --- | --- | --- |
| 0 | 100 | 0 | +50 | 50 |
| 1 | 100 | 100 | -80 | 20 |

EPT は 20。サンプル duration の和は 200。次サブセグメントの先頭サンプルの DTS は 200 であり、その CTO が 0 で、かつ次サブセグメント内にそれより早い PTS が無ければ、次の EPT は 200 なので、仕様上の duration は 180 である。実装は 200 を書く。後続サンプルがより早い PTS を持つ場合は、書く時点では次の EPT が決まらない。

このメソッドは、次のセグメントのサンプルをまだ受け取っていない。次セグメント側の CTO で EPT が「次サンプルの DTS」より前になる場合は、書く時点では仕様の差を計算できない。

## 設計方針

- `subsegment_duration` を「このセグメントの直後の DTS」から「このセグメントの EPT」を引いた値にする。直後の DTS は、今の累積 DTS に参照トラックの duration の和を足した値である
- 差が負、または `u32` に収まらないときはエラーにする。今の実装がエラーにするのは duration の和が `u32` を超えるときだけで、EPT が直後の DTS より後ろでも和を書いてしまう
- 次セグメントの EPT が直後の DTS と違う場合は、この値が 8.16.3.3 の差と一致しない。その限界を `create_media_segment_metadata_with_sidx` の doc に書く
- 8.16.3.3 は、ストリーム最後のサブセグメントでは終端の presentation time との差を書く。終端の presentation time が直後の DTS と同じだとは、この節には書かれていない。この呼び出しが最後かどうかは書く時点では分からないので、次のセグメントが続くものとして直後の DTS を使う
- `earliest_presentation_time` の計算は変えない

## 完了条件

- 上の表の入力で、`subsegment_duration` が 180 になる
- 全サンプルの CTO が 0 のとき、`subsegment_duration` は duration の和のままである
- doc に、次セグメント自身の CTO は含めないことが書いてある
