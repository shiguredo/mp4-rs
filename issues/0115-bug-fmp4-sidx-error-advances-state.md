# `create_media_segment_metadata_with_sidx` が失敗したあとにシーケンス番号と DTS が進んでいる

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-sidx-error-advances-state
- Polished: 2026-10-06

## 目的

`Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` がエラーを返したとき、muxer の内部状態を呼び出し前のままにする。同じ入力の再試行で、シーケンス番号とデコード時刻が二重に進まないようにする。

## 現状

`src/mux_fmp4_segment.rs` の `create_media_segment_metadata_with_sidx` の doc は、PTS が負、または PTS か累積 DTS が `u64` に収まらないときに `MuxError::Overflow` を返し、そのエラーでは内部状態を変えない、と書いている。この検査は `compute_earliest_presentation_time` にあり、`build_media_segment_bytes` より前である。

その後 `build_media_segment_bytes` が成功すると、`sequence_number`、各トラックの `decode_time`、`tfra_entries`、`media_bytes_written` を更新してから戻る。呼び出し側は、そのあとで次を `u32` に変換する。

- メディアセグメントのバイト長から作る `referenced_size`
- 参照トラックの duration の和である `subsegment_duration`

どちらかが `u32` に収まらないとエラーを返す。バイト列は呼び出し側に渡らない。状態は更新済みである。同じサンプル列でもう一度呼ぶと、シーケンス番号と DTS はもう一度進む。

`referenced_size` が `u32` に収まらないのは、メディアセグメントのバイト長が `u32::MAX` を超えるとき（4 GiB、つまり 2^32 バイト以上）である。`subsegment_duration` が `u32` を超えるのは、参照トラックの duration の和が `u32::MAX` を超えるときである。

duration が `u32::MAX` のサンプルを 2 つ渡すと、バイト列は返らずエラーになる。その直後に duration 10 のサンプルを 1 つ渡すと成功し、その `moof` の `mfhd.sequence_number` は 2、`tfdt.base_media_decode_time` は 8589934590（`u32::MAX` の 2 倍）になる。失敗した呼び出しの duration とシーケンス番号が残っている。

## 設計方針

- `u32` に収まると分かってから `build_media_segment_bytes` を呼ぶ。duration の和は呼ぶ前に計算できる。セグメントのバイト長は、状態を更新する前に分かるように `build_media_segment_bytes` から分離するか、失敗時に更新前の値へ戻す
- 戻す場合に戻すのは、このメソッドが変える `sequence_number`、`decode_time`、`tfra_entries`、`media_bytes_written`、サンプルエントリの蓄積である。どれをどの関数が更新しているかをコードで確認してから戻す
- PTS 検査で状態が変わらないことは維持する

## 完了条件

- `subsegment_duration` が `u32` に収まらない入力でエラーを返したあと、同じ muxer の `sequence_number` と各トラックのデコード時刻が、呼び出し前と一致する。同じ入力でもう一度呼んでも、エラーであり、時刻はさらに進まない
- `referenced_size` が `u32` に収まらない入力（メディアセグメント長が `u32::MAX` を超える。`data_size` が `u32::MAX` のサンプル 1 つで再現できる）でエラーを返したあとでも、同じ muxer の `sequence_number` と各トラックのデコード時刻が、呼び出し前と一致する。同じ入力でもう一度呼んでも、エラーであり、時刻はさらに進まない
- PTS が負の入力でエラーを返したときの状態は、今どおり呼び出し前と一致する
- 成功した呼び出しのバイト列と、その後のデコード時刻は今と変わらない
