# `Fmp4SegmentDemuxer` が件数だけの `trun` から入力のバイト数を超える数のサンプルを展開する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-segment-demuxer-count-only-expansion
- Polished: {YYYY-MM-DD}

## 目的

`Fmp4SegmentDemuxer` が 1 回の呼び出しで返すサンプルの数を、渡された入力のバイト数以下に制限する。これにより、細工した小さなメディアセグメントを渡されても、入力の大きさに見合わないメモリと CPU を消費しないようにする。

shiguredo-rust の規約は、入力バイナリのデコードで、サイズやカウントを示す値によってメモリ消費が膨らむことを避け、「メモリ消費量のオーダーは実際の入力データのサイズから大きく乖離することはない」状態を保つよう求めている。

## 現状

### 件数だけの `trun` とその展開

`src/boxes_fmp4.rs` の `TrunBox::decode` は、次の `trun` を `TrunSamples::CountOnly(sample_count)` として返し、サンプル行を実体化しない。

- サンプルごとのフィールドのフラグが 1 つも立っていない `trun`
- `sample_count` が 0 の `trun`

このときの件数の上限は `MAX_SAMPLES_WITHOUT_DATA`（16,777,216）である。この定数のコメントは、上限が `trun` 1 つあたりのものであり、デマルチプレクサーが出力するサンプル全体に対する資源の制限ではないと明記している。件数だけの `trun` は、通常ヘッダー（8 バイト）、FullBox ヘッダー（4 バイト）、`sample_count`（4 バイト）の 16 バイトで書ける。

`src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment_inner` は、対応トラックの `traf` の各 `trun` を `trun_sample_rows` で 1 行ずつ処理する。

- `trun_sample_rows` は、`TrunSamples::CountOnly` に対して件数分の `None` を返す
- 各行について `PendingSample` を作り、`pending_samples` に積む
- 最後に `pending_samples` の全要素を `Sample` に変換し、`Vec` で返す

件数を制限する処理は、サンプルごとの境界検査だけである。各サンプルについて、`sample_data_offset` にサイズを足した終端が `mdat` の終端を超えると、`sample data range exceeds mdat boundary` を返す。

### 件数が入力の大きさで制限されない条件

条件は 2 つある。

1 つ目は、件数だけの `trun` のサンプルサイズが 0 に決まる場合である。

- サイズは `tfhd` の `default_sample_size`、なければ `trex` の `default_sample_size` で決まる
- サイズが 0 だとサンプルの終端が進まないため、境界検査は失敗しない
- その結果、16 バイトの `trun` 1 つから最大 16,777,216 個のサンプルが生まれる
- 上限は `trun` ごとなので、`trun` や `traf` を並べれば合計はさらに増える

`Fmp4SegmentMuxer` は `trex` の `default_sample_size` に 0 を書く（`src/mux_fmp4_segment.rs` の `TrexBox` の構築）。そのため、このクレートが出力した初期化セグメントに対しては、`default_sample_size` を持たない `tfhd` と、16 バイトの件数だけの `trun` があれば足りる。

2 つ目は、サイズが 1 以上で、複数のランが同じ範囲を指す場合である。

- 1 つのランの件数は、`mdat` の終端までのバイト数で抑えられる
- しかし、`data_offset` を持つ `trun` は `traf` の基準位置からの位置を指定でき、複数の `trun` が同じ範囲を指せる
- `default_base_is_moof` の `traf` も、それぞれ `moof` の先頭を基準にできる
- そのため、合計の件数は「ランの数 × 範囲のバイト数」まで増える

サンプル行を持つ `trun`（`TrunSamples::PerSample`）は、1 行あたり 4 バイト以上の入力を必要とし、`TrunBox::decode` の時点で行を実体化する。そのため、サンプル数は入力のバイト数に比例する。

### 再現と測定

いずれも develop の release ビルドで確認した。

1 つ目の条件は次の入力で確認した。

- 初期化セグメント（630 バイト）: `Fmp4SegmentMuxer` が出力したもの。SPS / PPS を持たない avc1 のサンプルエントリーを使い、`trex.default_sample_size` は 0
- メディアセグメント（104 バイト）: 80 バイトの `moof` と 24 バイトの `mdat` からなる
  - `moof` の唯一の `traf` の `trun` を、`data_offset` なしの `TrunSamples::CountOnly(16_777_216)`（16 バイト）に置き換えた
  - `tfhd` に `default_sample_size` はない

測定値は次のとおり。ピークのメモリは、macOS の `/usr/bin/time -l` が出力する maximum resident set size である。

| 対象 | 結果 | 時間 | ピークのメモリ |
| --- | --- | --- | --- |
| `Fmp4SegmentDemuxer::handle_media_segment`（104 バイトのメディアセグメント） | 16,777,216 個のサンプルを返す | 約 0.2 秒 | 約 1.08 GB |
| `Fmp4FileDemuxer`（初期化セグメントとメディアセグメントを連結した 734 バイトのファイル） | 16,777,216 個のサンプルを返す | 最初のサンプルまで約 2.1 秒 | 約 8.6 GB |

この環境の `size_of::<Sample>()` は 64 バイトだった。

2 つ目の条件は次の入力で確認した。結果は 4,194,304 個のサンプルになった。

- 66,892 バイトのメディアセグメント
- `tfhd` の `default_sample_size` を 1 にする
- `mdat` のペイロード（65,536 バイト）の先頭を `data_offset` で指す `TrunSamples::CountOnly(65_536)` の `trun` を 64 個並べる

### `Fmp4FileDemuxer` の扱い

`src/demux_fmp4_file.rs` の `Fmp4FileDemuxer::read_media_segment` は、`moof` の先頭から `mdat` の末尾までの範囲を内部の `Fmp4SegmentDemuxer::handle_media_segment_with_file_offset` に渡す。そのため、同じ展開がそのまま起きる。

その後、`build_pending_samples` が返されたサンプルごとに `PendingSample` を作り直し、並べ替えたうえで `pending_samples`（`VecDeque`）に積む。`PendingSample` にはトラックの `SampleEntry` を clone して入れる。`next_sample` は 1 個ずつ取り出すが、メディアセグメント 1 つ分のサンプルは `handle_input` の時点ですべて実体化される。上の測定で `Fmp4SegmentDemuxer` よりピークが大きいのはこのためである。

`SampleEntry` をサンプルごとに clone する処理は、サンプル行を持つ `trun` でも行われる。その量は入力に比例するため、本 issue では扱わない。

### wasm

次の 2 つの経路から、この展開に到達する。

- `crates/wasm/src/fmp4_segment_demux.rs` の `handleMediaSegment`: 返されたサンプル全体を 1 つの JSON 文字列にする
- `crates/wasm/src/fmp4_file_demux.rs`: `Fmp4FileDemuxer` を包む

### 既存の issue との関係

- issue 0126 の設計方針には、「この課題はボックスの空行の実体化をなくすことに限定する。デマルチプレクサーの一括出力の契約や、出力全体に対する資源制限は変更しない」とある
- 本 issue は、そこで対象外とした出力全体に対する資源の制限を扱う

## 設計方針

- `Fmp4SegmentDemuxer::handle_media_segment_inner` で、1 回の呼び出しで返すサンプルの総数を、渡された `data` のバイト数以下に制限する
  - 超える場合は `DemuxError::DecodeError`（`ErrorKind::InvalidData`）を返す
  - 判定は、各 `trun` のサンプルを `pending_samples` に積む前に、それまでの件数とその `trun` の件数の和で行う
  - これにより、上限を超える入力でも、`data` のバイト数に比例する量しかサンプルを実体化しない
  - エラーの文言は英語にする
- この上限は資源の制限であり、サイズ 0 のサンプルが仕様上許されるかを判断するものではない
- 正当な入力が上限に当たらない根拠は次のとおり
  - サイズが 1 以上で互いに重ならないサンプルの総数は、`mdat` のペイロードのバイト数以下である
  - サンプル行を持つ `trun` のサンプル数は、その行のバイト数の 4 分の 1 以下である
  - したがって、上限に当たるのは次の入力に限られる
    - 件数だけの `trun` で、サイズ 0 のサンプルを入力のバイト数を超えて宣言する入力
    - 複数のランが同じ範囲を指す入力
- 読み飛ばすトラックの `traf` はサンプルを返さないため、件数に数えない
  - `skipped_traf_data_end` は、件数だけの `trun` のデータ末尾を既定サイズと件数の積で求めており、件数に比例する処理をしない
- `Fmp4FileDemuxer` はメディアセグメントの範囲を内部の `Fmp4SegmentDemuxer` に渡すので、同じ上限がそのまま効く。`Fmp4FileDemuxer` 側には変更を加えない
- `TrunBox::decode` の `MAX_SAMPLES_WITHOUT_DATA` は、ボックス単位の受理範囲として変えない
- `handle_media_segment` の doc の「次の入力もエラーになる」に、この条件を追加する
- 「エラーを返した場合も内部状態は変わらない」という今の契約を保つ
  - この判定は作業用の状態を `track_runtimes` に書き戻す前に行われる

## 完了条件

- `tests/test_demux_fmp4_segment.rs` の単体テストで、1 つ目の条件の入力に対して `handle_media_segment` が `ErrorKind::InvalidData` の `DemuxError::DecodeError` を返すことを確認する
  - 入力は、上の再現と同じく、サイズ 0 の `TrunSamples::CountOnly(16_777_216)` を持つメディアセグメントとする
- サイズ 0 の件数だけの `trun` について、境界を単体テストで確認する
  - サンプルの総数が `data` のバイト数と等しい場合は成功する
  - 1 多い場合はエラーになる
  - 件数だけの `trun` のバイト数は件数によらないので、`data` のバイト数を固定したまま件数だけを変えられる
- 2 つ目の条件（サイズが 1 以上で、複数の `trun` が同じ範囲を指す）で、総数が `data` のバイト数を超える入力がエラーになることを単体テストで確認する
- エラーを返した後に正当なメディアセグメントを渡すと、エラーがなかった場合と同じサンプルを返すことを確認する
- `Fmp4FileDemuxer` に 1 つ目の条件のメディアセグメントを含むファイルを渡すと、`next_sample` が `DemuxError::DecodeError` を返すことを確認する
- 既存の単体テストと PBT が成功する
  - 件数だけの `trun` を扱う `count_only_trun_matches_explicit_rows_on_supported_track` と `count_only_trun_data_end_is_used_by_next_traf_on_skipped_track` を含む
- `CHANGES.md` の `## develop` に `[FIX]` を追加する
- `cargo test --workspace` が成功する
