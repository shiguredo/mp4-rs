# `SampleTableAccessor` が `sample_count == 0` の `stts` / `ctts` で隣の run の値を返すことがある

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-sample-table-zero-sample-count-run
- Polished: {YYYY-MM-DD}

## 目的

`stts` または `ctts` に `sample_count == 0` のエントリがあると、`SampleTableAccessor` がその次の run と同じ累計キーを並べる。`binary_search_by_key` は、同じキーが複数あるとき、どれを返すかを保証しない。返る位置は決定的だが、将来の Rust で変わり得る。0 件の run を表に残すと、尺と composition offset がその選択に依存する。0 件の run は表に載せない。

## 現状

`src/auxiliary.rs` の `SampleTableAccessor::new` は、`stts` の各エントリについて、`sample_count` を足す前に `(累計サンプル数, sample_delta, 累計尺)` を `sample_durations` へ push する。`ctts` も同じ順で `(累計サンプル数, sample_offset)` を push する。`sample_count == 0` のエントリは累計を進めないので、次のエントリと同じキーが並ぶ。

`SampleAccessor::duration`、`SampleAccessor::timestamp`、`SampleAccessor::composition_time_offset` は、この配列を `binary_search_by_key` で引く。Rust のドキュメントは、同じキーが複数あるとき、どれを返すかは決定的だが将来のバージョンで変わり得る、と書いている。特定の位置は保証しない。

例: `stts` が `(sample_count: 0, sample_delta: 999)` の次に `(sample_count: 1, sample_delta: 10)` を持つ。テーブルのキーは `[0, 0]` になる。`ctts` を `(0, 999)` の次に `(1, 7)` にしても同じである。`SampleTableAccessor::new` は成功する。

この並びを今の安定版で引くと、`duration` は 10、`composition_time_offset` は 7 になる。0 件の run の 999 にはならない。今の安定版は同じキーの右端を返す。左端を返す実装だと 999 になる。

0 件 run は、同じキーの実 run より左に積まれる。実 run の直前に挟んだ場合も、右端を返す限り尺は実 run になる。`stts` が `(1, 10)`、`(0, 999)`、`(1, 20)` のとき、キーは `[0, 1, 1]` になる。2 個目のサンプルを今の安定版で引くと `duration` は 20 であり、0 件 run の 999 ではない。中点や左端を返す実装だと 999 になる。`ctts` を `(1, 3)`、`(0, 999)`、`(1, 4)` にすると、右端なら 4、それ以外なら 999 になる。

`sample_index_offsets` 側は、`sample_per_chunk == 0` で同じ開始位置が並ぶことをコメントで扱い、`SampleAccessor::chunk` は `partition_point` で「その位置以下の最後」を選んでいる。`sample_durations` と `sample_composition_offsets` には同じ対処がない。

`unsigned int(32)` の `sample_count` に 0 は書ける。`SttsBox` / `CttsBox` のデコードは 0 を拒否しない。

## 設計方針

- `sample_count == 0` のエントリは run の表に push しない。累計尺には `sample_delta * 0` を足す今の式のまま（増分は 0）で、次のエントリへ進む
- 検索は今の `binary_search_by_key` のままにする。0 件の run を載せなければ、1 つのキーが 2 つの run を指さない
- `ctts` も同じにする

## 完了条件

- 0 件のエントリを run の表に push しない
- 上の先頭が 0 件の例では、最初のサンプルの `duration` が 10、`composition_time_offset` が 7 である。この結果が、同値キーの探索順に依存しない
- `(1, 10)`、`(0, 999)`、`(1, 20)` の `stts` と、`(1, 3)`、`(0, 999)`、`(1, 4)` の `ctts` では、2 個目のサンプルの `duration` が 20、`composition_time_offset` が 4 である。0 件 run の 999 にはならない
- `sample_count == 0` だけの表は、サンプル数 0 のまま `new` が成功する
- 0 件の run が無い入力の尺とオフセットは今と変わらない
