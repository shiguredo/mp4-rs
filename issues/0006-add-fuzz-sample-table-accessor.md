# SampleTableAccessor の fuzz ターゲットを追加する

- Priority: Medium
- Created: 2026-05-26
- Model: Opus 4.7
- Branch: feature/add-fuzz-sample-table-accessor

## 目的

`auxiliary.rs` の `SampleTableAccessor` は `pub mod aux` で公開されている API だが、
専用の fuzz ターゲットが存在しない。

`SampleTableAccessor::new()` は `StblBox` の `stsc` / `stts` / `ctts` / `stsz` / `stco` / `co64` の
整合性検証と内部インデックス構築を行う非自明なロジックを持つ。
`fuzz_mp4_file_demux.rs` 経由で間接的にテストされているが (`demux_mp4_file.rs:517`)、
demuxer が `SampleTableAccessor::new()` に到達するには正しい moov / trak 構造が必要であり、
fuzzer の到達効率が低い。専用ターゲットにより直接的にカバーすべき。

## 優先度根拠

公開 API であり、ユーザーが直接利用しうるコンポーネント。
demuxer 経由の間接カバレッジはあるが、到達効率が低い。Medium とする。

## 現状

`SampleTableAccessor` は `demux_mp4_file.rs` 内部でのみ使用されており、
fuzzer からは `fuzz_mp4_file_demux.rs` → `Mp4FileDemuxer` → `SampleTableAccessor::new()` の経路でしか到達しない。

## 設計方針

1. 任意バイト列を `StblBox::decode()` でデコードする
2. 成功した場合、`SampleTableAccessor::new()` でインスタンスを生成する
3. 成功した場合、`samples()` と `chunks()` のイテレータを全走査する
4. 各 `SampleAccessor` / `ChunkAccessor` のメソッド (`duration()`, `timestamp()`, `data_offset()`,
   `is_sync_sample()`, `composition_time_offset()` 等) を呼び出す

## 完了条件

- `fuzz/fuzz_targets/fuzz_sample_table_accessor.rs` が追加されている
- `fuzz/Cargo.toml` に `[[bin]]` エントリが追加されている
- `cargo fuzz build fuzz_sample_table_accessor` が成功する
- 短時間の fuzzing 実行でパニックが発生しない
