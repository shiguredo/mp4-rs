# sidx 付き media segment と mfra の moof_offset が不整合になる

- Priority: High
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-sidx-mfra-moof-offset
- Polished: YYYY-MM-DD

## 目的

`Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx()` が sidx を返却バイト列の先頭に載せる一方で `media_bytes_written` に sidx サイズを加算しないため、`mfra_bytes()` の `tfra.moof_offset` が実際のファイル上の moof 位置より sidx 分だけ前を指す問題を修正する。

## 優先度根拠

`examples/fmp4.rs:189,237` が `create_media_segment_metadata_with_sidx` と `mfra_bytes` の組み合わせを使用しており、実例で発火する。生成された MP4 の `tfra.moof_offset` が不正確になると、シーク・ランダムアクセスで moof の位置を外し、再生・解析ツールが誤動作する。High。

## 現状

```rust
// src/mux_fmp4_segment.rs:297-300 (create_media_segment_metadata_with_sidx)
let sidx_bytes = sidx_box.encode_to_vec()?;
let mut result = sidx_bytes;
result.extend_from_slice(&media_segment);
Ok(result)
```

```rust
// src/mux_fmp4_segment.rs:407-411 (build_media_segment_bytes 内)
self.media_bytes_written = self
    .media_bytes_written
    .checked_add(segment.len() as u64)
    .and_then(|written| written.checked_add(mdat_payload_size))
    .ok_or(MuxError::Overflow)?;
```

```rust
// src/mux_fmp4_segment.rs:454 (mfra_bytes)
    moof_offset: init_segment_size + e.moof_relative_offset,
```

`build_media_segment_bytes` は `moof_relative_offset = media_bytes_written` を記録し、`media_bytes_written` には moof + mdat のみを加算する。`create_media_segment_metadata_with_sidx` はその後 sidx を先頭に付けるが、`media_bytes_written` / `moof_relative_offset` には sidx サイズを加算しない。

実ファイル: `init + [sidx + moof + mdat] + ...`
mfra が指す位置: `init + [moof + mdat] 累積`（sidx 分ずれる）

PBT `mfra_bytes_roundtrip` は sidx なし経路のみ検証しているため未検出。

## 設計方針

sidx サイズを `media_bytes_written` に含める。`create_media_segment_metadata_with_sidx` で sidx エンコード後に `media_bytes_written` に sidx バイト数を `checked_add` する。これにより後続セグメントの `moof_relative_offset` も正しくなる。

代替案として「sidx 使用時の mfra 併用を明示的に拒否する」もあるが、`examples/fmp4.rs` が併用しているため整合させる方を採用する。

## 完了条件

- sidx 付き media segment と mfra を併用したとき、`tfra.moof_offset` が実際の moof 位置と一致すること
- sidx なし経路の従来挙動が変わらないこと
- `mfra_bytes_roundtrip` に sidx 付きのケースを追加し、オフセットが正しいことを検証すること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `create_media_segment_metadata_with_sidx` で sidx エンコード後に `media_bytes_written` に sidx バイト数を `checked_add` する
2. `mfra_bytes_roundtrip` に sidx 付きのテストケースを追加する
