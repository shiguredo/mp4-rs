# `Mp4FileDemuxer` に対するランダム Nav 操作列 PBT を追加する

- Created: 2026-08-19
- Completed: {YYYY-MM-DD}
- Branch: feature/add-mp4-file-demuxer-nav-pbt
- Polished: {YYYY-MM-DD}

## 目的

`Mp4FileDemuxer` の `next_sample` / `prev_sample` / `seek` の 3 API の相互作用を、真にランダムな操作列で検証する PBT を追加する。

現行のテストでは Next / Prev / Seek を混在させたランダム操作列は検証されておらず、Seek 直後の Prev などの相互作用は状態遷移の組合せ爆発 (3^N) を持つにもかかわらず網羅が薄い。

## 現状

- `pbt/tests/prop_demux.rs::prev_sample_roundtrip`: `next` を N 回 → `prev` を N 回 → `next` を N 回 の対称パターンのみ検証
- `pbt/tests/prop_demux.rs::seek_returns_sample_containing_position`: 単一 `seek` + 単一 `next` のみ、operation sequence の探索にはなっていない
- Nav API を混在させたランダム操作列を回すテストは無い

## 設計方針

### 生成する操作列

noprop の命令型クロージャで operation sequence を生成する。

- 操作列の長さ: 3-20 (境界値 3 / 5 / 20 を `sample_with_boundaries` で確保)
- 各操作は `sample_weighted_index` で 3 択:
  - `Next`: `demuxer.next_sample()`
  - `Prev`: `demuxer.prev_sample()`
  - `Seek(duration)`: `demuxer.seek(duration)` (duration は track duration の 0-125 % を境界化して生成)

### 参照実装

- 各テストケースの冒頭で demuxer から全 sample を一括取得し、`Vec<Sample>` を model として保持する
- model は「track_id ごとの current cursor (Option<usize>)」を持ち、Next / Prev / Seek の各操作で cursor を進める純関数として実装
- モックではなく、テストごとに独立してビルドし直す (`Vec<Sample>` は真の期待値集合として使う)

### 対象ファイル

- `pbt/tests/testdata/beep-aac-audio.mp4` (音声 1 track)
- `pbt/tests/testdata/black-h264-video.mp4` (映像 1 track)
- 将来 multi-track のテストファイルが用意されたら追加する

### coverage gate

`Cell<usize>` で以下 3 分岐が exercised されたことを事後検証:

1. 操作列に `Seek` が含まれたケース (シーク後の Nav 相互作用)
2. 操作列に `Prev` が含まれたケース (逆方向遷移)
3. 操作列で cursor が両端 (最初 / 最後) に到達したケース (境界)

いずれかが 0 件なら fail。

## 想定される検出対象

- Seek 直後の Prev で cursor が seek 前の位置に戻ってしまう / 戻らないの一貫性
- 境界 (最初 / 最後の sample) での cursor 消失や overshoot
- キーフレーム跨ぎでの sync_sample rewind の一貫性
- 複数 sample_entry を跨ぐ Nav でのメタデータ復元

## 対象外

- `Fmp4FileDemuxer` / `Fmp4SegmentDemuxer` への同等テスト (別 issue で扱う)
- 実 demuxer にバグが見つかった場合の修正 (発見時に別 issue で切り出す)
- 参照実装を production コードに露出させる作業 (テスト内ローカル実装のみ)

## 完了条件

- `pbt/tests/prop_demux.rs` にランダム Nav 操作列テストが追加されている
- 上記の coverage gate 3 分岐が exercised されていることが `Cell<usize>` の事後 assert で確認されている
- `cargo test -p pbt --test prop_demux` が通る
- `MP4_RS_PBT_SEED` 環境変数で失敗ケースを再現できる
