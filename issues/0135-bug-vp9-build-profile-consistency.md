# `build_vp09_box` が profile と bit depth / chroma subsampling の不整合を vpcC に書き出す

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-vp9-build-profile-consistency
- Polished: {YYYY-MM-DD}

## 目的

`build_vp09_box` が、VP Codec ISO Media File Format Binding の profile 表に反する `vpcC` を生成しないようにする。

## 現状

VP Codec ISO Media File Format Binding の profile 表では、profile 0 は 8 bit / 4:2:0 のみ、profile 1 は 8 bit / 4:2:2 または 4:4:4、profile 2 は 10 / 12 bit / 4:2:0、profile 3 は 10 / 12 bit / 4:2:2 または 4:4:4 である。

`src/bitstream/vp9.rs` の `build_vp09_box` は、bit depth が 8 / 10 / 12 のいずれか、subsampling が 0 / 1、`(0, 1)` でないことだけを検査し、profile との組み合わせは検査しない。そのため手組みの `Vp9FrameHeader` から、profile 0 / 10 bit / 4:4:4 のような `vpcC` を生成できる。

再現:

```rust
let header = Vp9FrameHeader {
    profile: 0,
    show_existing_frame: None,
    frame_type: Vp9FrameType::Key,
    show_frame: true,
    error_resilient_mode: false,
    intra_only: false,
    refresh_frame_flags: 0xFF,
    bit_depth: 10,
    color_space: 1,
    color_range: 0,
    subsampling_x: 0,
    subsampling_y: 0,
    frame_size: Vp9FrameSize::Resolved { width: 320, height: 240 },
    render_size: None,
};
let config = Vp9SampleEntryConfig {
    level: 0,
    colour_primaries: 1,
    transfer_characteristics: 1,
    matrix_coefficients: 1,
    width: 320,
    height: 240,
};
let vp09 = build_vp09_box(&header, &config).unwrap();
```

確認結果:

```text
built vpcC: profile=0, bit_depth=10, chroma=3
```

## 設計方針

- `build_vp09_box` の先頭で、`profile` と `bit_depth` / `subsampling_x` / `subsampling_y` の組み合わせを Binding の profile 表と照合する
- 表にない組み合わせは `InvalidInput` を返す
- `matrix_coefficients == 0` のとき chroma が 4:4:4 であることの検査は、この issue の範囲に含めない
- parse 側の代入処理は変えない
- 不整合な `Vp9FrameHeader` のテストを追加する

## 完了条件

- profile 0 / 10 bit / 4:4:4 の組み合わせが `InvalidInput` になる
- profile 0 / 8 bit / 4:2:0、profile 2 / 10 bit / 4:2:0 などの正当な組み合わせは引き続き成功する
- `(subsampling_x, subsampling_y) == (0, 1)` の拒否は現状を維持する
- `tests/test_bitstream_vp9.rs` に profile と色設定の不整合テストが追加されている
- `cargo test --workspace` が成功する
