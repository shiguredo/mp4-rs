# `build_av01_box` が `chroma_sample_position == 3` と構文に現れない `twelve_bit` を av1C に書き出す

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-av1-build-reserved-fields
- Polished: {YYYY-MM-DD}

## 目的

`build_av01_box` が、`parse_sequence_header` の検証結果と矛盾しない `av1C` だけを生成するようにする。

## 現状

`src/bitstream/av1.rs` の `parse_sequence_header` は `chroma_sample_position == 3` (CSP_RESERVED) を `InvalidData` として拒否する。一方 `build_av01_box` は `chroma_sample_position > 3` だけを拒否するため、3 を受理して `av1C` に書き出す。

また `twelve_bit` は AV1 の構文上、`seq_profile == 2` かつ `high_bitdepth == 1` のときだけ現れる。`build_av01_box` はこの組み合わせを検査せず、`Av1SequenceHeader` の `twelve_bit` をそのまま `av1C` に写す。

再現:

```rust
let seq = Av1SequenceHeader {
    seq_profile: 0,
    operating_points_cnt_minus_1: 0,
    operating_point_idc_0: 0,
    seq_level_idx_0: 0,
    seq_tier_0: 0,
    high_bitdepth: true,
    twelve_bit: true,
    monochrome: false,
    chroma_subsampling_x: 1,
    chroma_subsampling_y: 1,
    chroma_sample_position: 3,
    max_frame_width: 320,
    max_frame_height: 240,
    reduced_still_picture_header: false,
};
let config = Av1SampleEntryConfig {
    initial_presentation_delay_minus_one: None,
};
let av01 = build_av01_box(&seq, &[], &config).unwrap();
```

確認結果:

```text
built av1C: profile=0, high=1, twelve=1, csp=3
```

## 設計方針

- `build_av01_box` で `chroma_sample_position > 2` を拒否する
- `seq_profile == 2 && high_bitdepth` 以外で `twelve_bit == true` のときは拒否する。`Av1SequenceHeader` は公開フィールドを持つため、parse を通らない構築でも panic せず `InvalidInput` を返す
- `Av1cBox` のフィールドは `Av1SequenceHeader` と一致させる現状の写像を維持する
- parse 側の検証は変えない
- 不整合な `Av1SequenceHeader` のテストを追加する

## 完了条件

- `chroma_sample_position == 3` の `Av1SequenceHeader` を `build_av01_box` に渡すと `InvalidInput` になる
- `seq_profile == 0` / `high_bitdepth == true` / `twelve_bit == true` の組み合わせで `InvalidInput` になる
- `seq_profile == 2` / `high_bitdepth == true` / `twelve_bit == true` の正当な組み合わせは引き続き成功する
- `tests/test_bitstream_av1.rs` に上記のテストが追加されている
- `cargo test --workspace` が成功する
