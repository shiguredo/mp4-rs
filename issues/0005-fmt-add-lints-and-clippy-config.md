# lint / clippy 設定の追加と既存コードの対応

- Priority: Medium
- Created: 2026-05-22
- Model: Composer 2.5
- Branch: feature/add-lints-and-clippy-config

## 目的

AGENTS.md の「lint 警告を抑制する必要がある時は `#[allow(...)]` ではなく `#[expect(...)]` を使う」「`.unwrap()` ではなく `.expect("MESSAGE")` を使用する」等の方針を、
Cargo / Clippy 設定とコード修正で実際に enforce する。
prek / CI で既に `cargo clippy ... -- -D warnings` を実行しているため、
追加の restriction lint を導入し、no_std ライブラリとして堅牢なエラー処理・バッファ操作を徹底する。

## 優先度根拠

Medium。機能追加ではないが、入力バイナリの破損耐性（暗黙キャスト・未検証スライス等）に直結する。
既存 CI が clippy を走らせているため、設定を明示化しないと lint 方針がコードレビュー依存のままになる。

## 現状

- `prek.toml` / `.github/workflows/ci.yml` で `cargo clippy ... -- -D warnings` を実行済み
- `[workspace.lints.*]` や `clippy.toml` は未整備
- `src/lib.rs` は `#![no_std]` のみで、restriction lint は crate 属性にも Cargo.toml にも未設定
- バッファ操作は `buf[offset..]` や `as` キャストが散在
- `#[allow(...)]` が一部残存（c-api / wasm 含む）
- c-api / wasm は `unsafe` を使うため workspace の `unsafe_code = deny` とは分離する必要がある

## 設計方針

### 1. workspace 共通 lint (`Cargo.toml`)

```toml
[workspace.lints.rust]
unsafe_code = "deny"  # c-api / wasm は workspace lint を継承しない

[workspace.lints.clippy]
# Clippy 組み込みデフォルト (correctness / style 等) は触らない
# prek / CI の `-D warnings` が warn を deny に昇格する
alloc_instead_of_core = "deny"
allow_attributes_without_reason = "deny"
dbg_macro = "deny"
unimplemented = "deny"

[lints]
workspace = true
```

- `edition` / `rust-version` は `[workspace.package]` に集約し、各 crate から `*.workspace = true` で参照
- `pbt` も `[lints] workspace = true` で継承

### 2. `clippy.toml`

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-panic-in-tests = true
```

`src/` 内の `#[test]` / `#[cfg(test)]` 向けの緩和。
`tests/` 配下は別 crate のため届かない（integration test 側は従来どおり）。

### 3. ライブラリ crate 専用 lint (`src/lib.rs`)

workspace lint に書かない理由: 同じ package の `tests/` crate にも lint が乗ってしまうため。

`#![cfg_attr(not(test), deny(clippy::...))]` で library target のみに適用:

| lint | 意図 |
|---|---|
| `std_instead_of_core` | no_std で `core` を使う |
| `std_instead_of_alloc` | `alloc` と `std` の使い分け |
| `panic` / `unreachable` | 本番コードでの panic 禁止 |
| `unwrap_used` / `expect_used` | 明示的エラー処理へ |
| `as_conversions` | `From` / `TryFrom` / 検証ヘルパへ |
| `indexing_slicing` | 範囲外 panic しうるスライス禁止 |
| `panic_in_result_fn` | `Result` 返却関数内の panic 禁止 |
| `index_refutable_slice` | 長さ未検証スライスパターン禁止 |
| `integer_division` / `integer_division_remainder_used` | ゼロ除算 panic 禁止 |

`#![cfg_attr(not(test), no_std)]` に変更し、lib ビルド時のみ no_std を維持する。

### 4. コード修正

- `codec::buf` モジュールを追加し、バッファ操作・型変換を集約
  - `suffix_mut`, `range`, `range_len`, `usize_to_u64`, `u64_to_u32` 等
  - タイムスタンプ変換等で除算が必要な箇所のみ `#[expect(..., reason = "...")]` で例外
- 全 `src/` モジュールの `buf[offset..]` / `as` キャストを `buf::` ヘルパ経由に置換
- ISO BMFF フィールド名等は `#[expect(missing_docs, reason = "...")]` で理由付き抑制
- demuxer の `new_without_default` 等も理由付き `#[expect]`
- c-api / wasm の `#[expect(...)]` に `reason = "..."` を追加（`allow_attributes_without_reason` 対応）

### 5. c-api / wasm

- workspace lint は継承しない（`unsafe` 利用のため `unsafe_code = deny` と両立しない）
- `allow_attributes_without_reason` 対応として `reason` 付き `#[expect]` へ移行

## 完了条件

- [ ] `Cargo.toml` に workspace lint / `[workspace.package]` が追加されている
- [ ] `clippy.toml` が追加されている
- [ ] `src/lib.rs` に library target 専用 deny lint が設定されている
- [ ] `cargo clippy -p shiguredo_mp4 --all-targets -- -D warnings` が通る
- [ ] `cargo clippy -p pbt --all-targets -- -D warnings` が通る
- [ ] `cargo clippy -p c-api --all-targets -- -D warnings` が通る
- [ ] `cargo test --workspace` が通る
- [ ] `CHANGES.md` の `## develop` に misc エントリを追記する

## 解決方法

1. 上記設計方針どおり `Cargo.toml` / `clippy.toml` / `src/lib.rs` を設定
2. `codec::buf` を追加し、`src/` 全体のバッファ操作・型変換を修正
3. `#[allow]` → `#[expect(..., reason = "...")]` へ移行
4. clippy / test を workspace 全体で確認
5. `CHANGES.md` に追記
