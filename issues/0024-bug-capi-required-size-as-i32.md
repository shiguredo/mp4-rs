# c-api の required_input_size が usize as i32 で切り捨てられ -1（EOF）と衝突する

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-capi-required-size-as-i32
- Polished: YYYY-MM-DD

## 目的

C API の `mp4_file_demuxer_get_required_input` / `mp4_file_kind_detector_get_required_input` で `required.size`（`Option<usize>`）を `as i32` で変換しており、`> i32::MAX` で負値になり `-1`（EOF までの意味）と衝突する問題を修正する。

## 優先度根拠

巨大 moov 等で要求サイズが `i32::MAX`（約 2 GiB）を超えると負値になり、API 上の `-1`（末尾まで必要）と誤認される。呼び出し側が不足バッファを供給して不正デコード・状態破壊に至る。`usize as i32` は黙って下位ビット切り捨てするため暗黙。

## 現状

```rust
// crates/c-api/src/demux.rs:388
*out_required_input_size = required.size.map(|n| n as i32).unwrap_or(-1);
```

```rust
// crates/c-api/src/mp4_file_kind_detector.rs:116
*out_required_input_size = required.size.map(|n| n as i32).unwrap_or(-1);
```

`RequiredInput.size` は `Option<usize>`。`n as i32` は `n > i32::MAX` で下位ビットを符号付き再解釈する。例: `usize = 0xFFFF_FFFF` → `i32 = -1`（EOF と衝突）。`> i32::MAX` のすべてが負値や過小な正値になり得る。

API 仕様上 `-1` は「末尾まで必要」（`mp4.h:1229`）。`0` は「入力不要」。

## 設計方針

`i32::try_from(n)` で変換し、失敗時はエラー状態にするか飽和させる。巨大 moov を実用的に扱うには 64-bit の out 引数にする必要があるが、後方互換のため `i32` のまま `try_from` で防御する。

## 完了条件

- `required.size > i32::MAX` で `-1` と衝突しないこと
- 巨大サイズ要求時に呼び出し側が誤認しないこと
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `as i32` を `i32::try_from(n).map_err(|_| ...)` に変更し、変換失敗時はエラー状態または `MP4_ERROR_OTHER` を返す
2. または飽和させ、`i32::MAX` にクランプして `-1` と区別できるようにする
3. 巨大 moov のテストを追加する（可能なら）
