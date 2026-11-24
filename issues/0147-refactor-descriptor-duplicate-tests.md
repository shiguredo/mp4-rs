# ディスクリプターの単体テストから、ほかのテストと同じ確認を繰り返すものを削除する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-descriptor-duplicate-tests
- Polished: {YYYY-MM-DD}

## 目的

`tests/test_descriptors.rs` から、ほかの単体テストや PBT と同じ分岐を同じ観点で確かめているだけのテストを削除する。PBT は `pbt/tests/prop_descriptors.rs` の `descriptors_decode_sequentially` を指す。

shiguredo-rust の「PBT でカバーできるものを単体テストで書かないこと」と、shiguredo-test の「無駄なテストを絶対に追加しないこと」に合わせる。

## 現状

### 単体テストどうしの重複

`descriptor_error_tests` の次の 3 件は、どれも同じ形の入力を使う。タグは 6、サイズは 1 で、`predefined` だけを 2 以外の値にしている。3 件とも、`SlConfigDescriptor::decode_payload` の同じ分岐（`predefined != 2`）を通る。

| テスト | `predefined` | 照合 |
| --- | --- | --- |
| `sl_config_descriptor_unsupported_predefined` | 0 | `ErrorKind::Unsupported` まで照合する |
| `sl_config_descriptor_predefined_1` | 1 | `is_err()` だけ |
| `sl_config_descriptor_predefined_3` | 3 | `is_err()` だけ |

`predefined` の値による分岐は、2 か、2 以外かの 2 通りしかない。後の 2 件は、1 件目より弱い確認を繰り返しているだけである。

### PBT と重なる単体テスト

`descriptors_boundary_tests` の次の 3 件は、エンコードしてからデコードするラウンドトリップのテストである。使っている値に依存する分岐はない。通る分岐は、任意フィールドの有無と `DecoderSpecificInfo` の有無で、どちらも `descriptors_decode_sequentially` がケースごとに半々の確率で選ぶ。`descriptors_decode_sequentially` は、生成したディスクリプターがラウンドトリップで値と消費バイト数を保つことを、全フィールドについて確認している。

- `sl_config_descriptor_fixed`
  - `SlConfigDescriptor` はフィールドを持たない
  - PBT の `arb_any_descriptor` は 4 分の 1 の確率で `SlConfigDescriptor` を生成し、`EsDescriptor` は必ず `SlConfigDescriptor` を子に持つ
- `decoder_config_descriptor_aac_defaults`
  - AAC 用の定数を使った、`DecoderSpecificInfo` を持たない `DecoderConfigDescriptor` である
  - デコード後に `object_type_indication` / `stream_type` / `up_stream` を 0x40 / 0x05 / 0 と比べているが、これは定数を定義したリテラルと同じ値を比べているだけである
- `es_descriptor_minimal`
  - 任意フィールド（`depends_on_es_id` / `url_string` / `ocr_es_id`）をすべて持たない `EsDescriptor` である
  - `es_id` と `stream_priority` を 1 と 0 と比べているが、これは `EsDescriptor::MIN_ES_ID` と `EsDescriptor::LOWEST_STREAM_PRIORITY` を定義したリテラルと同じ値を比べているだけである

### 重複に含めないもの

- `descriptors_boundary_tests` の境界値を使う次の 3 件は、PBT と重ならない
  - 対象は、`decoder_specific_info_empty`（ペイロード長 0）、`decoder_config_descriptor_stream_type_boundary`（`stream_type` = 63、`up_stream` = 1）、`es_descriptor_all_options`（`stream_priority` = 31、`buffer_size_db` = 0xFFFFFF、ビットレート `u32::MAX`）である
  - PBT が使う noprop の `sample_usize_in` / `sample_u64_in` / `sample_u32` は一様分布で、境界値に寄せない。そのため、PBT はこれらの値を確実には生成しない
- `descriptor_error_tests` の `*_invalid_tag` の 4 件
  - `EsDescriptor` / `DecoderConfigDescriptor` / `SlConfigDescriptor` の 3 件は、どれも `decode_descriptor_payload` の同じタグ検査を通る
  - ただし、それぞれが自分の型のタグを使っていることを確かめているため、重複ではない
- `DecoderSpecificInfo` の境界検査の 2 件
  - `tests/test_descriptors.rs` の `decoder_specific_info_payload_exceeds_boundary` と、`src/descriptors.rs` の `mod tests` の `decoder_specific_info_payload_exceeds_boundary_with_usize_max_size` は、同じ分岐を通る
  - どちらも 0146 で期待値が変わるか削除されるため、この issue では扱わない
- `esds` ボックスのテストの重なり
  - `tests/test_boxes_sample_entry.rs` の `esds_box_aac_lc` と、`pbt/tests/prop_codec_boxes.rs` の `esds_box_roundtrip` も同じ形で重なっている
  - これはボックスのテストなので、この issue の対象外とする

## 設計方針

- `tests/test_descriptors.rs` から次の 5 件を削除する
  - `sl_config_descriptor_predefined_1`
  - `sl_config_descriptor_predefined_3`
  - `sl_config_descriptor_fixed`
  - `decoder_config_descriptor_aac_defaults`
  - `es_descriptor_minimal`
- 残すテストと PBT は変えない
- 削除によって使われなくなる `use` があれば、合わせて取り除く

## 完了条件

- 5 件を削除した状態で、`cargo test --workspace` が成功する
- `MP4_RS_PBT_SEED` に同じ値を指定し、削除の前後で `cargo llvm-cov --tests --workspace` を実行する。`src/descriptors.rs` の行カバレッジが下がらないことを確認する
