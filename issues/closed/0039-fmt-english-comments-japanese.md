# boxes_moov_tree.rs と boxes_sample_entry.rs に英語コメントが 7 箇所残存している

- Priority: Low
- Created: 2026-07-20
- Completed: 2026-07-29
- Model: qwen3.8-max-preview
- Branch: develop
- Polished: 2026-07-20

## 目的

AGENTS.md に「コメントは全て日本語にすること」と明記されているが、次のコメントが英語のまま残っている可能性があったため、規約整合を取る。

## 優先度根拠

AGENTS.md の規約違反だが、機能的な影響はない。

## 完了条件

- 対応が不要と判明した箇所は対象から外し、必要な場合のみ日本語化する
- 既存のテストが通ること（コード変更がある場合）

## 解決方法

対応不要と判断し、コード変更なしで closed にした。

1. 起票時点で挙げていた 7 箇所のうち、次の 6 箇所は `#[expect(missing_docs)]` 撤廃（`CHANGES.md` の `### misc` に「`CttsBox` / `CslgBox` / `MdhdBox::language` の既存英語 doc を日本語化」と記載）に伴い、既に日本語 doc になっている:
   - `MdhdBox::language`（`ISO-639-2/T 言語コード` …）
   - `CttsBox::version` / `CslgBox::version`（`FullBox バージョン` …）
   - `CslgBox::composition_to_dts_shift` / `composition_start_time` / `composition_end_time`
2. 残る `DopsBox::encode` 内の `// ChannelMappingFamily` は、英語の説明文ではなく Opus `dOps` のフィールド名を指すインラインメモである。既存コードでも仕様上の識別子そのものをコメントに残す例があり、翻訳対象にしないと判断した。
3. したがって本 issue としての残作業はない。
