# wasm の hev1/hvc1 sample entry free が mp4_free(ptr, 0) で no-op になりリーク + free_array_list の count 不一致で UB になる

- Priority: High
- Created: 2026-07-15
- Completed: 2026-07-28
- Model: opencode-go glm-5.2
- Branch: feature/fix-wasm-hev1-hvc1-free
- Polished: 2026-07-27

## 目的

WebAssembly 版の HEV1 / HVC1 サンプルエントリのメモリ解放処理が壊れており、(1) `mp4_free(ptr, 0)` で常に no-op になりヒープリークする、(2) `free_array_list` に渡す count が確保時の要素数と不一致でヒープ破壊 UB になる、これら 2 つの問題を修正する。

## 優先度根拠

発火条件は 2 つの問題で異なるため、分けて述べる。

- 問題 1 のリークは `naluArrays` が非空であれば必ず発火する。ストリーミング用途でサンプルエントリーを繰り返し parse / free するたびに蓄積し、長時間実行で OOM に至る
- 問題 2 のヒープ破壊（UB）は `nalu_counts` の総和が `nalu_array_count` と一致しない入力で発火する（発火する入力・しない入力の具体例は現状セクションを参照）。影響は方向によって異なり、総数 > 配列数では余剰バッファのリークと layout 不一致の `dealloc`、総数 < 配列数では確保外の読み出しと不正なポインタの解放になる。後者はアロケータが abort しうるため、クラッシュ・セキュリティリスクに直結する

いずれも wasm の公開 API 経路（`mp4_mux_sample_from_json` → `mp4_mux_sample_free` → `mp4_sample_entry_free`）から到達するため High とする。

## 現状

### 問題 1: mp4_free(ptr, 0) で no-op（リーク）

```rust
// crates/wasm/src/lib.rs:58-61
pub unsafe extern "C" fn mp4_free(ptr: *mut u8, size: u32) {
    if ptr.is_null() || size == 0 {
        return;
    }
```

```rust
// crates/wasm/src/boxes_hev1.rs:203-212
if !entry.nalu_types.is_null() {
    unsafe {
        crate::mp4_free(entry.nalu_types.cast_mut(), 0);
    }
    // ...
}
if !entry.nalu_counts.is_null() {
    unsafe {
        crate::mp4_free(entry.nalu_counts.cast_mut() as *mut u8, 0);
    }
```

`mp4_free` は `size == 0` で即 return する。一方 free 側は常に `size = 0` を渡している。確保側（`allocate_and_copy_bytes`）は実サイズで確保しているため、解放されずに常にリークする。`crates/wasm/src/boxes_hvc1.rs:203-212` も同型（キャストの書き方だけが異なる）。

wasm 側で `mp4_free` に 0 を渡しているのはこの 4 箇所だけであり、`boxes_av01.rs` / `boxes_flac.rs` / `boxes_mp4a.rs` / `boxes_stpp.rs` / `boxes_wvtt.rs` はいずれも実サイズを渡している。

### 問題 2: free_array_list の count 不一致（UB）

```rust
// crates/wasm/src/boxes_hev1.rs:137 (確保側)
let (nalu_data, nalu_sizes, _) = crate::boxes::allocate_and_copy_array_list(&nalu_data_vec);
```

```rust
// crates/wasm/src/boxes_hev1.rs:191
    nalu_array_count: nalu_types_vec.len() as u32,
```

```rust
// crates/wasm/src/boxes_hev1.rs:219-223 (解放側)
crate::boxes::free_array_list(
    entry.nalu_data as *mut *mut u8,
    entry.nalu_sizes as *mut u32,
    entry.nalu_array_count,
);
```

`allocate_and_copy_array_list` は全 NALU を平坦化したリスト（`nalu_data_vec`）を受け取るため、確保される `nalu_data` / `nalu_sizes` の要素数は **全 NALU 総数** である。しかし保存する `nalu_array_count` は **NALU 配列の個数**（`nalu_types_vec.len()`）であり、解放側はこちらを `free_array_list` に渡している。

両者が食い違う方向は 2 つあり、いずれも UB になる:

- **総数 > 配列数**: 余剰の NALU バッファが未解放のまま残り、さらにポインタ配列・サイズ配列を確保時と異なる layout で `dealloc` する
- **総数 < 配列数**: `free_array_list` が確保していない領域まで読み出し、読み取ったゴミ値をポインタとして `mp4_free` に渡す

偶然一致するのは「`nalu_counts` の総和が `nalu_array_count` と等しいとき」だけである。「各配列に 1 NALU ずつ」はその一例にすぎない。たとえば `[{"naluType": 32, "units": [[1, 2], [3, 4]]}, {"naluType": 33, "units": []}]` は 1 配列に複数 NALU があり空の配列も含むが、総数 2 = 配列数 2 で一致するため発火しない。

既存テスト `test_json_to_hev1` / `test_json_to_hvc1` は 3 配列 × 各 1 NALU（総数 3 = 配列数 3）であり、この不一致を踏まない。実際に発火する入力は次のとおり:

- 総数 > 配列数: `[{"naluType": 32, "units": [[1, 2], [3, 4]]}]`（総数 2、配列数 1）
- 総数 < 配列数: `[{"naluType": 32, "units": [[1, 2]]}, {"naluType": 33, "units": []}]`（総数 1、配列数 2）

`crates/wasm/src/boxes_hvc1.rs:137` / `:191` / `:219-223` も同型。

## 設計方針

前提として、次の 2 つは変更しない。

- `nalu_array_count` の意味は「NALU 配列の個数」であり、これは公開 C API の契約である（`crates/c-api/include/mp4.h:463` のループ例、`crates/c-api/src/boxes.rs:922` の `to_sample_entry`、`crates/wasm/src/boxes_hev1.rs:65` の JSON 出力がいずれもこの意味に依存する）
- `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` に全 NALU 総数を保持するフィールドを追加しない。これらは `crates/c-api/src/boxes.rs` の `#[repr(C)]` 構造体で、`crates/c-api/build.rs` の cbindgen が `crates/c-api/include/mp4.h` を生成しているため、フィールド追加は公開 C ABI の破壊的変更になる。そもそも総数は既存フィールドから導出できるため不要である

そのうえで次の方針で修正する。

- 問題 1: `mp4_free` に確保時の実バイト数を渡す。解放関数が受け取るのは `entry` だけなので、バイト数は `entry.nalu_array_count` から導く。`nalu_types` は `entry.nalu_array_count` バイト、`nalu_counts` は `entry.nalu_array_count * size_of::<u32>()` バイト
- 問題 2: 全 NALU 総数を `nalu_counts[0..nalu_array_count]` の総和として求め、それを `free_array_list` の count に渡す。この総和の求め方は `crates/c-api/src/boxes.rs:982-993` の `nalu_data_index` が既に採っているものと同じ
- 解放順序の制約: 現行実装は `nalu_counts` を `free_array_list` より先に解放している。総和は `nalu_counts` の解放より前に算出しなければ use-after-free になる
- null の扱い: `allocate_and_copy_bytes` は空データのときと `mp4_alloc` が失敗したときに `(null, 0)` を返す（`crates/wasm/src/boxes.rs:235-237` / `:242-244`）。前者では `"naluArrays": []` で `nalu_array_count == 0` かつ各ポインタが null になり、後者では `nalu_array_count > 0` なのにポインタが null という状態が起こり得る。したがって総和の算出は既存の `if !entry.nalu_counts.is_null()` 検査の内側に置き、null のときは総数 0 として扱う。`nalu_types` / `nalu_counts` / `nalu_data` の既存の null 検査はいずれも維持し、無検査のデリファレンスを新たに持ち込まない（この方針は `issues/0011-bug-capi-hev1-hvc1-null-check.md` が c-api 側で進めている方向とも揃う）

変更は `crates/wasm/src/boxes_hev1.rs` / `crates/wasm/src/boxes_hvc1.rs` の解放関数に閉じる。`crates/c-api/src/boxes.rs` の構造体定義と `crates/c-api/include/mp4.h` は変更しない。

なお本修正は、`entry.nalu_counts` から `u32` を読む処理を解放関数に新設する。この領域は `crates/wasm/src/boxes_hev1.rs:129` の `allocate_and_copy_bytes`（align 1）で確保され `:193` で `*const u32` にキャストされたものなので、align 1 の領域を `u32` として読むアラインメント UB が 1 箇所増えることになる（`boxes_hvc1.rs:129` / `:193` も同型）。これは `issues/0048-refactor-wasm-alloc-alignment.md` が扱う UB クラスと同種だが、0048 の解決方法は `crates/wasm/src/boxes.rs` の `allocate_and_copy_*` / `free_*` に閉じており、`boxes_hev1.rs` / `boxes_hvc1.rs` 側の `nalu_counts` の確保・キャストには触れない。本 issue はメモリ解放の不一致の修正に集中し、アラインメントは 0048 側で `nalu_counts` の確保・キャストも対象に含めて解消する。

## 完了条件

不一致が実行時に必ず観測されるわけではないため、コード上の不変条件で完了を判定する。観測可能性と検出ツールの状況は次のとおり:

- 問題 1 のリークと、問題 2 の「総数 > 配列数」方向の layout 不一致 `dealloc` は、ホストのアロケータが黙って受理するため観測されない
- 問題 2 の「総数 < 配列数」方向は不正なポインタを `mp4_free` に渡すためアロケータが abort しうるが、読み取ったゴミ値が null なら `free_array_list` の null 検査で素通りするため、必ず観測されるとは限らない
- fuzz は `crates/wasm` を対象にしていない。`fuzz` はルート `Cargo.toml:21` の `exclude = ["fuzz"]` でワークスペース外に置かれ、`fuzz/Cargo.toml` のワークスペース内依存は `shiguredo_mp4` だけである
- `cargo +nightly miri test -p wasm` はリポジトリ側の追加設定なしで実行できるが、wasm クレートには align 1 由来のアラインメント UB が `crates/wasm/src/boxes.rs` の `free_array_list` や `crates/wasm/src/boxes_tx3g.rs` など複数箇所に残っており、テストは並列実行されるためどれが先に報告されるかも定まらない。本 issue の修正だけでは miri をグリーンにできない

判定する不変条件:

- `mp4_free` に渡すバイト数が確保サイズと一致すること（`nalu_types` は `entry.nalu_array_count` バイト、`nalu_counts` は `entry.nalu_array_count * size_of::<u32>()` バイト）
- `free_array_list` に渡す count が、確保時に `allocate_and_copy_array_list` が返した count（= `nalu_counts` の総和）と一致すること
- 総和の算出が `nalu_counts` の解放より前、かつ `entry.nalu_counts` の null 検査の内側で行われること
- 既存の `nalu_types` / `nalu_counts` / `nalu_data` の null 検査が維持されていること

あわせて、現状セクションで挙げた「総数 > 配列数」「総数 < 配列数」の入力について parse → free を通すテストを HEV1 / HVC1 の両方に追加し、`cargo test --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` と `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz -- -D warnings` が通ること。ただしこのテストは回帰の網であり、上記のとおり不一致が必ず観測されるわけではないため、pass したことをもって修正できたとは判定しない（fail した場合は回帰とみなしてよい）。

## 解決方法

`crates/wasm/src/boxes_hev1.rs` の `mp4_sample_entry_hev1_free` と `crates/wasm/src/boxes_hvc1.rs` の `mp4_sample_entry_hvc1_free` を書き換える。既存の 3 つの null 検査ブロック（`nalu_types` / `nalu_counts` / `nalu_data`）の構造と順序はそのまま残し、各ブロックの中身だけを直す。

1. 全 NALU 総数を保持するローカル変数を関数の先頭で 0 に初期化する
2. `nalu_types` の既存ブロック: 解放サイズを 0 から `entry.nalu_array_count` バイトに変える
3. `nalu_counts` の既存ブロック: 解放する前に `nalu_counts[0..entry.nalu_array_count]` を総和して手順 1 の変数へ入れ、そのあと `entry.nalu_array_count * size_of::<u32>()` バイトで `mp4_free` する
4. `nalu_data` の既存ブロック: `free_array_list` に渡す count を `entry.nalu_array_count` から手順 3 で求めた総数に変える
5. 末尾の `entry.nalu_array_count = 0;` と各ポインタのクリアは現行のまま残す

`nalu_counts` が null の場合は手順 3 のブロックに入らないため、総数は 0 のままとなり、手順 4 の `free_array_list` は即座に return する。

テストは既存の `test_json_to_hev1` / `test_json_to_hvc1` と同じ `#[cfg(test)] mod tests` に、現状セクションで挙げた「総数 > 配列数」「総数 < 配列数」の入力例を使って追加する。

実装後のレビューで次の防御的な改善を追加した:

- 総和とバイト数の計算を `checked_add(...).expect(...)` / `checked_mul(...).expect(...)` にし、想定外の overflow を即検出できるようにする
- hvc1 側の `nalu_counts` のキャストを hev1 側 (`.cast_mut() as *mut u8`) に揃える
- 既存 `test_json_to_hev1` / `test_json_to_hvc1` に `nalu_data` / `nalu_sizes` の null 検査を追加して新規テストと対称化する
- 空 `naluArrays`（`nalu_array_count == 0`）の境界値テストを HEV1 / HVC1 に追加する

後日別 issue として起票する検討事項:

- hev1 / hvc1 の free 関数とテスト JSON の重複共通化（refactor）
- issue 0048 の scope 拡張: `nalu_counts` の align 4 化（本 PR で align 1 → `u32` 読みの UB が 1 箇所増えるため）
- `allocate_and_copy_array_list` の部分割当失敗時に `nalu_data` / `nalu_sizes` が非対称 null になる既存構造
- `parse_json_mp4_sample_entry_hev1` / `_hvc1` の中途エラーで確保済み領域が leak する既存構造
