# fMP4 のデマルチプレクサーが `moof` と `mdat` の間や `mdat` の後ろにある `free` などのボックスをエラーにする

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
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

- `src/demux_fmp4_segment.rs`: `handle_media_segment` の読み飛ばし処理と、モジュール doc・`handle_media_segment` の doc（「# 制限事項」）を更新する
- `src/demux_fmp4_file.rs`: `read_mdat_box_header` と、`Phase::ReadMdatBoxHeader` / `Phase::ReadMediaSegment` の範囲の計算を更新する
- `crates/c-api/src/fmp4_segment_demux.rs`: `fmp4_segment_demuxer_handle_media_segment` の doc（「`mdat` の後ろに追加データがある場合はエラーになる」）を更新し、cbindgen で `crates/c-api/include/mp4.h` を再生成する
- `skills/shiguredo-mp4/SKILL.md`: `handle_media_segment` の行（「複数ペアや `mdat` の後ろの追加データはエラー」）を更新する
- テスト
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`
    - `moof` と `mdat` の間、`mdat` の後ろに任意のボックスを置くプロパティを、両方のデマルチプレクサーについて追加する
    - 間に置く場合は、`trun` の `data_offset` を置いたボックスの合計サイズだけ増やした入力を作る。対象は、`default_base_is_moof` が true ならすべての `traf`、false なら今の実装では最初の `traf` だけである
    - 置くボックスの種類からは、間では `mdat` と `moof` を、後ろでは `moof` を除く。`arb_leading_box` を流用する場合は、`LEADING_BOX_TYPES` に `mdat` が含まれることに注意する
    - `Fmp4SegmentDemuxer` では、置かない場合と比べて、`data_offset` 以外のフィールドが一致すること、`data_offset` が間に置いた場合は合計サイズ分ずれ、後ろに置いた場合はずれないことを確認する。後ろに置くボックスの最後が、32 ビットの size=0 のボックスになる場合も含める
    - `Fmp4FileDemuxer` では、次のトップレベルボックスの位置が `mdat` の末尾になることを確かめるため、メディアセグメントを 2 つ以上連結したファイルを使う。`data_offset` はファイル先頭からの位置なので、各サンプルの `data_offset` が、ファイル上でそのサンプルのデータより前に置いたボックス（前のメディアセグメントに置いたものも含む）の合計サイズ分ずれることを確認する
    - `rejects_multiple_moof_mdat_pairs_in_one_input` の doc（「末尾データを黙って無視せずエラーを返すことを確認する」）を、`mdat` の後ろの `moof` を拒否する趣旨に直す
  - `tests/test_demux_fmp4_segment.rs`: エラーケースとして次の入力を追加する
    - `mdat` の前に `moof` が出る入力
    - `moof` と `mdat` の間に size=0 のボックスがある入力
    - `mdat` の後ろに size=1 + largesize=0 のボックスがある入力
    - `mdat` の後ろに、宣言サイズが入力の末尾を超えるボックスがある入力
    - `mdat` の後ろに 8 バイト未満の端数がある入力
  - `tests/test_demux_fmp4_file.rs`（新設）: メディアセグメントが 1 つで、その `moof` と `mdat` の間に size=0 のボックス（32 ビットの size=0 と、size=1 + largesize=0 の両方）があるファイルを、`required_input()` が `Some` の間だけ要求された範囲を渡すループで処理する。ループが回数の上限内に終わることと、`next_sample()` が `DecodeError` を返すことを確認する
    - size=0 の検査がないと `mdat_offset` が進まず、エラーにならないまま同じ範囲を要求し続けてループが終わらなくなる。これを検出するため、ループには回数の上限を設ける
    - エラーを `next_sample()` などで取り出した後に、`required_input()` が同じ範囲をもう一度要求するのは既存の挙動であり（issue 0096 の現状を参照）、この issue では変えない
- `CHANGES.md` に `[FIX]` として記載する
