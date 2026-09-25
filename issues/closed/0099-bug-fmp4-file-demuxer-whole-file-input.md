# `Fmp4FileDemuxer` に要求より多いデータ（ファイル全体など）を渡すとエラーになる

- Created: 2026-09-24
- Completed: 2026-09-25
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

`Fmp4FileDemuxer` を次のように直した。

### 実装

- `src/demux_fmp4_file.rs`
  - `available_bytes` で、返すデータを要求された `required_size` バイトに切り詰めるようにした。メディアセグメントの処理で `mdat` の後ろのデータが内部の demuxer に渡らなくなる
  - `available_bytes` の終端の判定を、入力が要求された位置を含む場合は常に「入力の終端をファイルの終端とみなす」に変えた。これまでは入力の開始位置が要求位置と一致する場合だけだった
  - `input_is_acceptable` を、要求された範囲を満たすかどうかではなく、入力が要求された位置を含むかどうかで判定するように変えた。判定には `available_bytes` と同じ `Input::slice_range` を使う。ファイルの途中で切れた入力で `available_bytes` が `InputRequired` を返し、同じ範囲の要求が繰り返されることを防ぐ
  - `handle_input` の doc を追加した。要求より多いデータ（ファイル全体）を渡せること、`required_input()` が `Some` を返す間は繰り返し呼ぶ必要があること、入力の終端が要求位置と一致する場合はそこでファイルの終端に達したものとして処理し、要求された範囲の終端より手前で終わっている場合は切れた位置までのデータだけが処理されること、入力が要求位置を含まない場合（要求位置が入力の終端より後ろにある場合と、入力が要求位置より後ろから始まる場合）は入力のエラーになることを書いた
- `tests/test_boxes_moov_tree.rs`: `subtitle_track_via_fmp4_file_demuxer` の doc を、要求より多いデータを渡せることと、`required_input()` が `Some` を返す間は `handle_input` を繰り返す必要があることを分けた書き方に直した
- `CHANGES.md`: `[FIX]` を追加した。位置 0 からファイル全体を渡した場合に、要求された範囲が入力の終端を超えるときの扱いが変わること（これまでは要求された位置から入力が始まる場合を除いて拒否して `ErrorKind::InvalidInput` を返していたが、いまは要求された位置を含む入力を受理すること）と、`available_bytes` でデータを取り出す処理ではデータが足りないときのエラー種別が `ErrorKind::InvalidInput` から `ErrorKind::InvalidData` に変わることも書いた
- `skills/shiguredo-mp4/SKILL.md`: `Fmp4FileDemuxer` の節に、`Mp4FileDemuxer` と同様に要求より多いデータを渡せることと、ファイル全体を渡す場合も `required_input()` が `Some` を返す間は `handle_input()` を繰り返す必要があることを追記した

### テスト

- `pbt/tests/prop_fmp4_segment_mux_demux.rs`
  - `fmp4_file_demuxer_accepts_whole_file_input` を追加した。要求された範囲だけを渡した場合と、常に位置 0 からファイル全体を渡した場合で、取り出せるサンプル列と最後の `next_sample()` の結果が一致することを確認する。次のファイルを含める
    - メディアセグメントが 1 つのファイルと 2 つ以上のファイル
    - 最後の `mdat` の宣言サイズが 32 バイト未満のファイル（payload の長さを短くして作り、`mdat` のサイズフィールドを読んで 32 バイト未満であることを確かめる）
    - 最後の `mdat` の後ろにボックスがあるファイル
    - 最後の `mdat` の size が 0 のファイル（`segment_size` が `None` になり、`available_bytes` の切り詰めを通らない経路になる）
    - `moof` と `mdat` の間にボックスがあるファイル（`segment_size` が読み飛ばし分を含むため、切り詰めが読み飛ばし分を切らないことを確認できる）
  - `moof` と `mdat` の範囲内の任意の位置で切った入力も使い、その場合も両者が一致することを確認する。切る位置を `moof` と `mdat` の範囲に限る理由（間に置いたボックスの内部で切ると、そのボックスの宣言サイズだけ読み飛ばした先が入力の終端より後ろになる）をコメントに書いた
  - 切っていないファイルでは、比較元が全サンプルを取り出して `Ok(None)` になることも確かめる。`moof` または `mdat` の途中で切った場合は、比較元がエラーになることも確かめる。どちらも、2 通りの供給方法が同じ誤りを共有する偽の成功を防ぐためである
  - 供給ループには回数の上限を設け、`available_bytes` の終端の判定が揃っていないときにハングではなく失敗するようにした
  - 補助として `feed_fmp4_file_demuxer_with_whole_file`、`ComparableFinalResult`、`collect_demux_result` を追加した

- `tests/test_demux_fmp4_file.rs`: 要求された位置が入力の終端より後ろになるときは、位置 0 からファイル全体を渡しても受理されず `InvalidInput` の `DecodeError` になることを確かめるテストを追加した（読み飛ばすボックスの宣言サイズがファイルの末尾を超える入力）

### 確認したこと

- `src/demux_fmp4_file.rs` の変更を `git stash` で戻した状態で、`fmp4_file_demuxer_accepts_whole_file_input` が失敗することを確認した（要求された範囲だけを渡した場合はサンプルが取れるのに、位置 0 からファイル全体を渡した場合は `found moof box after mdat in media segment` などになる）
- `cargo test --workspace --exclude c-api`、`cargo test -p c-api --lib`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all --check`、`RUSTDOCFLAGS=-D warnings cargo doc` が通ることを確認した
- `MP4_RS_PBT_SEED` を変えて `fmp4_file_demuxer_accepts_whole_file_input` を 30 回実行し、すべて通ることを確認した
