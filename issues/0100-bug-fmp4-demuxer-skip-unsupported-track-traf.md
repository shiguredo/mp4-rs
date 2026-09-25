# fMP4 のデマルチプレクサーが未対応ハンドラーのトラックの `traf` を含むメディアセグメントを丸ごと拒否する

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-demuxer-skip-unsupported-track-traf
- Polished: 2026-09-25

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

- `handle_init_segment` で読み飛ばしたトラックについて、track_id と、`moov` の `mvex` 内の対応する `trex`（存在する場合）を記録する
  - メディアセグメントのサンプルサイズは `trun` の各サンプル → `tfhd.default_sample_size` → `trex.default_sample_size` の順に解決される。読み飛ばす `traf` のデータ末尾の計算にもサンプルサイズが必要なため、`trex` を記録しておかないと、`trun` / `tfhd` のどちらにもサイズが無い入力でデータ末尾を計算できない
  - `mvex` に `trex` が無いトラックは `trex` なしとして記録し、サイズを `trun` / `tfhd` からのみ解決する
- `handle_media_segment` では、記録した track_id の `traf` からサンプルを返さない。その一方で、読み飛ばす `traf` についてもサンプルサイズの解決とデータ末尾の計算は行い、後続の `traf` の基準位置に反映する
  - `default_base_is_moof = false` かつ `base_data_offset` なしの場合、次の `traf` の基準位置は直前の `traf` のデータ末尾になるため、読み飛ばす `traf` についてもデータ末尾の計算が必要になる。この規則は現行実装のものであり、issue 0098 が見直し中なので、実装時は同 issue の結論に合わせる（0098 の案 A を採用した場合は、各トラックの最初の `traf` が入力の先頭を基準にするため、読み飛ばす `traf` のデータ末尾はほかのトラックの基準位置に影響しなくなる。計算しておいても誤りにはならない）
- `moov` に存在しない track_id の `traf` は、今と同じくエラーにする

## 完了条件

- 未対応ハンドラーのトラックを含む fMP4 で、`Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` が、対応しているトラックのサンプルを返し、読み飛ばしたトラックのサンプルは返さない
- 対応しているトラックの各サンプルについて、`track_id`・`timestamp`・`duration`・`keyframe`・`data_size`・`sample_entry` の有無とデータ内容が、未対応ハンドラーのトラックがない場合と同じになる
  - `data_offset` は入力のレイアウト（読み飛ばす `traf` のデータの位置とサイズ）に依存して変わり得るため、比較の対象にしない。代わりに、読み飛ばす `traf` の直後にある `traf` のサンプルが正しい位置から取り出せること（`data_offset` + `data_size` が指す場所にサンプルデータがあること）を確認する
- `moov` に存在しない track_id の `traf` を含むメディアセグメントは、今と同じくエラーになる

## 解決方法

- `src/demux_fmp4_segment.rs` の `handle_init_segment` / `handle_media_segment` と doc を更新する
- テスト
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: muxer が生成した init / メディアセグメントを書き換え、init セグメントに未対応ハンドラーのトラック（`mvex` に `trex` も追加）を、メディアセグメントにそのトラックの `traf` を加えても、対応しているトラックのサンプル列が変わらないプロパティを追加する
    - muxer は未対応トラックを生成できないため、`TrakBox` / `TrafBox` はテスト内で組み立て、既存の `rewrite_init_segment` と `MoofBox` の再エンコードで挿入する
    - 読み飛ばす `traf` のデータをメディアセグメント内に挿入すると対応トラックの `trun` の `data_offset` がずれるため、比較からは除外し、`data_offset` + `data_size` が指す場所のデータ内容で検証する（完了条件の note 参照）
    - `default_base_is_moof = false` に書き換えた場合も検証し、読み飛ばす `traf` のデータ末尾の計算が後続の `traf` に正しく反映されることを確認する
    - `Fmp4FileDemuxer` についても、init セグメントとメディアセグメントを連結したファイルを `feed_fmp4_file_demuxer` で処理し、同じ検証を行う
  - `tests/test_demux_fmp4_segment.rs`: `moov` に存在しない track_id の `traf` がエラーになる単体テストを追加する
- `CHANGES.md` に `[FIX]` として記載する
