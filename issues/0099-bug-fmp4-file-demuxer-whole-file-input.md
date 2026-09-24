# `Fmp4FileDemuxer` にファイル全体を渡すと、フラグメントが 2 つ以上あるファイルでエラーになる

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-file-demuxer-whole-file-input
- Polished: {YYYY-MM-DD}

## 目的

`Fmp4FileDemuxer::handle_input` に、要求された範囲より多いデータ（ファイル全体など）を渡しても正しく動くようにする。

`Mp4FileDemuxer::handle_input` の doc は、`required_input()` で指定された範囲より多くのデータを渡してもよく、入力ファイル全体を一度に渡してもよいと明記している。`Fmp4FileDemuxer` も同じ `RequiredInput::is_satisfied_by` で入力を受け付けている。ところが、ファイル全体を渡すと、フラグメントが 2 つ以上あるファイルでは最初のメディアセグメントの処理でエラーになる。

## 現状

`src/demux_fmp4_file.rs`:

- `Fmp4FileDemuxer::read_media_segment` は、`available_bytes` で `moof` の位置からのデータを取り出し、そのまま `Fmp4SegmentDemuxer::handle_media_segment` に渡す
- `available_bytes` は、入力が要求サイズ（`moof` + `mdat`）以上あることを確認するだけで、`input.slice_range(position, None)` をそのまま返す。要求サイズで切り詰めない
- そのため、後続のフラグメントまで含んだデータが `handle_media_segment` に渡り、`media segment contains trailing data after mdat` になる

再現手順（develop で確認）:

1. `Fmp4SegmentMuxer` で init セグメントとメディアセグメント 2 つを作り、連結してファイルにする
2. `Fmp4FileDemuxer` に、`required_input()` が `Some` の間、常に位置 0 からファイル全体を `handle_input` で渡す
3. `next_sample()` が `media segment contains trailing data after mdat` を返し、サンプルを 1 つも取得できない

既存の PBT（`pbt/tests/prop_fmp4_segment_mux_demux.rs` の `feed_fmp4_file_demuxer`）は要求された範囲だけを渡すため、この問題を検出できない。

## 設計方針

- `available_bytes` で、返すデータを要求サイズに切り詰める
- `mdat` の size が 0（ファイルの末尾まで）の場合は、今と同じく末尾までを渡す
- `Fmp4FileDemuxer::handle_input` の doc に、`Mp4FileDemuxer` と同じく、要求より多くのデータを渡してもよいことを書く

## 完了条件

- フラグメントが 2 つ以上あるファイルで、ファイル全体を渡した場合と要求された範囲だけを渡した場合に、同じサンプル列が得られる

## 解決方法

- `src/demux_fmp4_file.rs` の `available_bytes` と `handle_input` の doc を更新する
- `pbt/tests/prop_fmp4_segment_mux_demux.rs` に、ファイル全体を渡す場合と要求された範囲だけを渡す場合の結果が一致するプロパティを追加する
  - issue 0074（partial input / 中断再開の PBT）は、要求より少ない入力や別の範囲の先出しを扱う。この issue は要求より多い入力を扱うため別にする
- `CHANGES.md` に `[FIX]` として記載する
