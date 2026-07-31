# wasm の mp4_alloc 失敗（OOM）時の方針を abort か Result かに揃える

- Priority: Medium
- Created: 2026-07-31
- Completed: YYYY-MM-DD
- Branch: feature/change-wasm-alloc-oom-policy
- Polished: YYYY-MM-DD

## 目的

`crates/wasm` では `mp4_alloc` が確保失敗時に null を返し得る一方、呼び出し側の多くは失敗を明示エラーにせず、`(null, 0)` や `(null, 非ゼロ)` を構造体に載せたまま `Ok` を返す。通常の Rust では OOM 時にプロセスが abort することが多く、ここだけ中途半端に「死なない OOM」を抱えると、フォーマット経路での防御的ガードや、パース経路だけ `Err` 化するような部分対応では方針がぶれる。

wasm 全体で OOM（`mp4_alloc` 失敗）の扱いを **abort に寄せるか、一貫して `Result` で伝播するか** を決め、実装を揃える。

## 優先度根拠

現実の wasm 実行で `mp4_alloc` が null を返す頻度は低い。ただし契約としては「壊れたポインタ状態を `Ok` で返す」経路が残っており、closed issue 0034 でフォーマット側の UB は潰しても、パース時の確保失敗そのものは未解決のままである。部分的な `Err` 化は他経路との温度差を生むため、方針決定付きの change として扱う。

## 現状

- `mp4_alloc`（`crates/wasm/src/lib.rs`）は `std::alloc::alloc` の結果をそのまま返す。失敗時は null
- `allocate_and_copy_bytes`（`crates/wasm/src/boxes.rs`）は空入力・確保失敗で `(null, 0)` を返す
- `allocate_and_copy_array_list`（同）は要素ポインタに `allocate_and_copy_bytes(...).0` だけを使い、サイズは `array.len()` から作る。非空要素の確保失敗後は `(null, 非ゼロ)` が並び得る
- `parse_json_mp4_sample_entry_*` 系は確保失敗を検知せず `Ok(構造体)` を返し得る
- closed issue 0034 で JSON フォーマット側は null を `from_raw_parts` に渡さないガードを入れたが、フォーマット側ではエラーにせず空配列として出力するだけである

## 設計方針

次のいずれか **1 つ** に wasm 全体を揃える。パース経路だけ `Err` にする部分対応は採らない。

### 案 A: abort に寄せる

- `mp4_alloc` 失敗時に panic / `handle_alloc_error` 相当で落とす
- 呼び出し側の null チェックや「失敗を空データとして抱える」経路を整理する
- 通常の Rust の OOM 方針に近い

### 案 B: `Result` で一貫伝播する

- `allocate_and_copy_*` および `parse_json_*` 等の確保経路を失敗時 `Err` にする
- 部分確保後の巻き戻し（free）を設計する
- JS / 呼び出し側が観測できる失敗にする

実装着手前に A / B を確定する。確定後の変更範囲は `mp4_alloc` 利用者を grep して洗い出す。

## 完了条件

- OOM 方針が abort または `Result` 伝播のどちらかに文書化・実装として確定していること
- 方針に反する「null を抱えたまま `Ok`」経路が、対象とした wasm 確保経路から除去されていること（または abort 方針なら失敗時に確実に落ちること）
- 既存のテストおよび `cargo clippy` が通ること
- `CHANGES.md` に方針に応じたエントリがあること

## 解決方法

（方針確定後に記載する）

## 関連

- closed issue 0034（フォーマット側の `from_raw_parts` UB 除去。本 issue の前提）
