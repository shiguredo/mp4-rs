# mux_mp4_file.rs で sample.data_size の usize→u32 によるトランケーションが発生する

Created: 2026-05-20
Completed: 2026-05-20
Model: opencode mimo-v2.5-pro
Branch: feature/fix-mux-mp4-file-data-size-truncation

## 概要

`src/mux_mp4_file.rs:548` (`append_sample` 内の `SampleMetadata` 構築箇所) で
`sample.data_size as u32` のキャストが行われており、
`data_size` が `u32::MAX` を超える場合に上位ビットが切り捨てられ、
壊れた MP4 ファイルが生成される。

## 根拠

根本原因は `Sample.data_size` (usize, `src/mux_mp4_file.rs:255`) と
`SampleMetadata.size` (u32, `src/mux_mp4_file.rs:380`) の型不一致。

```rust
// src/mux_mp4_file.rs:548 (append_sample 内)
size: sample.data_size as u32,
```

比較対象の `src/mux_fmp4_segment.rs:744` では正しく `u32::try_from` を使用している:

```rust
size: Some(u32::try_from(sample.data_size).map_err(|_| {
    MuxError::EncodeError(Error::invalid_data(
        "sample data size exceeds u32::MAX",
    ))
})?),
```

## 同種キャストの安全性確認

同ファイル内の他の `as u32` / `as u64` キャストは以下のように安全:

- `src/mux_mp4_file.rs:604` — `sample.data_size as u64`: usize→u64 の拡大変換で安全 (32-bit プラットフォームでも u64 に収まる)
- `src/mux_mp4_file.rs:773, 778, 792` — `trak_boxes.len() as u32 + 1`: トラック数は最大 2 (audio + video) なので安全
- `src/mux_mp4_file.rs:980, 983, 1019` — `idx as u32` / `i as u32`: サンプルエントリ数やチャンクインデックスで、実用上 u32::MAX を超えない
- `src/mux_mp4_file.rs:1004` — `c.offset as u32`: `src/mux_mp4_file.rs:998` で `self.next_position > u32::MAX as u64` のガードがあり、StcoBox を使うパスでは u32 範囲内であることが保証されている

fmp4 側 (`src/mux_fmp4_segment.rs:126`) も `data_size: usize` のまま保持しているが、
744 行で `u32::try_from` により防御済み。

## 再現手順

`data_size` フィールドだけ大きな値に設定し、実際のデータは書かない形で再現可能。

```rust
// src/mux_mp4_file.rs のテストモジュール内の create_avc1_sample_entry() を使用
let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
let initial_size = muxer.initial_boxes_bytes().len() as u64;

let sample = Sample {
    track_kind: TrackKind::Video,
    sample_entry: Some(create_avc1_sample_entry()),
    keyframe: true,
    timescale: NonZeroU32::MIN.saturating_add(30 - 1),
    duration: 1,
    composition_time_offset: None,
    data_offset: initial_size,
    data_size: u32::MAX as usize + 1, // 4GB 超
};
// 現状: append_sample() が Ok(()) を返し、壊れた MP4 が生成される
// 期待: MuxError が返る
```

## 修正方針

`src/mux_mp4_file.rs:548` の `as u32` を `u32::try_from()` に変更し、エラーを返す。
`MuxError::EncodeError(Error::invalid_data(...))` を使用する (`MuxError::Overflow` も候補だが、
fmp4 側と一貫性を取るため `EncodeError` を採用する)。

`Sample.data_size` の型自体を `u32` や `u64` に変更する選択肢もあるが、
呼び出し側の `Vec::len()` 等との整合性を考慮し、今回は `try_from` による実行時チェックに留める。

## テスト戦略

- 単体テスト: `data_size` が `u32::MAX` の場合は成功、`u32::MAX + 1` の場合はエラーを返すことの検証
- PBT: `prop_mux_demux.rs` の既存 strategy (`data_size: 100..10000`) は u32 境界値をカバーしないため、
  境界境界値テストは単体テストで対応する

## 後方互換

修正後、4GB 超のサンプルを渡した場合に `MuxError` が返るようになる (動作変更)。
既存コードで 4GB 超のサンプルを渡していた場合、サイレントな成功からエラー返却に変わる。

## CHANGES.md

`[FIX]` で記載する。CHANGES.md の既存エントリ (122-125 行目) では同種の
「暗黙切り捨て → エラー」を `[FIX]` としているため、一貫性を取る。

## C API への影響

C API (`crates/c-api/src/mux.rs`) の `Mp4MuxSample` でも `data_size` を受け取るが、
C API 側では `u32` として扱っているため、今回の問題は発生しない。

## 解決方法

- `src/mux_mp4_file.rs:548` の `sample.data_size as u32` を
  `u32::try_from(sample.data_size).map_err(|_| MuxError::EncodeError(Error::invalid_data("sample data size exceeds u32::MAX")))?`
  に置き換え、`u32::MAX` を超えるサイズで明示的なエラーを返すようにした。
  `mux_fmp4_segment.rs:744` 周辺と同じ防御パターンに揃えた。
- レビューで `src/mux_mp4_file.rs:986` の `c.samples.len() as u32` も同型の暗黙
  トランケーションを抱えていることが判明したため、同じく `u32::try_from()`
  ベースに置き換え、`build_stbl_box` の `entries` 構築 closure を `Result<StscEntry, MuxError>`
  返却にして `collect::<Result<_, _>>()?` で伝播するようにした。エラー文面は
  `"samples per chunk exceeds u32::MAX"`。
- `Mp4FileMuxer::append_sample()` の doc コメントに、エラー返却時の内部状態の
  不変条件 (`MissingSampleEntry` 経路を除き `next_position` / `chunks` 等は
  変更されない) を明記した。
- `src/mux_mp4_file.rs` の `#[cfg(test)] mod tests` に以下 4 テストを追加した:
  - `test_append_sample_data_size_u32_max_succeeds`: 境界値 `u32::MAX` で
    `append_sample` と `finalize` が成功し、Co64Box 経路を通ることを検証する
  - `test_append_sample_data_size_u32_max_with_faststart`: faststart 有効化
    (`with_options`) 経路でも同じ境界値挙動を検証する
  - `test_append_sample_data_size_exceeds_u32_max`: `u32::MAX + 1` で
    `MuxError::EncodeError` が返り、その Display 出力が
    `"sample data size exceeds u32::MAX"` を含むことを検証する (64-bit 限定)
  - `test_append_sample_error_keeps_muxer_state`: エラー後に同じ `data_offset`
    で正常サンプルを再投入できることを検証する (64-bit 限定)
