# crates/c-api を削除する

- Created: 2026-11-13
- Completed: {YYYY-MM-DD}
- Branch: feature/remove-c-api
- Polished: {YYYY-MM-DD}

## 目的

`crates/c-api` の維持コストが、C 言語から利用したい利用者に対して見合わないため削除する。

C API は Python バインディングを想定して作られたが、Python は PyO3 + Maturin で直接 Rust を呼ぶ方針になった。iOS / Android は Rust をそのまま組み込めばよく、C ABI の構造体を公開する必要がない。リポジトリ内には JNI / Swift / Objective-C のバインディングも、モバイル向けのドキュメントも存在しない。

## 現状

- `crates/c-api` は 20 ファイル 10,231 行である
  - 実装 (`src/`) が 5,061 行、自動生成ヘッダ (`include/mp4.h`) が 2,597 行、C サンプルが 1,005 行
- 公開している C 関数は 42 個、`#[repr(C)]` の構造体は 23 個
- 起票時点の全 1,933 コミットのうち 235 件 (12%) が `c-api` を変更しており、そのうち 167 件は `c-api` だけを変更している (wasm もコアも触らない。マージコミットを含む)
- `mp4.h` だけで 126 コミットの更新がある
- コーデックを 1 つ追加するたびに、コアの実装と同規模の追加工事が発生する
  - 字幕 `tx3g` の追加では、コアが 326 行、`c-api` が 297 行、wasm が 386 行だった
- C ABI の生ポインタ設計に起因する不具合が繰り返し発生している
  - 配列の null 検査漏れによる未定義動作
  - 解放時のサイズ不一致によるリークと未定義動作
  - サイズ 0 のポインタを `from_raw_parts` に渡すことによる未定義動作
- 配布は GitHub Release の 8 プラットフォーム向けアーカイブで、CI には専用のビルドジョブ、MSVC 検証、MinGW への切り替えがある

## 設計方針

- `crates/wasm` のコア直結化 (0121) は完了している。wasm は `c-api` に依存しておらず、`c-api` を削除しても wasm とコアのビルドは通る
- 削除対象は次のとおり
  - `crates/c-api` 配下のすべて
  - ルート `Cargo.toml` の workspace メンバー
  - `Makefile` の `c-api` 除外指定
  - `ci.yml` の `build-c-api` ジョブと Windows 向け MSVC 検証
  - `release.yml` の 8 プラットフォームビルドジョブ
  - `README.md` / `README.en.md` の C API に関する記述
  - `skills/shiguredo-mp4/SKILL.md` の C API に関する記述
- iOS / Android 向けには、必要になった時点で利用側が薄いファサードクレートを作るか、UniFFI で Swift / Kotlin のバインディングを生成する。本リポジトリでは C ABI の構造体を公開しない

## 完了条件

- `crates/c-api` が存在しない
- `cargo build --workspace` / `cargo test --workspace` / `cargo clippy --workspace` / `cargo doc --workspace` が通る
- `ci.yml` と `release.yml` から C API 関連のジョブと手順が消えている
- `README.md` / `README.en.md` / `skills/shiguredo-mp4/SKILL.md` に C API の記述が残っていない
- `prek` のフックがすべて通る

## 解決方法

{実装後に記載する}
