# Hybrid MP4 の取り扱いについての補足ドキュメントを追加する

- Priority: Medium
- Created: 2026-07-28
- Model: Opus 4.7
- Branch: feature/add-hybrid-mp4-doc
- Polished: {YYYY-MM-DD}
- Reporter: @tohta

## 目的

`Mp4FileMuxer::advance_position()` を用いた Hybrid MP4 の書き出し方を、利用者が単独で組み立てられるレベルまで説明する補足ドキュメントを `docs/` 配下に追加する。

現状、当該 API は Rust および C API 双方の doc コメントで「OBS の Hybrid MP4 のように、サンプルデータの間に moof / mdat ヘッダなどの非サンプルデータが挿入される場合に使用する」と説明されているが、以下が API リファレンス側からは伝わらない。

- Hybrid MP4 とは何か（何を解こうとしている形式か）
- 標準 MP4 / fMP4 との違い
- 本 crate が担当する範囲と、利用側が担当する範囲（moof / mdat ヘッダの組み立ては利用側の責務）
- 実際に書き出す際の骨格コード

`docs/subtitle.md` と同じ粒度で 1 本の補足ドキュメントを追加し、`src/docs.rs` から `include_str!` で取り込む。

## 優先度根拠

Medium。既存の API (`Mp4FileMuxer::advance_position()`) は 2026.3.0 で追加済みで、機能そのものは動作するため High ではない。ただし、この API 単体では組み立て手順の全体像が伝わらず、利用者が OBS のブログ記事や hisui の実装を都度読みに行く必要がある。本 crate 単体で自己完結させるためには追加が必要であり、後回しにする理由も無いため Low ではない。

## 現状

- `src/mux_mp4_file.rs:520` の `Mp4FileMuxer::advance_position()` に、Hybrid MP4 を意図した用途である旨のコメントがある
- 同等の API が C API にも公開されている
  - `crates/c-api/src/mux.rs:827`（`mp4_file_muxer_advance_position()`）
  - `crates/c-api/include/mp4.h:2430`
- `CHANGES.md` の 2026.3.0 に `[ADD] Mp4FileMuxer::advance_position() メソッドを追加する` のエントリがあり、Hybrid MP4 対応が動機であることが記載されている
- 一方、`docs/` 配下には Hybrid MP4 の説明が無く、既存の補足ドキュメントは `docs/subtitle.md` のみ
- `src/docs.rs` は `docs/*.md` を `include_str!` で取り込み `pub mod` として公開する index になっており、Rust コード例は doctest で検証される仕組み

## 設計方針

- `docs/hybrid_mp4.md` を新規追加する
- `src/docs.rs` に `pub mod hybrid_mp4 {}` を追加し、`#[doc = include_str!("../docs/hybrid_mp4.md")]` で取り込む
  - `subtitle.md` と同じ扱いにする
  - Rust コード例は doctest として検証されるため、`cargo test --doc` が通る形で書く
- 文体・粒度は `docs/subtitle.md` に揃える（節構成・箇条書きスタイル・コード例の見せ方）
- Rust API の骨格コード例を必ず入れる。C API の呼び出し例は Rust の doctest では扱えないので、必要なら文中で C API 側の対応関数名を参照するだけに留めるか、別ファイルにするかは実装者判断
- OBS のブログ記事および hisui の実装への参考リンクを本文末尾に置く

想定する節構成（`docs/subtitle.md` を参考にした叩き台。実装時に必要に応じて調整）:

- Hybrid MP4 とは
- 標準 MP4 / fMP4 との違い
- 本 crate での対応方針（`advance_position()` の役割、利用側と crate 側の責任分界）
- 書き出しの骨格コード（Rust）
- 注意事項（`advance_position()` を呼んだ直後の次サンプルは強制的に新規チャンク開始になる、`data_offset` は crate 内部の書き込み位置と一致させる必要がある、など既存 API doc に書かれている制約を利用者視点で整理）
- 参考リンク

## 完了条件

- `docs/hybrid_mp4.md` が新規作成され、以下をすべて含んでいること
  - Hybrid MP4 の定義と背景（何を解く形式か、標準 MP4 / fMP4 との違い）
  - 本 crate での対応方針（`Mp4FileMuxer::advance_position()` の役割、moof / mdat ヘッダの組み立ては利用側の責務であること）
  - Rust API での骨格コード例（doctest として `cargo test --doc` が通る）
  - 利用側で意識すべき制約（`advance_position()` を呼んだ後の挙動、`data_offset` と内部書き込み位置の整合など）
  - 参考リンク（OBS ブログ・hisui 実装）
- `src/docs.rs` に `hybrid_mp4` モジュールが追加され、`cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` および `cargo test --doc` が通ること
- 粒度と体裁が `docs/subtitle.md` と揃っていること

## 解決方法

- `docs/hybrid_mp4.md` を新規作成する
  - 「設計方針」節の想定構成を叩き台にしつつ、OBS ブログと hisui `hybrid_writer.rs` を読んで正確な説明を書く
  - Rust コード例は `Mp4FileMuxer::new()` → `initial_boxes_bytes()` → サンプル書き出しループ（`advance_position()` を挟みつつ `append_sample()`）→ `finalize()` の骨格を示す
- `src/docs.rs` に `pub mod hybrid_mp4 {}` を追加し、`#[doc = include_str!("../docs/hybrid_mp4.md")]` で取り込む
- 検証
  - `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps`
  - `cargo test --doc`
- `CHANGES.md` に `[ADD] Hybrid MP4 の取り扱いについての補足ドキュメントを追加する` のエントリを追加する

## 参考

- OBS Studio Hybrid MP4 の解説: <https://obsproject.com/blog/obs-studio-hybrid-mp4>
- hisui の Hybrid MP4 ライター実装: <https://github.com/shiguredo/hisui/blob/516339747ad8083b6ddb61e88546ce128cafe586/src/mp4/hybrid_writer.rs#L84>
- 既存 API の doc コメント: `src/mux_mp4_file.rs:520`（`Mp4FileMuxer::advance_position()`）
- 既存の補足ドキュメント参考例: `docs/subtitle.md`
