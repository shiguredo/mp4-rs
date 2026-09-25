# PBT のヘルパー `rewrite_media_segment_tfhd_sample_description_index` が `moof` のサイズの変化を `trun` の `data_offset` に反映しない

- Created: 2026-09-25
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-pbt-tfhd-rewrite-data-offset
- Polished: {YYYY-MM-DD}

## 目的

PBT に渡す入力を、サンプルが元の payload を正しく指すメディアセグメントにする。

今は、このヘルパーで `tfhd` を書き換えた入力のサンプルが、元の payload ではなく 4 バイト前を指したまま、テストが通っている。テストの入力が壊れていると、テストは意図とは別の内容を検証することになり、demuxer の `data_offset` の計算に不具合が入っても検出できない。

## 現状

`pbt/tests/prop_fmp4_segment_mux_demux.rs` の `rewrite_media_segment_tfhd_sample_description_index`:

- `moof` をデコードし、すべての `traf` の `tfhd.sample_description_index` を書き換えて再エンコードする。その後ろに、元の `moof` より後ろのバイト列をそのまま連結する。`trun` の `data_offset` は変えない
- `Fmp4SegmentMuxer` は、sample description index が 1 のとき `tfhd.sample_description_index` を省略する（`src/mux_fmp4_segment.rs` で `Some(0) => None` としている箇所）。省略された値を `Some` に書き換えると `tfhd` が 4 バイト伸び、`moof` も 4 バイト大きくなる
- muxer は `default_base_is_moof = true` かつ `base_data_offset` なしで出力するため、`trun` の `data_offset` は `moof` の先頭からの相対値である。`moof` が大きくなると `mdat` は後ろにずれるが、`data_offset` は元のままなので、demux したサンプルはずれた分だけ前を指す

呼び出し元:

- `sample_entry_prefers_tfhd_index`: muxer が省略した index を `Some(1)` に書き換えるため、`moof` が 4 バイト大きくなる。assert は `sample_entry` だけを見るため、サンプルが誤った位置を指したまま通っている
- `invalid_sample_description_index_is_rejected`: `Some(2)` を `Some(3)` に書き換えるため、`moof` のサイズは変わらない。エラーになることだけを確かめている

確認結果（develop で、このヘルパーと同じ書き換えを再現して確認）:

- 8 バイトのサンプル 2 つからなるセグメントで、`None` を `Some(1)` に書き換えると、`moof` は 108 バイトから 112 バイトになる
- 書き換える前は、各サンプルが元の payload を指す
- 書き換えた後は、1 つ目のサンプルが `mdat` のヘッダーの `mdat`（`6d 64 61 74`）から始まる 8 バイトを、2 つ目のサンプルが 1 つ目の payload を指す

## 設計方針

- `rewrite_media_segment_tfhd_sample_description_index` を、同じファイルの `rewrite_media_segment_moof` を使う形に作り直す
  - `rewrite_media_segment_moof` は、`moof` を書き換えた後にサイズの差を各 `trun` の `data_offset` に足して、サンプルデータを指す位置を保つ
- `sample_entry_prefers_tfhd_index` で、demux した各サンプルの `data_offset` と `data_size` が指すバイト列が、元のサンプルの payload と一致することを確認する
  - ヘルパーが `data_offset` を正しく保つことを、このテストで検出できるようにするためである
- `invalid_sample_description_index_is_rejected` は、エラーになることだけを確かめるテストであり、書き換えで `moof` のサイズも変わらないため、期待値は変えない

## 完了条件

- `rewrite_media_segment_tfhd_sample_description_index` で書き換えた入力でも、demux した各サンプルが元の payload を指す
- `sample_entry_prefers_tfhd_index` がこれを確認している。ヘルパーを `data_offset` を補正しない実装に戻すと、このテストが失敗する

## 解決方法

- `pbt/tests/prop_fmp4_segment_mux_demux.rs` の `rewrite_media_segment_tfhd_sample_description_index` を、`rewrite_media_segment_moof` の上に作り直す
- 同じファイルの `sample_entry_prefers_tfhd_index` に、demux した各サンプルが元の payload を指すことの確認を追加する
- ヘルパーを `data_offset` を補正しない実装に一時的に戻し、`sample_entry_prefers_tfhd_index` が失敗することを確認する
- ライブラリの挙動には影響しないため、`CHANGES.md` の `### misc` に記載する
