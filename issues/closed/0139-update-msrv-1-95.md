# 最小サポート Rust バージョンを 1.95 に上げる

- Created: 2026-11-24
- Completed: 2026-11-24
- Branch: feature/update-msrv-1-95
- Polished: {YYYY-MM-DD}

## 目的

shiguredo_mp4 の最小サポート Rust バージョン (MSRV) と、開発・CI で使うツールチェーンを 1.95 に上げ、shiguredo-rust の規約に合わせる。

規約は、ライブラリの MSRV を 1.95 とし、`Cargo.toml` の `rust-version` と `rust-toolchain.toml` の `channel` に同じ値を指定するよう定めている。今はどちらも 1.93 である。

## 現状

1.93 を指定している箇所は次のとおり。

- `Cargo.toml` の `[package]` の `rust-version = "1.93"`
- `rust-toolchain.toml` の `channel = "1.93"`
- `.github/workflows/ci.yml` の `ci` / `test-wasm` / `build-wasm` の各ジョブで、`shiguredo/github-actions/.github/actions/rust-cache` に渡している `toolchain: "1.93"`（3 か所）
- `Makefile` の `miri` ターゲットの前のコメント（「`rust-toolchain.toml` の MSRV 1.93」「リポジトリ規定の 1.93」の 2 か所）
- `skills/shiguredo-mp4/SKILL.md` の「最小 Rust バージョン: 1.93」

次の箇所は 1.93 を直接指定していないため、変更は不要である。

- `crates/wasm`、`examples/dump_wasm`、`examples/transcode_wasm`、`pbt`、`fuzz` の各 `Cargo.toml` は `rust-version` を持たない。ツールチェーンはリポジトリ直下の `rust-toolchain.toml` に従う
- `.github/workflows/release.yml` と `.github/workflows/deploy.yml` は、`rustup show` で `rust-toolchain.toml` のツールチェーンを導入するだけで、バージョンを書いていない
- `ci.yml` の `miri` ジョブと、`Makefile` の `miri` / `fuzzing` ターゲットは nightly を使う

`CHANGES.md` では、MSRV を 1.93 に上げた `[CHANGE]` が 2026.4.0 に記載済みである。`## develop` の `### misc` には、「ツールチェーンを `rust-toolchain.toml` で MSRV の 1.93 に固定する」という `[UPDATE]` がある。

リポジトリの複製で `rust-version` と `channel` を 1.95 に変えたところ、1.95.0 で次がすべて成功した。コードの変更は必要なかった。

- `cargo fmt --all --check` と、`fuzz/` での `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings` と、CI と同じ `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz -- -D warnings`
- `fuzz/` での `cargo clippy --all-targets -- -D warnings`
- `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm`
- `RUSTDOCFLAGS="-D warnings"` を付けた `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz`

`wasm32-unknown-unknown` 向けのビルド（`test-wasm` / `build-wasm` ジョブと `deploy.yml`）は、1.95 では確認していない。

## 設計方針

- 規約どおり、`Cargo.toml` の `rust-version` と `rust-toolchain.toml` の `channel` を同時に `"1.95"` にする
- `ci.yml` の rust-cache の `toolchain` 3 か所を `"1.95"` にする。`CHANGES.md` の既存の記載どおり、rust-cache には `rust-toolchain.toml` と同じバージョンを指定する
- `Makefile` のコメントと `skills/shiguredo-mp4/SKILL.md` の 1.93 を 1.95 にする。`SKILL.md` のほかの記載は本 issue では変えない
- `CHANGES.md`
  - `## develop` に `[CHANGE] 最小サポート Rust バージョンを 1.95 に上げる` を追加する。MSRV を 1.93 に上げたときと同じ種別である
  - `### misc` の「MSRV の 1.93 に固定する」`[UPDATE]` は、派生元ブランチとの最終的な差分を書く規約に従い、1.95 に直す

## 完了条件

- `Cargo.toml` の `rust-version` と `rust-toolchain.toml` の `channel` が、どちらも `"1.95"` である
- `Cargo.toml`、`rust-toolchain.toml`、`.github/workflows/`、`Makefile`、`skills/shiguredo-mp4/SKILL.md` に、1.93 の指定が残っていない
- CI の全ジョブが成功する
- `CHANGES.md` の `## develop` に MSRV を 1.95 に上げる `[CHANGE]` があり、`### misc` のツールチェーンの固定の `[UPDATE]` が 1.95 になっている

## 解決方法

- `rust-version` と `channel` を 1.95 にそろえ、CI の `ci` / `test-wasm` / `build-wasm` のキャッシュ指定も 1.95 に変更した。Miri と fuzzing の nightly 指定は維持した。
- Makefile のコメント、`shiguredo-mp4` スキルの最小バージョン、未リリースの変更履歴を更新した。Rust コードと依存関係は変更していない。
- Rust 1.95.0 で既存テスト、WebAssembly とデモのビルド、Node.js 向けとブラウザー向けのバインディング生成・最適化を確認した。GitHub Actions の CI では全 8 ジョブが成功した。
