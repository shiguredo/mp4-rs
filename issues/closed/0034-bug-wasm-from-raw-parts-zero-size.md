# wasm の JSON フォーマット経路で from_raw_parts にサイズ 0 時の null ポインタを渡し UB になる

- Priority: Medium
- Created: 2026-07-20
- Completed: 2026-07-31
- Model: qwen3.8-max-preview
- Branch: feature/fix-wasm-from-raw-parts-zero-size
- Polished: 2026-07-31

## 目的

WASM の JSON フォーマット処理で `std::slice::from_raw_parts(ptr, 0)` を呼び出している箇所がある。`ptr` が null の場合、Rust の仕様上これは UB（未定義動作）である（「even for zero-length slices, the pointer must be non-null and properly aligned」）。`allocate_and_copy_bytes` は空データに対して `(null, 0)` を返すため、JSON → 構造体 → JSON の往復でこの経路が通り得る。

## 優先度根拠

UB であるが、wasm32 ターゲットでは実際にはクラッシュしない可能性が高い。ただし Rust の仕様上は UB であり修正すべき。closed issue 0048（アラインメント UB）と同水準として扱う。

## 現状

`allocate_and_copy_bytes`（`crates/wasm/src/boxes.rs`）は空データに `(null, 0)` を返す:

```rust
if data.is_empty() {
    return (std::ptr::null(), 0);
}
```

確保失敗時も `(null, 0)` を返す。いずれの場合も呼び出し側がサイズガードなしで `from_raw_parts` すると UB になる。

加えて `allocate_and_copy_array_list`（同ファイル）は、各要素ポインタに `allocate_and_copy_bytes(array).0` だけを使い、サイズ配列は `array.len()` から別途作る。そのため非空要素の確保失敗では **ポインタ null・サイズ非ゼロ** が並び得る。NALU 系の fmt はサイズだけ見ていても null を避けられない。

対比として、文字列用の `raw_bytes_as_str`（同ファイル）は既に `size == 0 || data.is_null()` のとき `""` を返すガードを持っており、`boxes_stpp` / `boxes_wvtt` の fmt 経路はこの helper 経由で安全である。

以下の呼び出しが、空データ（`(null, 0)`）や NALU 要素の確保失敗（`(null, 非ゼロ)`）で null ポインタを渡し得る:

**1. `crates/wasm/src/boxes_av01.rs` の `fmt_json_mp4_sample_entry_av01`**:

```rust
let config_obus =
    unsafe { std::slice::from_raw_parts(data.config_obus, data.config_obus_size as usize) };
```

**2. `crates/wasm/src/boxes_mp4a.rs` の `fmt_json_mp4_sample_entry_mp4a`**:

```rust
let dec_specific_info = unsafe {
    std::slice::from_raw_parts(data.dec_specific_info, data.dec_specific_info_size as usize)
};
```

**3. `crates/wasm/src/boxes_flac.rs` の `fmt_json_mp4_sample_entry_flac`**:

```rust
let streaminfo = unsafe {
    std::slice::from_raw_parts(data.streaminfo_data, data.streaminfo_size as usize)
};
```

**4. `crates/wasm/src/boxes_avc1.rs` の `NaluList`（`DisplayJson::fmt` 内）**:

```rust
let nalu = unsafe { std::slice::from_raw_parts(nalu_ptr, nalu_size) };
```

空の SPS/PPS 要素は `allocate_and_copy_array_list` 経由で要素ごとに `allocate_and_copy_bytes` されるため、要素ポインタが null・サイズ 0 になり得る。

**5. `crates/wasm/src/boxes.rs` の `HevcNaluArrays`（`DisplayJson::fmt` 内）**:

```rust
unsafe { std::slice::from_raw_parts(nalu_ptr, nalu_size) };
```

`boxes_hev1.rs` の `fmt_json_mp4_sample_entry_hev1` と `boxes_hvc1.rs` の `fmt_json_mp4_sample_entry_hvc1` は、いずれもこの共通構造体に委譲するだけである（closed issue 0056 で `NaluArrays` を `HevcNaluArrays` として `boxes.rs` に集約済み）。hev1 / hvc1 側モジュール自体に `from_raw_parts` は無い。

## 設計方針

`raw_bytes_as_str` と同型の「サイズ 0 **または** null なら空スライス」ガードを入れる。サイズ 0 だけでは、`allocate_and_copy_array_list` 経由の NALU 要素確保失敗（`(null, 非ゼロ)`）を防げない。

- av01 / mp4a / flac: 各 `fmt_json_mp4_sample_entry_*` で `if size == 0 || ptr.is_null() { &[] } else { unsafe { from_raw_parts(...) } }`
- avc1: `NaluList::fmt` 内の各 NALU 単位で同様のガード
- hev1 / hvc1: `HevcNaluArrays::fmt`（`boxes.rs`）内の各 NALU 単位で同様のガード。hev1 / hvc1 モジュール自体は変更不要（共通実装を直せば両方に効く）

## 完了条件

- 次の 5 箇所すべての `from_raw_parts` 呼び出しで、サイズ 0 またはポインタ null のときに null ポインタを渡さない分岐が追加されること
  - `boxes_av01.rs` の `fmt_json_mp4_sample_entry_av01`
  - `boxes_mp4a.rs` の `fmt_json_mp4_sample_entry_mp4a`
  - `boxes_flac.rs` の `fmt_json_mp4_sample_entry_flac`
  - `boxes_avc1.rs` の `NaluList::fmt`
  - `boxes.rs` の `HevcNaluArrays::fmt`（hev1 / hvc1 の両方の JSON 出力経路をカバー）
- 空データ（サイズ 0）の JSON 往復テストが追加されること（例: `configObus: []` の JSON を parse して再度 JSON に変換するテスト。NALU 要素が空のケースも含める）
- 既存のテストが通ること
- `cargo clippy` が通ること

## 解決方法

### 実装

JSON フォーマット経路の次の箇所で、`size == 0 || ptr.is_null()` のとき `&[]` を返し、それ以外だけ `from_raw_parts` するガードを入れた。

- `crates/wasm/src/boxes_av01.rs` の `fmt_json_mp4_sample_entry_av01`
- `crates/wasm/src/boxes_mp4a.rs` の `fmt_json_mp4_sample_entry_mp4a`
- `crates/wasm/src/boxes_flac.rs` の `fmt_json_mp4_sample_entry_flac`
- `crates/wasm/src/boxes_avc1.rs` の `NaluList::fmt`
- `crates/wasm/src/boxes.rs` の `HevcNaluArrays::fmt`（hev1 / hvc1 の両方をカバー）
- `crates/wasm/src/boxes_tx3g.rs` の `FtabList::fmt`（同種 UB のため本 issue で合わせて修正）

フォーマット側はパース時に格納済みのポインタ／サイズを読むだけで、確保失敗をエラーにはしない（UB 回避の防御）。

### テスト

空データの JSON 往復テストを追加した。

- `test_json_to_av01_empty_config_obus_roundtrip`
- `test_json_to_mp4a_empty_dec_specific_info_roundtrip`
- `test_json_to_flac_empty_streaminfo_roundtrip`
- `test_json_to_avc1_empty_nalu_element_roundtrip`
- `test_json_to_hev1_empty_nalu_element_roundtrip`
- `test_tx3g_json_roundtrip_with_empty_font_name`

### ドキュメント

- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加した

## 後方互換

UB の除去であり、正常系の実挙動は変わらない。空データの JSON 往復で UB が解消される。

## CHANGES.md

`[FIX]` で記載する。
