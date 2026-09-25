# `Fmp4SegmentDemuxer::handle_media_segment` がエラーを返す前にサンプルエントリーの送出状態を更新してしまう

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
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
  - サンプルを組み立てる間は、トラックごとの新しい sample description index をローカルに持ち、成功が確定してから `track_runtimes` に書き戻す
  - ループの後にある `mdat` の後ろの追加データの検査も、この書き戻しより前に評価される。このため、検査の位置は変えない
- `handle_media_segment` の doc に、エラーを返した場合は内部状態を変更しないことを書く（`src/mux_mp4_file.rs` の `Mp4FileMuxer::append_sample` の doc にある「# エラー返却時の内部状態」節の書き方にならう）
- 次の 2 つは本 issue を直しても残る別の問題なので、対象外とし、別の issue で扱う
  - `Fmp4FileDemuxer::build_sample` は `then_some` の引数を先に評価する。このため、`sample_entry` をキャッシュしていないトラックに `sample_entry` が `None` のサンプルが来ると、エラーを経なくても panic する（同じトラックの `traf` が 2 つあり、2 つ目の `tfdt` の方が小さいため、並べ替えで `None` のサンプルが先頭に来る場合など）
  - C API の `fmp4_segment_demuxer_handle_media_segment`（`crates/c-api/src/fmp4_segment_demux.rs`）は、内部の `handle_media_segment` が成功して状態を確定した後で、未対応のサンプルエントリーがあると `MP4_ERROR_UNSUPPORTED` を返す。その後のセグメントでは、対応しているトラックも含めて `sample_entry` が NULL になる。wasm の `fmp4_segment_demuxer_handle_media_segment_json` も同じ経路を通る
- C API の `fmp4_segment_demuxer_handle_media_segment` の doc には、「エラーを返した場合は内部状態を変更しない」という保証を書き写さない。上記の C API の経路が直るまでは成り立たないためである
- 関連 issue との関係
  - issue 0097 は、同じ `mdat` の後ろの検査を「`moof` 以外のボックスは読み飛ばし、`moof` が出たらエラーにする」形に変える。issue 0098 と issue 0100 は、同じ `traf` のループを変える
  - どの issue が先に入っても、後から入る側で「エラーを返した場合は内部状態を変更しない」ことを保つ

## 完了条件

- `handle_media_segment` がどのエラーを返した場合も、呼び出しの前後で demuxer の内部状態が変わらない
- エラーの後に正しいメディアセグメントを渡すと、各トラックの最初のサンプルで `sample_entry` が `Some` になる

## 解決方法

- `src/demux_fmp4_segment.rs` の `handle_media_segment` で、状態の更新を成功後に移し、doc を更新する
- テスト（shiguredo-rust の役割分担に従い、PBT で検証する）
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs` の `rejects_multiple_moof_mdat_pairs_in_one_input` を拡張するか、同じファイルにプロパティを追加する
  - 次のエラー入力を渡してエラーになった後、正しいメディアセグメントを渡した結果が、新しく作った demuxer に同じセグメントを渡した結果と一致することを確認する（最初のサンプルの `sample_entry` が `Some` になることを含む）
    - `moof` + `mdat` を 2 組連結した入力（再現手順と同じ）
    - 2 番目の `traf` の track_id を、`moov` に存在しない値に書き換えた入力
    - 2 番目以降のサンプルが `mdat` の範囲を超える入力（最初のサンプルは範囲内にする）
  - 修正前の実装で、これらのプロパティが失敗することを確認する
- `CHANGES.md` に `[FIX]` として記載する
