# SampleTableAccessor::new が入力サイズと乖離した sample_data_offsets を eager に確保する

- Created: 2026-08-21
- Completed: 2026-08-21
- Branch: feature/fix-sample-table-accessor-memory-amplification
- Polished: {YYYY-MM-DD}

## 目的

`SampleTableAccessor::new`（`src/auxiliary.rs`）は、全整合性チェックを通過した後に `sample_data_offsets: Vec<u64>`（サンプル 1 件あたり 8 バイト）を全サンプル分 eager に構築する。`stts` / `stsc` は run-length 形式なので、わずか数バイトの値で最大 `u32::MAX - 1` 個のサンプルを宣言でき、`stsz` を `Fixed` にするとサンプルサイズ配列がワイヤ上に存在しないため、約 100 バイトの `stbl` から最大約 34 GB の確保に到達する。

`shiguredo-rust` は「入力データが破損している場合などに、サイズやカウントを示す値のデコード結果が極端に大きくなり、メモリを大量に消費してしまうリスク」を明示的に禁じ、「メモリ消費量のオーダーは実際の入力データのサイズから大きく乖離することはない」ことを求めている。この規約違反を解消するのが本 issue の目的である。

外部ファイル由来の入力から到達する点が優先度の根拠である。`Mp4FileDemuxer::read_moov_box`（`src/demux_mp4_file.rs`）が `MoovBox::decode` 直後にトラックごとに `SampleTableAccessor::new` を呼ぶため、`aux` モジュールを直接使わない `Mp4FileDemuxer` の利用者（C API・WASM を含む）にも、細工した MP4 を渡すだけで到達する。確保は `Mp4FileDemuxer::handle_input`（`handle_input_inner` → `read_moov_box`）の内部で起きる。`tracks()` はパースを行わず、保存済みエラーか `InputRequired` を返すだけである。被害は `Err` ではなく確保失敗（`handle_alloc_error` による abort）か OOM であり、C API では復帰できず、WASM（wasm32）ではアドレス空間が 4 GB しかないため一層早く trap する。

## 現状

`SampleTableAccessor::new` の末尾、全整合性チェックを通過した後にある `sample_data_offsets` 構築ループが、チャンクとサンプルを二重に走査して 1 サンプルにつき 1 要素を `Vec<u64>` に push する。この値は `SampleAccessor::data_offset`（公開 API）が参照する。

`Fixed` が危険なのは、`Variable` では `entry_sizes` がワイヤ上に 4 バイト/サンプル物理的に必要（増幅率は約 2 倍で入力サイズに比例する）なのに対し、`Fixed` にはその下限が無く、`sample_size`（4 バイト）+ `sample_count`（4 バイト）の 8 バイトだけで任意個のサンプルを宣言できるためである。同一 `stbl` に対して既存の整合性チェックはすべて通過するため、`SampleTableAccessorError` のどのバリアントにもならずに確保だけが先行する。

`StblBox` のフィールドはすべて `pub` なので、`stsz` を `Fixed`、`stsc` を 1 チャンク・`sample_per_chunk = N`、`stco`/`co64` を 1 チャンクにした `StblBox` を組み立てて `SampleTableAccessor::new` に渡すだけで再現する。カスタムグローバルアロケータで確保量を測ると以下になった。

- `Fixed { sample_size: 1, sample_count: 1_000_000 }`（1 チャンク 100 万サンプル）: `new` が最終 8 MB / realloc ピーク 12 MB を確保して `Ok` を返す（入力側の `stsz` はワイヤ上 8 バイト相当）
- 同じ構成の `Variable { entry_sizes: vec![1; 1_000_000] }`: 確保は同等だが、入力側に `entry_sizes` として 4 MB が既に必要

100 万サンプルで 8 MB という比率は `u32::MAX - 1`（約 42.9 億）まで線形に伸び、`(u32::MAX - 1) * 8` = 約 34 GB になる。

`sample_data_offsets` 以外の `Vec`（`stts` エントリごとの `sample_durations`、`ctts` エントリごとの `sample_composition_offsets`、チャンクごとの `sample_index_offsets`）はいずれも入力に含まれるエントリ数・チャンク数に比例するため、サンプル数に比例する確保は `sample_data_offsets` だけである。

`issues/closed/0009-bug-sample-table-accessor-overflow.md` の「### スコープ外」に「入力サイズに対するメモリ増幅（124 バイトの `stbl` から 160 MB を確保できる）」「`shiguredo-rust` の『メモリ消費量のオーダーは実際の入力データのサイズから大きく乖離することはない』という意図に反する」として明記され、「起票の要否とタイミングは担当者判断とする」とされている。open / pending には未起票である。

## 設計方針

対象を `Fixed` に限定する。`Fixed` は全サンプルが同一サイズなので、`data_offset()` を prefix-sum テーブルなしに算術で算出できる。

- サンプルが属するチャンクの先頭オフセットを `base`、チャンク内でのそのサンプルの序数（0 始まり）を `k`、共通サンプルサイズを `s` とすると、`data_offset = base + k * s` で求まる。チャンク先頭のサンプルインデックスは既存の `sample_index_offsets` から引ける。
- `Variable` は従来どおり prefix-sum テーブル（`sample_data_offsets`）を維持する。`Variable` の増幅率は約 2 倍かつ入力サイズに比例するため規約を満たしており、テーブルを廃止して都度計算にすると `data_offset()` が O(1) から O(n)、`samples()` の全走査が O(n^2) に悪化するためである。

### overflow 検出契約の維持

`issues/closed/0009` は「`SampleTableAccessor::new` は `sample_data_offsets` 構築時の `u64` overflow を検出して `SampleDataOffsetOverflow` を `Err` で返す」ことを仕様として固定しており、`tests/test_auxiliary.rs` はチャンク末尾の捨てられる加算まで含めて `sample_index` / `accumulated_offset` / `sample_data_size` の具体値を照合している。`Fixed` を算術化してテーブル構築ループを外すと、この検出が消える。

したがって `Fixed` 経路でも `new` の時点で overflow を検出し、eager ループが生成していたのと同一の `SampleDataOffsetOverflow`（同一 `sample_index` / `accumulated_offset` / `sample_data_size`）を返すこと。判定はチャンクをインデックス順に走査し、最初に overflow するチャンクで停止する（全体で O(チャンク数)。チャンク数は `stco`/`co64` の配列長に等しく入力サイズに比例する）。オフセット `base`・サンプル数 `k`・サイズ `s` のチャンクでは、eager ループは `floor((u64::MAX - base) / s) + 1` 番目のサンプルで最初に overflow し、その時点の累計オフセットは `base + (その序数 - 1) * s` になる。`sample_size` は `NonZeroU32` なので `s >= 1` が常に成り立ち、除算は安全である。

この式は `issues/closed/0009` が固定した 2 ケース（`Fixed { sample_size: 1 }` に読み替え）で検算済みである。

- `chunk_offsets: [u64::MAX - 1]`・3 サンプル → `sample_index: 2`、`accumulated_offset: u64::MAX`、`sample_data_size: 1`
- `chunk_offsets: [u64::MAX]`・1 サンプル → `sample_index: 1`、`accumulated_offset: u64::MAX`、`sample_data_size: 1`

### スコープ外

`issues/closed/0009` の他のスコープ外項目（`stss_box.sample_numbers` の範囲・ソート順検証、`len() as u32` の narrowing cast、`SampleTableAccessorError` への `Copy` derive、`sample_count` の実効上限が `u32::MAX - 1` になっている off-by-one、`NonZeroU32::saturating_add` 群）は扱わない。`StszBox::Fixed { sample_count }` が `stts` 合計と突き合わされていない件は別 issue とする（本 issue とは独立で、修正しても本 issue のメモリ増幅は防げない）。

## 完了条件

- `SampleTableAccessor::new` の `Fixed` 経路が `sample_data_offsets` テーブルを構築せず、`SampleAccessor::data_offset()` が算術で算出されること
- `Variable` 経路の挙動（prefix-sum テーブルの構築と `data_offset()` の O(1) 参照）が変わらないこと
- `Fixed` 経路でも `new` の時点で `u64` overflow を検出し、従来の eager ループと同一の `sample_index` / `accumulated_offset` / `sample_data_size` を持つ `SampleDataOffsetOverflow` を `Err` で返すこと。これにより公開 API のシグネチャも `data_offset()` の返り値も不変であること
- `Fixed` の巨大 `sample_count` に対して `new` が入力サイズと乖離した確保を行わないこと
- `pbt/tests/prop_auxiliary.rs` に差分 PBT を追加すること。同一の論理テーブルを `Fixed { sample_size: s }` と `Variable { entry_sizes: vec![s; n] }` の 2 通りで構築し、全サンプルの `data_offset()` が一致することをランダム入力で検証する。`stsc` は `sample_per_chunk` が異なる複数エントリを生成し、チャンクオフセットも単調増加でない配置を含めること（`Fixed` の算術は `sample_index_offsets` からチャンク先頭インデックスを引くため、均一チャンクだけではチャンク境界のずれを検出できない）。現状の実装で複数チャンク 5 ケースの一致を実測済みである
- `Fixed` 経路の overflow を検証する単体テストを `tests/test_auxiliary.rs` に追加すること。既存の overflow テストはすべて `StszBox::Variable` を使っており `Fixed` 経路の回帰を捕捉できないため、上記 2 ケースの各フィールドを照合する
- `Fixed` で `sample_count = u32::MAX - 1` を宣言しても `new` が即座に成功し、算術的に正しい `data_offset()` を返す単体テストを追加すること。修正前はこのテストが abort（確保失敗）するため、CI 上は「失敗」ではなく「abort」になるトレードオフを、この完了条件に明示して記録する
- `make test` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check` が通ること
- `CHANGES.md` の `## develop` に `[FIX]` エントリと担当者行が追記されていること（新しいエラーバリアントを追加せず既存の `SampleDataOffsetOverflow` を流用するため後方互換は保たれる）

## 関連 issue

- `StszBox::Fixed { sample_count }` の未検証（別 issue）: 独立している。あちらを修正しても、攻撃者は `stsz.Fixed.sample_count` を `stts` 合計に一致させれば本 issue のメモリ増幅に到達できる（本 issue の再現も一致させた値を使っている）。目的（メモリ確保のオーダー抑制 / 整合性検証の網羅）が異なるため別 issue に分ける

## 解決方法

- `SampleTableAccessor::new` の `StszBox::Fixed` 経路で `sample_data_offsets` テーブルを構築しないようにした
  - チャンク単位で `k > (u64::MAX - base) / s` により `u64` overflow を検出し、従来の eager ループと同一の `SampleDataOffsetOverflow`（`sample_index` / `accumulated_offset` / `sample_data_size`）を返す
  - `Variable` 経路は従来どおり prefix-sum テーブルを構築する
- `SampleAccessor::data_offset` の Fixed 経路を `base + チャンク内序数 × sample_size` の算術算出に変更した
- `SampleAccessor::chunk` は `sample_index_offsets` の探索を `binary_search` から `partition_point` に切り替え、`sample_per_chunk == 0` で同一値が連続しても index 以下の最右（実サンプル側チャンク）を返すようにした
- `tests/test_auxiliary.rs` に Fixed 経路の overflow 2 ケース、巨大 `sample_count` での成功、空チャンクを挟む Fixed/Variable の `data_offset` 一致テストを追加した
- `pbt/tests/prop_auxiliary.rs` に Fixed / Variable の `data_offset` 差分 PBT（複数 `stsc`・非単調チャンクオフセットのカバレッジゲート付き）を追加した
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追記した
