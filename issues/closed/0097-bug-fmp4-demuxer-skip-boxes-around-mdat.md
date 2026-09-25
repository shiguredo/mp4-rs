# fMP4 のデマルチプレクサーが `moof` と `mdat` の間や `mdat` の後ろにある `free` などのボックスをエラーにする

- Created: 2026-09-24
- Completed: 2026-09-25
- Branch: feature/fix-fmp4-demuxer-skip-boxes-around-mdat
- Polished: 2026-09-25

## 目的

fMP4 のデマルチプレクサーが、`moof` と `mdat` の間や、メディアセグメントの `mdat` の後ろにあるトップレベルボックスを読み飛ばせるようにする。

ISO/IEC 14496-12:2022 の 4.2.2 では、認識できない種別のボックスは無視して読み飛ばすことになっている（shall）。`free` / `skip`（8.1.2）もファイルのトップレベルに置ける。今は、こうしたボックスが次の位置にあるとエラーになる。

- `moof` と `mdat` の間: `Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` の両方
- `mdat` の後ろ: `Fmp4SegmentDemuxer` だけ（`Fmp4FileDemuxer` は、要求された範囲だけを渡す場合は読み飛ばせる）

issue 0094 では `moof` より前のボックスだけを対象にし、`moof` と `mdat` の間と `mdat` の後ろの扱いは変えないと決めた。この issue はその残りを扱う。

## 現状

- `src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment`
  - `moof` の直後のボックスが `mdat` でなければ `expected mdat box after moof but got ...` を返す
  - `mdat` の後ろにデータがあれば `media segment contains trailing data after mdat` を返す（`moof` + `mdat` が 2 組ある入力を拒否するための検査を兼ねている）
- `src/demux_fmp4_file.rs` の `Fmp4FileDemuxer`
  - `read_mdat_box_header`: `moof` の直後のボックスが `mdat` でなければ `expected mdat box after moof` を返す
  - `mdat` より後ろは、`read_top_level_box_header` が `moof` 以外を読み飛ばす。このため、要求された範囲だけを渡す場合は問題ない。要求より多いデータを渡すと、`mdat` の後ろも内部の `handle_media_segment` に渡ってエラーになる（issue 0099）
  - `read_mdat_box_header` は、メディアセグメントの範囲を `moof` のサイズと `mdat` のサイズの和として求めており、`moof` と `mdat` が隣接していることを前提にしている
  - `read_top_level_box_header` は、size=0 のボックスをエラーにせず `Phase::EndOfFile` として扱う

再現手順（develop で確認）:

1. `Fmp4SegmentMuxer` で 1 トラックの init セグメントとメディアセグメントを作り、init セグメントで `Fmp4SegmentDemuxer` を初期化する
2. メディアセグメントの `moof` と `mdat` の間に `free` を置いて `handle_media_segment` に渡すと、`expected mdat box after moof but got BoxType("free")` になる
3. メディアセグメントの `mdat` の後ろに `free` を置いて渡すと、`media segment contains trailing data after mdat` になる
4. init セグメントと手順 2 のメディアセグメントを連結したファイルを、要求された範囲だけ `Fmp4FileDemuxer` に渡すと、`expected mdat box after moof` になる

## 設計方針

- 両方のデマルチプレクサーに共通
  - `moof` の後ろは、`mdat` が出るまでトップレベルボックスを種別を問わず読み飛ばす。読み飛ばすボックスの中身は解釈しない
  - `mdat` より先に `moof` が出たら、今と同じくエラーにする
  - `mdat` が見つからないまま入力の末尾に達したら、今と同じく `mdat box not found after moof` のエラーにする
  - `moof` と `mdat` の間に size=0 のボックス（32 ビットの size=0、または size=1 + largesize=0）が出たら、エラーにする
    - 4.2.2 により、32 ビットの size=0 のボックスはコンテナの最後のボックスなので、その後ろに `mdat` は存在し得ない。size=1 + largesize=0 は仕様上の意味が定められておらず、読み飛ばし先を決められない。どちらもボックスサイズが 0 になり、検査がないと読み飛ばしの位置が進まずループが終わらなくなる
    - `Fmp4SegmentDemuxer` では、`moof` より前の読み飛ばし（`found box with size=0 before moof in media segment`）と同じ形のエラーにする
    - `Fmp4FileDemuxer` では、`read_top_level_box_header` が size=0 を `Phase::EndOfFile` として扱うのとは異なり、エラーにする
  - 読み飛ばしたボックスの分を `data_offset` に足さない。`trun` の `data_offset` は `tfhd` で決まる基準（8.8.7.1）に足す値であり（8.8.8.3）、`moof` と `mdat` の間にボックスがあるファイルでは、書き手がその分を含めた値を `trun` に書く
  - サンプル範囲の上限検査に使う `mdat` の末尾の位置は変えない
- `Fmp4SegmentDemuxer::handle_media_segment`
  - `mdat` の後ろも、入力の末尾まで `moof` 以外のボックスは読み飛ばす
    - `moof` が出たら、1 回の呼び出しで処理できるのは `moof` + `mdat` 1 組だけという制限のため、今と同じくエラーにする
    - 32 ビットの size=0 のボックスは、入力の末尾まで続くものとして受け付ける。4.2.2 でコンテナの最後のボックスとして認められており、`Fmp4FileDemuxer` が `mdat` の後ろの size=0 を受け付けるのとも揃う
    - size=1 + largesize=0 のボックス、宣言サイズが入力の末尾を超えるボックス、8 バイト未満の端数はエラーにする
  - `mdat` の後ろの読み飛ばしは、今の `mdat` の後ろの追加データの検査と同じ位置（`traf` のループの後）に置き、位置は変えない
    - issue 0096 の設計方針も、この検査の位置を変えずに、状態の書き戻しをその後ろに置くと決めている。この位置のままでも、書き戻しより前に評価されるため「エラーを返した場合は内部状態を変更しない」は保たれる
    - `traf` のループの前に移すと、issue 0096 の再現手順とテストで使う「`moof` + `mdat` を 2 組連結した入力」が状態を更新する前にエラーになる。0096 より先にこの issue が入った場合に、0096 の再現手順とこの入力では、0096 の修正を確かめられなくなる
- `Fmp4FileDemuxer`
  - `Phase::ReadMdatBoxHeader` で `mdat` / `moof` 以外のボックスを読んだら、`mdat_offset` をそのボックスのサイズだけ進めて、同じ `Phase` に留まる
  - メディアセグメントの範囲は、`moof` の先頭から `mdat` の末尾まで（最終的な `mdat_offset` + `mdat` のサイズ - `moof_offset`）とする。次のトップレベルボックスの位置は `mdat` の末尾とする
- 関連 issue
  - issue 0096: 同じ `mdat` の後ろの検査を扱う。どちらが先に入っても、「エラーを返した場合は内部状態を変更しない」ことを保つ
  - issue 0099: この issue で決めるメディアセグメントの範囲に合わせて、`available_bytes` の返すデータを切り詰める

## 完了条件

- `Fmp4SegmentDemuxer::handle_media_segment` が、`moof` + `free` + `mdat` と `moof` + `mdat` + `free` のメディアセグメントを処理できる
  - `moof` と `mdat` の間に置く場合は、`trun` の `data_offset` を `free` のサイズ分増やした入力を使う
  - 返るサンプル列は、`free` がない場合と `data_offset` 以外のフィールドが一致する。`data_offset` は、`moof` と `mdat` の間に置いた場合は `free` のサイズ分ずれ、`mdat` の後ろに置いた場合はずれない
- `Fmp4SegmentDemuxer::handle_media_segment` が、`mdat` の後ろに 32 ビットの size=0 のボックスがあるメディアセグメントを処理でき、そのボックスがない場合と同じサンプル列を返す
- `Fmp4FileDemuxer` が、`moof` と `mdat` の間に `free` を含むファイルを、メディアセグメントが 2 つ以上ある場合も含めて処理できる
- `moof` と `mdat` の間に size=0 のボックスがある入力は、両方のデマルチプレクサーでエラーになる
- `moof` + `mdat` が 2 組ある入力は、今と同じく `Fmp4SegmentDemuxer::handle_media_segment` でエラーになる

## 解決方法

`Fmp4SegmentDemuxer::handle_media_segment` と `Fmp4FileDemuxer` を次のように直した。

### 実装

- `src/demux_fmp4_segment.rs`
  - `moof` の直後は、`mdat` が出るまでトップレベルボックスを種別を問わず読み飛ばすループに置き換えた。`moof` の後ろで `mdat` より先に別の `moof` が出た場合は、これまでと同じ `expected mdat box after moof but got ...` を返す
  - `moof` と `mdat` の間にサイズが 0 のボックス（32 ビットの size=0、または size=1 + largesize=0）がある場合は `found box with size=0 between moof and mdat in media segment` を返す。この検査がないと読み飛ばし位置が進まずループが終わらない
  - `mdat` の後ろは、入力の末尾まで `moof` 以外のトップレベルボックスを読み飛ばすループに置き換えた。`moof` が出た場合は `found moof box after mdat in media segment` を返す。32 ビットの size=0 のボックスは、入力の末尾まで続くものとして受け付ける。size=1 + largesize=0 のボックスは `found box with size=0 after mdat in media segment`、宣言サイズが入力の末尾を超えるボックスは `box after mdat exceeds media segment boundary` を返す
  - 読み飛ばしたボックスの分は `data_offset` に足さない。`mdat` の末尾の位置（サンプル範囲の上限検査に使う値）も変えない
  - 読み飛ばしは `traf` のループの後ろ（`mdat` の後ろの追加データの検査と同じ位置）に置いた。作業用の sample description index の書き戻しはそのままその後ろにあり、「エラーを返した場合は内部状態を変更しない」は保たれる
  - `mdat` の後ろの 8 バイト未満の端数は、専用のエラーにせず `BoxHeader` のデコードエラー（`ErrorKind::InsufficientBuffer`）を返す。`moof` より前と `moof` と `mdat` の間の読み飛ばしと同じ扱いである
  - モジュール doc と `handle_media_segment` の doc（「# 制限事項」）を更新した
- `src/demux_fmp4_file.rs`
  - `Phase::ReadMdatBoxHeader` で `mdat` / `moof` 以外のボックスを読んだら、`mdat_offset` をそのボックスのサイズだけ進めて同じ `Phase` に留まるようにした。`moof` が出た場合は今と同じ `expected mdat box after moof`、サイズが 0 のボックスは `found box with size=0 between moof and mdat in media segment` を返す。後者の検査がないと `mdat_offset` が進まず、`required_input()` が同じ範囲を要求し続ける
  - メディアセグメントの範囲を `moof` の先頭から `mdat` の末尾まで（読み飛ばしたボックスを含む）に変え、次のトップレベルボックスの位置を `mdat` の末尾にした。`Phase::ReadMdatBoxHeader` の `moof_size` は不要になったため削除した
  - 範囲の計算に伴い、`segment size overflow` を `segment size exceeds usize::MAX` に、`segment offset overflow` を `mdat offset overflow` に置き換えた
  - モジュール doc の「# 制限事項」に、`moof` と `mdat` の間の読み飛ばしとエラー条件を追記した
- `crates/c-api/src/fmp4_segment_demux.rs`: `fmp4_segment_demuxer_handle_media_segment` の doc を Rust 側に合わせて更新し、cbindgen で `crates/c-api/include/mp4.h` を再生成した
- `skills/shiguredo-mp4/SKILL.md`: `handle_media_segment` の行を更新した
- `CHANGES.md`: `[FIX]` を追加した。`mdat` の後ろの `moof` の扱いが `Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` で異なること、エラーになる条件として増えるもの（`moof` と `mdat` の間の size=0）、エラー理由・エラー種別が変わる箇所（`mdat` の後ろの `moof`、`mdat` の後ろの 8 バイト未満の端数、`Fmp4SegmentDemuxer` の `mdat` の後ろの size=1 + largesize=0 と宣言サイズが入力の末尾を超えるボックス・`moof` の後ろの宣言サイズが入力の末尾を超えるボックス・`moof` と `mdat` の間の size=0、`Fmp4FileDemuxer` の `moof` と `mdat` の間の size=0 と `moof` の後ろの宣言サイズがファイルの末尾を超えるボックス、どちらのデマルチプレクサーでもセグメントの範囲の計算のエラー）も書いた

### テスト

- `pbt/tests/prop_fmp4_segment_mux_demux.rs`
  - `boxes_between_moof_and_mdat_and_after_mdat_are_skipped` を追加した。`moof` と `mdat` の間、`mdat` の後ろに任意のトップレベルボックス（largesize 形式を含む）を置いても、置かない場合と比べて `data_offset` 以外のフィールドが一致し、`data_offset` が間に置いたボックスの合計サイズ分だけずれることを確認する。`default_base_is_moof` が true / false の両方、`mdat` の後ろの最後が 32 ビットの size=0 のボックスになる場合、`mdat` の size が 0 で間にボックスを置く場合を含める
  - `fmp4_file_demuxer_skips_boxes_between_moof_and_mdat` を追加した。メディアセグメントを 2 つ以上連結したファイルで、次のトップレベルボックスの位置が `mdat` の末尾になることと、各サンプルの `data_offset` がファイル上でそのサンプルより前に置いたボックスの合計サイズ分だけずれることを確認する。最後のセグメントの `mdat` の size を 0 にする場合を含める
  - `arb_leading_box` の生成処理を `arb_skippable_box(ctx, excluded_types)` に一般化し、間に置くボックスから `mdat` を外せるようにした。種別の候補は `SKIPPABLE_BOX_TYPES` に改名した
  - `rejects_multiple_moof_mdat_pairs_in_one_input` の doc を、`mdat` の後ろの `moof` を拒否する趣旨に直し、`InvalidMediaSegmentKind::ConcatenatedPairs` の期待するエラー理由を `found moof box after mdat in media segment` に変えた
  - `feed_fmp4_file_demuxer` に供給回数の上限を設け、`required_input()` が同じ範囲を要求し続ける回帰がハングではなく失敗になるようにした
- `tests/test_demux_fmp4_segment.rs`: エラーケースとして次の 9 件を追加した
  - `mdat` より先に `moof` が出る入力
  - `moof` の後ろのボックスの宣言サイズが入力の末尾を超える入力
  - `moof` と `mdat` の間に size=0 のボックスがある入力（32 ビットの size=0 と size=1 + largesize=0 の 2 件）
  - `mdat` の後ろに size=1 + largesize=0 のボックスがある入力
  - `mdat` の後ろのボックスの宣言サイズが入力の末尾を超える入力
  - `mdat` の後ろに 8 バイト未満の端数がある入力
  - `moof` と `mdat` の間、および `mdat` の後ろのボックスの largesize が大きすぎて位置を計算できない入力（2 件）
- `tests/test_demux_fmp4_file.rs`（新設）: 次の 4 件を追加した。いずれも `required_input()` が `Some` の間だけ要求された範囲を渡すループで処理する
  - `moof` と `mdat` の間に size=0 のボックス（32 ビットの size=0 と size=1 + largesize=0 の両方）があるファイル。size=0 の検査がないと `mdat_offset` が進まず、エラーにならないまま同じ範囲を要求し続けてループが終わらなくなるため、ループには回数の上限を設ける
  - `mdat` より先に `moof` が出るファイル
  - `moof` と `mdat` の間のボックスの largesize が大きすぎて位置を計算できないファイル
  - `moof` と `mdat` の間のボックスの宣言サイズがファイルの末尾を超えるファイル

### 確認したこと

- `src/` の変更を `git stash` で戻した状態でのテスト結果
  - `tests/test_demux_fmp4_segment.rs`: 追加した 9 件のうち 8 件が失敗する。`decode_error_moof_before_mdat` は変更の前後で同じエラーになる契約テストなので、どちらでも通る
  - `tests/test_demux_fmp4_file.rs`: 追加した 4 件のうち 3 件が失敗する。`decode_error_moof_before_mdat` は同じ理由でどちらでも通る
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: 追加した 2 件と、期待するエラー理由を変えた `media_segment_error_does_not_change_state` と `rejects_multiple_moof_mdat_pairs_in_one_input` が失敗する
- `cargo test --workspace --exclude c-api`、`cargo test -p c-api --lib`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all --check`、`RUSTDOCFLAGS=-D warnings cargo doc` が通ることを確認した
