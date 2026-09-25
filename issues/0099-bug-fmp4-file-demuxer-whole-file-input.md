# `Fmp4FileDemuxer` にファイル全体を渡すと、フラグメントが 2 つ以上あるファイルでエラーになる

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-file-demuxer-whole-file-input
- Polished: 2026-09-25

## 目的

`Fmp4FileDemuxer::handle_input` に、要求された範囲より多いデータ（ファイル全体など）を渡しても正しく動くようにする。

`Mp4FileDemuxer::handle_input` の doc は、`required_input()` で指定された範囲より多くのデータを渡してもよく、入力ファイル全体を一度に渡してもよいと明記している。`Fmp4FileDemuxer` も同様に、入力に要求範囲が含まれていれば受け付け、余分なデータがあっても問題ないことを doc として明記する。ところが、現在はファイル全体を渡すと、フラグメントが 2 つ以上あるファイルでは最初のメディアセグメントの処理でエラーになる。

## 現状

`src/demux_fmp4_file.rs`:

- `Fmp4FileDemuxer::read_media_segment` は、`available_bytes` で `moof` の位置からのデータを取り出し、そのまま `Fmp4SegmentDemuxer::handle_media_segment` に渡す
- `available_bytes` は、入力が要求サイズ（`moof` + `mdat`）以上あることを確認するだけで、`input.slice_range(position, None)` をそのまま返す。要求サイズで切り詰めない
- そのため、後続のフラグメントまで含んだデータが `handle_media_segment` に渡り、`media segment contains trailing data after mdat` になる

もう 1 つ、`handle_input` の受け入れ判定（`input_is_acceptable`）が `RequiredInput::is_satisfied_by` を基準にしている問題がある。`is_satisfied_by` は要求範囲全体が入力に含まれることを求めるが、ヘッダー読み取りのフェーズは要求サイズとして `BoxHeader::MAX_SIZE`（32 バイト）を指定する。ファイル末尾付近では要求範囲がファイル末尾を超えるため、位置 0 からファイル全体を渡しても `is_satisfied_by` が `false` になり、`expected input starting at position ...` で拒否される。具体例は以下のとおり:

- `ReadMdatBoxHeader`: `mdat` のヘッダーとペイロードを合わせて 32 バイト未満しか残っていないと、要求範囲がファイル末尾を超える（フラグメントが 1 つだけのファイルでも発生する）
- `ReadTopLevelBoxHeader`（EOF 検出）: ファイル末尾から 32 バイトを要求するため、常に要求範囲がファイル末尾を超える

再現手順（develop で確認）:

1. `Fmp4SegmentMuxer` で init セグメントとメディアセグメント 2 つを作り、連結してファイルにする
2. `Fmp4FileDemuxer` に、`required_input()` が `Some` の間、常に位置 0 からファイル全体を `handle_input` で渡す
3. `next_sample()` が `media segment contains trailing data after mdat` を返し、サンプルを 1 つも取得できない

既存の PBT（`pbt/tests/prop_fmp4_segment_mux_demux.rs` の `feed_fmp4_file_demuxer`）は要求された範囲だけを渡すため、この問題を検出できない。

## 設計方針

- `available_bytes` で、返すデータを要求サイズに切り詰める
  - これにより、`read_media_segment` は `moof` + `mdat` だけを `handle_media_segment` に渡すようになり、後続フラグメントが混入しない
- `mdat` の size が 0（ファイルの末尾まで）の場合は、今と同じく末尾までを渡す
- `handle_input` の受け入れ判定を、「入力データが要求位置を含んでいること（`required.position` が入力データの範囲内にあること）」に緩める
  - ヘッダー読み取りフェーズの要求サイズ（`BoxHeader::MAX_SIZE` = 32 バイト）は、ファイル末尾付近では満たせない場合があるため、要求範囲の包含をそのまま求めるとファイル全体を渡す方式が末尾で拒否される
  - 位相関数側は既に、要求位置からのデータが不足していれば再要求（`DemuxError::InputRequired`）またはデコードエラーを返し、トップレベルボックスヘッダー読み取りでは空の入力を EOF として扱っているため、判定の緩和で不正な入力が処理を進めてしまうことはない
- `Fmp4FileDemuxer::handle_input` の doc に、`Mp4FileDemuxer` と同じく、要求より多くのデータを渡してもよいことを書く

## 完了条件

- フラグメント数に関わらず、ファイル全体を渡した場合と要求された範囲だけを渡した場合に、同じサンプル列が得られる

## 解決方法

- `src/demux_fmp4_file.rs` の `available_bytes` を要求サイズで切り詰め、`input_is_acceptable` を「`required.position` が入力データの範囲内にあること」の判定に変え、`handle_input` の doc を更新する
- `pbt/tests/prop_fmp4_segment_mux_demux.rs` に、ファイル全体を位置 0 から渡すフィーダーと、要求された範囲だけを渡すフィーダーの両方で demux し、サンプル列（トラック ID・タイムスタンプ・duration・keyframe・data_offset・data_size・sample_entry の有無）が一致するプロパティを追加する
  - メディアセグメントは 2 つ以上生成して、後続フラグメントが `handle_media_segment` に混入する経路を必ず通るようにする
  - ファイル全体を渡すフィーダーは、`next_sample()` が `DemuxError::InputRequired` を返したときも再度ファイル全体を渡す（`required_input()` はサンプル待ちの間 `None` を返すため、`while let Some(...)` だけで回すと末尾に到達しない）
  - issue 0074（partial input / 中断再開の PBT）は、要求より少ない入力や別の範囲の先出しを扱う。この issue は要求より多い入力を扱うため別にする
- なお、issue 0097 は同じ `src/demux_fmp4_file.rs` でメディアセグメントの範囲（`read_mdat_box_header`）の求め方を変える。この issue の切り詰めはその範囲を使って行うため、どちらを先に実装しても矛盾しない
- `CHANGES.md` に `[FIX]` として記載する
