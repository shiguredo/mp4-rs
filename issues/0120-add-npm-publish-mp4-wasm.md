# @shiguredo/mp4-wasm を npm に公開する

- Created: 2026-10-31
- Completed: {YYYY-MM-DD}
- Branch: feature/add-npm-publish-mp4-wasm
- Polished: {YYYY-MM-DD}

## 目的

MP4 ライブラリを JavaScript / TypeScript から容易に利用できるようにするため、`crates/wasm` の WebAssembly ビルドを npm パッケージ `@shiguredo/mp4-wasm` として公開する。現状の配布手段は GitHub Release の tarball のみで、ブラウザ・Node からの利用には利用者自身が wasm のロードと C ABI のラップを実装する必要がある。

## 現状

- `crates/wasm` は cdylib として `mp4_wasm.wasm` を出力し、C ABI 関数 (`mp4_demuxer_*` / `mp4_muxer_*` / `mp4_alloc` 等) と JSON ベースのサンプル入出力を提供する (wasm-bindgen は未使用)
- 配布は `release.yml` の `upload-wasm-assets` ジョブによる GitHub Release の tarball のみ (`mp4-{version}_wasm.tar.gz`)
- JS / TS 向けのローダー・型定義・パッケージは存在しない

## 設計方針

- パッケージは別リポジトリではなく本リポジトリに置く。wasm は `shiguredo_mp4` / `c-api` の配布形態の 1 つであり、JSON スキーマと型定義の整合を同一 CI で検証できるようにするため
- パッケージのプロジェクトルートは `crates/wasm/npm/` とし、パッケージマネージャーに pnpm、JS 側のビルド・テスト・lint に vite-plus (`vp` CLI) を使う
  - Node / pnpm のバージョンは `package.json` の `packageManager` / `engines` で宣言する (Rust の `rust-toolchain.toml` との対応)
- `.wasm` はパッケージに同梱し、ローダーは `new URL()` で wasm を解決する。サイズは gzip 後約 174 KB (2026.5.0 実測) で同梱に十分小さい
- wasm-bindgen は導入しない。C ABI + JSON の現構成を維持し、使い勝手はローダーで吸収する
- 型定義は `crates/wasm/README.md` の JSON スキーマに対応する TypeScript 型として用意し、Rust 側のスキーマ変更と同一 PR で更新する
- テストは Vitest (`vp test`) で Node 環境と実ブラウザの両方で実行する
  - Node: wasm のロードと demux / mux の動作確認
  - ブラウザ: Vitest Browser Mode (playwright provider) で同一 spec を実行し、fetch 経由の wasm ロードを検証する
  - モックやスタブは使わず、実 wasm と実 MP4 データで検証する
- `ci.yml` に JS 側のテストジョブを追加する
- 公開は `release.yml` に npm publish ジョブを追加する。既存の wasm-opt 最適化フローを再利用し、crates.io publish と同じく OIDC trusted publishing を使う。タグのバージョンを `package.json` に注入し、canary タグは `--tag canary` で公開する
- npm バージョンは mp4-rs のバージョン (calver) と同一にし、`@shiguredo/mp4-wasm@2026.6.0` が GitHub Release と crates.io の同一バージョンに対応するようにする

## 完了条件

- `crates/wasm/npm/` に `@shiguredo/mp4-wasm` パッケージが存在し、ローダー + 型定義 + wasm 同梱の構成でビルドできる
- `vp test` で Node 環境とブラウザ (Playwright) 環境の両方のテストが pass する
- `ci.yml` で JS 側のテストが実行される
- タグ push を契機に npm へ公開され、`npm install @shiguredo/mp4-wasm` で利用できる
- canary タグで `--tag canary` での公開が行われる
- `prek` のフックと `.gitignore` が Node 側のファイル (`node_modules` / `dist` 等) に対応している
