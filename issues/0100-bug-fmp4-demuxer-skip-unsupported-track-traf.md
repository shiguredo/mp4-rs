# fMP4 のデマルチプレクサーが未対応ハンドラーのトラックの `traf` を含むメディアセグメントを丸ごと拒否する

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-demuxer-skip-unsupported-track-traf
- Polished: {YYYY-MM-DD}

## 目的

映像・音声・字幕以外のハンドラーのトラック（タイムドメタデータやヒントトラックなど）を含む fMP4 で、対応しているトラックのサンプルを取り出せるようにする。

今は初期化セグメントの処理でそのトラックを黙って読み飛ばすのに、メディアセグメントにそのトラックの `traf` があると、セグメント全体をエラーにする。初期化は成功するのに、メディアセグメントが 1 つも処理できない。通常の MP4 を扱う `Mp4FileDemuxer` は、同じハンドラーのトラックを読み飛ばして、ほかのトラックを処理できる。

## 現状

- `src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_init_segment`
  - `hdlr` のハンドラー種別が `vide` / `soun` / `subt` / `text` 以外のトラックは、トラック情報に登録せずに読み飛ばす（`_ => continue`）
- `Fmp4SegmentDemuxer::handle_media_segment`
  - `traf` の track_id がトラック情報にないと、`unknown track_id in media segment: {track_id}` を返す
- `src/demux_fmp4_file.rs` の `Fmp4FileDemuxer` も、内部で `Fmp4SegmentDemuxer` を使うため同じ動きになる
- `src/demux_mp4_file.rs` の `Mp4FileDemuxer` は、同じハンドラー種別の分岐でトラックを読み飛ばし、ほかのトラックのサンプルを返す

## 設計方針

- `handle_init_segment` で読み飛ばしたトラックの track_id を記録する
- `handle_media_segment` では、記録した track_id の `traf` からサンプルを返さない
  - `default_base_is_moof = false` かつ `base_data_offset` なしの場合、次の `traf` の基準位置は直前の `traf` のデータ末尾になる。読み飛ばす `traf` についても、サンプルは返さずにデータ末尾だけは計算する
- `moov` に存在しない track_id の `traf` は、今と同じくエラーにする

## 完了条件

- 未対応ハンドラーのトラックを含む fMP4 で、`Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` が、対応しているトラックのサンプルを返す
- 返るサンプル列は、未対応ハンドラーのトラックがない場合と同じになる
- `moov` に存在しない track_id の `traf` を含むメディアセグメントは、今と同じくエラーになる

## 解決方法

- `src/demux_fmp4_segment.rs` の `handle_init_segment` / `handle_media_segment` と doc を更新する
- テスト
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: init セグメントに未対応ハンドラーのトラックを追加し、メディアセグメントにそのトラックの `traf` を加えても、対応しているトラックのサンプル列が変わらないプロパティを追加する
  - `tests/test_demux_fmp4_segment.rs`: `moov` に存在しない track_id の `traf` がエラーになる単体テストを追加する
- `CHANGES.md` に `[FIX]` として記載する
