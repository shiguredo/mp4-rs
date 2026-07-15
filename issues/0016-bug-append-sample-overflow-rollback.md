# mux_mp4_file.rs の append_sample が Overflow 時に chunks をロールバックせず状態が残留する

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-append-sample-overflow-rollback
- Polished: YYYY-MM-DD

## 目的

`Mp4FileMuxer::append_sample` が `MuxError::Overflow` 返却時に `audio_chunks` / `video_chunks` の変更をロールバックせず、ドキュメント契約に違反してエラー後にリトライするとサンプルが二重登録される問題を修正する。

## 優先度根拠

ドキュメントは「エラー時は `next_position` / `audio_chunks` / `video_chunks` / `last_sample_kind` を変更しない」と明記しており、実装と契約が不一致。エラー後に同じ `data_offset` で再呼び出しすると chunks にサンプルが二重に push され、壊れた MP4 が生成される。`next_position` が `u64::MAX` 付近で到達可能。

## 現状

```rust
// src/mux_mp4_file.rs:601-613
        if is_new_chunk_needed {
            // ...
            chunks.push(Chunk {
                offset: sample.data_offset,
                sample_entry,
                samples: Vec::new(),
            });
        }

        chunks.last_mut().expect("bug").samples.push(metadata);

        self.next_position = self
            .next_position
            .checked_add(sample.data_size as u64)
            .ok_or(MuxError::Overflow)?;
        self.last_sample_kind = Some(sample.track_kind);
```

`chunks.push` / `samples.push` の **後に** `next_position` の `checked_add` で `Overflow` が発生し得る。Overflow 時は `?` で return するため `next_position` / `last_sample_kind` は未更新だが、`audio_chunks` / `video_chunks` は既に変更済みでロールバックされない。

ドキュメント（`src/mux_mp4_file.rs:535-536` 付近）はエラー時に chunks も不変と主張しているが、Overflow 経路では契約が破れる。

## 設計方針

`next_position` の `checked_add` を chunks の変更 **前に** 行い、成功確定後にだけ chunks を更新する。`Fmp4SegmentMuxer` の clone-then-commit パターンと同型にする。

## 完了条件

- `Overflow` エラー時に `audio_chunks` / `video_chunks` / `next_position` / `last_sample_kind` がすべて変更されないこと
- エラー後に同じ `data_offset` で再呼び出しすると正常に登録されること（二重登録なし）
- 既存の `test_append_sample_error_keeps_muxer_state` が通ること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `append_sample` で `next_position` の `checked_add` を chunks 更新前に移動する
2. Overflow 時は chunks に触れずに `Err` を返す
3. 既存の `test_append_sample_error_keeps_muxer_state` で Overflow 経路も検証するよう拡充する
