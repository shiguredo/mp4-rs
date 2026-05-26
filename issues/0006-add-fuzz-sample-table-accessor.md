# SampleTableAccessor の fuzz ターゲットを追加する

- Priority: Medium
- Created: 2026-05-26
- Model: Opus 4.7
- Branch: feature/add-fuzz-sample-table-accessor

## 目的

`auxiliary.rs` の `SampleTableAccessor` は `pub mod aux` として公開されている API だが、
専用の fuzz ターゲットが存在しない。
demuxer 経由（`fuzz_mp4_file_demux.rs` → `Mp4FileDemuxer` → `SampleTableAccessor::new()`）
では正しい moov / trak 構造が必要で fuzzer の到達効率が低い。

`StblBox::decode()` から直接 `SampleTableAccessor` を構築する専用ターゲットにより、
`new()` の整合性検証と各アクセサメソッドのパニック安全性を効率的に検証する。

既存の `fuzz_stbl_box.rs` は `StblBox` の decode/encode のみを対象としており、
`SampleTableAccessor` のロジックはカバーしない。

## 優先度根拠

公開 API であり、ユーザーが直接利用しうるコンポーネント。
demuxer 経由の間接カバレッジはあるが、到達効率が低い。Medium とする。

## 設計方針

### 基本フロー

1. 任意バイト列を `StblBox::decode()` でデコードする
2. 成功した場合、`SampleTableAccessor::new(stbl_box)` でインスタンスを生成する
   - `new()` が `Err` を返した場合はそこで終了（エラーパス自体のパニック安全性は検証される）
3. 成功した場合、以下の全 public メソッドを呼び出す

### `SampleTableAccessor` のメソッド

- `sample_count()`
- `chunk_count()`
- `stbl_box()`
- `samples()`: 全走査し、各 `SampleAccessor` のメソッドを呼ぶ（後述）
- `chunks()`: 全走査し、各 `ChunkAccessor` のメソッドを呼ぶ（後述）
- `get_sample(NonZeroU32::MIN)`: 下限境界値
- `get_sample(NonZeroU32::MAX)`: 上限境界値（存在しないインデックス）
- `get_chunk(NonZeroU32::MIN)`: 下限境界値
- `get_chunk(NonZeroU32::MAX)`: 上限境界値（存在しないインデックス）
- `get_sample_by_timestamp(0)`: 二分探索の Equal 分岐
- `get_sample_by_timestamp(u64::MAX)`: 二分探索の Greater 分岐（範囲外）

### `SampleAccessor` の全メソッド

- `index()`
- `duration()`
- `timestamp()`
- `data_size()`
- `data_offset()`
- `is_sync_sample()`
- `sync_sample()`
- `composition_time_offset()`
- `chunk()`

### `ChunkAccessor` の全メソッド

- `index()`
- `offset()`
- `sample_entry()`
- `sample_entry_index()`
- `sample_count()`
- `samples()`: 全走査

### `new()` 内部でのアクセサ呼び出し

`new()` の内部（150-157 行目付近）で `this.chunks()` → `chunk.samples()` → `sample.data_size()` を
呼んで `sample_data_offsets` を構築している。つまり `new()` が `Ok` を返した時点で、
`ChunkAccessor::stsc_entry()`, `ChunkAccessor::samples()`, `SampleAccessor::data_size()` の
パニック箇所は既に通過済みである。

fuzz ターゲットで改めてこれらを呼ぶのは冗長だが、
`new()` 後に追加で走査する `get_sample_by_timestamp()`, `sync_sample()`, `timestamp()` 等は
`new()` 内では呼ばれないため、明示的な呼び出しが必要。

### パニックリスク箇所

以下の `expect` / 減算パニック箇所がある。いずれも `new()` の整合性検証が正しければ到達しない前提:

- `SampleAccessor::duration()`: `checked_sub(1).expect("unreachable")` -- `binary_search` が `Err(0)` を返した場合
- `SampleAccessor::timestamp()`: 同上
- `SampleAccessor::composition_time_offset()`: 同上
- `SampleAccessor::data_offset()`: `sample_data_offsets[index - 1]` -- インデックス範囲外
- `SampleAccessor::chunk()`: `unwrap_or_else(|i| i - 1)` -- `i == 0` でアンダーフロー
- `ChunkAccessor::offset()`: `chunk_offsets[index - 1]` -- インデックス範囲外
- `ChunkAccessor::stsc_entry()`: `unwrap_or_else(|i| i - 1)` -- `i == 0` でアンダーフロー
- `ChunkAccessor::samples()`: `.expect("unreachable")` -- `get_sample` が `None` を返した場合
- `new()` 内: `unwrap_or_else(|j| j - 1)` -- `j == 0` でアンダーフロー

fuzz の目的は「`new()` の整合性検証にバグがあり、これらのパニックに到達する入力が存在しないか」を検証すること。

## CHANGES.md

fuzz ターゲットの追加は機能に直接影響しない変更のため、`### misc` に記載する。

## 完了条件

- `fuzz/fuzz_targets/fuzz_sample_table_accessor.rs` が追加されている
- `fuzz/Cargo.toml` に `[[bin]]` エントリが追加されている
- `cargo fuzz build fuzz_sample_table_accessor` が成功する
