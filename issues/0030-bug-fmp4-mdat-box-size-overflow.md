# Fmp4SegmentMuxer の mdat サイズ計算と mfra の moof_offset 計算でオーバーフローチェックが欠落している

- Priority: Medium
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/fix-fmp4-mdat-box-size-overflow
- Polished: 2026-07-20

## 目的

`Fmp4SegmentMuxer` 内の未チェック `u64` 加算を、既存の `checked_add` → `MuxError::Overflow` パターンに揃える。対象は次の 2 系統で、どちらも debug ビルドでは panic、release ビルドではラップアラウンドによる不正値になる。

1. **mdat ボックスサイズ**（`build_media_segment_bytes`）: `mdat_box_size_value` と `extended_box_size`
2. **mfra の `moof_offset`**（`mfra_bytes`）: `init_segment_size + e.moof_relative_offset`（version 判定と `TfraEntry` 組み立ての 2 箇所）

同一ファイル・同一エラー種・同一修正パターンのため、別 issue に分けず本 issue でまとめて直す。

## 優先度根拠

現実的に `u64::MAX` 近傍のサイズ・オフセットは発生しないが、debug の panic と release の silent corruption を防ぐ防御的修正である。同ファイルの `media_segment_size` や `media_bytes_written` 更新では既に `checked_add` を使っており、対象箇所だけが一貫していない。

## 現状

### 1. mdat ボックスサイズ（`build_media_segment_bytes`）

```rust
let mdat_box_size_value = BoxHeader::MIN_SIZE as u64 + mdat_payload_size;
let (mdat_box_size, mdat_header_size) = if mdat_box_size_value <= u32::MAX as u64 {
    // ...
} else {
    let extended_box_size = 16u64 + mdat_payload_size;
    // ...
};
```

`BoxHeader::MIN_SIZE` は 8（`basic_types.rs`）。`mdat_payload_size` は各トラックの `payload_end` の最大値。

#### mdat オーバーフローの到達経路

1. `mux::Sample.data_offset`（ユーザー入力、`u64`。`mux_mp4_file.rs` の `Sample`）
2. `resolve_segment_tracks` 内で `data_offset.checked_add(data_size as u64)` → 成功すれば `u64::MAX` まで到達可能
3. その結果が `payload_end` になり、`mdat_payload_size` の最大値に使われる
4. `BoxHeader::MIN_SIZE as u64 + mdat_payload_size` または `16u64 + mdat_payload_size` でオーバーフロー

根本原因はユーザー入力の `data_offset` が未検証であること。`payload_end` 自体は `checked_add` で計算されるため `u64::MAX` に到達し得る。

#### mdat 境界値

| `mdat_payload_size` | `8 + payload` | 分岐 | `16 + payload` | 結果 |
|---------------------|---------------|------|----------------|------|
| `u32::MAX - 8` | `u32::MAX` | U32 | - | 正常（U32 上限） |
| `u32::MAX - 7` | `u32::MAX + 1` | U64 | `u32::MAX + 9` | 正常（U64 移行） |
| `u64::MAX - 16` | `u64::MAX - 8` | U64 | `u64::MAX` | 正常（U64 上限） |
| `u64::MAX - 15` | `u64::MAX - 7` | U64 | `u64::MAX + 1` | **オーバーフロー（`extended_box_size`）** |
| `u64::MAX - 8` | `u64::MAX` | U64 | `u64::MAX + 8` | **オーバーフロー（`extended_box_size`）** |
| `u64::MAX - 7` | `u64::MAX + 1` | - | - | **オーバーフロー（`mdat_box_size_value`）** |
| `u64::MAX` | `u64::MAX + 8` | - | - | **オーバーフロー（`mdat_box_size_value`。release: 7 にラップ → `BoxSize::U32(7)` の不正サイズ）** |

### 2. mfra の `moof_offset`（`mfra_bytes`）

```rust
let needs_v1 = entries.iter().any(|e| {
    let moof_offset = init_segment_size + e.moof_relative_offset;
    e.time > u32::MAX as u64 || moof_offset > u32::MAX as u64
});
// ...
moof_offset: init_segment_size + e.moof_relative_offset,
```

`init_segment_size` は `init_segment_bytes()` の長さ、`moof_relative_offset` は各メディアセグメント生成時の `media_bytes_written`（sidx 経路ではさらに sidx サイズを加算）。どちらも大きくなり得るため、和が `u64` を超え得る。

`needs_v1` は `.any()` クロージャ内のため、`?` をそのまま使えず、明示ループなどへ組み替える必要がある。

## 設計方針

対象の未チェック加算をすべて `checked_add` に置き換え、オーバーフロー時に `MuxError::Overflow` を返す。`MuxError::Overflow` は同ファイルの他箇所（`media_segment_size`、`media_bytes_written` 更新、`decode_time` 更新など）で使われている既存バリアント。

- mdat: `mdat_box_size_value` と `extended_box_size` の 2 箇所
- mfra: version 判定用と `TfraEntry.moof_offset` 用の 2 箇所（計算は共通化してよい）

本 issue の対象は上記 4 箇所（実質 2 系統）に限定する。同ファイルの他の加算は既に `checked_add` 済み、または別の意味を持つため触らない。

## 完了条件

- `build_media_segment_bytes` の `mdat_box_size_value` / `extended_box_size` が `checked_add` になり、オーバーフロー時に `MuxError::Overflow` が返ること
- `mfra_bytes` の `init_segment_size + e.moof_relative_offset`（version 判定・`TfraEntry` 組み立て）が `checked_add` になり、オーバーフロー時に `MuxError::Overflow` が返ること
- mdat オーバーフローをトリガーするテストが追加されること（例: `data_size = 1, data_offset = u64::MAX - 1` として `payload_end = u64::MAX` を作り、`create_media_segment_metadata()` が `Err(MuxError::Overflow)` を返すこと。サンプルには `sample_entry: Some(...)` が必要。未設定だと `MissingSampleEntry` で早期リターンしオーバーフロー地点に到達しない）
- mfra オーバーフローをトリガーするテストが追加されること（例: 巨大な `mdat_payload_size` で第 1 セグメントを通し `media_bytes_written` を `u64::MAX` 近傍まで進め、続く小さな第 2 セグメントで大きな `moof_relative_offset` を確定させたうえで `mfra_bytes()` が `Err(MuxError::Overflow)` を返すこと。第 2 セグメントの合計サイズが残りの `u64` 余裕に収まり、かつ `init_segment_size` がその余裕より大きい必要がある）
- 既存のテストが通ること
- `cargo clippy` が通ること

## 解決方法

1. `BoxHeader::MIN_SIZE as u64 + mdat_payload_size` を `(BoxHeader::MIN_SIZE as u64).checked_add(mdat_payload_size).ok_or(MuxError::Overflow)?` に、`16u64 + mdat_payload_size` を同様に `checked_add` に置き換える。
2. `mfra_bytes` で `init_segment_size.checked_add(e.moof_relative_offset).ok_or(MuxError::Overflow)?` を使い、version 判定は `.any()` から失敗を伝播できるループ（または同等）へ変更する。
3. テストは `tests/test_mux_fmp4_segment.rs` を新規作成して追加する（同モジュール向けの既存単体テストファイルは無い）。mdat / mfra それぞれのオーバーフローケースを入れる。

## 後方互換

本修正は debug ビルドの panic と release ビルドの silent corruption を `Err` に置き換えるものであり、正当な入力に対する挙動は不変。API シグネチャの変更もない（既に `Result` を返している）。

## CHANGES.md

`[FIX]` で記載する。mdat サイズ計算と `mfra` の `moof_offset` 計算の両方を、1 エントリにまとめて書いてよい。CHANGES.md の develop セクションの既存エントリ（`Mp4FileMuxer` の防御的修正）と同方向。
