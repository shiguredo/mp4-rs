# wasm クレートに対する miri を CI で継続実行する仕組みを追加する

- Created: 2026-07-30
- Completed: YYYY-MM-DD
- Branch: feature/add-wasm-miri-ci
- Polished: 2026-07-30

## 目的

`crates/wasm` は unsafe raw pointer 経由の生 alloc / dealloc・FFI 境界（`extern "C"` の `mp4_alloc` / `mp4_free`）・JSON パーサ経由の `Vec<u8>` 確保など、UB リスクの高い経路を多く含む。しかし現状の動的検証手段は限定的で:

- `cargo test`: align 契約違反 / uninit read / UAF などの UB は検出できない
- fuzz: `fuzz/Cargo.toml` は wasm クレートを対象外にしている（依存は `shiguredo_mp4` のみ）

`issues/closed/0048-refactor-wasm-alloc-alignment.md` の align 契約違反はまさに miri で検出できる種類の UB だった。修正後の現行ソースでは `cargo +nightly miri test -p wasm` が全 58 テスト pass することを確認できるが、CI に miri が入っていないため、同種の潜在 UB が再発しても検出網がない。

本 issue では wasm クレートに限定して miri を CI で継続実行する仕組みを追加する。

## 現状

- `.github/workflows/ci.yml` は stable Rust で `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` を回す。wasm クレートは対象に含まれ、通常テストは走る（`cargo test -p wasm` で全 58 テストが pass する）
- 既存ジョブ ID は `ci` / `test-wasm` / `build-c-api` / `build-wasm` / `slack_notify`
- `ci.yml` の `on:` は `push`（`**.md` 除外）と `schedule`（`cron: "0 2 * * 1-5"`、月–金）
- miri は CI にも `Makefile` にも設定されていない（実装前の状態。本 issue で追加する）
- fuzz は `Makefile` の `fuzzing` ターゲットに `cargo +nightly fuzz run` として定義されているが、CI からは呼ばれない（手動運用のみ）
- `rust-toolchain.toml` は `channel = "stable"`。nightly は fuzz と同じ扱いで別途セットアップが必要
- 失敗時 Slack 通知は `ci.yml` の `slack_notify` が `shiguredo/github-actions` の `slack-notify` を `failure_and_fixed` モード・`slack_channel: rust-oss` で使う

## 設計方針

**スコープ**: `crates/wasm` のみ。まずここで miri の運用実績を積み、必要になれば他クレートへ広げる。主クレート（`shiguredo_mp4`）や `c-api` は本 issue では対象外。

**ワークフロー配置**: `.github/workflows/ci.yml` に独立ジョブ `miri` を追加する。既存ジョブ（`ci` / `test-wasm` / `build-c-api` / `build-wasm`）の手順は変えない。`slack_notify.needs` に `miri` を足す。

**実行タイミング**: 既存 CI と同じ `on:`（`push` + 月–金 cron）で回す。ジョブは他と並列なので、miri が遅くてもワークフロー全体の wall time は「最長ジョブ」側に支配される。

当初は「毎日 cron + `ci: miri` ラベル付き PR」の独立 `miri.yml` も検討したが、ローカル実測で `make miri` が数十秒程度だったため、まずは既存 CI と同じ条件に載せて運用し、CI 上の実測で遅すぎる場合に分離・間引きを再検討する。

**Makefile 目標**: fuzz の `fuzzing` と同じ扱いで `miri` ターゲットを追加する（`cargo +nightly miri test -p wasm`）。ローカル手動実行と CI から共通で叩けるようにする。

**GitHub Actions ジョブ（`ci.yml` の `miri`）**:
- toolchain: nightly + `miri` component（`rustup` で入れ、`cargo +nightly miri setup` する）
- 実行: `make miri`
- 失敗時通知: 既存の `slack_notify` に載せる（`needs` に `miri` を追加。チャネルは `rust-oss`）

## 完了条件

- `.github/workflows/ci.yml` に `miri` ジョブが追加され、既存 CI と同じトリガーで `make miri` が走る
- `slack_notify.needs` に `miri` が含まれている
- 既存ジョブ（`ci` / `test-wasm` / `build-c-api` / `build-wasm`）の手順は本 issue の変更で変わっていない
- `Makefile` に `miri` ターゲットが追加され、ローカルでも `make miri` で同じコマンドが走る
- wasm クレートの現行テストがすべて miri で pass する（`cargo test -p wasm` は 58 テスト。`cargo +nightly miri test -p wasm` も同数 pass することを実装着手時に再確認する）
- miri 特有の失敗（FFI 境界の未サポート等）が出た場合の対処方針（skip か fix）が Makefile コメントまたは README に明記されている
- 失敗時の通知経路が確立している（既存 `slack_notify` 経由）

## 解決方法

以下の順に対応する。前提確認で問題が判明した場合は設計方針に戻る:

1. **前提確認（ローカルで実施）**: `cargo +nightly miri test -p wasm` を走らせ、以下を確認する
   - 全テストが miri で pass する（実装着手時に再確認。現行ソースでは 58 件 pass を確認できる）
   - 実行時間の実測（CI 同居の妥当性を判断する材料）
   - FFI 境界（`mp4_alloc` / `mp4_free`）が miri で問題なく通ること
2. `Makefile` に `miri` ターゲットを追加する（`.PHONY` に追記、`cargo +nightly miri test -p wasm` を実行するレシピ、失敗時の対処方針コメント）
3. `.github/workflows/ci.yml` に `miri` ジョブを追加する
   - toolchain セットアップ: nightly + `miri` component
   - 実行: `make miri`
   - `slack_notify.needs` に `miri` を追加する
4. CI 上で miri ジョブが green になること、および実行時間を確認する（遅すぎれば分離・間引きを再検討する）

## CHANGES.md

`[ADD]` として `### misc` に記載する。CI infrastructure の追加であり、ライブラリ本体の挙動には影響しない。既存の `[ADD] fuzz ターゲット` 系エントリと同水準の扱い。
