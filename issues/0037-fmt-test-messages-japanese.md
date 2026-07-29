# PBT テストと C API テストの expect / assert メッセージが英語であり AGENTS.md 規約に違反している

- Priority: Medium
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/update-test-messages-japanese
- Polished: 2026-07-29

## 目的

AGENTS.md に「テストのログメッセージは全て日本語にすること」と明記されているが、`pbt/tests/` と `crates/c-api/tests/e2e.rs` に英語のパニック／アサーションメッセージが残っている。`tests/decode_encode_test.rs` は日本語メッセージで統一されており規約に準拠している。

## 優先度根拠

AGENTS.md の規約違反であり、プロジェクト全体の一貫性の観点から修正すべき。ただし機能的な影響はない。

## 現状

2026-07-29 時点の実測（対象パスのみ。メッセージ引数のうち、ひらがな・カタカナ・漢字を含まない英字メッセージの件数）:

| マクロ | `pbt/tests/` | `e2e.rs` |
| --- | ---: | ---: |
| `.expect()` | 約 380 | 6 |
| `assert!` | 約 13 | 7 |
| `assert_eq!`（第 3 引数以降） | 約 4 | 0 |
| `prop_assert!` | 約 18 | 0 |
| `prop_assert_eq!`（第 3 引数以降） | 約 23 | 0 |
| `panic!` | 約 7 | 0 |

- `pbt/tests/` は 14 ファイル。同じディレクトリには 0054 で追加された日本語 `.expect()` も約 426 件あり、日英が混在している
- `e2e.rs` の `assert!` 7 件のうち 6 件はメッセージが次行
- `crates/c-api/tests/test_boxes.rs` の `.expect()` は既に日本語であり、本 issue の対象外

英語メッセージの例:

- `pbt/tests/prop_fmp4_segment_mux_demux.rs:198`: `.expect("failed to create media segment")`
- `pbt/tests/prop_auxiliary.rs:769`: `.expect("sync sample")`
- `pbt/tests/prop_container_boxes.rs:376`: `prop_assert!(false, "Expected StcoBox, got Co64Box")`
- `pbt/tests/prop_mux_demux.rs:416`: `prop_assert_eq!(..., "keyframe mismatch at sample {}", i)`
- `pbt/tests/prop_boxes_moov_tree.rs:430`: `panic!("Expected Fixed variant")`
- `crates/c-api/tests/e2e.rs:58`: `.expect("Failed to execute cc command")`
- `crates/c-api/tests/e2e.rs:113`: `assert!(..., "simple_mux_demux execution failed")`

`shiguredo-git` のブランチ prefix に `fmt-` は無いため、表記統一の先例 0004 に合わせ `feature/update-` を採用する（0054 の `feature/refactor-` は `.unwrap()` 置換が主眼だったため本 issue とは性質が異なる）。

## 設計方針

- 対象はパニック／アサーションの**メッセージ引数**のみ。テストの doc コメント（`///`）や行末コメントは触らない
- メッセージ引数の定義:
  - `.expect("...")` の文字列引数
  - `assert!` / `prop_assert!` / `panic!` の失敗時メッセージ（第 2 引数以降。`panic!("...")` のように引数が 1 つの場合はその文字列全体）
  - `assert_eq!` / `prop_assert_eq!` の第 3 引数以降のフォーマット文字列（0004 と同じ）
- 既に日本語のメッセージ（0054 由来、`test_boxes.rs` など）は変更しない
- `.expect()` のうち「発生しない想定」を述べるものは、0054 と同じくパニックしない根拠が分かる日本語にする。失敗内容を述べるもの、および `assert!` / `prop_assert!` / `panic!` のメッセージは意味が通る日本語に翻訳する
- 次はログメッセージではないため対象外（英語のまま残し、変更しない）:
  - `assert!(x.contains("Video"))` のような Display / エラー文字列の**期待部分文字列**
  - `assert_eq!` / `prop_assert_eq!` の第 1・第 2 引数の比較値（例: `"WEBVTT"`、`"http://example.com"`、`b"Serif"`、FourCC）
- 対象パスは `pbt/tests/` 配下の全 `.rs` と `crates/c-api/tests/e2e.rs` のみ。`crates/c-api/tests/test_boxes.rs` は対象外
- 対象マクロは `.expect()` / `assert!` / `assert_eq!` / `prop_assert!` / `prop_assert_eq!` / `panic!`

## 完了条件

- 上記対象パス・対象マクロについて、**メッセージ引数**（設計方針の定義どおり）に残っている英語（ひらがな・カタカナ・漢字を含まない英字メッセージ）が 0 件になること
- 既存の日本語メッセージ、Display / エラー文字列の期待部分文字列（`contains("Video")` 等）、および `assert_eq!` / `prop_assert_eq!` の第 1・第 2 引数の比較値は変更されていないこと（`git diff` で確認）
- 検証は「`"` 直後が ASCII」だけでは判定しない。0054 由来の `"Vec への書き込みは失敗しない"` や `"Strategy の値域が ..."` が偽陽性になるため、**日本語文字を含まないメッセージ引数だけ**を対象に数えること。複数行の `assert!(...)` もメッセージ行を含めて確認すること
- 既存のテストが通ること

## 解決方法

各対象ファイルの英語メッセージ引数を日本語に翻訳する。ファイル数が多い場合はファイル単位でコミットを分けてよい。
