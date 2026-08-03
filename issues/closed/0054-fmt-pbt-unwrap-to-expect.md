# `pbt/tests` と `src/descriptors.rs` のテストコードに `.unwrap()` が 350 箇所残っており `.expect("MESSAGE")` の規約に違反している

- Priority: Low
- Created: 2026-07-27
- Completed: 2026-07-28
- Model: Opus 5
- Branch: feature/refactor-pbt-unwrap-to-expect
- Polished: 2026-07-28

## 目的

`pbt/tests` 配下と `src/descriptors.rs` のテストコードに残っている `.unwrap()` を `.expect("MESSAGE")` に置き換え、`shiguredo-rust` の規約に揃える。

規約は次のとおり定めている。

> `.unwrap()` ではなく `.expect("MESSAGE")` を使用すること
>
> - `.unwrap()` では情報が少ない
> - `.expect("MESSAGE")` を使用して、最低限「このパニックが状況によっては発生する可能性がある」のか、それとも「絶対に発生しない想定（発生した場合は実装バグ）」なのかがメッセージから分かるようにすること

## 優先度根拠

Low。規約違反ではあるがテストコードに閉じており、動作には影響しない。既存のテスト整備系 issue（0037 / 0038 / 0039 / 0041）と同じ性質の作業で、それらと同等かそれ以下の優先度で足りる。

## 現状

`.unwrap()` の出現箇所は 350 箇所（2026-07-27 時点の実測）。

| ファイル | 箇所数 |
| --- | --- |
| `pbt/tests/prop_boxes.rs` | 120 |
| `pbt/tests/prop_fmp4_boxes.rs` | 55 |
| `pbt/tests/prop_additional_boxes.rs` | 54 |
| `pbt/tests/prop_container_boxes.rs` | 36 |
| `pbt/tests/prop_codec_boxes.rs` | 30 |
| `pbt/tests/prop_basic_types.rs` | 28 |
| `pbt/tests/prop_descriptors.rs` | 18 |
| `pbt/tests/prop_boxes_sample_entry.rs` | 6 |
| `pbt/tests/prop_auxiliary.rs` | 1 |
| `src/descriptors.rs`（`#[cfg(test)] mod tests` 内） | 2 |

`pbt/tests/prop_mux_demux.rs` は対象外。同ファイルの `.unwrap()` 9 箇所（および `.unwrap_or(NonZeroU32::MIN)` 6 箇所）は 0046 の対応時に置き換え済みで、本 issue の先行事例になる。

```rust
// 置き換え後の例（pbt/tests/prop_mux_demux.rs）
let video_timescale = NonZeroU32::new(30).expect("30 は非ゼロ");
let timescale = NonZeroU32::new(timescale).expect("Strategy の値域が 1 以上なので非ゼロ");
```

`unwrap_or()` で既定値を与えている箇所も同時に見直した。値域が保証されていて `None` にならない場合、既定値で握り潰すと将来 Strategy の値域を広げたときに黙って別の値でテストが走るため、`.expect()` にして根拠をメッセージに書いてある。この見直しは本 issue の必須スコープには含めない（下記「設計方針」「完了条件」参照）。

対象ファイルには既に英語で書かれた `.expect()` メッセージが多数存在する（実測: `prop_container_boxes.rs` 71 件、`prop_auxiliary.rs` 51 件、`prop_boxes_sample_entry.rs` 19 件など）。先行事例の `prop_mux_demux.rs` も `.expect("failed to create muxer")` 等の英語メッセージが多く残っており、0046 は `.unwrap()` からの新規置換分のうち「発生しない根拠を説明する型」だけを日本語で書き、既存の英語 `.expect()` には手を付けない方針で完了している。これら既存の英語 `.expect()` の日本語化は 0037（PBT テストと C API テストの expect / assert メッセージ日本語化）のスコープであり、本 issue のスコープ外。本 issue は `.unwrap()` からの新規置換分のみを対象とし、新規分は設計方針に従って日本語で書く（結果として 0037 完了後に全メッセージが日本語で揃う）。

## 設計方針

- メッセージは AGENTS.md の「テストのログメッセージは全て日本語にすること」に従い日本語で書く
- 「なぜパニックしないと言えるのか」を書く。単に `.expect("failed")` のような情報量の無いメッセージにしない
- `Strategy` が保証する値域に依存する箇所は、その値域を根拠として書く
- `unwrap_or()` は本 issue の必須スコープには含めない。ただし置換作業中に値域保証があり実質的に握り潰しになっている `unwrap_or()` を見つけた場合は、先行事例 `pbt/tests/prop_mux_demux.rs`（0046 で対応済み）と同じ扱いで `.expect()` に置き換えてよい。完了条件は `.unwrap()` の grep 結果のみで判定するため、`unwrap_or()` の残置は完了判定に影響しない
- ファイル数が多いため、ファイル単位でコミットを分けることを検討する

## 完了条件

- `grep -r "\.unwrap()" pbt/tests src/descriptors.rs` が 0 件になること
- 置き換えたメッセージがすべて日本語で、パニックしない根拠を説明していること
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --all-targets -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` が通ること

## 解決方法

1. 対象 350 箇所の `.unwrap()` をすべて `.expect("MESSAGE")` へ置き換えた。メッセージは設計方針に従い日本語で、パニックしない根拠（`Strategy` の値域保証、直前エンコード結果のデコード保証、null 未含有、`prop_assert!` / `is_some` 検証済みの取り出しなど）を明示した
2. 対象ファイル: `src/descriptors.rs` / `pbt/tests/prop_auxiliary.rs` / `pbt/tests/prop_basic_types.rs` / `pbt/tests/prop_boxes_sample_entry.rs` / `pbt/tests/prop_descriptors.rs` / `pbt/tests/prop_codec_boxes.rs` / `pbt/tests/prop_container_boxes.rs` / `pbt/tests/prop_additional_boxes.rs` / `pbt/tests/prop_fmp4_boxes.rs` / `pbt/tests/prop_boxes.rs`
3. `grep -r "\.unwrap()" pbt/tests src/descriptors.rs` が 0 件になること、および `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --all-targets -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm`（全 610 テスト）がすべて通ることを確認した
4. `unwrap_or()` の見直しは本 issue のスコープ外として実施していない（設計方針および完了条件どおり）。既存の英語 `.expect()` メッセージの日本語化も 0037 のスコープとして手を付けていない
