# `build_vp09_box` の PBT と重複する単体テストを削除する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-vp9-build-unit-tests
- Polished: {YYYY-MM-DD}

## 目的

`tests/test_bitstream_vp9.rs` にある `build_vp09_box` の受理系の単体テストのうち、3 本を削除する。この 3 本は、`pbt/tests/prop_bitstream_vp9.rs` の `build_vp09_box_config_roundtrip` が毎ケース確かめている内容しか持たない。shiguredo-rust 規約の「PBT でカバーできるものを単体テストで書かないこと」に従う。

## 現状

`build_vp09_box_config_roundtrip` は、各ケースで `PROFILE_TABLE` の全 9 通りの組み合わせ（profile、ビット深度、サブサンプリング）を順に試す。乱数で選ぶのは次のものだけで、組み合わせ自体は確率ではなく決定的に、毎ケースすべて通る。

- `color_range`
- frame の寸法
- config（level と色特性 3 つは任意の `u8`、width は 1..=1920、height は 1..=1080）

各組み合わせで、次を確かめる。

- `parse_frame_header` の結果が、意図した profile / bit_depth / subsampling と一致する
- `build_vp09_box` が成功する
- vpcC の profile、bit_depth、chroma_subsampling（`expected_vpcc_chroma`）、video_full_range_flag が期待どおりである
- vpcC の level と色特性 3 つ、visual の width / height が config と一致する
- codec_initialization_data と unknown_boxes が空である
- encode → decode で元の `Vp09Box` に戻り、decode が消費したサイズが encode の長さと一致する

`tests/test_bitstream_vp9.rs` の単体テストを、1 本ずつ判定した。

- `build_vp09_box_chroma_subsampling_422`: profile 1、4:2:2 のキーフレームで、chroma_subsampling が 2 になることだけを確かめる（color_space 1、color_range 0、`default_config`）。`PROFILE_TABLE` の (profile 1、8 ビット、(1, 0)、2) で、毎ケース同じことを確かめているため包含される
- `build_vp09_box_chroma_subsampling_444`: profile 1、4:4:4 で、chroma_subsampling が 3 になることだけを確かめる。(profile 1、8 ビット、(0, 0)、3) で包含される
- `build_vp09_box_roundtrip`: profile 2、10 ビット、4:2:0 のキーフレームと、level 41・BT.709 の定数・640x480 の config で、encode → decode のラウンドトリップを確かめる。定数の値は確かめていない。(profile 2、10 ビット、(1, 1)、1) で、乱数の config によるラウンドトリップを毎ケース行っているため包含される
- `build_vp09_box_config_colour_reflected`: 包含されないため対象外とする
  - このテストは、config に `Vp9SampleEntryConfig::COLOUR_PRIMARIES_BT2020` / `TRANSFER_CHARACTERISTICS_BT2020` / `MATRIX_COEFFICIENTS_BT2020` を渡し、vpcC の値をリテラルの 9 / 14 / 9 と比べている
  - PBT は、乱数の値がそのまま写ることしか確かめない。リポジトリ全体で、この 3 つの定数を `src/bitstream/vp9.rs` の定義以外で使うのはこのテストだけなので、定数の値を確かめているのもこのテストだけである

次の 2 本も、PBT にない内容を確かめているので対象外とする。

- `build_vp09_box_fixed_and_derived_values`: 固定値と、`default_config` が写った値を確かめる
- `build_vp09_box_srgb_full_range`: PBT が扱わない color_space 7（sRGB）を確かめる

3 本を削除すると、`Decode`、`Encode`、`Vp09Box` の import が使われなくなる。`cargo clippy --tests` は、これらを未使用の import として警告する（develop で確認）。

## 設計方針

- `build_vp09_box_chroma_subsampling_422` / `build_vp09_box_chroma_subsampling_444` / `build_vp09_box_roundtrip` を削除する
- 削除で使われなくなる import（`Decode`、`Encode`、`Vp09Box`）を取り除く
- 他の単体テストと PBT は変更しない

## 完了条件

- `tests/test_bitstream_vp9.rs` から上の 3 本が削除されている
- `build_vp09_box_config_colour_reflected`、`build_vp09_box_fixed_and_derived_values`、`build_vp09_box_srgb_full_range` は残っている
- `cargo clippy --workspace --all-targets -- -D warnings` が成功する
- `cargo test --workspace` が成功する
