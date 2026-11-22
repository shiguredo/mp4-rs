# `build_vp09_box` で VP9 プロファイルとビット深度・クロマの組み合わせを検査する

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
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
