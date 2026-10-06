# `TfraBox` の可変長整数が、宣言した幅に収まらない値を切り詰めて書く

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-tfra-variable-uint-truncation
- Polished: 2026-10-06

## 目的

`tfra` の `traf_number`、`trun_number`、`sample_number` が、`length_size_of_*` のバイト幅に収まらないとき、値を切り詰めずにエンコードを失敗させる。

## 現状

ISO/IEC 14496-12:2022 の 8.8.10.2 では、これらのフィールドの幅は `(length_size_of_* + 1)` バイトである。`length_size_of_*` は 2 ビットなので、幅は 1 から 4 バイトである。

`src/boxes_fmp4.rs` の `encode_variable_uint` は、幅 1 から 3 バイトのとき、その幅に収まらない上位ビットを捨てる。1 バイト幅は `value as u8` なので、256 以上は下位 8 ビットだけが残る。2 バイト幅は 16 ビットより上を、3 バイト幅は 24 ビットより上を捨てる。4 バイト幅は `u32` 全体を書く。バッファ長の不足は `Error::check_buffer_size` で返す。値の幅は見ていない。issue 0015 が直したのはバッファ長であり、値の切り詰めは残っている。

`Fmp4SegmentMuxer::mfra_bytes` は、エントリの最大値から `length_size_of_traf_num` を選ぶ。`trun_number` と `sample_number` は 1 なので、この経路では幅を超えない。公開の `TfraBox` を直接 encode すると、`length_size_of_traf_num == 0`（1 バイト）かつ `traf_number > 255` のとき値が変わる。デコードはそのバイト列を読むので、切り詰めた値で往復する。

## 設計方針

- `encode_variable_uint` は、`value` が `byte_count` バイトに収まらないとき `ErrorKind::InvalidInput` を返す
- 幅の選択を muxer 側で変える必要はない。今の最大値からの選択は、この検査を通過する

## 完了条件

- 1 バイト幅で 256 以上の `traf_number` を持つ `TfraBox` の encode がエラーになる
- 1 バイト幅で 255 以下の値の encode は成功し、デコードすると同じ値に戻る
- `mfra_bytes` が、traf 番号の最大値に合わせて今どおり幅を選んだ結果は成功する
