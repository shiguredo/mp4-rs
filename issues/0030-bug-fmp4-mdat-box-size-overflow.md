# mux_fmp4_segment.rs の mdat ボックスサイズ計算でオーバーフローチェックが欠落している

- Priority: Medium
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/fix-fmp4-mdat-box-size-overflow
- Polished: 2026-07-20

## 目的

`Fmp4SegmentMuxer` のメディアセグメント生成で、`mdat_box_size_value` と `extended_box_size` の計算にオーバーフローチェックがない。`mdat_payload_size` が `u64::MAX - 7` より大きい場合、debug ビルドでは panic、release ビルドではラップアラウンドにより不正なボックスサイズが生成される。

### オーバーフローの到達経路

1. `mux::Sample.data_offset`（ユーザー入力、`u64`。`mux_mp4_file.rs:179` 定義の `Sample` 構造体）
2. `data_offset.checked_add(data_size as u64)` → 成功すれば `u64::MAX` まで到達可能（`mux_fmp4_segment.rs:879-884`）
3. `payload_end = expected_next_data_offset`（885 行目）
4. `resolved_tracks.iter().map(|track| track.payload_end).max()`（319-322 行目）
5. `BoxHeader::MIN_SIZE as u64 + mdat_payload_size`（324 行目）← ここでオーバーフロー

根本原因はユーザー入力の `data_offset` が未検証であること。`payload_end` 自体は `checked_add` で計算されるため `u64::MAX` に到達し得る。

## 優先度根拠

現実的に `u64::MAX` に近いペイロードサイズは発生しないが、debug ビルドでの panic と release ビルドでの silent corruption を防ぐ防御的プログラミングの観点から修正すべき。同ファイル内の `media_segment_size` 計算（261-264 行目）では `checked_add` を使っており一貫性がない。

## 現状

`src/mux_fmp4_segment.rs:324-333`:

```rust
let mdat_box_size_value = BoxHeader::MIN_SIZE as u64 + mdat_payload_size;
let (mdat_box_size, mdat_header_size) = if mdat_box_size_value <= u32::MAX as u64 {
    // ...
} else {
    let extended_box_size = 16u64 + mdat_payload_size;
    // ...
};
```

`BoxHeader::MIN_SIZE` は 8（`basic_types.rs:104`）。`payload_end` は `checked_add` で計算されているため `u64::MAX` になり得る。その場合 `8 + u64::MAX` はオーバーフローする。

### 境界値

| `mdat_payload_size` | 324 行目 `8 + payload` | 分岐 | 332 行目 `16 + payload` | 結果 |
|---------------------|----------------------|------|------------------------|------|
| `u32::MAX - 8` | `u32::MAX` | U32 | - | 正常（U32 上限） |
| `u32::MAX - 7` | `u32::MAX + 1` | U64 | `u32::MAX + 9` | 正常（U64 移行） |
| `u64::MAX - 16` | `u64::MAX - 8` | U64 | `u64::MAX` | 正常（U64 上限） |
| `u64::MAX - 15` | `u64::MAX - 7` | U64 | `u64::MAX + 1` | **オーバーフロー（332 行目）** |
| `u64::MAX - 8` | `u64::MAX` | U64 | `u64::MAX + 8` | **オーバーフロー（332 行目）** |
| `u64::MAX - 7` | `u64::MAX + 1` | - | - | **オーバーフロー（324 行目）** |
| `u64::MAX` | `u64::MAX + 8` | - | - | **オーバーフロー（324 行目。release: 7 にラップ → `BoxSize::U32(7)` の不正サイズ）** |

## 設計方針

324 行目と 332 行目の両方を `checked_add` に置き換え、オーバーフロー時に `MuxError::Overflow` を返す。`MuxError::Overflow` は同ファイルの他箇所（263-264 行目、362 行目等）で使用されている既存のバリアント。

本 issue は `build_media_segment_bytes` 内の mdat サイズ計算に限定する。`mfra_bytes()` 内（445 行目・454 行目）の `init_segment_size + e.moof_relative_offset` にも同種の unchecked addition が存在するが、これは別 issue として扱う。

## 完了条件

- 324 行目・332 行目が `checked_add` に置き換えられ、オーバーフロー時に `MuxError::Overflow` が返ること
- オーバーフローをトリガーするテストが追加されること（例: `data_size = 1, data_offset = u64::MAX - 1` として `payload_end = u64::MAX` を作り、`create_media_segment_metadata()` が `Err(MuxError::Overflow)` を返すことの検証。テストサンプルには `sample_entry: Some(...)` の設定が必要。未設定の場合 `MissingSampleEntry` で早期リターンしオーバーフロー地点に到達しない）
- 既存のテストが通ること
- `cargo clippy` が通ること

## 解決方法

`BoxHeader::MIN_SIZE as u64 + mdat_payload_size` を `(BoxHeader::MIN_SIZE as u64).checked_add(mdat_payload_size).ok_or(MuxError::Overflow)?` に、`16u64 + mdat_payload_size` を同様に `checked_add` に置き換える。

テストは `tests/test_mux_fmp4_segment.rs` を新規作成して追加する（同モジュールには既存の単体テストが存在しない）。

## 後方互換

本修正は debug ビルドの panic と release ビルドの silent corruption を `Err` に置き換えるものであり、正当な入力に対する挙動は不変。API シグネチャの変更もない（既に `Result` を返している）。

## CHANGES.md

`[FIX]` で記載する。CHANGES.md の develop セクションの既存エントリ（`Mp4FileMuxer` の防御的修正）と同方向。
