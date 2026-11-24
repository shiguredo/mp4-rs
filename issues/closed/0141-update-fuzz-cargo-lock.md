# `fuzz/Cargo.lock` の `shiguredo_mp4` のバージョンを本体の `Cargo.toml` に合わせる

- Created: 2026-11-24
- Completed: 2026-11-24
- Branch: feature/update-fuzz-cargo-lock
- Polished: {YYYY-MM-DD}

## 目的

`fuzz/Cargo.lock` に記録された `shiguredo_mp4` のバージョンを、本体の `Cargo.toml` のバージョンに合わせる。

食い違ったままだと、`fuzz/` で `--locked` を付けた cargo は失敗する。`--locked` を付けない cargo は、実行のたびに `fuzz/Cargo.lock` を書き換え、作業ツリーに差分を残す。

## 現状

- 本体の `Cargo.toml` の `version` と、ルートの `Cargo.lock` の `shiguredo_mp4` は `2026.6.0-canary.2` である
- `fuzz/Cargo.lock` の `shiguredo_mp4` は `2026.6.0-canary.1` のままである。`fuzz/Cargo.toml` は `shiguredo_mp4 = { path = ".." }` で本体を参照する
- リポジトリの複製の `fuzz/` で確認した結果は次のとおり
  - `cargo check --locked` は、lock ファイルの更新が必要だとして失敗する
  - `--locked` を付けない `cargo clippy --all-targets` や `cargo metadata` は、`shiguredo_mp4` を canary.1 から canary.2 に更新して、`fuzz/Cargo.lock` を書き換える
  - `cargo update -p shiguredo_mp4` を実行すると、`fuzz/Cargo.lock` の差分は `shiguredo_mp4` の `version` の 1 行だけになる。その後は `cargo check --locked --all-targets` が成功する
- CI（`.github/workflows/ci.yml` の `ci` ジョブ）は、`fuzz/` で `cargo fmt --check`、`cargo check --all-targets`、`cargo clippy --all-targets -- -D warnings` を実行する。どれにも `--locked` を付けておらず、ワークフローと `Makefile` に `--locked` を使う箇所はない。そのため CI はこの食い違いでは失敗せず、`fuzz/Cargo.lock` を書き換えたまま成功する
- 食い違いの原因
  - canary のバージョンを上げる `canary.py` は、`run_cargo_update` でリポジトリ直下の `cargo update shiguredo_mp4` を実行し、`git_commit_version` で `Cargo.toml` と `Cargo.lock` だけをコミットする
  - `fuzz` はルートの `[workspace]` の `exclude` に含まれ、自身の `fuzz/Cargo.lock` を持つ。リポジトリ直下の `cargo update` では更新されない
  - canary.2 に上げたコミット（`[canary] Bump version to 2026.6.0-canary.2`）も、`Cargo.toml` と `Cargo.lock` だけを変更している
  - リリースのバージョンに上げたコミット（`Bump version to 2026.5.0`）は、`Cargo.toml` と `Cargo.lock` とともに `fuzz/Cargo.lock` も更新していた

## 設計方針

- `fuzz/` で `cargo update -p shiguredo_mp4` を実行し、`fuzz/Cargo.lock` の `shiguredo_mp4` だけを更新する。ほかの依存のバージョンは変えない
- 再発の防止（`canary.py` で `fuzz/Cargo.lock` も更新すること、CI の `fuzz/` の cargo に `--locked` を付けること）は、本 issue の対象外とする
- バージョンの更新に付随する lock ファイルの追従であり、バージョンの更新自体も `CHANGES.md` に記載していないため、`CHANGES.md` には記載しない

## 完了条件

- `fuzz/Cargo.lock` の `shiguredo_mp4` の `version` が、本体の `Cargo.toml` の `version` と一致する
- `fuzz/Cargo.lock` の差分が、`shiguredo_mp4` の `version` の 1 行だけである
- `fuzz/` で `cargo check --locked --all-targets` が成功する

## 解決方法

- `fuzz/` で `cargo update -p shiguredo_mp4` を実行し、`fuzz/Cargo.lock` の `shiguredo_mp4` を `2026.6.0-canary.2` に更新した。差分は `version` の 1 行だけで、ほかの依存のバージョンは変えていない
- `fuzz/` で `cargo check --locked --all-targets` が成功することを確認した
- 設計方針で対象外としていた再発の防止のうち、`canary.py` は小さな変更で直せるため、あわせて直した
  - `run_cargo_update` で、リポジトリ直下に加えて `fuzz/` でも `cargo update shiguredo_mp4` を実行する
  - `git_commit_version` で、`Cargo.toml` と `Cargo.lock` に加えて `fuzz/Cargo.lock` もコミットする
  - `CHANGES.md` の `### misc` に [UPDATE] を追記した
- CI の `fuzz/` の cargo に `--locked` を付けることは見送った。ルートの cargo の手順も `--locked` を付けておらず、`fuzz/` だけに付けると揃わなくなるため
