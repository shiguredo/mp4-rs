# pbt/tests に `.unwrap()` が 350 箇所残っており `.expect("MESSAGE")` の規約に違反している

- Priority: Low
- Created: 2026-07-27
- Completed: YYYY-MM-DD
- Model: Opus 5
- Branch: feature/fmt-pbt-unwrap-to-expect
- Polished: YYYY-MM-DD

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

`pbt/tests/prop_mux_demux.rs` は対象外。同ファイルの 12 箇所は 0046 の対応時に置き換え済みで、本 issue の先行事例になる。

```rust
// 置き換え後の例（pbt/tests/prop_mux_demux.rs）
let video_timescale = NonZeroU32::new(30).expect("30 は非ゼロ");
let timescale = NonZeroU32::new(timescale).expect("Strategy の値域が 1 以上なので非ゼロ");
```

`unwrap_or()` で既定値を与えている箇所も同時に見直した。値域が保証されていて `None` にならない場合、既定値で握り潰すと将来 Strategy の値域を広げたときに黙って別の値でテストが走るため、`.expect()` にして根拠をメッセージに書いてある。

## 設計方針

- メッセージは AGENTS.md の「テストのログメッセージは全て日本語にすること」に従い日本語で書く
- 「なぜパニックしないと言えるのか」を書く。単に `.expect("failed")` のような情報量の無いメッセージにしない
- `Strategy` が保証する値域に依存する箇所は、その値域を根拠として書く
- `unwrap_or()` で `None` を握り潰している箇所があれば、値域が保証されているかを確認し、保証されているなら `.expect()` に変える
- ファイル数が多いため、ファイル単位でコミットを分けることを検討する

## 完了条件

- `grep -r "\.unwrap()" pbt/tests src/descriptors.rs` が 0 件になること
- 置き換えたメッセージがすべて日本語で、パニックしない根拠を説明していること
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --all-targets -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` が通ること
