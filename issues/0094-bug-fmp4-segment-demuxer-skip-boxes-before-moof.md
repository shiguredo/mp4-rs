# `Fmp4SegmentDemuxer::handle_media_segment` が `moof` より前の `styp` などのトップレベルボックスを受け付けない

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-segment-demuxer-skip-boxes-before-moof
- Polished: 2026-09-24

## 目的

`Fmp4SegmentDemuxer::handle_media_segment` は、`moof` の前に先頭の `sidx` 1 個以外のトップレベルボックス（`styp` など）があるメディアセグメントをエラーにしている。これを受け付けるように直す。

外部からのフィードバックで、`styp` + `moof` + `mdat` 形式のメディアセグメントが拒否されると報告された。ISO/IEC 14496-12 の 8.16.2 では、セグメントを個別のファイルとして置く場合は `styp` を含めることが推奨されており、含める場合は先頭に置かなければならない。

## 現状

`src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment`:

- 先頭が `sidx` の場合だけ、そのボックスを 1 個スキップする
- その次のボックスが `moof` でなければ `expected moof box but got ...` エラーを返す

このため、仕様上有効な次の並びがすべてエラーになる:

- `styp` + `moof` + `mdat`
- `styp` + `sidx` + `moof` + `mdat`
- `sidx` + `sidx` + `moof` + `mdat`（トップレベルの `sidx` が複数ある場合）
- `sidx` + `ssix` + `moof` + `mdat`
- `sidx` + `prft` + `moof` + `mdat`
- `moof` の前に `free` / `skip` を含むもの

関係する仕様は ISO/IEC 14496-12 の 8.16.2 (`styp`)、8.16.3 (`sidx`)、8.16.4 (`ssix`)、8.16.5 (`prft`)、8.1.2 (`free` / `skip`)、4.2.2（未知のボックスは無視して読み飛ばす）。

同じライブラリのほかの処理は、すでにこうしたボックスを読み飛ばしている:

- `Fmp4SegmentDemuxer::handle_init_segment`: `ftyp` の後、`moov` が出るまで任意のトップレベルボックスを読み飛ばす
- `Fmp4FileDemuxer`（`src/demux_fmp4_file.rs`）: `moof` 以外のトップレベルボックスをすべて読み飛ばし、`moof` から始まる範囲だけを `handle_media_segment` に渡す。このため `styp` を含むファイルでも問題は起きない

## 設計方針

- `styp` を個別に許可するのではなく、`moof` が出るまでトップレベルボックスを読み飛ばすループにする
  - 4.2.2 の規定、および `handle_init_segment` / `Fmp4FileDemuxer` の振る舞いに揃える
  - `styp` / 複数の `sidx` / `ssix` / `prft` / `free` を個別に列挙しなくて済む
- 読み飛ばしたボックスの中身はデコードしない（今の `sidx` と同じ扱い）
- `styp` が先頭にあるかどうかは検証しない（8.16.2 では、先頭にない `styp` は無視してよいとされている）
- エラーの扱い
  - `data` が空の場合: 今と同じく `empty media segment`
  - `moof` より前に size=0 のボックスがある場合: エラー（`handle_init_segment` の `found box with size=0 before moov in init segment` と同じ扱い）
  - `moof` が見つからないまま末尾に達した場合: `moof box not found in media segment`（`Error::invalid_data`。`handle_init_segment` の `moov box not found in init segment` と同じ種別）
    - 今は `sidx` だけのデータが `empty media segment`（`Error::invalid_input`）になっているが、修正後はこのエラーになる。C API の戻り値も `MP4_ERROR_INVALID_INPUT` から `MP4_ERROR_INVALID_DATA` に変わる
  - ボックスサイズの `usize` 変換失敗やオフセットのオーバーフロー: 今の `sidx` 用エラーと同じ形にし、メッセージはボックス種別に依存しないものにする
- `Sample::data_offset` と `tfhd` の明示 `base_data_offset` の基準は、これまでどおり `data` スライスの先頭とする（今の `sidx` と同じ扱い）
- `moof` と `mdat` の間、`mdat` より後ろの扱い、1 回の呼び出しで処理する `moof` + `mdat` は 1 組だけという制限は変えない

## 完了条件

- `moof` の前に `styp` / `sidx`（複数を含む）/ `ssix` / `prft` / `free` などのトップレベルボックスがあっても `handle_media_segment` が成功する。返るサンプル列は `moof` + `mdat` だけの場合と一致する。`tfhd` に `base_data_offset` が明示されていなければ、違いは `data_offset` が先頭ボックスの合計サイズ分ずれることだけである
- `moof` がないセグメントや、`moof` より前に size=0 のボックスがあるセグメントはエラーになる
- ドキュメントとテストが新しい振る舞いに合っている

## 解決方法

- `src/demux_fmp4_segment.rs`
  - `Fmp4SegmentDemuxer::handle_media_segment` の先頭にある `sidx` スキップ処理を、`moof` が出るまでトップレベルボックスを読み飛ばすループに置き換える
  - モジュール doc の「メディアセグメント」の説明と、`handle_media_segment` の doc（制限事項にある `sidx` 自動スキップの記述）を更新する
- `crates/c-api/src/fmp4_segment_demux.rs`
  - `fmp4_segment_demuxer_handle_media_segment` の doc（「`moof` + `mdat` または `sidx` + `moof` + `mdat`」）を更新し、cbindgen で `crates/c-api/include/mp4.h` を再生成する
- `skills/shiguredo-mp4/SKILL.md`
  - `handle_media_segment` の行（「先頭の `sidx` は自動スキップ」）を更新する
- テスト（shiguredo-rust の役割分担に従い、正常系は PBT、PBT で扱えないエラーケースだけを単体テストにする）
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`
    - `build_complete_media_segment` と `build_complete_media_segment_with_sidx` で生成したセグメントの前に、`moof` 以外のトップレベルボックスを任意の種類・個数で付けるプロパティを追加する
    - 付けるボックスの種類は `styp` / `sidx` / `ssix` / `prft` / `free` / `skip` と任意の 4CC から引く（`moof` と、拡張型が必要な `uuid` は除く）
    - 何も付けない場合と比べて、サンプル列が一致し、`data_offset` が付けたボックスの合計サイズ分だけずれることを確認する
  - `tests/test_demux_fmp4_segment.rs`
    - エラーケースとして、`styp` だけのデータと、`moof` の前に size=0 のボックスがあるデータを追加する
    - モジュール doc（今は `InvalidState` 経路だけを対象とする記述）を、`DecodeError` 経路も含む記述に更新する
- `CHANGES.md` に `[FIX]` として記載する
