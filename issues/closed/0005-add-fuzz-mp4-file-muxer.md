# Mp4FileMuxer の fuzz ターゲットを追加する

- Priority: Medium
- Created: 2026-05-26
- Completed: 2026-05-26
- Model: Opus 4.7
- Branch: feature/add-fuzz-mp4-file-muxer

## 目的

`mux_mp4_file.rs` (`Mp4FileMuxer`) に対応する fuzz ターゲットが存在しない。
CLAUDE.md のテスト方針「Fuzzing: 任意入力に対するクラッシュ耐性（パニック安全性）」に基づき、
公開 API である `Mp4FileMuxer` のパニック安全性を検証する fuzz ターゲットを追加する。

## 優先度根拠

demux 側 (`fuzz_mp4_file_demux.rs`) や fMP4 mux 側 (`fuzz_fmp4_segment_mux.rs`) は既にカバー済みだが、
通常 MP4 の mux だけが抜けている。バグではなくカバレッジの欠落のため Medium とする。

## 現状

`mux_mp4_file.rs` に対応する fuzz ターゲットが存在しない。

関連する既存 fuzz ターゲット:
- `fuzz_fmp4_segment_mux.rs`: fMP4 セグメント mux 用。demux → mux のパターン
- `fuzz_mp4_file.rs`: `Mp4File<RootBox>` の decode/encode 用。muxer のテストではない

## 設計方針

`fuzz_fmp4_segment_mux.rs` の demux → mux パターンをベースにしつつ、
`Mp4FileMuxer` 固有の制約と分岐に対応した設計にする。

### demux → mux パターン

1. 任意バイト列を `Mp4FileDemuxer` でデマルチプレクスする
2. 取得したサンプル情報を `Mp4FileMuxer` で再マルチプレクスする
3. `finalize()` の戻り値 `FinalizedBoxes` のメソッド
   (`is_faststart_enabled()`, `moov_box_size()`, `offset_and_bytes_pairs()`, `moov_box()`)
   を呼び出してパニックしないことを確認する

### `data_offset` の調整

`Mp4FileMuxer::append_sample()` は `sample.data_offset == next_position` を要求する。
demuxer から得た `data_offset` は元ファイル内のオフセットであり、
muxer の `next_position`（= `initial_boxes_bytes().len()` から開始）とは一致しない。

`fuzz_fmp4_segment_mux.rs` と同様に、`data_offset` を
`initial_boxes_bytes().len()` から順次積み上げる形で再計算する:

```rust
let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
// demuxer の各サンプルについて:
mux_sample.data_offset = data_offset;
data_offset += sample.data_size as u64;
```

サンプルが 0 件の場合は muxer を生成する前に早期リターンする
（`fuzz_fmp4_segment_mux.rs:52-54` と同じパターン）。

### faststart 経路のカバー

`Mp4FileMuxer::with_options()` で `reserved_moov_box_size > 0` を指定すると
faststart 経路（`build_head_boxes_bytes` の分岐）を通る。`new()` のみでは
`reserved_moov_box_size = 0` 固定となり、この経路がカバーされない。

ファズ入力の先頭 1 バイトの最上位ビットで分岐する:
- ビットが立っている場合: `reserved_moov_box_size = 8192` で `with_options()` を使用
- ビットが立っていない場合: `new()` を使用（`reserved_moov_box_size = 0`）

`reserved_moov_box_size` をファズ入力から直接導出すると、
`build_initial_boxes` 内の `vec![0; shared_free_payload_size]` で OOM が発生するため、
固定値を使用する。

### パニックリスク箇所

以下の箇所にパニックリスクがある:

- `build_free_box_bytes`: `assert!(total_size >= BoxHeader::MIN_SIZE, ...)`
  - `build_head_boxes_bytes` 内の条件分岐でガードされているため、通常の demux → mux パターンでは到達しない可能性が高い
- `chunks.last_mut().expect("bug")`: `is_new_chunk_needed` が true のときのみチャンクが push された直後に呼ばれるため、ロジック上は到達しない
- `build_head_boxes_bytes`: `usize::try_from(self.mdat_box_offset).expect(...)` と `checked_sub(...).expect(...)`
  - `mdat_box_offset` は `initial_boxes_bytes` の長さから導出されるため、通常はオーバーフローしない

### demux → mux パターンの限界

demux が成功した入力のみ muxer に到達するため、以下のエラーパスには構造的に到達しにくい:

- `TimescaleMismatch`: demuxer は同一トラック内で一貫した timescale を返す
- `MixedSampleEntries`: demuxer は一貫した sample entry を返す
- `Overflow`: demuxer の出力から自然にオーバーフローする値は生成されにくい
- `advance_position()`: demux → mux パターンでは呼び出し契機がない
- `creation_timestamp` のエッジケース: デフォルト値 (`Duration::ZERO`) 固定

これらは fuzzing ではなく単体テスト / PBT の責務として割り切る。

## CHANGES.md

fuzz ターゲットの追加は機能に直接影響しない変更のため、`### misc` に記載する。

## 完了条件

- `fuzz/fuzz_targets/fuzz_mp4_file_mux.rs` が追加されている
- `fuzz/Cargo.toml` に `[[bin]]` エントリが追加されている
- `cargo fuzz build fuzz_mp4_file_mux` が成功する

## 解決方法

- `fuzz/fuzz_targets/fuzz_mp4_file_mux.rs` を新規作成した
  - `fuzz_fmp4_segment_mux.rs` の demux → mux パターンをベースに、`Mp4FileMuxer` 固有の制約に対応
  - demux ループ内で直接 `mux::Sample` を構築し `Vec<Sample>` に収集する
  - `data_offset` は 0 起点で積み上げ、muxer 生成後に `initial_boxes_bytes().len()` を一括加算する
  - 先頭 1 バイトの最上位ビットで `new()` / `with_options(reserved_moov_box_size: 8192)` を切り替え、faststart 経路をカバーする
  - `finalize()` 後に `FinalizedBoxes` の全メソッドを呼び出してパニック安全性を確認する
- `fuzz/Cargo.toml` に `[[bin]]` エントリを追加した
- 30 秒間の fuzzing 実行（8,220,271 回）でクラッシュなしを確認した
