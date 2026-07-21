# boxes_moov_tree.rs と boxes_sample_entry.rs に英語コメントが 7 箇所残存している

- Priority: Low
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/fmt-english-comments-japanese
- Polished: 2026-07-20

## 目的

AGENTS.md に「コメントは全て日本語にすること」と明記されているが、以下の 7 箇所が英語のまま:

1. `src/boxes_moov_tree.rs:776`: `/// ISO-639-2/T language code`
2. `src/boxes_moov_tree.rs:1807`: `/// full box version`
3. `src/boxes_moov_tree.rs:1921`: `/// full box version`
4. `src/boxes_moov_tree.rs:1924`: `/// composition to decode time shift`
5. `src/boxes_moov_tree.rs:1933`: `/// composition start time`
6. `src/boxes_moov_tree.rs:1936`: `/// composition end time`
7. `src/boxes_sample_entry.rs:1855`: `// ChannelMappingFamily`

## 優先度根拠

AGENTS.md の規約違反だが、機能的な影響はない。

## 完了条件

- 7 箇所すべてが日本語に翻訳されること
- 既存のテストが通ること

## 解決方法

各コメントを日本語に翻訳する。例: `/// ISO-639-2/T 言語コード`、`/// フルボックスバージョン`、`// チャネルマッピングファミリ`。
