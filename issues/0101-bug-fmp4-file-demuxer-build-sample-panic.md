# `Fmp4FileDemuxer::build_sample` が `sample_entry` をキャッシュしていないトラックのサンプルで panic する

- Created: 2026-09-25
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-file-demuxer-build-sample-panic
- Polished: {YYYY-MM-DD}

## 目的

`Fmp4FileDemuxer::next_sample` が、入力の内容によってライブラリの中で panic しないようにする。

今は、同じトラックのサンプルの取り出し順が `traf` / `trun` の並び順と入れ替わる入力で、`sample_entry` をまだキャッシュしていないトラックのサンプルを取り出すと panic する。入力が不正でなくても起きる。

## 現状

`src/demux_fmp4_file.rs` の `Fmp4FileDemuxer`:

- `build_pending_samples` は、内部の `Fmp4SegmentDemuxer::handle_media_segment` が返したサンプルを、`compare_pending_samples` でタイムスタンプの順（同じなら `track_index`、`data_offset` の順）に並べ替えて `pending_samples` に入れる
- 内部の demuxer は、各トラックの最初のサンプルと、sample description index が変わったサンプルにだけ `sample_entry` を付ける。どのサンプルが「最初」かは `traf` / `trun` の並び順で決まる
- `build_sample` は、`has_sample_entry.then_some(track_runtime.sample_entry.as_ref().expect("bug: sample entry must be cached before borrowing"))` で `sample_entry` を返す。`then_some` は引数を先に評価するため、`sample_entry` が `None` のサンプルでも `expect` が実行される。そのトラックの `sample_entry` をまだキャッシュしていなければ panic する

このため、並べ替えの結果、`sample_entry` が `None` のサンプルが、同じトラックの `sample_entry` 付きのサンプルより先に来ると panic する。

再現手順（develop で確認）:

1. `Fmp4SegmentMuxer` で、映像 1 トラック、1 サンプルの init セグメントとメディアセグメントを作る
2. メディアセグメントの `moof` の `traf` を複製して 2 つにし、1 つ目の `traf` の `tfdt` を 90000、2 つ目の `traf` の `tfdt` を 0 にする。`trun` の `data_offset` は `moof` の先頭からの相対値なので、`moof` が大きくなった分だけ足し直す
3. init セグメントとメディアセグメントを連結したファイルを、要求された範囲だけ `Fmp4FileDemuxer` に渡して `next_sample()` を呼ぶと、`build_sample` で `bug: sample entry must be cached before borrowing` の panic が起きる

issue 0096 の経路（内部の demuxer がエラーを返した後、再試行で最初のサンプルの `sample_entry` を `None` で返す）でも同じ panic に至る。0096 はその原因（エラー時の状態の更新）を直すもので、この `build_sample` の問題は残る。

## 設計方針

- `build_sample` は、`sample_entry` を持つサンプルのときだけキャッシュを参照する。`sample_entry` が `None` のサンプルでキャッシュを参照しない
- `Sample::sample_entry` の「各トラックの最初のサンプル、または sample description index が変わったサンプルでのみ `Some`」という約束を、`Fmp4FileDemuxer` の取り出し順で守る
  - `build_pending_samples` で、内部の demuxer が返した順（`traf` / `trun` の並び順）にサンプルをたどり、各サンプルが属するサンプルエントリーを求める（`sample_entry` が `None` のサンプルは、同じトラックの直前のサンプルと同じエントリーに属する。そのメディアセグメントで最初のサンプルなら、前のメディアセグメントまでにキャッシュしたエントリーに属する）
  - 並べ替えた後の取り出し順で、トラックごとにエントリーが最初に現れたサンプルと、エントリーが変わったサンプルにだけ `sample_entry` を付ける

## 完了条件

- 並べ替えで同じトラックのサンプルの順序が `traf` / `trun` の並び順と入れ替わる入力でも、`next_sample()` が panic しない
- 取り出し順で、各トラックの最初のサンプルと、サンプルエントリーが変わったサンプルでだけ `sample_entry` が `Some` になる

## 解決方法

- `src/demux_fmp4_file.rs` の `PendingSample`、`build_pending_samples`、`build_sample` を変更する
- テスト
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: muxer の出力の `moof` を書き換え、同じトラックの `traf` を分けて `tfdt` の順を入れ替えた入力で、`Fmp4FileDemuxer` が panic せず、取り出し順で上記の約束を満たすことを確認するプロパティを追加する
- `CHANGES.md` に `[FIX]` として記載する
