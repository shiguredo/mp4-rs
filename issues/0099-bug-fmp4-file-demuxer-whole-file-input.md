# `Fmp4FileDemuxer` に要求より多いデータ（ファイル全体など）を渡すとエラーになる

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-file-demuxer-whole-file-input
- Polished: 2026-09-25

## 目的

`Fmp4FileDemuxer::handle_input` に、要求された範囲より多いデータ（ファイル全体など）を渡しても正しく動くようにする。

`Mp4FileDemuxer::handle_input` の doc は、`required_input()` で指定された範囲より多くのデータを渡してもよく、入力ファイル全体を一度に渡してもよいと明記している。`Fmp4FileDemuxer` も `RequiredInput::is_satisfied_by` を使って入力を受け付けている（`input_is_acceptable`）。ところが、位置 0 からファイル全体を渡し続けると、メディアセグメントの数にかかわらずエラーになる。

## 現状

`src/demux_fmp4_file.rs` の `Fmp4FileDemuxer` には、原因が 2 つある。

原因 1: メディアセグメントの後ろのデータまで、内部の demuxer に渡る

- `read_media_segment` は、`available_bytes` で `moof` の位置からのデータを取り出し、そのまま `Fmp4SegmentDemuxer::handle_media_segment` に渡す
- `available_bytes` は、入力が要求サイズ（`moof` + `mdat`）以上あることを確認するだけで、`input.slice_range(position, None)` をそのまま返す。要求サイズで切り詰めない
- そのため、`mdat` の後ろのデータ（次のメディアセグメントや、`mfra` などのボックス）まで `handle_media_segment` に渡り、`media segment contains trailing data after mdat` になる

原因 2: ファイル末尾付近の要求を、位置 0 からの入力が満たせない

- `Phase::ReadTopLevelBoxHeader` と `Phase::ReadMdatBoxHeader` は、`BoxHeader::MAX_SIZE`（32 バイト）を要求する。最後のメディアセグメントの後は、ファイル長の位置から 32 バイトを要求する
- `input_is_acceptable` が受け付けるのは、`RequiredInput::is_satisfied_by` を満たす入力か、要求位置から始まり要求サイズに満たない入力（ファイルの終端の合図）だけである。位置 0 から始まり、要求範囲の途中で終わる入力はどちらにも当たらないため、`handle_input` がエラー状態になる
- 最後の `mdat` やファイル末尾のボックスが 32 バイト未満の場合は、そのヘッダーを読む時点で同じエラーになる

`Mp4FileDemuxer` では原因 2 は起きない。`moov` を読んだ後は `required_input()` が `None` を返し、`next_sample()` も入力を要求しない。このため、正しいファイルでファイルの末尾を超える範囲を要求することがない。

再現手順（develop で確認）:

1. `Fmp4SegmentMuxer` で init セグメントとメディアセグメント 2 つを作り、連結してファイルにする
2. `Fmp4FileDemuxer` に、`required_input()` が `Some` の間、常に位置 0 からファイル全体を `handle_input` で渡す
3. `next_sample()` が `media segment contains trailing data after mdat` を返し、サンプルを 1 つも取得できない（原因 1）
4. メディアセグメント 1 つだけのファイルで同じことをすると、サンプルは取り出せる。しかし次に要求された範囲として位置 0 からファイル全体を渡すと、`handle_input` がエラー状態になる。その後の `next_sample()` は `handle_input() error: expected input starting at position ..., but got ... bytes starting at position 0` を返す（原因 2）。最後の `mdat` が 32 バイト未満なら、サンプルを取り出す前に同じエラーになる

既存の PBT（`pbt/tests/prop_fmp4_segment_mux_demux.rs` の `feed_fmp4_file_demuxer`）は要求された範囲だけを渡すため、この問題を検出できない。

## 設計方針

- `available_bytes` で、返すデータを要求サイズに切り詰める。`mdat` の size が 0（ファイルの末尾まで）の場合は、今と同じく末尾までを渡す
- ファイルの終端の扱いを広げる。入力が要求位置を含み、要求範囲の終端より手前で終わる場合は、入力の終端をファイルの終端とみなす
  - これは今の「要求位置から始まり、要求サイズに満たない入力をファイルの終端とみなす」扱いを、要求位置より前から始まる入力にも広げたものである
  - `input_is_acceptable` と、`available_bytes` の `input.position == position` の分岐の両方を、同じ条件に揃える。`input_is_acceptable` だけを広げると、ファイルの途中で切れた入力で `available_bytes` が `InputRequired` を返す。`handle_input` は `InputRequired` をエラー状態にしないため、同じ範囲の要求が繰り返され、呼び出し側のループが終わらなくなる
  - 要求位置でちょうど終わる入力も、ファイルの終端とみなされる。今の、要求位置から始まる空の入力と同じ扱いになる
  - 要求位置が入力の終端より後ろにある場合（読み飛ばすボックスの宣言サイズがファイルの末尾を超える壊れたファイルなど）は、入力が要求位置を含まないため、この拡張の対象外とし、今と同じく入力を拒否する
- `Fmp4FileDemuxer::handle_input` の doc を更新する
  - `Mp4FileDemuxer` と同じく、要求より多いデータやファイル全体を渡してよい
  - `Mp4FileDemuxer` と異なり、ファイル全体を渡す場合も、`required_input()` が `Some` の間は `handle_input` を繰り返し呼ぶ必要がある（`next_sample()` が `InputRequired` を返した後も同じ）。`Mp4FileDemuxer` の doc にある「一度に渡してしまっても構わない」は写さない
  - 入力が要求範囲の途中で終わっている場合は、入力の終端をファイルの終端とみなす
- 関連 issue
  - issue 0097 は、`Fmp4FileDemuxer` のメディアセグメントの範囲を、`moof` の先頭から `mdat` の末尾までに変える。切り詰めはこの範囲（`segment_size`）に対して行うため、どちらが先に入っても成り立つ
  - issue 0074 は、要求より少ない入力を渡す PBT を扱う。今の実装は要求位置から始まる短い入力をファイルの終端とみなしており、この issue はその扱いを広げる。0074 の前提（短い入力を後から補える）とは今も食い違っており、0074 側で API 契約を確認する必要がある

## 完了条件

- 次のファイルで、位置 0 からファイル全体を渡した場合と、要求された範囲だけを渡した場合に、同じサンプル列が得られる。どちらの場合も、最後のサンプルの後の `next_sample()` が `Ok(None)` を返す
  - メディアセグメントが 1 つのファイルと、2 つ以上のファイル
  - 最後の `mdat` が 32 バイト未満のファイル
  - 最後の `mdat` の後ろにボックス（`mfra` など）があるファイル
- `moof` または `mdat` の途中で切れたファイルでも、位置 0 からファイル全体を渡した場合と、要求された範囲だけを渡した場合に、取り出せるサンプル列と、その後の `next_sample()` が返す `DecodeError` が一致する。`required_input()` が `Some` の間 `handle_input` を呼ぶループは、どちらの場合も有限回で終わる

## 解決方法

- `src/demux_fmp4_file.rs`: `available_bytes` の切り詰めと終端の判定、`input_is_acceptable`、`handle_input` の doc を更新する
- `tests/test_boxes_moov_tree.rs` の `subtitle_track_via_fmp4_file_demuxer` の doc コメント（「バッファ全体を渡すのではなく要求に応じて `handle_input()` を繰り返す」）を、ファイル全体を渡せるかどうかと、`handle_input` を繰り返し呼ぶ必要があるかどうかを分けた書き方に直す
- `pbt/tests/prop_fmp4_segment_mux_demux.rs` に、位置 0 からファイル全体を渡す場合と、要求された範囲だけを渡す場合の結果が一致するプロパティを追加する。完了条件に挙げたファイルを含める
  - 生成したファイルを、`moof` または `mdat` の範囲内の任意の位置で切った入力も使う。両者の結果（取り出せるサンプル列と、最後の `next_sample()` の結果）が一致することを確認する
  - 読み飛ばすボックス（末尾の `mfra` など）の、サイズを読める位置より後ろで切ると、次の要求位置が入力の終端より後ろになり、設計方針で対象外とした場合に当たる。このため両者の結果が一致しないので、切る位置は `moof` と `mdat` の範囲に限る
  - `handle_input` を呼ぶループには回数の上限を設ける。`available_bytes` の終端の判定を揃え忘れたときに、ループが終わらないことを検出するため
  - issue 0074（partial input / 中断再開の PBT）は、要求より少ない入力を後から補う供給方法や、別の範囲の先出しを扱う。この issue は要求位置を含む入力（ファイル全体など）の扱いを決めるもので、短い入力を後から補う供給方法は扱わないため別にする
- `CHANGES.md` に `[FIX]` として記載する
