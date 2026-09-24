# fMP4 のデマルチプレクサーが `moof` と `mdat` の間や `mdat` の後ろにある `free` などのボックスをエラーにする

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-demuxer-skip-boxes-around-mdat
- Polished: {YYYY-MM-DD}

## 目的

fMP4 のデマルチプレクサーが、`moof` と `mdat` の間や、メディアセグメントの `mdat` の後ろにあるトップレベルボックスを読み飛ばせるようにする。

ISO/IEC 14496-12:2022 の 4.2.2 では、認識できない種別のボックスは無視して読み飛ばすことになっている。`free` / `skip`（8.1.2）もファイルのトップレベルに置ける。今はこうしたボックスが `moof` の直後や `mdat` の後ろにあるだけで、メディアセグメント全体がエラーになる。

issue 0094 では `moof` より前のボックスだけを対象にし、`moof` と `mdat` の間と `mdat` の後ろの扱いは変えないと決めた。この issue はその残りを扱う。

## 現状

- `src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment`
  - `moof` の直後のボックスが `mdat` でなければ `expected mdat box after moof but got ...` を返す
  - `mdat` の後ろにデータがあれば `media segment contains trailing data after mdat` を返す（`moof` + `mdat` が 2 組ある入力を拒否するための検査を兼ねている）
- `src/demux_fmp4_file.rs` の `Fmp4FileDemuxer::read_mdat_box_header`
  - `moof` の直後のボックスが `mdat` でなければ `expected mdat box after moof` を返す
  - `mdat` より後ろは `read_top_level_box_header` が `moof` 以外を読み飛ばすため、問題はない
  - メディアセグメントの範囲を `moof` のサイズと `mdat` のサイズの和として求めており、`moof` と `mdat` が隣接していることを前提にしている

## 設計方針

- `moof` の後ろは、`mdat` が出るまでトップレベルボックスを種別を問わず読み飛ばす
  - `mdat` より先に `moof` が出たら、今と同じくエラーにする
  - size=0 のボックス（`mdat` 以外）が出たら、`moof` より前の読み飛ばしと同じくエラーにする
- `Fmp4SegmentDemuxer::handle_media_segment` では、`mdat` の後ろも `moof` 以外のボックスは読み飛ばす
  - `moof` が出たら、1 回の呼び出しで処理できるのは `moof` + `mdat` 1 組だけという制限のため、今と同じくエラーにする
- `Fmp4FileDemuxer` では、メディアセグメントの範囲を `moof` の先頭から `mdat` の末尾までとして求める
- サンプル範囲の上限検査に使う `mdat` の末尾の位置は変えない

## 完了条件

- `Fmp4SegmentDemuxer::handle_media_segment` が、`moof` + `free` + `mdat` と `moof` + `mdat` + `free` のメディアセグメントを処理でき、`free` がない場合と同じサンプル列を返す（`data_offset` は `free` の位置に応じてずれる）
- `Fmp4FileDemuxer` が、`moof` と `mdat` の間に `free` を含むファイルを処理できる
- `moof` + `mdat` が 2 組ある入力は、今と同じく `Fmp4SegmentDemuxer::handle_media_segment` でエラーになる

## 解決方法

- `src/demux_fmp4_segment.rs` と `src/demux_fmp4_file.rs` の該当処理と doc を更新する
- テスト
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: `moof` と `mdat` の間、`mdat` の後ろに任意のボックスを置いても、置かない場合と同じサンプル列が得られるプロパティを、両方のデマルチプレクサーについて追加する
  - `tests/test_demux_fmp4_segment.rs`: `mdat` の前に `moof` が出る入力、`moof` と `mdat` の間に size=0 のボックスがある入力のエラーケースを追加する
- `CHANGES.md` に `[FIX]` として記載する
