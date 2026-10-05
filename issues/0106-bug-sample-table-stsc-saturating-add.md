# `SampleTableAccessor` が `stsc` のサンプル数合計のオーバーフローを飽和加算で見逃し、`ChunkAccessor::samples` が panic する

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-sample-table-stsc-saturating-add
- Polished: 2026-10-05

## 目的

`SampleTableAccessor::new` が、`stsc` から数えたサンプル数の合計が `u32::MAX` に達して 1 始まりの次サンプルインデックス（合計 + 1）が `u32` に収まらなくなってもエラーにせず、`stts` のサンプル数と一致したとみなすことがある。そのあと `ChunkAccessor::samples` が、存在しないサンプルを取りに行って panic する。インデックスが `u32` に収まらない入力はエラーで拒否する。

## 現状

`src/auxiliary.rs` の `SampleTableAccessor::new` は、`stts` と `ctts` の `sample_count` 合計を `checked_add` で足し、溢れたら `SampleTableAccessorError::SampleCountOverflow` を返す。`stsc` のチャンクごとの開始サンプル位置だけ `NonZeroU32::saturating_add` で足す。溢れると開始位置は `u32::MAX` で止まり、その後の検査は `first_sample_index.get() - 1 == sample_count` なので、`stts` 側が `u32::MAX - 1` のときに通過する。

再現する `StblBox` は次のとおり。ボックス自体は小さい。

- `stts`: エントリ 1 個。`sample_count = u32::MAX - 1`、`sample_delta = 1`
- `stsz`: `StszBox::Fixed`。`sample_count = u32::MAX - 1`、`sample_size = 1`
- `stsd`: サンプルエントリ 1 個
- `stco`: チャンクオフセット 2 個
- `stsc`（フィールド名は `sample_per_chunk`。仕様の語は `samples_per_chunk`）:
  - `first_chunk = 1`、`sample_per_chunk = u32::MAX - 1`
  - `first_chunk = 2`、`sample_per_chunk = 1`

`sample_per_chunk` の本当の合計は `u32::MAX` であり、`stts` の `u32::MAX - 1` と一致しない。1 個目のチャンクのあと開始位置はちょうど `u32::MAX` になり、2 個目で `saturating_add(1)` が `u32::MAX` のまま止まる。検査は `u32::MAX - 1` と比較するので成功し、`SampleTableAccessor::new` は `Ok` を返す。

2 個目のチャンクの `ChunkAccessor::samples` は、開始インデックス `u32::MAX` を `get_sample` に渡す。`get_sample` は `sample_index <= sample_count` のときだけ `Some` を返すので `None` になり、`expect("unreachable")` が panic する。`StszBox::Variable` のときは `SampleTableAccessor::build_variable_sample_data_offsets` が同じ `samples` を `new` の中で呼ぶ。こちらは `entry_sizes` がサンプル数ぶん要るため、上記の Fixed より先に巨大な確保が必要になる。

`src/demux_mp4_file.rs` の `Mp4FileDemuxer` はサンプルを `get_sample` で 1 件ずつ取る。上記の入力では `new` が成功したあと、`next_sample` 自体は 2 個目のチャンクの `samples` を回さない。panic するのは `ChunkAccessor::samples` を呼んだときである。`validate_fixed_sample_data_offsets` のコメントは、この突き合わせでサンプル位置が `sample_count` 以内に収まると書いている。飽和した場合はその前提が崩れる。

## 設計方針

- `stsc` の開始位置の加算を `checked_add` にし、溢れたら `SampleTableAccessorError::SampleCountOverflow` を返す。`box_type` は `StscBox::TYPE` とする
- 加算が成功したときの `stts` との本数検査は今のまま残す
- `ChunkAccessor::samples` の `expect` は、構築時に同じ条件を潰したあとの不変条件として残す
- `SampleCountOverflow` の doc は、溢れるボックスを `stts` ないし `ctts` と書いている。`stsc` でもこの variant を返すので、次の 3 箇所を直す
  - variant の冒頭 doc: `stsc` では累計サンプル数が `u32` に収まっていても 1 始まりの次サンプルインデックスが `u32` を超えることを書く
  - `box_type` フィールドの doc: `stts` / `ctts` / `stsc` を挙げる
  - `accumulated_sample_count` フィールドの doc: `stts` / `ctts` では累計サンプル数、`stsc` では累計サンプル数に 1 を足した 1 始まりの次サンプルインデックスだと書く
- `SampleCountOverflow` の `Display` は `stsc` のときだけ文言を分ける。`stts` / `ctts` は現行の文言を維持し、`stsc` は 1 始まりの次サンプルインデックスが `u32` を超えたことが分かる文言にする（例: `Sample index derived from stsc box exceeds u32 (accumulated {accumulated_sample_count}, adding {entry_sample_count})`。実装時は `box_type` を埋め込む現行の流儀に合わせてよい）。`tests/test_auxiliary.rs` の `Display` のテストは `stts` の場合を固定しているため、`stts` / `ctts` の文言を変えない限り更新は不要である
- `stsc` の加算で溢れたときは、`accumulated_sample_count` に溢れる直前の `first_sample_index`（1 始まりの次サンプルインデックス。累計サンプル数に 1 を足した値）、`entry_sample_count` にそのチャンクの `sample_per_chunk` を入れる

## 完了条件

- 上記の `StblBox` で `SampleTableAccessor::new` が `Ok` を返さず、`SampleCountOverflow` を返す。`box_type` が `StscBox::TYPE`、`accumulated_sample_count` が `u32::MAX`、`entry_sample_count` が 1 である
- `sample_per_chunk` の合計が `u32::MAX - 1` 以下で `stts` と一致し、`stsz` のサンプル数も `stts` と一致する入力は、今どおり `Ok` になる
- `sample_per_chunk` の合計が `u32::MAX` で `stts` と一致し、`stsz` のサンプル数も `stts` と一致する入力（例: `stts` の `sample_count` が `u32::MAX`）は、現行の `InconsistentSampleCount` から `SampleCountOverflow` に変わる。1 始まりの次サンプルインデックスが `u32::MAX` を超えるためであり、いずれにせよ `Ok` にはならない
- `sample_per_chunk` の合計が `u32::MAX` 未満で `stts` と一致しない入力は、今どおり `InconsistentSampleCount` になる
- `SampleCountOverflow` の variant の doc が `stsc` を含み、`stsc` では 1 始まりの次サンプルインデックスが `u32` を超えることを説明している
- `SampleCountOverflow` の `box_type` フィールドの doc が `stts` / `ctts` / `stsc` を挙げている
- `SampleCountOverflow` の `accumulated_sample_count` フィールドの doc が、`stsc` では 1 始まりの次サンプルインデックスが入ることを説明している
- `SampleCountOverflow` の `Display` が、`stts` / `ctts` では現行の文言を返し、`stsc` では 1 始まりの次サンプルインデックスが上限を超えたことを表す別の文言を返す
