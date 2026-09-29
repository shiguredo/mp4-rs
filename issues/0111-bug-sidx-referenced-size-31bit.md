# `SidxBox` が 31 ビットを超える `referenced_size` をマスクして書き、エラーにしない

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-sidx-referenced-size-31bit
- Polished: {YYYY-MM-DD}

## 目的

`sidx` の `referenced_size` が 31 ビットに収まらないとき、上位ビットを捨てずにエンコードを失敗させる。2 GiB 以上 4 GiB 未満の値は bit 31 が立っているので、マスクすると書かれる長さは入力よりちょうど 2 GiB（2^31 バイト）小さい。

## 現状

ISO/IEC 14496-12:2022 の 8.16.3.2 では、`referenced_size` は `unsigned int(31)` である。`sap_type` は `unsigned int(3)` である。

`src/boxes_fmp4.rs` の `SidxBox` の encode は、`referenced_size` を `& 0x7FFFFFFF` してから 31 ビットに置く。`0x8000_0000` は 0 として書かれる。`sap_type` は `& 0x7` で下位 3 ビットだけを書く。どちらもエラーにならない。デコードは 31 ビットと 3 ビットを読むので、この実装が書いた値は往復する。仕様上の幅を超えた入力だけが欠ける。同じワードの `sap_delta_time` も `& 0x0FFFFFFF` で 28 ビットに切る。この issue では `sap_delta_time` は変えない。

`src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` は、メディアセグメント長を `u32::try_from` で検査する。`u32::MAX` までは通過し、その値が 31 ビットを超えていても `SidxReference.referenced_size` に入る。その後の `SidxBox` の encode で上位ビットが落ちる。

## 設計方針

- `SidxBox` の encode は、`referenced_size` が 31 ビットに収まらない、または `sap_type` が 3 ビットに収まらないとき `ErrorKind::InvalidInput` を返す
- muxer は `SidxBox` に渡す前に、同じ上限で失敗させる。失敗時の内部状態は issue の範囲外とし、この issue では値の検査だけを足す
- デコードは、ワイヤ上の 31 ビットと 3 ビットを今どおり読む

## 完了条件

- `referenced_size == 0x8000_0000` の `SidxBox` の encode がエラーになる
- `referenced_size == 0x7FFFFFFF` の encode は成功し、デコードすると同じ値に戻る
- `sap_type == 8` の encode がエラーになる
- `create_media_segment_metadata_with_sidx` は、参照サイズが 31 ビットを超えるときバイト列を返さない
