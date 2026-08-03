# 公開 enum の ErrorKind / MuxError / DemuxError に #[non_exhaustive] が付いており規約に違反している

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-31
- Model: opencode-go glm-5.2
- Branch: feature/change-remove-non-exhaustive
- Polished: 2026-07-31

## 目的

`shiguredo-rust` 規約「`#[non_exhaustive]` を使わないこと」に違反している 3 つの公開 enum から `#[non_exhaustive]` を削除する。

## 優先度根拠

規約違反。`#[non_exhaustive]` は利用側の `match` 網羅性チェックの恩恵を失わせ、ワイルドカードパターンを強制する。将来 variant を追加するときは素直に破壊的変更として扱うのが規約の方針。

`crates/c-api/src/error.rs` では次の状態になっている。

- `From<DemuxError>` / `From<MuxError>`: 既知 variant はすべて明示済みで、末尾の `_ =>` は `#[non_exhaustive]` 由来のフォールバックのみ
- `From<Error>`（`ErrorKind`）: `InvalidInput` / `InvalidData` / `Unsupported` だけ明示し、既存の `InsufficientBuffer` も `_ => MP4_ERROR_OTHER` に落ちている（`#[non_exhaustive]` と未明示マップが同居）

## 現状

```rust
// src/codec.rs の ErrorKind
#[non_exhaustive]
pub enum ErrorKind {
```

```rust
// src/mux_mp4_file.rs の MuxError
#[non_exhaustive]
pub enum MuxError {
```

```rust
// src/demux_mp4_file.rs の DemuxError
#[non_exhaustive]
#[derive(Clone)]
pub enum DemuxError {
```

許可コメント・例外根拠なし。

対照的に `src/auxiliary.rs` の `SampleTableAccessorError` は `#[non_exhaustive]` なしで一貫していない。

## 設計方針

3 箇所から `#[non_exhaustive]` を削除する。

`crates/c-api/src/error.rs` の 3 つの `match`（`From<Error>` / `From<DemuxError>` / `From<MuxError>`）から `_ =>` を削除し、既知 variant をすべて明示する。将来 variant 追加をコンパイルエラーで検知できるようにする。

- `ErrorKind::InsufficientBuffer` は現状どおり `MP4_ERROR_OTHER` に明示マップする（マッピング先の意味変更はしない）
- `DemuxError` / `MuxError` は既知 variant がすでに列挙済みのため、`_ =>` を削除するだけでよい（残すと `#[non_exhaustive]` 削除後に `unreachable_patterns` で clippy が落ちる）

破壊的変更のため `CHANGES.md` の `## develop` に `[CHANGE]` で記載する。

カテゴリはランタイムバグ修正ではなく規約是正と後方互換のない API 変更なので `change`（ブランチは `feature/change-remove-non-exhaustive`）とする。

## 完了条件

- `ErrorKind` / `MuxError` / `DemuxError` から `#[non_exhaustive]` が削除されること
- `crates/c-api/src/error.rs` の `From<Error>` / `From<DemuxError>` / `From<MuxError>` から `_ =>` が削除され、既知 variant（`ErrorKind::InsufficientBuffer` を含む）がすべて明示されていること
- `CHANGES.md` の `## develop` に `[CHANGE]` で記載されること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

`feature/change-remove-non-exhaustive` ブランチで対応した。

1. `src/codec.rs` の `ErrorKind`、`src/mux_mp4_file.rs` の `MuxError`、`src/demux_mp4_file.rs` の `DemuxError` から `#[non_exhaustive]` を削除した
2. `crates/c-api/src/error.rs` の `From<Error>` / `From<DemuxError>` / `From<MuxError>` から `_ =>` を削除し、既知 variant をすべて明示した（`ErrorKind::InsufficientBuffer => Self::MP4_ERROR_OTHER` を追加）
3. `CHANGES.md` の `## develop` に `[CHANGE]` エントリを追記し、利用側影響と C API マッピング不変を記載した
4. `cargo test` / `cargo clippy` が通ることを確認した
