# boxes_sample_entry.rs の avcC PPS 上限が 31 になっており仕様と取り違えている（正しくは 255）

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-30
- Model: opencode-go glm-5.2
- Branch: feature/fix-avcc-pps-max-count
- Polished: 2026-07-30

## 目的

`AvccBox::encode` で PPS の上限を 31 にしているが、ISO/IEC 14496-15 では `numOfPictureParameterSets` は `unsigned int(8)`（最大 255）であり、SPS の `unsigned int(5)`（最大 31）と取り違えている問題を修正する。

## 優先度根拠

32〜255 個の合法 PPS を持つ MP4 のエンコードを拒否する。decode 側は PPS を `u8` で読んでおり非対称。仕様違反の MP4 を生成するわけではないが、合法入力を誤って弾くため修正が必要。

## 現状

`src/boxes_sample_entry.rs` の `impl Encode for AvccBox` において:

```rust
// SPS — 正しい（最大 31）
if self.sps_list.len() > 31 {
    return Err(Error::invalid_input("Too many SPSs (max 31)"));
}
```

```rust
// PPS — 誤り（仕様上の最大は 255）
if self.pps_list.len() > 31 {
    return Err(Error::invalid_input("Too many PPSs (max 31)"));
}
```

```rust
let pps_count = self.pps_list.len() as u8;
offset += pps_count.encode(&mut buf[offset..])?;
```

ISO/IEC 14496-15 `AVCDecoderConfigurationRecord`:
- `numOfSequenceParameterSets`: `unsigned int(5)` → 最大 31（SPS 側は正しい）
- `numOfPictureParameterSets`: `unsigned int(8)` → 最大 255

PPS は `u8` 全体で書き出している（`pps_count.encode()`）ため、フィールド幅は 8 bit だが、上限チェックだけ 31 になっている。decode 側（`impl Decode for AvccBox` の `pps_count`）は PPS を `u8` で読むため 0〜255 を受理する。

`CHANGES.md` の過去エントリにも「SPS と PPS は各最大 31」と誤記がある。

## 設計方針

PPS の上限を 31 から 255 に変更する。`u8::try_from` で 256 以上をエラーにする。エラーメッセージも `max 255` に修正する。

## 完了条件

- PPS が 32〜255 個の MP4 をエンコードできること
- PPS が 256 個以上でエラーを返すこと
- SPS の上限 31 は変更しないこと
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `impl Encode for AvccBox` 内の `pps_list.len()` 上限チェックを、`u8::try_from(self.pps_list.len()).map_err(|_| Error::invalid_input("Too many PPSs (max 255)"))?` に変更する（または同等の `> 255` チェック + メッセージ `max 255`）
2. `pbt/tests/prop_codec_boxes.rs` に PPS が 32 個のエンコード成功テストを追加する（回帰の本命。現状は `> 31` で 32 個以上を拒否するため）
3. 既存の `avcc_error_tests::avcc_box_too_many_pps`（256 個でエラー）が修正後も通ることを確認する（新規追加は不要）
