# `Fmp4SegmentDemuxer::handle_media_segment` がエラーを返す前にサンプルエントリーの送出状態を更新してしまう

- Created: 2026-09-24
- Completed: 2026-09-25
- Branch: feature/fix-fmp4-segment-demuxer-state-on-error
- Polished: 2026-09-25

## 目的

`Fmp4SegmentDemuxer::handle_media_segment` がエラーを返したときに、内部状態を変更しないようにする。

今はエラーを返す前に、トラックごとの「直前に使った sample description index」を更新してしまう場合がある。その後に正しいメディアセグメントを渡すと、エラーの前にサンプルを処理したトラックでは、最初のサンプルの `sample_entry` が `None` になる。`sample_entry` は「各トラックの最初のサンプル、または sample description index が変わったサンプルでのみ `Some` になる」と doc に書かれている。利用者は `sample_entry` からしかコーデック設定を得られない（C API も同じ）ため、デコーダーを設定できなくなる。

内部で `Fmp4SegmentDemuxer` を使う `Fmp4FileDemuxer` では、そのトラックの `sample_entry` をまだキャッシュしていなければ、この状態でサンプルを取り出すと panic する。

## 現状

`src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment`:

- `traf` / `trun` のループの中で、サンプルを 1 つ処理するたびに `track_runtime.current_sample_description_index` を更新する。このメソッドが変更する内部状態はこれだけである
- 更新の後で、次のエラーを返すことがある
  - ループの中で、同じ呼び出しですでにサンプルを 1 つ以上処理した後のエラー。たとえば、2 番目以降の `traf` での `unknown track_id in media segment` や sample_description_index の範囲外、2 番目以降のサンプルでの `sample data range exceeds mdat boundary`
    - `sample data range exceeds mdat boundary` は、そのサンプルで状態を更新する前に検査する。このため、最初のサンプルで範囲を超えた場合は状態が変わらない
  - `trun decode time overflow`: 状態を更新した直後に検査するため、最初のサンプルでも状態が変わる
  - ループの後: `media segment contains trailing data after mdat`

再現手順（develop で確認）:

1. `Fmp4SegmentMuxer` で 1 トラックのメディアセグメントを 2 つ作り、init セグメントで `Fmp4SegmentDemuxer` を初期化する
2. 2 つのセグメント（`moof` + `mdat` が 2 組）を連結して `handle_media_segment` に渡すと、`media segment contains trailing data after mdat` になる
3. 続けて 1 つ目のセグメントだけを渡すと成功するが、最初のサンプルの `sample_entry` が `None` になる（新しく作った demuxer なら `Some`）

issue 0094 の対応で `moof` より前のボックスを読み飛ばすようになったため、`styp` で始まり `moof` + `mdat` を複数含む CMAF のセグメントもこの経路に入る（develop で確認）。

`Fmp4FileDemuxer`（`src/demux_fmp4_file.rs`）への影響:

- 内部の `handle_media_segment` が返したエラーは、`next_sample()` などで一度返すと消える。その後、`required_input()` は同じ範囲をもう一度要求する
- 要求された範囲を渡して再試行すると、内部の demuxer は最初のサンプルの `sample_entry` を `None` で返す。`Fmp4FileDemuxer::build_sample` はそのトラックの `sample_entry` をまだキャッシュしていないため、`bug: sample entry must be cached before borrowing` で panic する
- 最初のエラーは、たとえば issue 0099 の経路（ファイル全体を渡すと `media segment contains trailing data after mdat` になる）で起きる（develop で確認）

## 設計方針

- 状態の更新は、そのメディアセグメントのすべての検証が通った後にまとめて反映する
  - 呼び出しの最初に、各トラックの `current_sample_description_index` を作業用の配列に複製する
  - `emit_sample_entry` の判定（直前の sample description index と比べる）と、サンプルごとの更新は、この作業用の値に対して行う。`track_runtimes` の値と比べてはいけない
    - ISO/IEC 14496-12:2022 の 8.8.6.1 は、1 つの `moof` に同じトラックの `traf` を複数置くことを認めている（"Within the movie fragment there is a set of track fragments, zero or more per track."）。2 つ目以降の `traf` は、同じ呼び出しで先に処理した `traf` の結果と比べる必要がある
    - `track_runtimes` の値と比べると、sample description index が同じ `traf` が続く場合は二重に通知し、途中で変わって元に戻る場合は通知が漏れる。今の実装は、ループの中で `track_runtimes` を更新しているため、この場合も正しく判定できている
  - `mdat` の後ろの追加データの検査を通った後で、作業用の値を `track_runtimes` に書き戻す。この検査はループの後にあり、書き戻しより前に評価される。このため、検査の位置は変えない
- `handle_media_segment` の doc に、エラーを返した場合は内部状態を変更しないことを書く（`src/mux_mp4_file.rs` の `Mp4FileMuxer::append_sample` の doc にある「# エラー返却時の内部状態」節の書き方にならう）
- 次の 2 つは本 issue を直しても残る別の問題なので、対象外とする
  - `Fmp4FileDemuxer::build_sample` は `then_some` の引数を先に評価する。このため、`sample_entry` をキャッシュしていないトラックに `sample_entry` が `None` のサンプルが来ると、エラーを経なくても panic する（同じトラックの `traf` が 2 つあり、2 つ目の `tfdt` の方が小さいため、並べ替えで `None` のサンプルが先頭に来る場合など）。issue 0101 で扱う
  - C API の `fmp4_segment_demuxer_handle_media_segment`（`crates/c-api/src/fmp4_segment_demux.rs`）は、内部の `handle_media_segment` が成功して状態を確定した後で、未対応のサンプルエントリーがあると `MP4_ERROR_UNSUPPORTED` を返す。その後のセグメントでは、対応しているトラックも含めて `sample_entry` が NULL になる。wasm の `fmp4_segment_demuxer_handle_media_segment_json` も同じ経路を通る。issue 0102 で扱う
- C API の `fmp4_segment_demuxer_handle_media_segment` の doc（cbindgen で `crates/c-api/include/mp4.h` に出力される）に、エラーを返した場合の状態についての保証を書くかどうかは、実装に着手した時点で issue 0102 が入っているかどうかで決める
  - issue 0102 がまだ入っていない場合: 書かない。上記の C API の経路が残っており、保証が成り立たないためである。C API 側の保証は、issue 0102 の対応で書く
  - issue 0102 がすでに入っている場合: どのエラーを返した場合も、内部の `Fmp4SegmentDemuxer` の状態（次の呼び出しで `sample_entry` を返すかどうかの判定に使う、各トラックの sample description index）を変更しないことを書き、`mp4.h` を再生成する
  - 保証の範囲は、内部の `Fmp4SegmentDemuxer` の状態に限る。C API の `Fmp4SegmentDemuxer` 構造体は、`inner` のほかに `last_error_string`、変換済みのサンプルエントリーのキャッシュ（`sample_entries`）、トラック情報のキャッシュ（`tracks_cache`）を持つ。エラーのときも `fmp4_segment_demuxer_get_last_error()` が返すメッセージは更新されるため、「内部状態を変更しない」とだけ書くと不正確になる
- 関連 issue との関係
  - issue 0097 は、同じ `mdat` の後ろの検査を「`moof` 以外のボックスは読み飛ばし、`moof` が出たらエラーにする」形に変える。issue 0100 と issue 0103 は、同じ `traf` / `trun` のループを変える。pending の issue 0098 も、案 A を採った場合は同じ `traf` のループを変える
  - どの issue が先に入っても、後から入る側で「エラーを返した場合は内部状態を変更しない」ことを保つ
  - issue 0101 と issue 0102 は、上記の対象外とした 2 つを扱う

## 完了条件

- `handle_media_segment` がどのエラーを返した場合も、呼び出しの前後で demuxer の内部状態が変わらない
- エラーの後に正しいメディアセグメントを渡すと、各トラックの最初のサンプルで `sample_entry` が `Some` になる
- 成功した呼び出しで `sample_entry` が `Some` になるサンプルは、修正の前後で変わらない。同じ `moof` に同じトラックの `traf` が複数ある次の場合を含む
  - 2 つの `traf` の sample description index が同じ（例: 1 → 1）
  - 直前のセグメントと同じ sample description index に、同じ `moof` の中で一度変わってから戻る（例: 直前のセグメントが 1 で、今回の `traf` が 2 → 1）

## 解決方法

`src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment` を次のように直した。

- 呼び出しの最初に、各トラックの `current_sample_description_index` を作業用の配列 `current_sample_description_indices` に複製し、`emit_sample_entry` の判定とサンプルごとの更新はこの配列に対して行うようにした。ループの中では `track_runtimes` を共有参照でしか借用しない
- `mdat` の後ろの追加データの検査を通った後で、作業用の配列を `track_runtimes` に書き戻すようにした。書き戻しより後にエラーを返す経路はない
- 判定で `track_runtimes` の値と比べてはいけない理由（ISO/IEC 14496-12:2022 の 8.8.6.1 が 1 つの `moof` に同じトラックの `traf` を複数置くことを認めていること）をコメントに書いた
- doc に「# エラー返却時の内部状態」節を追加した（`Mp4FileMuxer::append_sample` の書き方にならった）

C API の `fmp4_segment_demuxer_handle_media_segment` の doc と `crates/c-api/include/mp4.h` は、設計方針どおり変えていない（issue 0102 がまだ入っていないため）。

テストは `pbt/tests/prop_fmp4_segment_mux_demux.rs` に PBT を 2 つ追加した。

- `media_segment_error_does_not_change_state`
  - 映像と音声の 2 トラックで、映像が sample description index 2 を使うセグメントを、次の 5 種類のエラー入力に書き換えて渡す（`InvalidMediaSegmentKind`）
    - `moof` + `mdat` を 2 組連結した入力
    - 2 番目の `traf` の track_id を `moov` に存在しない値に書き換えた入力
    - 2 番目の `traf` の sample description index を範囲外にした入力
    - 最後のサンプルのサイズを 1 増やして `mdat` の範囲を超えさせた入力
    - 最初の `traf` の `tfdt` を `u64::MAX` にしてデコード時間を溢れさせた入力
  - エラーの `reason` が狙った理由であること、エラーの前後で demuxer の `Debug` 出力が一致すること、その後に正しいセグメントを渡した結果がエラーを経ない demuxer の結果と一致することを確認する
  - 渡す前の demuxer は、index 1 のセグメントを処理した後と初期化直後の両方を試し、それぞれの分岐を通ったことも確認する
- `sample_entry_emission_with_split_trafs_of_same_track`
  - `moof` を書き換えて同じトラックの `traf` を 2 つに分けた入力で、直前のセグメントの有無と index、2 つの `traf` の index のすべての組み合わせについて、`sample_entry` が `Some` になるサンプルを、実装と独立に求めた期待値と比べる
- 補助として、`moof` を書き換えたときに `trun` の `data_offset` を補正する `rewrite_media_segment_moof` などのヘルパーを追加した

確認したこと:

- 修正前の実装では、`media_segment_error_does_not_change_state` が 5 種類のエラー入力のそれぞれ単独で失敗する（`reason` の照合を通ったうえで、内部状態の比較で失敗する）
- 判定を一時的に `track_runtimes` の値と比べる形に変えると、`sample_entry_emission_with_split_trafs_of_same_track` が 1 → 1 の場合と、直前のセグメントが 1 で今回が 2 → 1 の場合のそれぞれで失敗する
- `Fmp4FileDemuxer` にファイル全体を渡してエラーにした後、要求された範囲を渡して再試行すると、修正前は `bug: sample entry must be cached before borrowing` で panic し、修正後は panic せずにサンプルを取り出せる

`CHANGES.md` に `[FIX]` として記載し、`skills/shiguredo-mp4/SKILL.md` の `handle_media_segment` の説明にエラー時の保証を追記した。
