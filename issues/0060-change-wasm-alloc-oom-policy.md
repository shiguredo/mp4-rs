# wasm の mp4_alloc 失敗（OOM）時の方針を abort に統一する

- Priority: Medium
- Created: 2026-07-31
- Completed: YYYY-MM-DD
- Branch: feature/change-wasm-alloc-oom-policy
- Polished: 2026-07-31

## 目的

`crates/wasm` では `mp4_alloc` が確保失敗時に null を返し得る一方、呼び出し側の多くは失敗を明示エラーにせず、`(null, 0)` や `(null, 非ゼロ)` を構造体に載せたまま `Ok` を返す。通常の Rust では OOM 時にプロセスが abort することが多く、ここだけ中途半端に「死なない OOM」を抱えると、フォーマット経路での防御的ガードや、パース経路だけ `Err` 化するような部分対応では方針がぶれる。

wasm 全体で OOM（`mp4_alloc` 失敗）の扱いを **abort に統一** し、実装を揃える。

## 優先度根拠

現実の wasm 実行で `mp4_alloc` が null を返す頻度は低い。ただし契約としては「壊れたポインタ状態を `Ok` で返す」経路が残っており、closed issue 0034 でフォーマット側の UB は潰しても、パース時の確保失敗そのものは未解決のままである。部分的な `Err` 化は他経路との温度差を生むため、方針決定付きの change として扱う。

## 現状

- `mp4_alloc`（`crates/wasm/src/lib.rs`）は `std::alloc::alloc` の結果をそのまま返す。失敗時は null
- `allocate_and_copy_bytes`（`crates/wasm/src/boxes.rs`）は空入力・確保失敗で `(null, 0)` を返す
- `allocate_and_copy_array_list`（同）は要素ポインタに `allocate_and_copy_bytes(...).0` だけを使い、サイズは `array.len()` から作る。非空要素の確保失敗後は `(null, 非ゼロ)` が並び得る
- `parse_json_mp4_sample_entry_*` 系は確保失敗を検知せず `Ok(構造体)` を返し得る
- closed issue 0034 で JSON フォーマット側は null を `from_raw_parts` に渡さないガードを入れたが、フォーマット側ではエラーにせず空配列として出力するだけである

## 設計方針

wasm 全体を **abort に寄せる** で確定する（案 A）。パース経路だけ `Err` にする部分対応は採らない。

### abort を採る理由

- **標準 Rust の慣行と一致する**。`Vec` / `Box` などは OOM で abort する。`parse_json_mp4_sample_entry_*` は内部で `Vec<Vec<u8>>` を構築するため、そもそも `mp4_alloc` を `Result` 化しても `Vec` 経路の OOM は abort する。「全 OOM を `Result` で観測可能にする」は原理的に達成できない
- **コードが単純化される**。`mp4_alloc` を `handle_alloc_error` で落とせば、`allocate_and_copy_bytes` / `allocate_and_copy_aligned` の `null` 返却分岐、`allocate_and_copy_array_list` の「非空要素側の確保失敗で `(null, 非ゼロ)` が並ぶ」非常態、closed issue 0034 で入れた fmt 側の null ガードや `free_hevc_sample_entry_fields` の「`nalu_counts == null && nalu_data != null`」経路の脚注がまるごと不要になる
- **JS 側の観測性は失われない**。wasm での abort は trap になり、JS 側では `RuntimeError` として throw される。「壊れたポインタを載せた `Ok`」を silent に返すより明確な失敗になる
- **OOM 頻度が低いため `Err` 回復要件が弱い**。優先度根拠で確認済み

### `Result` 伝播（案 B）を採らない理由

- `Vec` などの標準 API 経路の OOM が abort する以上、部分的 `Err` 化しか達成できず「一貫伝播」の看板と実態が乖離する
- `allocate_and_copy_array_list` の途中失敗ロールバック（先行確保分の `mp4_free`）を新規で書く必要があり、追加 unsafe と invariant を持ち込む
- API 破壊が大きい（`allocate_and_copy_*` / `parse_json_*` の呼び出し 70 箇所前後を全修正）

## 解決方法

### 1. `mp4_alloc` を abort 側に寄せる

- `crates/wasm/src/lib.rs` の `mp4_alloc` を、`std::alloc::alloc` が null を返したときに `std::alloc::handle_alloc_error(layout)` を呼び出すよう変更する
- `size == 0` は従来通り null を返す（「空入力は null」の呼び出し規約は維持）
- doc コメントの契約を「サイズ 0 以外では必ず有効ポインタを返す」に更新する

### 2. `allocate_and_copy_*` から OOM 分岐を撤去する

- `allocate_and_copy_bytes`（`crates/wasm/src/boxes.rs`）: `mp4_alloc` 直後の `is_null()` 分岐を削除する。「空入力 → `(null, 0)`」は残す
- `allocate_and_copy_aligned`（同）: `std::alloc::alloc` を直接呼ぶ経路のため、null 分岐を `handle_alloc_error(layout)` に置き換える。「空入力 → `(null, 0)`」は残す
- `allocate_and_copy_array_list`（同）: 上記変更により部分失敗の非常態自体が消えるので、追加のロジック変更は不要。コメントを更新する

### 3. closed issue 0034 由来の防御的ガード・脚注を整理する

abort 後も空入力は `(null, 0)` のままなので、空要素判定の `size == 0` ガードは残す。OOM 由来の `(null, 非ゼロ)` と「確保失敗」前提だけを消す。方針は次で統一する。

- **配列要素系**（`size == 0 || ptr.is_null()` → `size == 0` のみ、`(null, 非ゼロ)` コメント削除）:
  - `HevcNaluArrays::fmt`（`crates/wasm/src/boxes.rs`）
  - `NaluList::fmt`（`crates/wasm/src/boxes_avc1.rs`）
  - `FtabList::fmt`（`crates/wasm/src/boxes_tx3g.rs`）
- **単一バッファ系**（`size == 0 || ptr.is_null()` → `size == 0` のみ、「空入力・確保失敗」コメントを「空入力で `(null, 0)`」に更新）:
  - `fmt_json_mp4_sample_entry_av01`（`crates/wasm/src/boxes_av01.rs`）
  - `fmt_json_mp4_sample_entry_mp4a`（`crates/wasm/src/boxes_mp4a.rs`）
  - `fmt_json_mp4_sample_entry_flac`（`crates/wasm/src/boxes_flac.rs`）
- `HevcSampleEntryFields`（`crates/wasm/src/boxes.rs`）の doc を更新する。フェーズ 2 の確保失敗は `handle_alloc_error` でプロセス abort するため、OOM 途中の部分確保を `Drop` で回収する設計にはしない（空入力の `(null, 0)` 契約は維持）。所有権の再設計は本 issue の対象外とする
- `free_hevc_sample_entry_fields` の「`nalu_counts == null && nalu_data != null` は非常態」コメントを削除する（生産パスでは発生し得なくなるため）

### 4. フリー側 API の null 安全性は据え置く

- `free_array_list` の「要素 ptr が null なら skip」など、空入力用途で意味を持つガードは維持する
- `mp4_free` は JS 側から null が渡り得るため null 安全のまま

### 5. テスト

- 既存の空入力 null テスト（`test_allocate_and_copy_u16_array_empty_returns_null` など）は据え置く
- `test_free_hevc_sample_entry_fields_survives_partial_alloc_failure_state`（`crates/wasm/src/boxes.rs`）は削除する。前提としている「`allocate_and_copy_array_list` の部分的な `mp4_alloc` 失敗でのみ発生する非常態」が生産パスから消えるため
- OOM abort を単体テストで再現するのは現実的でないため、新規テストは追加しない
- 既存テスト・`cargo clippy` が通ることを確認する

### 6. CHANGES.md

- 「wasm の OOM 方針を abort に統一（`mp4_alloc` 失敗時は `handle_alloc_error` で abort）」の主旨で 1 エントリ追加する
- `mp4_alloc` の契約変更（サイズ非 0 では null を返さなくなる）に触れる

## 完了条件

- `mp4_alloc` および `allocate_and_copy_*` の内部 OOM 分岐が撤去され、失敗時に `handle_alloc_error` で abort することがコードで確認できる
- closed issue 0034 で入れた「壊れたポインタでも UB にならない」ガードのうち、abort 前提で不要になった分（Hevc / avc1 / tx3g / av01 / mp4a / flac の fmt 側）が整理されている
- `test_free_hevc_sample_entry_fields_survives_partial_alloc_failure_state` が削除されていること
- 既存のテストおよび `cargo clippy` が通ること
- `CHANGES.md` に方針変更のエントリがあること

## 関連

- closed issue 0034（フォーマット側の `from_raw_parts` UB 除去。本 issue の前提）
