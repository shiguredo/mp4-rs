# `Mp4FileDemuxer::prev_sample` が同一タイムスタンプで直前のサンプルを返さず、同じサンプルを二重に返す

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-prev-sample-tie-duplicate
- Polished: {YYYY-MM-DD}

## 目的

複数トラックのサンプルが同じタイムスタンプを持つとき、`prev_sample` が直前に返したサンプルを返し、`next_sample` との往復で同じサンプルを二重に返さないようにする。

## 現状

`src/demux_mp4_file.rs` の `Mp4FileDemuxer::prev_sample` は、各トラックの直前サンプルのうち最も遅いタイムスタンプを選ぶ。比較が `timestamp <= latest` なので、同着時は添字の小さいトラックが勝つ。

`next_sample` は同着を添字の小さい順に返すため、映像 (添字 0) → 音声 (添字 1) の順に返した直後に `prev_sample` を呼ぶと、直前に返した音声ではなく映像が返る。映像の `next_sample_index` だけが巻き戻るので、直後の `next_sample` は同じ映像サンプルをもう一度返す。

再現:

- 映像と音声を同じタイムスタンプで交互に 2 サンプルずつ mux した MP4 を作る
- `next_sample` を 2 回呼ぶと映像 1、音声 1 が返る
- 続けて `prev_sample` を呼ぶ

確認結果:

```text
1st: Video timestamp=0
2nd: Audio timestamp=0
prev: Video timestamp=0
after prev: Video timestamp=0
```

## 設計方針

- 同着時は `next_sample` と逆順 (トラック添字の降順) で選ぶ。これにより直前に返したサンプルが `prev_sample` の対象になる
- `prev_sample` が返したサンプルのトラックだけ `next_sample_index` を戻す現状の動作は維持する
- `prev_sample` の doc に、同着時の順序と、戻したサンプルが次の `next_sample` で再度返ることを明記する
- 複数トラックで `prev_sample` → `next_sample` を往復するテストを追加する

## 完了条件

- 上記の再現で `prev_sample` が音声 1 を返す
- その直後の `next_sample` が音声 1 を返し、映像 1 を再度返さない
- 3 トラック以上が同じタイムスタンプを持つ場合も、`next_sample` の逆順で `prev_sample` が選ばれる
- `tests/test_mux_mp4_file.rs` に複数トラックの往復テストが追加されている
- `cargo test --workspace` が成功する
