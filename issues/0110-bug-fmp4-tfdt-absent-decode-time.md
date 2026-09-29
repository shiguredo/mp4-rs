# `Fmp4SegmentDemuxer` が `tfdt` の無いフラグメントの先頭 DTS を 0 にする

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-tfdt-absent-decode-time
- Polished: {YYYY-MM-DD}

## 目的

`tfdt` が無いトラックフラグメントの先頭サンプルの DTS を、それより前のサンプルの decode duration の和にする。2 個目以降のフラグメントで時刻が 0 に戻らないようにする。

## 現状

ISO/IEC 14496-12:2022 の 8.8.12.1 では、`tfdt` は必須ではない。ボックスがあるとき、その値はトラックフラグメントのデコード順で最初のサンプルの絶対デコード時刻であり、先行サンプルの duration を全部足さなくてよい、とある。8.8.12.3 の `baseMediaDecodeTime` は、それより前の全サンプルの decode duration の和（このフラグメントで足す分は含まない）である。

`src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment` は、`traf.tfdt_box` が無いとき `base_media_decode_time` を 0 にする。トラックごとの累積 duration は、呼び出しをまたいで持たない。先行フラグメントにサンプルがあるトラックでは、`tfdt` の無い次のフラグメントの `Sample::timestamp` が 0 から再開する。

`src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::build_moof` は常に `tfdt` を書く。この muxer の出力を同じ demuxer で読む往復では、この経路に入らない。

最初のフラグメントで、先行サンプルが無く `tfdt` も無いときは、和は 0 なので今の結果と一致する。

## 設計方針

- トラックごとに、これまでに見たサンプルの duration の和を demuxer が持つ
- `tfdt` があるフラグメントは、その `base_media_decode_time` を先頭 DTS にし、処理後の累積を「その値 + このフラグメントの duration の和」で更新する
- `tfdt` が無いフラグメントは、保持している累積を先頭 DTS にする
- エラーで戻る呼び出しでは、この累積を更新しない（`handle_media_segment` がエラーのとき他の内部状態を変えない既存の契約に合わせる）
- 既に返したサンプルの `duration` は書き戻さない。8.8.12.1 は、後続の `tfdt` が先行サンプルの duration の和を超えるとき、直前サンプルの尺を、和がその `tfdt` と一致するまで延ばす。この延長は、この issue の対象外とする

## 完了条件

- 同一トラックで、1 個目のフラグメントに `tfdt` があり、2 個目に `tfdt` が無いとき、2 個目の先頭 `timestamp` が 1 個目の duration の和と一致する
- 両方に `tfdt` があるときの `timestamp` は今と変わらない
- 先行サンプルが無いトラックの、`tfdt` の無い最初のフラグメントの先頭 `timestamp` は 0 のままである
- エラーを返した呼び出しのあと、同じ入力を再試行したときの `timestamp` が、エラーが無かった場合と一致する
