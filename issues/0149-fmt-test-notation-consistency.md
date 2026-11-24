# テストコードの「エントリ」を「エントリー」に揃える

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-test-notation-consistency
- Polished: {YYYY-MM-DD}
- Refreshed: 2026-11-24

## 目的

テストコードのコメント、doc コメント、アサーションのメッセージで「エントリ」と「エントリー」が混在している。shiguredo-doc の、カタカナ語の末尾長音を残す規約に合わせて、「エントリー」に揃える。

## 現状

`rg -P "エントリ(?!ー)"` で数えた、テストコード中の「エントリ」の件数は次のとおり。

| ファイル | 件数 |
| --- | ---: |
| `tests/test_auxiliary.rs` | 7 |
| `pbt/tests/prop_fmp4_segment_mux_demux.rs` | 5 |
| `tests/test_boxes_moov_tree.rs` | 4 |
| `tests/test_boxes_fmp4.rs` | 3 |
| `pbt/tests/prop_boxes.rs` | 2 |
| `pbt/tests/prop_auxiliary.rs` | 2 |
| `tests/test_mux_fmp4_segment.rs` | 1 |
| `crates/wasm/src/boxes_tx3g.rs` の `mod tests` | 1 |
| 合計 | 25 |

- `tests/test_boxes_fmp4.rs` は、1 つのファイルの中で表記が揺れている
  - 冒頭の mod `encode_variable_uint_insufficient_buffer` は「1 エントリ」と書く
  - 後ろの mod `tfra_length_size_out_of_range` は「エントリー」と書く
- `pbt/tests/prop_fmp4_boxes.rs` には揺れがない。「エントリー」が 1 件あるだけである
- 揺れはアサーションのメッセージにもある。例は、`tests/test_auxiliary.rs` の「加算しようとしたエントリのサンプル数が 0x8000_0000 であること」と、`pbt/tests/prop_boxes.rs` の「隣接エントリが同じ sample_delta を持っている」である

揃える先は「エントリー」とする。

- shiguredo-doc の規約は「カタカナ語の末尾長音は基本的に残すこと」で、「エントリ」は例外として挙げられていない
- リポジトリ全体（`issues/` を除く）では、「エントリー」が 339 件、「エントリ」が 27 件ある。「サンプルエントリー」を除いても、「エントリー」は 98 件ある
- ただし、テストのディレクトリ（`tests/`、`pbt/`、`crates/wasm/tests/`）に限ると、「サンプルエントリー」を除いた「エントリー」は 23 件、「エントリ」は 24 件で、ほぼ同数である。揃える先は件数ではなく、規約で決める

## 設計方針

- 上の表の 25 件を「エントリー」に書き換える。「エントリ数」は「エントリー数」にする
- 書き換えるのは日本語の文言だけとし、テスト関数名や識別子は変えない
- テスト以外のコードと `CHANGES.md` は対象外とする。`src/` は `mod tests` を含めてすでに「エントリー」に揃っている
  - テスト以外のコードとは、`src/` と `crates/wasm/src/` のうち `mod tests` の外の部分を指す

## 完了条件

- `tests/`、`pbt/`、`crates/wasm/tests/` に `rg -P "エントリ(?!ー)"` を実行すると、0 件になる
- `crates/wasm/src/boxes_tx3g.rs` の `mod tests` に、「エントリ」が残っていない
- `cargo test --workspace` が成功する
