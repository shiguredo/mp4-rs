# `TfraBox::encode` で範囲外の長さフィールドを拒否する

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-tfra-length-size-u8-overflow
- Polished: 2026-11-22

## 目的

`TfraBox` の可変長整数の長さフィールドが仕様上の範囲（`0..=3`）を外れたとき、パニックせずに `ErrorKind::InvalidInput` を返すようにする。

## 現状

`src/boxes_fmp4.rs` の `TfraBox::encode` は、`length_size_of_traf_num`、`length_size_of_trun_num`、`length_size_of_sample_num` に `+ 1` した値を `encode_variable_uint` に渡す。これらのフィールドは公開の `u8` であり、255 を設定すると `u8` の加算が溢れる。

ISO/IEC 14496-12:2015 の 8.8.10.2 と 8.8.10.3 では、3 つの長さフィールドはそれぞれ 2 ビットで、各番号を格納するバイト数から 1 を引いた値を持つ。フィールドの範囲は `0..=3` で、対応する番号の幅は 1〜4 バイトである。

エントリーの書き込みで `255 + 1` に到達すると、オーバーフロー検査が有効なビルドではパニックし、無効なビルドでは 0 にラップして `encode_variable_uint` が `InvalidData` を返す。エントリーがない場合はその加算に到達せず、範囲外の長さフィールドも 2 ビットに切り詰めて出力される。

## 設計方針

- `TfraBox::encode` は、エントリーの数にかかわらず、ヘッダーを書き込む前に 3 つの `length_size_of_*` がすべて `0..=3` であることを検査する。範囲外のフィールドがあれば `ErrorKind::InvalidInput` を返す
- 検査済みの値に 1 を足して `encode_variable_uint` に渡す。結果は必ず `1..=4` なので、計算と引数の型は `u8` のままでよい。補助関数の幅・バッファ長の検査と、デコード時に各長さフィールドを 2 ビットとして読む処理は変えない
- 番号自体が宣言された幅に収まるかの検査は 0117 の対象とし、この課題では変更しない

## 完了条件

- 3 つの長さフィールドはいずれも、範囲外ならエントリーの有無を問わず `ErrorKind::InvalidInput` になる。`tests/test_boxes_fmp4.rs` では、各フィールドを単独で変更し、境界直後の値と `u8::MAX` をエントリーがある場合とない場合の両方で検証する
- 既存の `tfra_box_roundtrip` PBT で、範囲内の長さフィールドとその幅に収まる番号が従来どおり往復する。既存の短いバッファのテストでも `ErrorKind::InsufficientBuffer` が維持され、`cargo test --workspace` が成功する
