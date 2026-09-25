# fMP4 のデマルチプレクサーが未対応ハンドラーのトラックの `traf` を含むメディアセグメントを丸ごと拒否する

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-demuxer-skip-unsupported-track-traf
- Polished: 2026-09-25

## 目的

映像・音声・字幕以外のハンドラーのトラック（タイムドメタデータやヒントトラックなど）を含む fMP4 で、対応しているトラックのサンプルを取り出せるようにする。

今は初期化セグメントの処理でそのトラックを黙って読み飛ばすのに、メディアセグメントにそのトラックの `traf` があると、セグメント全体をエラーにする。初期化は成功するのに、未対応トラックの `traf` を含むメディアセグメントは処理できない。そのトラックがすべてのメディアセグメントに含まれる典型的なファイルでは、1 つも処理できない。通常の MP4 を扱う `Mp4FileDemuxer` は、同じハンドラーのトラックを読み飛ばして、ほかのトラックを処理できる。

## 現状

- `src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_init_segment`
  - `hdlr` のハンドラー種別が `vide` / `soun` / `subt` / `text` 以外のトラックは、トラック情報に登録せずに読み飛ばす（`_ => continue`）
  - `trex` を探すのは、この分岐の後である。このため、読み飛ばすトラックの `trex` は取り出しも検証もしていない（`trex` がなくても初期化は成功する）
- `Fmp4SegmentDemuxer::handle_media_segment`
  - `traf` の track_id がトラック情報にないと、`unknown track_id in media segment: {track_id}` を返す
  - サンプルサイズは、`trun` の値、`tfhd` の `default_sample_size`、`trex` の `default_sample_size` の順に決める
- `src/demux_fmp4_file.rs` の `Fmp4FileDemuxer` も、内部で `Fmp4SegmentDemuxer` を使うため同じ動きになる
- `src/demux_mp4_file.rs` の `Mp4FileDemuxer` は、同じハンドラー種別の分岐でトラックを読み飛ばし、ほかのトラックのサンプルを返す

再現手順（develop で確認）:

1. `Fmp4SegmentMuxer` で、映像 1 トラックの init セグメントとメディアセグメントを作る
2. init セグメントの `moov` に、映像の `trak` を複製して track_id を 2、`hdlr` のハンドラー種別を `meta` にした `trak` と、track_id が 2 の `trex` を足す
3. メディアセグメントの `moof` に、track_id が 2 の `traf` を足す
4. `Fmp4SegmentDemuxer::handle_init_segment` は成功し、`tracks()` は 1 トラックを返す。`handle_media_segment` は `unknown track_id in media segment: 2` を返す。`Fmp4FileDemuxer` でも同じエラーになる

## 設計方針

- `handle_init_segment` で、読み飛ばしたトラックの track_id と、そのトラックの `trex`（`Option<TrexBox>`）を記録する
  - ISO/IEC 14496-12:2022 の 8.8.3.1 では、`trex` は `moov` の各トラックに 1 つずつ必須とされている。ただし今は、読み飛ばすトラックに `trex` がない init セグメントも受け付けているため、初期化では拒否しない
- `handle_media_segment` では、記録した track_id の `traf` からサンプルを返さない
  - 今の実装の規則では、`default_base_is_moof = false` かつ `base_data_offset` なしの場合、2 番目以降の `traf` の基準位置は、トラックを問わず直前の `traf` のデータ末尾になる。このため、読み飛ばす `traf` についても、サンプルは返さずにデータ末尾だけは計算する
  - データ末尾の計算に必要なサンプルサイズは、対応しているトラックと同じ順（`trun`、`tfhd`、`trex`）で決める。`trex` の既定値が要るのに、そのトラックの `trex` がない場合は、メディアセグメントのエラーにする
  - 読み飛ばす `traf` では、sample_description_index の検証、duration と decode time の計算、`mdat` の範囲の検査はしない。データ末尾の計算に要るオーバーフローの検査だけを行う
- `moov` に存在しない track_id の `traf` は、今と同じくエラーにする
- 関連 issue
  - issue 0098（pending）: 基準位置の規則の見直しを扱う。ISO/IEC 14496-12:2022 の 8.8.7.1 の箇条では、同じ `moof` の中の同じトラックの 2 番目以降の `traf` だけが、同じトラックの直前の `traf` のデータ末尾を基準にする。0098 で 8.8.7.1 に合わせる案を採った場合、読み飛ばす `traf` のデータ末尾はほかのトラックの基準に影響しなくなり、この計算は不要になる
  - issue 0096: 同じ `traf` のループを変える。どちらが先に入っても、後から入る側で「エラーを返した場合は内部状態を変更しない」ことを保つ
    - 0096 が先に入った場合は、この issue で増えるエラー（`trex` がない場合など）でも内部状態を変更しないようにする
    - この issue が先に入った場合は、0096 がこのエラーも含めて扱う

## 完了条件

- 未対応ハンドラーのトラックを含む fMP4 で、`Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` が、対応しているトラックのサンプルを返す
  - `Fmp4SegmentMuxer` の出力で 1 トラックの `hdlr` のハンドラー種別を `meta` に書き換えた場合に、返るサンプル列が、書き換えない場合の結果からそのトラックのサンプルを除いたものと一致する（`data_offset` を含む）
  - `default_base_is_moof = false` で、読み飛ばす `traf` の後ろに対応しているトラックの `traf` がある場合も一致する
- `moov` に存在しない track_id の `traf` を含むメディアセグメントは、今と同じくエラーになる
- 読み飛ばすトラックの `traf` で `trex` の既定値が要るのに `trex` がない場合は、エラーになる

## 解決方法

- `src/demux_fmp4_segment.rs` の `handle_init_segment` / `handle_media_segment` と doc を更新する
- テスト
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: `Fmp4SegmentMuxer` で複数トラックの init セグメントとメディアセグメントを作り、1 トラックの `hdlr` のハンドラー種別を `rewrite_init_segment` で `meta` に書き換える。返るサンプル列が、書き換えない場合の結果からそのトラックのサンプルを除いたものと一致することを、`Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` の両方で確認する。`rewrite_media_segment_default_base_is_moof_false` で `default_base_is_moof = false` に書き換えた場合も含める
  - `tests/test_demux_fmp4_segment.rs`: エラーケースとして次の入力を追加する
    - `moov` に存在しない track_id の `traf` を含む入力
    - 読み飛ばすトラックに `trex` がなく、その `traf` のサンプルサイズが `trun` と `tfhd` のどちらにもない入力
- `CHANGES.md` に `[FIX]` として記載する
