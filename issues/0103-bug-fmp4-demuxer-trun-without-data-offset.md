# `Fmp4SegmentDemuxer::handle_media_segment` が `data_offset` のない 2 つ目以降の `trun` を `traf` の基準位置から読む

- Created: 2026-09-25
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-demuxer-trun-without-data-offset
- Polished: 2026-09-25

## 目的

`trun` に `data_offset` がないときのデータの開始位置を、ISO/IEC 14496-12:2022 の 8.8.8.1 に合わせる。

8.8.8.1 では、`data_offset` がない run のデータは、直前の run のデータの直後から始まる（`traf` の最初の run なら、`tfhd` で決まる base-data-offset から始まる）。今の実装は、2 つ目以降の run も `traf` の基準位置から読むため、エラーにならずに誤ったバイト列をサンプルデータとして返す。

## 現状

- `src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment` は、各 `trun` のデータの開始位置を、`traf` の基準位置（`base_data_offset`）に `trun.data_offset.unwrap_or(0)` を足して求める。`data_offset` がない `trun` は、何番目の run でも `traf` の基準位置から始まる
- `Fmp4FileDemuxer` も、内部で `Fmp4SegmentDemuxer` を使うため同じ動きになる
- `Fmp4SegmentMuxer` は、`traf` ごとに `data_offset` 付きの `trun` を 1 つだけ出力するため、自身の出力では起きない。ほかの実装が出力したファイルで起きる

再現手順（develop で確認）:

1. `Fmp4SegmentMuxer` で、映像 1 トラック、2 サンプル（ペイロードは 16 バイトずつで、1 つ目を 0x11、2 つ目を 0x22 で埋める）の init セグメントとメディアセグメントを作る
2. メディアセグメントの `moof` の `trun` を、1 サンプルずつの 2 つの `trun` に分け、2 つ目の `trun` の `data_offset` を省く。1 つ目の `trun` の `data_offset` は `moof` の先頭からの相対値なので、`moof` が大きくなった分だけ足し直す
3. `handle_media_segment` に渡すと、2 つ目のサンプルの `data_offset` が 0（`moof` の先頭）になり、`moof` のバイト列をサンプルデータとして返す。8.8.8.1 どおりなら、1 つ目のサンプルのデータの直後（先頭が 0x22）を指す

## 設計方針

- 同じ `traf` の中で、`data_offset` のない `trun` のデータは、直前の `trun` のデータ末尾から始める
- `traf` の最初の `trun` に `data_offset` がない場合は、今と同じく `traf` の基準位置から始める
- `data_offset` がある `trun` は、今と同じく `traf` の基準位置に `data_offset` を足した位置から始める（8.8.8.1 と 8.8.8.3）
- 関連 issue
  - issue 0098（pending）は、`traf` の基準位置の規則を扱う。この issue は `traf` の中の `trun` の並びを扱うもので、0098 の結論に依存しない
  - issue 0096 / 0100 は、同じ `traf` / `trun` のループを変える。0096 が先に入っていれば、この issue の変更でも「エラーを返した場合は内部状態を変更しない」ことを保つ

## 完了条件

- `data_offset` のない 2 つ目以降の `trun` のサンプルが、直前の `trun` のデータの直後から取り出される
- `data_offset` のある `trun` と、`traf` の最初の `trun` の扱いは変わらない

## 解決方法

- `src/demux_fmp4_segment.rs` の `handle_media_segment` を変更する
- テスト
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: muxer の出力の `trun` を複数に分け、2 つ目以降の `trun` の `data_offset` を省いた入力で、分けない場合と比べて、`data_offset` 以外のフィールドと、`data_offset` / `data_size` が指すバイト列が一致することを確認するプロパティを追加する。`Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` の両方で確認する
- `CHANGES.md` に `[FIX]` として記載する
