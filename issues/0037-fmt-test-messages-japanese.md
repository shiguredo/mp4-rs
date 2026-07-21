# PBT テストと C API テストの expect / assert メッセージが英語であり AGENTS.md 規約に違反している

- Priority: Medium
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/fmt-test-messages-japanese
- Polished: 2026-07-20

## 目的

AGENTS.md に「テストのログメッセージは全て日本語にすること」と明記されているが、`pbt/tests/` 配下の 9 ファイルで 350 件以上の英語 `.expect()` / `assert!` / `prop_assert!` メッセージが存在する。`crates/c-api/tests/e2e.rs` も 13 件（`.expect()` 6 件 + `assert!` 7 件）が英語。`tests/decode_encode_test.rs` は日本語メッセージで統一されており規約に準拠している。

## 優先度根拠

AGENTS.md の規約違反であり、プロジェクト全体の一貫性の観点から修正すべき。ただし機能的な影響はない。

## 現状

英語メッセージの例:

- `pbt/tests/prop_fmp4_segment_mux_demux.rs:198`: `.expect("failed to create media segment")`
- `pbt/tests/prop_auxiliary.rs:769`: `.expect("sync sample")`
- `pbt/tests/prop_container_boxes.rs:250`: `prop_assert!(false, "Expected StcoBox, got Co64Box")`
- `crates/c-api/tests/e2e.rs:58`: `.expect("Failed to execute cc command")`
- `crates/c-api/tests/e2e.rs:113`: `assert!(..., "simple_mux_demux execution failed")`

## 完了条件

- `pbt/tests/` 配下の全 expect / assert メッセージが日本語になること
- `crates/c-api/tests/e2e.rs` の expect / assert メッセージが日本語になること
- 修正後に `grep -rE '\.expect\("[a-zA-Z]' pbt/tests/ crates/c-api/tests/` および `grep -rE 'assert!\(.*"[a-zA-Z]' pbt/tests/ crates/c-api/tests/` で英語メッセージが 0 件であることを確認すること
- 既存のテストが通ること

## 解決方法

各ファイルの expect / assert メッセージを日本語に翻訳する。テストの doc コメント（`///`）は既に日本語で統一されているため、メッセージのみを対象とする。
