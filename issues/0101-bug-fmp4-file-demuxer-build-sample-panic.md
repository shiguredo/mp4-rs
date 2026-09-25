# `Fmp4FileDemuxer::build_sample` が `sample_entry` をキャッシュしていないトラックのサンプルで panic する

- Created: 2026-09-25
- Completed: 2026-09-25
- Branch: feature/fix-fmp4-file-demuxer-build-sample-panic
- Polished: 2026-09-25

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

issue 0096 の経路（内部の demuxer がエラーを返した後、再試行で最初のサンプルの `sample_entry` を `None` で返す）は、issue 0096 で内部状態を変更しないように直された（2026-09-25）。その経路では panic しなくなったため、ここで扱うのはエラーを経ない入力での panic だけである。

## 設計方針

- `build_sample` は、`sample_entry` を持つサンプルのときだけキャッシュを参照する。`sample_entry` が `None` のサンプルでキャッシュを参照しない
- `Sample::sample_entry` の約束を、`Fmp4FileDemuxer` の取り出し順で守る
  - `src/demux_mp4_file.rs` の `Sample::sample_entry` の doc は「前のサンプルから変更がない場合には `None` になる（最初のサンプルは常に `Some` となる）」としている。`src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment` の doc は同じ約束を「各トラックの最初のサンプル、または sample description index が変わったサンプルでのみ `Some`」としている
  - ここでの「最初のサンプル」「前のサンプル」は、ファイル全体の取り出し順で同じトラックの最初・直前のサンプルを指し、メディアセグメントをまたいで判定する
  - `build_pending_samples` で、内部の demuxer が返した順（`traf` / `trun` の並び順）にサンプルをたどり、各サンプルが属するサンプルエントリーを求める（`sample_entry` が `None` のサンプルは、同じトラックの直前のサンプルと同じエントリーに属する。そのメディアセグメントで最初のサンプルなら、前のメディアセグメントまでに取得した最後のエントリーに属する）
  - 並べ替えた後の取り出し順で、トラックごとに取り出し順で最初のサンプルと、取り出し順で直前のサンプル（前のメディアセグメントの最後のサンプルを含む）からエントリーが変わったサンプルにだけ `sample_entry` を付ける。直前のサンプルとの比較は、`Fmp4FileDemuxer` が保持する `track_runtimes` のキャッシュ済みサンプルエントリーを引き継いで行う

## 完了条件

- 並べ替えで同じトラックのサンプルの順序が `traf` / `trun` の並び順と入れ替わる入力でも、`next_sample()` が panic しない
- ファイル全体の取り出し順で、各トラックの最初のサンプルと、サンプルエントリーが変わったサンプルでだけ `sample_entry` が `Some` になる（メディアセグメントをまたいで判定する）

## 解決方法

- `src/demux_fmp4_file.rs`
  - `build_sample` を、`sample_entry` を持つサンプルのときだけキャッシュを参照するようにした。`then_some` が引数を先に評価していたため、`sample_entry` が `None` のサンプルでも `expect` が実行されて panic していた
  - `build_pending_samples` を、内部の demuxer が返した順（`traf` / `trun` の並び順）で各サンプルが属するサンプルエントリーを求め、並べ替えた後の取り出し順で「各トラックの最初のサンプル」と「直前のサンプルからサンプルエントリーが変わったサンプル」にだけ `sample_entry` を付けるようにした。直前のサンプルは、`track_runtimes` にキャッシュした前のメディアセグメントまでの最後のエントリーから引き継ぐ
  - `Fmp4FileDemuxer` の doc に、取り出し順と `sample_entry` の約束を書いた
- テスト
  - `tests/test_demux_fmp4_file.rs`: `sample_entry_follows_retrieval_order_across_segments` を追加した。stsd に 2 つ目のサンプルエントリーを足し、1 セグメント目を sample description index 1（`tfdt` 90000）と index 2（`tfdt` 0）の 2 つの `traf`、2 セグメント目を index 2 の 1 つの `traf` にした入力で、取り出し順でサンプルエントリーが変わるところにだけ `sample_entry` が付くことを固定リストで確認する。`rewrite_init_segment` と `rewrite_media_segment_moof` も追加した
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: `fmp4_file_demuxer_swapped_traf_order_keeps_sample_entry_contract` を追加した。muxer の出力の `moof` を書き換え、同じトラックの `traf` を複製して 1 つ目の `tfdt` を 90000、2 つ目を 0 にした入力で、`Fmp4FileDemuxer` が panic せず、取り出し順で最初のサンプルにだけ `sample_entry` が付くことを確認する
- `CHANGES.md` に `[FIX]` として記載した

### 確認したこと

- `src/demux_fmp4_file.rs` の変更を `git stash` で戻した状態では、`fmp4_file_demuxer_swapped_traf_order_keeps_sample_entry_contract` が `bug: sample entry must be cached before borrowing` で失敗する（issue の再現手順どおりの panic）
- `cargo test --workspace --exclude c-api`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all --check` が通ることを確認した
