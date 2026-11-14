# @shiguredo/mp4 を npm に公開する

- Created: 2026-10-31
- Completed: {YYYY-MM-DD}
- Branch: feature/add-npm-publish-mp4
- Polished: {YYYY-MM-DD}

## 目的

MP4 ライブラリを JavaScript / TypeScript から容易に利用できるようにするため、`crates/wasm` の WebAssembly ビルドを npm パッケージ `@shiguredo/mp4` として公開する。現状の配布手段は GitHub Release の tarball のみで、ブラウザ・Node からの利用には利用者自身が wasm のロードを実装し、生成されたバインディングを配置する必要がある。

## 現状

- `crates/wasm` は 0121 で wasm-bindgen ベースの API に移行済みである。生ポインタの C ABI は廃止し、`Mp4FileDemuxer` / `Mp4FileMuxer` / `Fmp4SegmentMuxer` / `Fmp4SegmentDemuxer` / `Mp4FileKindDetector` のクラスと `version()` を公開している
- `wasm-bindgen --target nodejs` と `--target web` で `crates/wasm/pkg/node` / `crates/wasm/pkg/web` に `.js` / `.d.ts` / `.wasm` を生成できる
- 配布は `release.yml` の `upload-wasm-assets` ジョブによる GitHub Release の tarball のみ (`mp4-{version}_wasm.tar.gz`、`node/` と `web/` を含む)
- npm パッケージ (`package.json` / `exports` / 型定義の同梱) は存在しない

## 設計方針

- パッケージは別リポジトリではなく本リポジトリに置く。wasm は `shiguredo_mp4` / `c-api` の配布形態の 1 つであり、JSON スキーマと型定義の整合を同一 CI で検証できるようにするため
- パッケージのプロジェクトルートは `crates/wasm/npm/` とし、パッケージマネージャーに pnpm、JS 側のビルド・テスト・lint に vite-plus (`vp` CLI) を使う
  - Node / pnpm のバージョンは `package.json` の `packageManager` / `engines` で宣言する (Rust の `rust-toolchain.toml` との対応)
- パッケージ名は `@shiguredo/mp4` とする。`@shiguredo/mp4` と `@shiguredo/mp4-wasm` はどちらも npm に未登録である
- `.wasm` はパッケージに同梱する。サイズは gzip 後約 188 KB (2026.6.0-canary.2 実測) で同梱に十分小さい
- Node.js 向け (CommonJS) とブラウザ向け (ESM) は wasm-bindgen の生成物が別物になるため、`exports` の conditions で分岐する
  - `require` は `pkg/node/mp4_wasm.js`、`import` は `pkg/web/mp4_wasm.js` を指す
  - `"type": "module"` を付けると Node 向けの生成物が ESM として解釈されて壊れるため、拡張子 (`.cjs` / `.mjs`) か `exports` の条件分岐で区別する
  - `--target bundler` を追加するか、web 向けは `init(wasmUrl)` を前提にするかを決める
- 型定義は wasm-bindgen が生成する `.d.ts` を同梱する。JSON 文字列で受け渡す部分 (`tracks()` / `nextSample()` / `appendSample()` など) は `string` のままなので、`typescript_custom_section` でスキーマに対応する型を補うか、手書きの追加型を同梱する
  - 生成される `.d.ts` は `Symbol.dispose` を含むため、TypeScript の `lib` に `ESNext.Disposable` が必要になる旨を README に書く
- テストは Vitest (`vp test`) で Node 環境と実ブラウザの両方で実行する
  - Node: wasm のロードと demux / mux の動作確認
  - ブラウザ: Vitest Browser Mode (playwright provider) で同一 spec を実行し、fetch 経由の wasm ロードを検証する
  - モックやスタブは使わず、実 wasm と実 MP4 データで検証する
- `ci.yml` に JS 側のテストジョブを追加する
- 公開は `release.yml` に npm publish ジョブを追加する。既存の wasm-opt 最適化フローを再利用し、タグのバージョンを `package.json` に注入し、canary タグは `--tag canary` で公開する
  - 初回公開は OIDC trusted publishing を使えない (npm CLI の制約)。手動で 1 回公開した後に Trusted Publisher を設定する
  - scoped パッケージの初回公開は `--access public` が必要になる
- npm バージョンは mp4-rs のバージョン (calver) と同一にし、`@shiguredo/mp4@2026.6.0` が GitHub Release と crates.io の同一バージョンに対応するようにする
  - `version()` の返り値と `package.json` の `version` が一致することを CI で検証する
  - 利用側が確認できるよう、`exports` に `./package.json` を含める

## 完了条件

- `crates/wasm/npm/` に `@shiguredo/mp4` パッケージが存在し、wasm-bindgen の生成物 + 型定義 + wasm 同梱の構成でビルドできる
- `require()` / `import` / ブラウザ bundler の 3 経路で読み込める
- `vp test` で Node 環境とブラウザ (Playwright) 環境の両方のテストが pass する
- `ci.yml` で JS 側のテストが実行される
- タグ push を契機に npm へ公開され、`npm install @shiguredo/mp4` で利用できる
- canary タグで `--tag canary` での公開が行われる
- `prek` のフックと `.gitignore` が Node 側のファイル (`node_modules` / `dist` 等) に対応している
