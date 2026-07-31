# auxiliary.rs の SampleTableAccessorError の Display メッセージに文法エラーがある

- Priority: Low
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: develop
- Polished: 2026-07-31

## 目的

`SampleTableAccessorError` の Display 実装に 3 箇所の文法エラーがある。エラーメッセージは英語で統一されており（規約通り）、文法誤りは可読性を損なう。

## 優先度根拠

機能的な影響はないが、エラーメッセージの品質の観点から修正すべき。

## 現状

`impl core::fmt::Display for SampleTableAccessorError` 内の次の 3 アーム:

`FirstChunkIndexIsNotOne`:

```rust
"First chunk index in `stsc` box is expected to 1, but got {actual_chunk_index}"
```

`LastChunkIndexIsTooLarge`:

```rust
"Last chunk index in `stsc` box is expected to `<= {max_chunk_index}`, but got {last_chunk_index}"
```

`ChunkIndicesNotMonotonicallyIncreasing`:

```rust
"Chunk indices in `stsc` box is not monotonically increasing"
```

## 完了条件

- `FirstChunkIndexIsNotOne` の Display が `"is expected to be 1"` になっていること
- `LastChunkIndexIsTooLarge` の Display が `"is expected to be \`<= {max_chunk_index}\`"` になっていること
- `ChunkIndicesNotMonotonicallyIncreasing` の Display が `"are not monotonically increasing"` になっていること（主語 "Chunk indices" は複数）
- `CHANGES.md` の `## develop` の `### misc` に `[UPDATE]` エントリが追記されていること
- 既存のテストが通ること

## 解決方法

機能影響のないメッセージ修正のため、作業ブランチは切らず `develop` 上で直接対応する。

1. `src/auxiliary.rs` の `impl core::fmt::Display for SampleTableAccessorError` で次を置換する
   - `FirstChunkIndexIsNotOne`: `"is expected to 1"` → `"is expected to be 1"`
   - `LastChunkIndexIsTooLarge`: `"is expected to \`<= ...\`"` → `"is expected to be \`<= ...\`"`
   - `ChunkIndicesNotMonotonicallyIncreasing`: `"is not"` → `"are not"`
2. `CHANGES.md` の `## develop` の `### misc` 末尾に、上記 Display 文法修正の `[UPDATE]` エントリを追記する
3. 既存テスト（`pbt/tests/prop_auxiliary.rs` の `error_display_*` 等）が通ることを確認する
