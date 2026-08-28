# エラーメッセージから絶対パスを除去する

- Created: 2026-08-19
- Completed: 2026-08-20
- Branch: feature/fix-error-display-absolute-path
- Polished: 2026-08-19

## 目的

`shiguredo_mp4::Error` の `Display` 実装が末尾に付加する発生位置情報から、ビルド環境固有の絶対パスを除去する。

crates.io から依存として使われるとき、`Location::caller()` 由来の `Location::file()` はコンパイル時のフルパス（例: `/home/user/.cargo/registry/src/index.crates.io-.../shiguredo_mp4-2026.4.0/src/basic_types.rs`）を返す。この文字列がそのままエラーメッセージに流れることで、下流（mp4-py 等）のユーザーがビルド環境の絶対パスを目にすることになる。ユーザーには何の意味もなく、他人のマシンのユーザー名や `.cargo` のパス構造を露出してしまう。

## 現状

`src/codec.rs` の `impl core::fmt::Display for Error` は以下のように書かれている:

```rust
write!(f, " (at {}:{})", self.location.file(), self.location.line())?;
```

`Error.location: &'static core::panic::Location<'static>` は `#[track_caller]` 経由の `Location::caller()` で取得され、`Error::new` / `Error::with_reason` / `Error::insufficient_buffer` などの生成ヘルパで埋まる。

- ローカル開発（cargo test など、ソースがワークスペース内）では `Location::file()` は `src/basic_types.rs` のような相対パスになる
- crates.io 経由の依存として使われるビルドでは、`Location::file()` は `.cargo/registry/src/...` 配下の絶対パスを返す

その結果、たとえば mp4-py の `RuntimeError` メッセージに以下のような文字列が入り込む:

```
InvalidData: ... (at /home/user/.cargo/registry/src/index.crates.io-0123456789abcdef/shiguredo_mp4-2026.4.0/src/basic_types.rs:461)
```

## 設計方針

`Display for Error` を修正し、`location.file()` の中から `src/` 以降だけを残した相対パス（例: `src/basic_types.rs`）でフォーマットする。

- 絶対パスが混入する主因は crates.io 経由での `.cargo/registry/.../shiguredo_mp4-<version>/src/...` 形式である。`src/` 以降だけを残せば、この形式でも `cargo test` などのローカル形式でも同じ相対パス表現に揃う
- モジュール階層（`src/boxes_moov_tree.rs` など）は残るので、エラー発生箇所の特定に必要な情報は失わない
- `src/` が見つからないケース（想定外のビルド構成、`build.rs` から呼ばれた場合など）は、フォールバックとして元の `location.file()` をそのまま出力する（安全側）

破壊的変更にしないため、以下は変更しない:

- `Error` 構造体のフィールド構成（`location` は公開のまま残す。下流が独自フォーマット可能）
- `impl Debug for Error` の Display 委譲
- `Location` の保持自体（生成ヘルパの `#[track_caller]` も維持）

## 完了条件

- crates.io 経由のビルドを想定した `location.file()` の入力（`/.../.cargo/registry/src/index.crates.io-<hash>/shiguredo_mp4-<version>/src/basic_types.rs` 形式）に対して、`Error` の `Display` 出力が `(at src/basic_types.rs:<line>)` の形になる
- ローカルビルド（`src/basic_types.rs` のような相対パス入力）に対しても、`Display` 出力が `(at src/basic_types.rs:<line>)` の形になる
- `src/` を含まない入力に対しても panic せず、元のパスをそのまま出力する
- 上記 3 パターンをカバーするユニットテストを追加する

## 解決方法

- `src/codec.rs` に private 関数 `fn shorten_source_path(file: &str) -> &str` を追加した
  - `file.rfind("src/")` で最後方の `src/` を検索し、見つかればそこから末尾までのスライスを返す。見つからなければ元の文字列をそのまま返す
  - Windows のパス区切り `\` は今回のスコープ外とし、対応の必要が出た場合は別 issue で扱う
- `impl core::fmt::Display for Error` の `write!(f, " (at {}:{})", self.location.file(), self.location.line())?;` を、`shorten_source_path(self.location.file())` 経由で出力する形に変更した
- `src/codec.rs` の `#[cfg(test)] mod tests` に完了条件の 3 パターンをカバーする単体テストを追加した
  - `shorten_source_path_strips_cargo_registry_prefix`: 合成した crates.io 形式の絶対パスから `src/basic_types.rs` だけが残ることを検証
  - `shorten_source_path_keeps_repository_relative_path`: 既にリポジトリ相対の入力が変換前後で一致することを検証
  - `shorten_source_path_falls_back_when_src_marker_is_missing`: `src/` を含まない入力（`build.rs`）で panic せずフォールバックする挙動を検証
- `CHANGES.md` の `## develop / ### misc` に `[FIX]` エントリを追加した

## 補足

- 本 issue は下流バインディング（mp4-py 等）のエラーメッセージに絶対パスが露出している事象への上流対応。根本原因が `Display for Error` のフォーマットにあるため、下流での対症的な後処理ではなくコア側で直す方針とする
- `location` フィールドは `pub` のまま残すため、より詳細な情報（フルパス）が必要な下流は `err.location.file()` を直接読んで独自にフォーマットできる
