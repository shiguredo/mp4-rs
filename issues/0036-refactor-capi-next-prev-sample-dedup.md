# c-api の next_sample / prev_sample が大規模に重複している

- Priority: Medium
- Created: 2026-07-20
- Completed: 2026-07-30
- Model: qwen3.8-max-preview
- Branch: feature/refactor-capi-next-prev-sample-dedup
- Polished: 2026-07-20

## 目的

C API の `mp4_file_demuxer_next_sample()` と `mp4_file_demuxer_prev_sample()` は、`demuxer.inner.next_sample()` を呼ぶか `demuxer.inner.prev_sample()` を呼ぶか以外のロジック（tracks 初期化、sample_entry のキャッシュ lookup/生成、`Mp4DemuxSample` の構築、エラーハンドリング）が完全に同一である。修正漏れのリスクが高い。

## 優先度根拠

直ちにバグを引き起こすわけではないが、エラーメッセージの関数名部分だけが異なる同一ロジックの重複は、将来的な修正漏れリスクが高い。

## 現状

`crates/c-api/src/demux.rs:608-680`（next_sample）と `714-786`（prev_sample）で、以下のロジックが重複:

- tracks 初期化（`mp4_file_demuxer_get_tracks()` 呼び出し）
- sample_entry のキャッシュ lookup / 生成
- `Mp4DemuxSample` の構築
- エラーハンドリング（`set_last_error`）

## 設計方針

共通の内部関数 `fn get_sample_impl(demuxer, out_sample, direction)` に抽出し、`next` / `prev` の違いは enum（`SampleDirection::Next` / `SampleDirection::Prev`）で吸収する。enum は shiguredo-rust スキルの「トレイトを作らないこと / Enum で十分なケースが多い」方針に合致する。

エラーメッセージの関数名部分（`[mp4_file_demuxer_next_sample]` vs `[mp4_file_demuxer_prev_sample]`）は enum から導出する。

## 完了条件

- 共通の内部関数に抽出され、重複が解消されること
- 公開 API の挙動が変わらないこと
- エラーメッセージの関数名が正しく維持されること
- 既存のテストが通ること
- `cargo clippy` が通ること

## 解決方法

コード変更なしで closed にした。

重複は実在するが、差は `inner.next_sample()` / `inner.prev_sample()` 呼び出しとエラーメッセージの関数名だけで、現状ドリフトもない。修正漏れの実害が出るまで先送りし、そのときに共通内部関数へ抽出すれば十分と判断した。
