# C API の `fmp4_segment_demuxer_handle_media_segment` が内部の状態を確定させた後で `MP4_ERROR_UNSUPPORTED` を返す

- Created: 2026-09-25
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-capi-fmp4-segment-demuxer-unsupported-entry-state
- Polished: 2026-09-25

## 目的

C API の `fmp4_segment_demuxer_handle_media_segment` がエラーを返したときに、内部の `Fmp4SegmentDemuxer` の状態を変えないようにする。

今は、C API が変換できないサンプルエントリーがあると `MP4_ERROR_UNSUPPORTED` を返すが、その時点で内部の demuxer の状態はすでに更新されている。その後のメディアセグメントでは、C API が対応しているトラックも含めて `sample_entry` が NULL になる。利用者は `sample_entry` からしかコーデック設定を得られないため、デコーダーを設定できなくなる。

## 現状

`crates/c-api/src/fmp4_segment_demux.rs` の `fmp4_segment_demuxer_handle_media_segment`:

- 内部の `Fmp4SegmentDemuxer::handle_media_segment` を呼び、成功したら返ったサンプルを C の構造体に変換する
- 変換の途中で、サンプルエントリーを `Mp4SampleEntryOwned::new`（`crates/c-api/src/boxes.rs`）で変換できない場合（`SampleEntry::Unknown` など、`_ => None` に当たるもの）、`MP4_ERROR_UNSUPPORTED` を返す
- このとき内部の demuxer は、成功した呼び出しとして、各トラックの「直前に使った sample description index」をすでに更新している。`sample_entry` は各トラックの最初のサンプル、または sample description index が変わったサンプルでのみ返る。このため、次のメディアセグメントからは、すべてのトラックで `sample_entry` が NULL になる

wasm の `fmp4_segment_demuxer_handle_media_segment_json`（`crates/wasm/src/fmp4_segment_demux.rs`）は、この関数を呼ぶため同じ経路を通る。

再現手順:

1. 映像（`avc1`）と、C API が変換できないサンプルエントリーの音声（`ac-3` など）を含む fMP4 の init セグメントを、`fmp4_segment_demuxer_handle_init_segment` に渡す
2. 1 つ目のメディアセグメントを `fmp4_segment_demuxer_handle_media_segment` に渡すと、`MP4_ERROR_UNSUPPORTED`（サンプル数 0）になる
3. 2 つ目のメディアセグメントを渡すと `MP4_ERROR_OK` になるが、映像を含むすべてのトラックで `sample_entry` が NULL になる

## 設計方針

- `fmp4_segment_demuxer_handle_media_segment` がエラーを返す場合は、内部の demuxer を呼び出し前の状態に戻す
  - 内部の（Rust 側の）`Fmp4SegmentDemuxer` は `Clone` を実装しているため、呼び出し前の状態を複製しておき、変換に失敗したら複製で置き換える
- C API が変換できないサンプルエントリーを持つトラックがあるとき、メディアセグメント全体をエラーにする今の振る舞いは変えない（そのトラックのサンプルだけを除くかどうかは、この issue では扱わない）
- 関連 issue: issue 0096 は、Rust 側の `handle_media_segment` がエラーを返した場合に状態を変えないようにする。この issue は、Rust 側が成功した後に C API 側でエラーにする経路を扱う。0096 の対応では直らない

## 完了条件

- `fmp4_segment_demuxer_handle_media_segment` が `MP4_ERROR_UNSUPPORTED` を返した呼び出しの前後で、内部の demuxer の状態が変わらない
  - 確認方法: エラーの後に、C API が対応しているトラックの `traf` だけを含むメディアセグメントを渡すと、各トラックの最初のサンプルで `sample_entry` が NULL にならない（エラーになる呼び出しをしなかった場合と同じ結果になる）

## 解決方法

- `crates/c-api/src/fmp4_segment_demux.rs` の `fmp4_segment_demuxer_handle_media_segment` と doc を更新し、cbindgen で `crates/c-api/include/mp4.h` を再生成する
  - doc には、エラーを返したどの場合も、内部の（Rust 側の）`Fmp4SegmentDemuxer` の状態（次の呼び出しで `sample_entry` を返すかどうかの判定に使う、各トラックの直前に使った sample description index）を変更しないことを書く
  - 保証の範囲は内部の `Fmp4SegmentDemuxer` の状態に限ることを書く。エラーを返すときも `last_error_string` は更新され、変換に成功したサンプルエントリーが `sample_entries` のキャッシュに残ることがあるため、「内部状態を変更しない」とだけ書くと不正確になる
- `crates/c-api/tests/test_fmp4_segment_demux.rs`（新設）に、完了条件の確認方法どおりのテストを追加する。C API が変換できないサンプルエントリーは、`SampleEntry::Unknown` になるボックス種別で init セグメントを組み立てて作る
- `CHANGES.md` に `[FIX]` として記載する
