# `build_vp09_box` で VP9 プロファイルとビット深度・クロマの組み合わせを検査する

- Created: 2026-11-22
- Completed: 2026-11-24
- Branch: feature/fix-vp9-build-profile-consistency
- Polished: 2026-11-23

## 目的

`build_vp09_box` で、`Vp9FrameHeader` のプロファイルに合わないビット深度またはクロマサブサンプリングを拒否する。

## 現状

[VP Codec ISO Media File Format Binding v1.1 の VP Codec Configuration Box](https://github.com/webmproject/vp9-dash/blob/master/VPCodecISOMediaFileFormatBinding.md#vp-codec-configuration-box) のプロファイル表は、`vpcC` に対して次の組み合わせを定める。

| プロファイル | ビット深度 | クロマサブサンプリング |
| --- | --- | --- |
| 0 | 8 ビット | 4:2:0 |
| 1 | 8 ビット | 4:2:2 または 4:4:4 |
| 2 | 10 または 12 ビット | 4:2:0 |
| 3 | 10 または 12 ビット | 4:2:2 または 4:4:4 |

`src/bitstream/vp9.rs` の `build_vp09_box` は、プロファイル、ビット深度、クロマサブサンプリングを個別に検査するが、それらの組み合わせは検査しない。`Vp9FrameHeader` は公開フィールドを持つため、解析を経ずに渡した値は、個々の範囲に収まっていても上の表に合うとは限らない。

## 設計方針

- `build_vp09_box` は既存の個別検査とクロマの変換を済ませてから、`VpccBox` の構築前に表との組み合わせを確認する。ビット深度またはクロマがプロファイルに合わなければ `ErrorKind::InvalidInput` を返す。公開ドキュメントのエラー条件にも追記する
- 正常な値はそのまま写す。既存の 4:2:0、4:2:2、4:4:4 の `vpcC` への変換と、`vpcC` に対応値のない 4:4:0 の拒否は変更しない
- `parse_frame_header`、`VpccBox` の直接のエンコード・デコード、および `matrix_coefficients` とクロマの整合性の検査は対象外とする

## 完了条件

- `tests/test_bitstream_vp9.rs` の公開 API のテストでは、各プロファイルで有効な設定を基準に、既存の個別検査とクロマの変換を通る値だけを使う。ビット深度だけが表に合わない場合と、クロマだけが表に合わない場合を別々に確認し、いずれも `ErrorKind::InvalidInput` となる。4:4:0 の既存の拒否テストも成功する
- 既存の `build_vp09_box_config_roundtrip` PBT で、表に含まれる全 9 通りの組み合わせを網羅し、構築に成功すること、プロファイル、ビット深度、クロマの変換結果が保たれること、ラウンドトリップできることを確認する
- `cargo test --workspace` が成功する

## 解決方法

- `src/bitstream/vp9.rs` の `build_vp09_box` に、既存の個別の検査とクロマの変換の後、`VpccBox` の構築前に、プロファイルとビット深度・クロマの組み合わせの検査を追加した
  - `(bit_depth != 8) != (profile >= 2)` なら `VP9 profile {p} requires bit_depth {8 | 10 or 12}, got {bd}` で拒否する
  - `(クロマが 4:2:0) != (profile が 0 か 2)` なら `VP9 profile {p} requires {4:2:0 | 4:2:2 or 4:4:4} chroma subsampling, got subsampling {x},{y}` で拒否する
  - いずれも `ErrorKind::InvalidInput` を返す。既存の 4:2:0 / 4:2:2 / 4:4:4 の変換と 4:4:0 の拒否は変えていない
  - 根拠 (Binding v1.1 の VP Codec Configuration Box の semantics にあるプロファイルの表、VP9 spec Section 7.2 の Profile の表、Section 6.2.2 と 6.2 の代入、Section 7.2.2 の bitstream conformance) と、将来の改訂で変わる可能性があることをコードコメントに残した
- `build_vp09_box` の doc のエラー条件に、組み合わせの検査を追記した
- 差分外の `read_color_config` の doc が color_config syntax を Section 7.2.2 としていたのを、正しい Section 6.2.2 に直した
- `parse_frame_header` が返すキーフレームと intra-only frame の header は表に合うか 4:4:0 で、inter frame / `show_existing_frame` の header は組み合わせの検査より前に拒否するため、`parse_frame_header` の結果をそのまま渡す場合の挙動は変わらない
- テスト
  - `tests/test_bitstream_vp9.rs` に、各プロファイルの有効な header を基準に、ビット深度だけ、クロマだけを表に合わない値にして拒否する 2 件と、複数の検査に当てはまる header でどの検査の理由になるかを固定する 1 件を追加した
  - 組み合わせの検査が同じ種別で拒否するため、既存の拒否テスト 7 件を、エラー種別だけでなく理由の文字列まで照合する形にした。あわせて節見出しを `build_vp09_box: 受理系` / `拒否系` に整理し、4:4:0 の拒否テストと inter frame の render_size のテストを該当する節へ移した
  - `pbt/tests/prop_bitstream_vp9.rs` の `build_vp09_box_config_roundtrip` で、表の全 9 通り (`PROFILE_TABLE`) を毎ケース順に試し、構築に成功すること、プロファイル・ビット深度・クロマの変換結果が保たれること、ラウンドトリップできることを確かめるようにした
- `CHANGES.md` の `## develop` に [FIX] を追記した
