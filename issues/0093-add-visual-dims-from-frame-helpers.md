# 単一フレーム由来の visual 寸法を u16 に落とすヘルパーを追加する

- Created: 2026-08-28
- Completed: {YYYY-MM-DD}
- Branch: feature/add-visual-dims-from-frame-helpers
- Polished: {YYYY-MM-DD}

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

- VP9 向けに、`Vp9FrameSize`（または `Vp9FrameHeader`）から Visual Sample Entry 用 `(u16, u16)` を返すヘルパーを追加する
  - `Resolved` かつ 1..=65535 なら `Ok`
  - `NotPresent` / `UsesRefFrames`、または 65536 は `ErrorKind::InvalidInput`
- VP8 向けは、キーフレーム header から `(u16, u16)` を返す薄いヘルパーを追加するか、上記 VP9 ヘルパーと対になる形で「フレームから visual 寸法を取る」手順を rustdoc で明示する（キーフレーム以外は Err）
- 既存の `build_vp08_box` / `build_vp09_box` のシグネチャと「トラック全体の上限は config」という契約は変えない
- 単一フレームから box まで一気に組む高レベル API は別 issue とし、本 issue は寸法変換に限定する

## 完了条件

- VP9 の `Resolved` 寸法を `u16` に落とす（または拒否する）公開ヘルパーがある
- VP8 についても、キーフレーム寸法を取る経路がヘルパーまたは rustdoc で対称に示されている
- 65536 や非 Resolved がエラーになることをユニットテストで確認している
- 既存の `build_vp08_box` / `build_vp09_box` の挙動は変わっていない
- `cargo test` が pass する
