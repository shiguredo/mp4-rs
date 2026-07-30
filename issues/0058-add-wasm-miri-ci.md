# wasm クレートに対する miri を CI で継続実行する仕組みを追加する

- Created: 2026-07-30
- Completed: YYYY-MM-DD
- Branch: feature/add-wasm-miri-ci
- Polished: YYYY-MM-DD

## 目的

`crates/wasm` は unsafe raw pointer 経由の生 alloc / dealloc・FFI 境界（`extern "C"` の `mp4_alloc` / `mp4_free`）・JSON パーサ経由の `Vec<u8>` 確保など、UB リスクの高い経路を多く含む。しかし現状の動的検証手段は限定的で:

- `cargo test`: align 契約違反 / uninit read / UAF などの UB は検出できない
- fuzz: `fuzz/Cargo.toml` は wasm クレートを対象外にしている

`issues/closed/0048-refactor-wasm-alloc-alignment.md` の align 契約違反はまさに miri で検出できる種類の UB で、修正後は miri で当該経路が実測クリーンに通ることを確認済み。しかし CI に miri が入っていないため、同種の潜在 UB が再発しても検出網がない。

本 issue では wasm クレートに限定して miri を CI で継続実行する仕組みを追加する。

## 現状

- `.github/workflows/ci.yml` は stable Rust で `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` を回す。wasm クレートは対象に含まれ、通常テストは走る（`cargo test -p wasm` で全 58 テストが pass する）
- miri は CI にも `Makefile` にも設定されていない（`grep miri Makefile .github/workflows/*.yml` の一致 0 件）
- fuzz は `Makefile` の `fuzzing` ターゲットに `cargo +nightly fuzz run` として定義されているが、CI からは呼ばれない（手動運用のみ）
- `rust-toolchain.toml` は `channel = "stable"`。nightly は fuzz と同じ扱いで別途セットアップが必要

## 設計方針

**スコープ**: `crates/wasm` のみ。まずここで miri の運用実績を積み、必要になれば他クレートへ広げる。主クレート（`shiguredo_mp4`）や `c-api` は本 issue では対象外。

**実行タイミング**: 案 A（PR ラベル + nightly cron の併用）を採用予定。案 B（毎 PR）と案 C（cron のみ）は不採用。

- 採用: 案 A（PR ラベル + nightly cron）
  - nightly cron: マージ済み develop に対する回帰検出を目的に毎日 1 回実行
  - PR ラベル: `ci: miri` ラベルが付いた PR で PR check として実行（事前検証）
  - デフォルトの PR check には入れないので通常の PR は影響を受けない
- 不採用: 案 B（毎 PR）
  - miri は通常テストの 5〜10 倍遅く、unsafe 密度の高い wasm 経路では更に遅くなる可能性が高い
  - 実測前に毎 PR 化するのは早計。運用実績を積んでから検討する
- 不採用: 案 C（cron のみ）
  - PR 段階で発見できず、develop マージ後に赤くなる期間ができる

**Makefile 目標**: fuzz の `fuzzing` と同じ扱いで `miri` ターゲットを追加する（`cargo +nightly miri test -p wasm`）。ローカル手動実行と CI から共通で叩けるようにする。

**GitHub Actions ジョブ**: 既存 `check` ジョブ（stable）は変えず、独立ジョブとして追加する。`dtolnay/rust-toolchain@nightly` で nightly + `miri` component を用意し、Makefile 経由で叩く。失敗時通知は既存の Slack 通知パターン（`shiguredo/github-actions` の `slack-notify` を `failure_and_fixed` モード）に合わせる。

## 完了条件

- `.github/workflows/` に miri ジョブが追加され、`crates/wasm` に対する `cargo +nightly miri test` が nightly cron と PR ラベル（`ci: miri`）の両トリガーで走る
- `Makefile` に `miri` ターゲットが追加され、ローカルでも `make miri` で同じコマンドが走る
- wasm クレートの現行テストがすべて miri で pass する（起票時点で `cargo test -p wasm` は 58 テストが pass し、うち `test_json_to_hev1_free_more_nalus_than_arrays` の miri 通過は `issues/closed/0048-refactor-wasm-alloc-alignment.md` の Round 3 で実測確認済み）
- miri 特有の失敗（FFI 境界の未サポート等）が出た場合の対処方針（skip か fix）が Makefile コメントまたは README に明記されている
- 失敗時の通知経路（Slack 等）が確立している

## 解決方法

以下の順に対応する。前提確認で問題が判明した場合は設計方針に戻る:

1. **前提確認（ローカルで実施）**: `cargo +nightly miri test -p wasm` を走らせ、以下を確認する
   - 全テストが miri で pass する（起票時点は確認済み。実装着手時に再確認）
   - 実行時間の実測（cron 頻度と PR label 運用の妥当性を判断する材料）
   - FFI 境界（`mp4_alloc` / `mp4_free`）が miri で問題なく通ること。miri は `#[unsafe(no_mangle)] extern "C"` を自プロセス内呼び出しでは通す想定だが、実測で確認する
2. `Makefile` に `miri` ターゲットを追加する（`.PHONY` に追記、`cargo +nightly miri test -p wasm` を実行するレシピ）
3. `.github/workflows/` に nightly cron + PR ラベルの両トリガーを持つ miri ジョブを追加する
   - トリガー: `schedule`（毎日 1 回）と `pull_request` の `types: [labeled]` + label フィルタ
   - toolchain セットアップ: nightly + `miri` component
   - 失敗時通知: 既存の Slack 通知パターンに合わせる
4. miri 特有の失敗が出た場合の対処方針（Makefile コメントまたは README への追記）を用意する
5. CI 上で miri ジョブが green になることを確認する

## CHANGES.md

`[ADD]` として `### misc` に記載する。CI infrastructure の追加であり、ライブラリ本体の挙動には影響しない。既存の `[ADD] fuzz ターゲット` 系エントリと同水準の扱い。
