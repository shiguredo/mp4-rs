# `Fmp4SegmentDemuxer::handle_media_segment` が `moof` より前の `styp` などのトップレベルボックスを受け付けない

- Created: 2026-09-24
- Completed: 2026-09-24
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

`Fmp4SegmentDemuxer::handle_media_segment` の先頭にあった `sidx` 1 個だけのスキップ処理を、`moof` が出るまでトップレベルボックスを種別を問わず読み飛ばすループに置き換えた。

### 実装

- `src/demux_fmp4_segment.rs`
  - `data` が空なら、これまでどおり `empty media segment`（`Error::invalid_input`）を返す
  - ループでは `BoxHeader` だけをデコードし、`moof` でなければボックスサイズの分だけ位置を進める。ボックスの中身は解釈しない
  - エラーは次のとおり
    - 読み飛ばした結果、`moof` が見つからないまま入力の末尾に達した場合: `moof box not found in media segment`（`Error::invalid_data`）
    - サイズが 0 のボックス（size=0、または size=1 + largesize=0）がある場合: `found box with size=0 before moof in media segment`
    - `usize` への変換の失敗とオフセットのオーバーフロー: `box size exceeds usize::MAX` / `box offset overflow in media segment`（`handle_init_segment` と同じ形）
  - `data_offset` と明示された `base_data_offset` の基準は `data` の先頭のまま。`moof` 以降の処理は変更していない
  - 実装コメントに ISO/IEC 14496-12:2022 の節番号（4.2.2、8.1.2、8.16.2〜8.16.5）と、将来の改訂で変わる可能性があることを書いた
  - モジュール doc と `handle_media_segment` の doc を更新した。読み飛ばすボックスの例、`ftyp` / `moov` / `mdat` も読み飛ばして `moov` の内容は反映しないこと、`data_offset` の基準、エラー条件を書いた
- `crates/c-api/src/fmp4_segment_demux.rs`: `fmp4_segment_demuxer_handle_media_segment` の doc を Rust 側に合わせて更新し、`crates/c-api/include/mp4.h` を再生成した
- `skills/shiguredo-mp4/SKILL.md`: `handle_media_segment` の行を更新した
- `CHANGES.md`: `[FIX]` を追加した。`sidx` のペイロードの途中または直後で入力が終わる場合のエラー種別が `InvalidInput` から `InvalidData` に変わることも書いた

### テスト

- `pbt/tests/prop_fmp4_segment_mux_demux.rs` に `leading_boxes_before_moof_are_skipped` を追加した
  - `sidx` あり / なしのセグメントの前に、`styp` / `sidx` / `ssix` / `prft` / `free` / `skip` / `ftyp` / `moov` / `mdat` と任意の 4CC（`moof` と `uuid` を除く）のボックスを 1〜4 個置く
  - ヘッダーは 32 ビットの size と、size=1 + largesize の両方の形式を使う
  - `default_base_is_moof` は、muxer の出力そのままの true と、`moof` を書き換えた false の両方を確認する
  - 置かない場合と比べて、`data_offset` 以外のフィールドが一致し、`data_offset` が置いたボックスの合計サイズ分だけずれることを確認する
- `tests/test_demux_fmp4_segment.rs` にエラーパスの単体テストを 8 件追加した
  - 空のデータ、`styp` だけ、`sidx` だけ、宣言サイズが入力の末尾を超えるボックス、size=0、size=1 + largesize=0、オフセットのオーバーフロー、読み飛ばした後ろでヘッダーが途中で切れているデータ
- 修正前の実装や、判定・基準の計算を変えた変異版で、追加したテストが失敗することを確認した

### 見送ったもの

- `handle_init_segment` と読み飛ばしループの共通化（`handle_init_segment` の文言にも手を入れることになるため）
- ヘッダーが途中で切れた場合の `InsufficientBuffer` を別の種別に変換すること（`moof` / `mdat` の解析や `handle_init_segment` と同じ扱いに揃える）
- largesize 形式の `uuid` は、`BoxHeader` が usertype と largesize を ISO/IEC 14496-12 の 4.2.2 と逆の順で読み書きしているため、正しく読み飛ばせない。既存の不具合として別に扱う
