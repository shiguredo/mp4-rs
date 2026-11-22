# `TrunBox::decode` が per-sample データを持たない `trun` で入力長と無関係に約 640 MiB を確保する

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-trun-no-sample-data-memory-amplification
- Polished: {YYYY-MM-DD}

## 目的

信頼できない MP4 を解析したときに、小さな `trun` ボックスから巨大なメモリ確保が発生しないようにする。

`trun` の per-sample フィールドは `tfhd` / `trex` の既定値で補えるため、`sample_count` はペイロード長に比例しない。しかし現在の `TrunBox::decode` は、per-sample フィールドがすべて無い場合でも `sample_count` 個の `TrunSample` を `Vec` に確保する。

## 現状

`src/boxes_fmp4.rs` の `TrunBox::decode` は、per-sample フィールドのフラグがすべて 0 のとき `bytes_per_sample == 0` となり、payload の残量と無関係に `sample_count` 個のサンプルを push する。上限は定数 `MAX_SAMPLES_WITHOUT_DATA = 16_777_216` で、コメントでは「`TrunSample` 約 32 バイト × 16M = 512MB」と見積もっている。

この環境の `size_of::<TrunSample>()` は 40 バイトであり、実際の確保量は約 640 MiB になる。

再現入力 (16 バイト):

```text
00 00 00 10 74 72 75 6e 00 00 00 00 01 00 00 00
```

これは `size=16`, `trun`, `version=0`, `flags=0`, `sample_count=0x01000000` (16,777,216) を表す。

確認結果:

```text
samples=16777216, consumed=16, size_of_trun_sample=40,
hwm_before_kb=3376, hwm_after_kb=658532
```

`/usr/bin/time -v` でも最大 RSS 658,532 KiB、経過 0.33 秒だった。

## 設計方針

- per-sample データが無い `trun` では、`sample_count` 個の `TrunSample` を実体化しない。サンプル数を保持するだけの表現にするか、必要になった時点で既定値を使って展開する
- 実体化を続ける場合は、`sample_count` の上限を入力長や親 `moof` のサイズから決める。単純に上限値を小さくするだけでは、大きな `sample_count` を持つ正当なファイルを拒否し得るため、呼び出し側が入力全体のサイズを知れる経路と組み合わせる
- いずれの方式でも、16 バイトの入力から数百 MiB を確保しないことをテストで固定する
- `trun` の per-sample データがある場合の上限検査は現状を維持する

## 完了条件

- 上記 16 バイトの入力で `TrunBox::decode` が成功する場合でも、確保量が入力長に対して十分小さい (目安として 16 MiB 以下) ことをテストで確認できる
- または、上記の入力が `InvalidData` として拒否される
- per-sample データを持たない正当な `trun` で、`sample_count` が小さい入力は今どおり decode できる
- PBT または fuzz の入力に、per-sample データ無しで巨大な `sample_count` を持つケースが追加されている
- `cargo test --workspace` が成功する
