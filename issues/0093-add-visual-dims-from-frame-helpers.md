# 単一フレーム由来の visual 寸法を u16 に落とすヘルパーを追加する

- Created: 2026-08-28
- Completed: {YYYY-MM-DD}
- Branch: feature/add-visual-dims-from-frame-helpers
- Polished: 2026-10-07

## 目的

VP8 / VP9 で「このフレームの解像度を Visual Sample Entry の width / height に載せる」ときに、利用側が毎回書く寸法変換とエラー処理を共通化する。

## 現状

- `Vp8SampleEntryConfig` / `Vp9SampleEntryConfig` の `width` / `height` は Visual Sample Entry 向けの `u16`
- doc 上、これらは「トラック全体を収容できる上限」を呼び出し側が集約して渡す前提になっている
- 一方で単一キーフレームから仮の sample entry を組む用途では、フレーム header の寸法をそのまま載せたいことが多い
- VP8: `parse_frame_header` の `Vp8KeyFrameInfo::{width,height}` は既に `u16` だが、キーフレーム以外では `keyframe` が `None`
- VP9: `Vp9FrameSize::Resolved { width, height }` は `u32`（仕様上 1..=65536）。Visual Sample Entry は `u16`（最大 65535）なので、65536 や `NotPresent` / `UsesRefFrames` を拒否する変換が毎回必要
- `build_vp08_box` は header を受け取らず寸法は常に config。`build_vp09_box` は header を受け取るが visual 寸法はやはり config から取る。この非対称さ自体はトラック集約の設計として妥当だが、単一フレーム用途のボイラープレートは残る

## 設計方針

`bitstream::vp9` と `bitstream::vp8` に、フレーム由来の寸法を Visual Sample Entry 用の `(u16, u16)` へ落とす公開ヘルパーを追加する。変換と拒否の判定を 1 箇所に集約し、呼び出し側が毎回書くボイラープレートをなくす。

VP9 (`src/bitstream/vp9.rs`):

```rust
pub fn visual_dimensions_from_frame_size(frame_size: Vp9FrameSize) -> Result<(u16, u16)>
```

- `Resolved` かつ width / height が 1..=65535 なら `Ok` で返す
- `NotPresent` / `UsesRefFrames`、または width / height が 0 / 65536 のときは `ErrorKind::InvalidInput` を返す
  (0 は `Vp9FrameSize` が pub なので手組みで作られ得る。parse 経由の `Resolved` は 1..=65536 になる)

VP8 (`src/bitstream/vp8.rs`):

```rust
pub fn visual_dimensions_from_frame_header(frame: &Vp8FrameHeader) -> Result<(u16, u16)>
```

- `keyframe` が `Some` なら `Ok` で返す。`Vp8KeyFrameInfo::horizontal_scale` / `vertical_scale` は適用しない (既存 API と同じく keyframe の width / height をそのまま使う)
- `keyframe` が `None` (interframe) のときは `ErrorKind::InvalidInput` を返す

- 既存の `build_vp08_box` / `build_vp09_box` のシグネチャと「トラック全体の上限は config」という契約は変えない
- 単一フレームから box まで一気に組む高レベル API は別 issue とし、本 issue は寸法変換に限定する

## 完了条件

- `bitstream::vp9` に `visual_dimensions_from_frame_size` が公開されている
- `bitstream::vp8` に `visual_dimensions_from_frame_header` が公開されている
- `Resolved` の 1..=65535 が `Ok` になり、0 / 65536 / `NotPresent` / `UsesRefFrames` がエラーになる
- キーフレームの VP8 header が `Ok`、interframe がエラーになる
- `tests/test_bitstream_vp9.rs` / `tests/test_bitstream_vp8.rs` のユニットテストで上記を確認している
- 既存の `build_vp08_box` / `build_vp09_box` の挙動は変わっていない
- `cargo test` が pass する
