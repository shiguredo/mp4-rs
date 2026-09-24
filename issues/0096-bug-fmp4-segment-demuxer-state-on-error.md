# `Fmp4SegmentDemuxer::handle_media_segment` がエラーを返す前にサンプルエントリーの送出状態を更新してしまう

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-segment-demuxer-state-on-error
- Polished: {YYYY-MM-DD}

## 目的

`Fmp4SegmentDemuxer::handle_media_segment` がエラーを返したときに、内部状態を変更しないようにする。

今はエラーを返す前に、トラックごとの「直前に使った sample description index」を更新してしまう場合がある。その後に正しいメディアセグメントを渡しても、各トラックの最初のサンプルの `sample_entry` が `None` になる。`sample_entry` は「各トラックの最初のサンプル、または sample description index が変わったサンプルでのみ `Some` になる」と doc に書かれている。利用者は `sample_entry` からしかコーデック設定を得られない（C API も同じ）ため、デコーダーを設定できなくなる。

## 現状

`src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment`:

- `traf` / `trun` のループの中で、サンプルを 1 つ処理するたびに `track_runtime.current_sample_description_index` を更新する
- その後で、次のエラーを返すことがある
  - ループの中: 2 番目以降の `traf` での `unknown track_id in media segment`、`sample data range exceeds mdat boundary`、`trun decode time overflow` など
  - ループの後: `media segment contains trailing data after mdat`

再現手順（develop で確認）:

1. `Fmp4SegmentMuxer` で 1 トラックのメディアセグメントを 2 つ作り、init セグメントで `Fmp4SegmentDemuxer` を初期化する
2. 2 つのセグメント（`moof` + `mdat` が 2 組）を連結して `handle_media_segment` に渡すと、`media segment contains trailing data after mdat` になる
3. 続けて 1 つ目のセグメントだけを渡すと成功するが、最初のサンプルの `sample_entry` が `None` になる（新しく作った demuxer なら `Some`）

issue 0094 の対応の後は、`styp` で始まり `moof` + `mdat` を複数含む CMAF のセグメントも、`styp` で即座に失敗せずにこの経路に入る。

## 設計方針

- 状態の更新は、そのメディアセグメントのすべての検証が通った後にまとめて反映する
  - サンプルを組み立てる間は、トラックごとの新しい sample description index をローカルに持ち、成功が確定してから `track_runtimes` に書き戻す
- `mdat` の後ろの追加データの検査のように、サンプルを組み立てる前にできる検査は前に移す
- `handle_media_segment` の doc に、エラーを返した場合は内部状態を変更しないことを書く（`src/mux_mp4_file.rs` の「# エラー返却時の内部状態」節の書き方にならう）

## 完了条件

- `handle_media_segment` がどのエラーを返した場合も、呼び出しの前後で demuxer の内部状態が変わらない
- エラーの後に正しいメディアセグメントを渡すと、各トラックの最初のサンプルで `sample_entry` が `Some` になる

## 解決方法

- `src/demux_fmp4_segment.rs` の `handle_media_segment` で、状態の更新を成功後に移す。検査の順序を見直す。doc を更新する
- `tests/test_demux_fmp4_segment.rs` に単体テストを追加する
  - `mdat` の後ろに追加データがある入力、2 番目の `traf` の track_id が未知の入力、サンプル範囲が `mdat` を超える入力のそれぞれでエラーになった後、正しいメディアセグメントを渡して最初のサンプルの `sample_entry` が `Some` になることを確認する
- `CHANGES.md` に `[FIX]` として記載する
