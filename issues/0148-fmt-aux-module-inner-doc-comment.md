# `aux` モジュールの説明を `//!` から `pub mod aux` の前の `///` に移す

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-aux-module-doc-comment
- Polished: {YYYY-MM-DD}

## 目的

`src/lib.rs` のインラインモジュール `aux` の説明を、モジュールの中の `//!` から、`pub mod aux` の直前の `///` に移す。shiguredo-rust の規約は、`//!` をファイル先頭以外で使わないと定めている。

## 現状

- `src/lib.rs` の `pub mod aux { ... }` は、ブロックの先頭で `//!` を使い、モジュールの説明（「MP4 の仕様とは直接は関係がない、実装上便利な補助的なコンポーネントを集めたモジュール」）を書いている
- `pub mod aux` の直前には、Windows の予約名を避けるため `auxiliary.rs` を `aux` に再エクスポートしている理由を書いた、`// [NOTE]` の実装コメントがある
- リポジトリ内の `*.rs` で、ファイル先頭以外にある `//!` はこの 1 か所だけである。`rg -n '^\s+//!' --type rust` と、ファイル先頭の `//!` の連なりより後ろにある `//!` を探した結果で確認した
- `src/auxiliary.rs` の先頭にも同じ文言の `//!` があるが、ファイル先頭なので規約に合っている

## 設計方針

- `pub mod aux` の直前（`// [NOTE]` のコメントの後）に同じ文言の `///` を置き、ブロック内の `//!` と、その後の空行を削除する
- `// [NOTE]` のコメントは実装上の理由の説明なので、`//` のまま残す
- 複製でこの変更を行い、`cargo doc -p shiguredo_mp4 --no-deps` の出力を比べたところ、`aux/index.html` とクレートのトップページ `index.html` の違いは、ソースへのリンクの行番号だけだった。`cargo fmt --all --check` も成功した
- 公開 API とドキュメントの説明文が変わらないため、`CHANGES.md` には記載しない

## 完了条件

- `rg -n '^\s+//!' --type rust` の結果が 0 件である
- 変更の前後で `cargo doc -p shiguredo_mp4 --no-deps` を実行し、`aux/index.html` と `index.html` の違いがソースへのリンクの行番号だけである
- `cargo fmt --all --check` と、CI と同じ `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` が成功する
