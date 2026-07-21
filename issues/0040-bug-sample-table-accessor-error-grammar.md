# auxiliary.rs の SampleTableAccessorError の Display メッセージに文法エラーがある

- Priority: Low
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/fix-sample-table-accessor-error-grammar
- Polished: 2026-07-20

## 目的

`SampleTableAccessorError` の Display 実装に 3 箇所の文法エラーがある。エラーメッセージは英語で統一されており（規約通り）、文法誤りは可読性を損なう。

## 優先度根拠

機能的な影響はないが、エラーメッセージの品質の観点から修正すべき。

## 現状

`src/auxiliary.rs:311`（`FirstChunkIndexIsNotOne`）:

```rust
"First chunk index in `stsc` box is expected to 1, but got {actual_chunk_index}"
```

`src/auxiliary.rs:320`（`LastChunkIndexIsTooLarge`）:

```rust
"Last chunk index in `stsc` box is expected to `<= {max_chunk_index}`, but got {last_chunk_index}"
```

`src/auxiliary.rs:336`（`ChunkIndicesNotMonotonicallyIncreasing`）:

```rust
"Chunk indices in `stsc` box is not monotonically increasing"
```

## 完了条件

- 311 行目が `"is expected to be 1"` に修正されること
- 320 行目が `"is expected to be `<= {max_chunk_index}`"` に修正されること
- 336 行目が `"are not monotonically increasing"` に修正されること（主語 "Chunk indices" は複数）
- 既存のテストが通ること

## 解決方法

311 行目: `"is expected to 1"` → `"is expected to be 1"`
320 行目: `"is expected to `<= ...`"` → `"is expected to be `<= ...`"`
336 行目: `"is not"` → `"are not"`
