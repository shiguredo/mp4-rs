# `TfraBox::encode` の `length_size_* + 1` が `u8` で溢れ、debug ビルドで panic する

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-tfra-length-size-u8-overflow
- Polished: {YYYY-MM-DD}

## 目的

`TfraBox` の可変長整数の長さフィールドが仕様上の範囲 (0..=3) を外れたとき、panic せずに `InvalidInput` を返すようにする。

## 現状

`src/boxes_fmp4.rs` の `TfraBox::encode` は、`length_size_of_traf_num`、`length_size_of_trun_num`、`length_size_of_sample_num` に `+ 1` した値を `encode_variable_uint` に渡す。これらのフィールドは公開の `u8` であり、255 を設定すると `u8` の加算が溢れる。

debug ビルドでは `attempt to add with overflow` で panic し、release ビルドでは 0 にラップして不正な長さとして扱われる。ISO/IEC 14496-12 の `length_size_of_*` は 2 ビット (0..=3) なので、255 は入力時点で不正である。

再現:

```rust
let tfra = TfraBox {
    version: 0,
    track_id: 1,
    length_size_of_traf_num: 255,
    length_size_of_trun_num: 0,
    length_size_of_sample_num: 0,
    entries: vec![TfraEntry {
        time: 0,
        moof_offset: 0,
        traf_number: 1,
        trun_number: 1,
        sample_number: 1,
    }],
};
tfra.encode_to_vec();
```

確認結果:

```text
panicked: attempt to add with overflow
```

## 設計方針

- `TfraBox::encode` の先頭で、3 つの `length_size_*` がすべて 0..=3 であることを検査し、範囲外なら `Error::invalid_input` を返す
- `+ 1` は `u32` または `usize` で計算し、`u8` の加算を使わない
- `encode_variable_uint` の `byte_count` 検査は現状を維持し、呼び出し前の検査と二重になってもよい
- decode 側は 2 ビットにマスクして読む現状の動作を維持する

## 完了条件

- `length_size_of_traf_num == 255` の `TfraBox::encode` が panic せず `InvalidInput` を返す
- `length_size_of_trun_num == 255` と `length_size_of_sample_num == 255` でも同様に `InvalidInput` を返す
- 0..=3 の値では従来どおり encode でき、decode すると同じ値に戻る
- `tests/test_boxes_fmp4.rs` に 255 のケースが追加されている
- `cargo test --workspace` が成功する
