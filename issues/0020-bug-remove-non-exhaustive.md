# 公開 enum の ErrorKind / MuxError / DemuxError に #[non_exhaustive] が付いており規約に違反している

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-remove-non-exhaustive
- Polished: YYYY-MM-DD

## 目的

`shiguredo-rust` 規約「`#[non_exhaustive]` を使わないこと」に違反している 3 つの公開 enum から `#[non_exhaustive]` を削除する。

## 優先度根拠

規約違反。`#[non_exhaustive]` は利用側の `match` 網羅性チェックの恩恵を失わせ、ワイルドカードパターンを強制する。将来 variant を追加するときは素直に破壊的変更として扱うのが規約の方針。`crates/c-api/src/error.rs` でも `_ =>` アームを強制されている。

## 現状

```rust
// src/codec.rs:14
#[non_exhaustive]
pub enum ErrorKind {
```

```rust
// src/mux_mp4_file.rs:259
#[non_exhaustive]
pub enum MuxError {
```

```rust
// src/demux_mp4_file.rs:232
#[non_exhaustive]
#[derive(Clone)]
pub enum DemuxError {
```

許可コメント・例外根拠なし。

対照的に `SampleTableAccessorError`（`src/auxiliary.rs:247`）は `#[non_exhaustive]` なしで一貫していない。

## 設計方針

3 箇所から `#[non_exhaustive]` を削除する。`crates/c-api/src/error.rs` の `_ =>` アームを既知 variant の明示 match に直し、将来 variant 追加をコンパイルエラーで検知できるようにする。破壊的変更のため `CHANGES.md` の `## develop` に `[CHANGE]` で記載する。

## 完了条件

- `ErrorKind` / `MuxError` / `DemuxError` から `#[non_exhaustive]` が削除されること
- `crates/c-api/src/error.rs` の `_ =>` アームが可能な限り明示 match に変更されること
- `CHANGES.md` の `## develop` に `[CHANGE]` で記載されること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `src/codec.rs:14` / `src/mux_mp4_file.rs:259` / `src/demux_mp4_file.rs:232` の `#[non_exhaustive]` を削除する
2. `crates/c-api/src/error.rs` の match 文で `_ =>` を減らし、既知 variant を明示する
3. `CHANGES.md` の `## develop` に `[CHANGE] ErrorKind / MuxError / DemuxError から #[non_exhaustive] を削除する` を追記する
