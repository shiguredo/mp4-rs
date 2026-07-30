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
- 既存ジョブ ID は `ci` / `test-wasm` / `build-c-api` / `build-wasm` / `slack_notify`。`check` という名前の GitHub Actions ジョブは無い（`Makefile` の `check` ターゲットとは別物）
- `ci.yml` の `on:` は `push` と `schedule`（`cron: "0 2 * * 1-5"`、月–金のみ）で、`pull_request` トリガーは無い
- miri は CI にも `Makefile` にも設定されていない（`Makefile` / `.github/workflows/*.yml` に `miri` の一致 0 件）
- fuzz は `Makefile` の `fuzzing` ターゲットに `cargo +nightly fuzz run` として定義されているが、CI からは呼ばれない（手動運用のみ）
- `rust-toolchain.toml` は `channel = "stable"`。nightly は fuzz と同じ扱いで別途セットアップが必要
- 失敗時 Slack 通知は `ci.yml` の `slack_notify` が `shiguredo/github-actions` の `slack-notify` を `failure_and_fixed` モード・`slack_channel: rust-oss` で使う（`release.yml` は同 action / mode だがチャネルが `hisui`）

## 設計方針

**スコープ**: `crates/wasm` のみ。まずここで miri の運用実績を積み、必要になれば他クレートへ広げる。主クレート（`shiguredo_mp4`）や `c-api` は本 issue では対象外。

**ワークフロー配置**: `.github/workflows/miri.yml` として **新規独立 workflow** を追加する。既存の `ci.yml`（ジョブ `ci` / `test-wasm` / `build-c-api` / `build-wasm` / `slack_notify` のトリガー・手順・`needs`）は変更しない。

理由: GitHub Actions の `on:` は workflow 単位で共有される。`ci.yml` に「毎日 cron」や `pull_request: types: [labeled]` を足すと、既存ジョブの起動条件まで変わる。独立 workflow にすれば既存 CI に影響せず、miri だけのトリガーを定義できる。

**実行タイミング**: 案 A（PR ラベル + 毎日 cron の併用）を採用する。案 B（毎 PR）と案 C（cron のみ）は不採用。

- 採用: 案 A（PR ラベル + 毎日 cron）
  - 毎日 cron: マージ済み develop に対する回帰検出を目的に毎日 1 回実行（`miri.yml` 専用の `schedule`。既存 `ci.yml` の月–金 cron とは別）
  - PR ラベル: `ci: miri` ラベルが付いた PR で PR check として実行（事前検証）
  - デフォルトの PR check（ラベル無し）には入れないので、通常の PR は影響を受けない
- 不採用: 案 B（毎 PR）
  - miri は通常テストより遅く、本クレート実測でも `cargo +nightly miri test -p wasm` が数十秒かかる。unsafe 密度の高い wasm 経路では更に遅くなる可能性が高い
  - 実測前に毎 PR 化するのは早計。運用実績を積んでから検討する
- 不採用: 案 C（cron のみ）
  - PR 段階で発見できず、develop マージ後に赤くなる期間ができる

**PR トリガーの具体**: `pull_request` の `types` は `labeled` / `synchronize` / `reopened` とする。ジョブの `if` は schedule とラベル付き PR の両方を通す形にする（例: `github.event_name == 'schedule' || contains(github.event.pull_request.labels.*.name, 'ci: miri')`）。ラベル条件だけをジョブ全体の `if` に置くと、`schedule` では `pull_request` が無く毎日 cron が走らない。`labeled` だけではラベル付与時のみ走り、付与後の追従 push で再実行されないため、事前検証としても不足する。

**Makefile 目標**: fuzz の `fuzzing` と同じ扱いで `miri` ターゲットを追加する（`cargo +nightly miri test -p wasm`）。ローカル手動実行と CI から共通で叩けるようにする。

**GitHub Actions ジョブ（`miri.yml`）**:
- toolchain: nightly + `miri` component（セットアップ手段は既存リポジトリの慣例に合わせ、`rustup` で nightly を入れ `miri` component を追加する。`dtolnay/rust-toolchain` を新規導入する場合は、既存 `ci.yml` が `rustup update stable` であることに対する意図を PR 説明に書く）
- 実行: Makefile 経由で `make miri`（中身は `cargo +nightly miri test -p wasm`）
- 失敗時通知: `miri.yml` 内に独立の Slack 通知ステップ（または専用ジョブ）を置き、`shiguredo/github-actions` の `slack-notify` を `failure_and_fixed` モード・`slack_channel: rust-oss`（`ci.yml` と同じチャネル）で使う。`ci.yml` の `slack_notify.needs` には触らない

## 完了条件

- `.github/workflows/miri.yml` が追加され、`crates/wasm` に対する `cargo +nightly miri test`（`make miri`）が、毎日 cron と `ci: miri` ラベル付き PR（`labeled` / `synchronize` / `reopened`）の両トリガーで走る
- 既存の `.github/workflows/ci.yml` のトリガー・ジョブ手順・`slack_notify.needs` が本 issue の変更で変わっていない
- `Makefile` に `miri` ターゲットが追加され、ローカルでも `make miri` で同じコマンドが走る
- wasm クレートの現行テストがすべて miri で pass する（`cargo test -p wasm` は 58 テスト。`cargo +nightly miri test -p wasm` も同数 pass することを実装着手時に再確認する）
- miri 特有の失敗（FFI 境界の未サポート等）が出た場合の対処方針（skip か fix）が Makefile コメントまたは README に明記されている
- 失敗時の通知経路が確立している（`miri.yml` から `slack-notify` / `failure_and_fixed` / `rust-oss`）

## 解決方法

以下の順に対応する。前提確認で問題が判明した場合は設計方針に戻る:

1. **前提確認（ローカルで実施）**: `cargo +nightly miri test -p wasm` を走らせ、以下を確認する
   - 全テストが miri で pass する（実装着手時に再確認。現行ソースでは 58 件 pass を確認できる）
   - 実行時間の実測（cron 頻度と PR label 運用の妥当性を判断する材料）
   - FFI 境界（`mp4_alloc` / `mp4_free`）が miri で問題なく通ること。miri は `#[unsafe(no_mangle)] extern "C"` を自プロセス内呼び出しでは通す想定だが、実測で確認する
2. `Makefile` に `miri` ターゲットを追加する（`.PHONY` に追記、`cargo +nightly miri test -p wasm` を実行するレシピ）
3. `.github/workflows/miri.yml` を新規追加する（既存 `ci.yml` は変更しない）
   - トリガー: `schedule`（毎日 1 回）と `pull_request` の `types: [labeled, synchronize, reopened]`。ジョブ `if` は `github.event_name == 'schedule' || contains(... 'ci: miri')`（または同等）とし、cron とラベル付き PR の両方を通す
   - toolchain セットアップ: nightly + `miri` component
   - 実行: `make miri`
   - 失敗時通知: `slack-notify` / `failure_and_fixed` / `rust-oss`（`miri.yml` 内で完結）
4. miri 特有の失敗が出た場合の対処方針（Makefile コメントまたは README への追記）を用意する
5. CI 上で miri ジョブが green になることを確認する（ラベル付き PR または cron 相当の手動実行）

## CHANGES.md

`[ADD]` として `### misc` に記載する。CI infrastructure の追加であり、ライブラリ本体の挙動には影響しない。既存の `[ADD] fuzz ターゲット` 系エントリと同水準の扱い。
