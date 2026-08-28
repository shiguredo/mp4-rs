# `Fmp4FileDemuxer` の partial input / 中断再開シーケンス PBT を追加する

- Created: 2026-08-19
- Completed: {YYYY-MM-DD}
- Branch: feature/add-fmp4-file-demuxer-partial-input-pbt
- Polished: {YYYY-MM-DD}

## 目的

`Fmp4FileDemuxer` の `required_input()` / `handle_input()` バッファリング機構を、partial input と中断再開のランダム操作列で検証する PBT を追加する。

現状は「要求された range を丸ごと渡す」パターンのみが検証されており、部分供給や順序入れ替えといった実運用でありがちな入力パターン (ネットワークストリーミング / ファイル chunk 読み出し) に対するバッファリング一貫性の検証が薄い。

## 現状

- `pbt/tests/prop_fmp4_segment_mux_demux.rs::feed_fmp4_file_demuxer`: `required_input()` の要求分をそのまま渡す実装
- `pbt/tests/prop_fmp4_segment_mux_demux.rs::fmp4_file_demuxer_roundtrip`: 単純な loop で全量渡し
- partial supply や順序入れ替えの検証は存在しない

## 設計方針

### 生成する操作列

- 操作列の長さ: 5-30
- 各操作は `sample_weighted_index` で 3 択:
  - `SupplyPartial { fraction }`: 要求されたサイズの `fraction` (0-100 %) だけ渡す
  - `SupplyExtraRange { start_offset, length }`: 要求位置以外の任意 range を先出しで渡す (バッファに残せることを検証)
  - `SupplyExact`: 要求どおり渡す (baseline)
- 最終的にすべての byte を供給しきったら next_sample の全 sample が期待通り取得できることを検証

### 参照実装

- baseline として `feed_fmp4_file_demuxer` (要求どおり全量渡す実装) と結果を並走比較
- 両者の (track_id, timestamp, duration, data_offset, data_size, sample_entry の Some/None) の一致を assert
- モックではなく、同じ実装の呼び方の違いを比較する

### coverage gate

`Cell<usize>` で以下が exercised されたことを事後検証:

1. `SupplyPartial` (fraction < 100 %) を含むケース
2. `SupplyExtraRange` を含むケース
3. 部分供給が 3 回以上連続したケース (バッファ蓄積の深さ)

## 想定される検出対象

- 部分供給後の `required_input()` の再計算バグ
- 順序を入れ替えて渡した際のバッファリング状態不整合
- 中断後の再要求で戻り値がずれる回帰
- multi-segment 境界での cursor 状態

## 実装コストの見積もり

`fraction` や `start_offset` の妥当な生成、`SupplyExtraRange` のバッファ蓄積が実装依存で受理されるかは事前検証が必要 (仕様上「要求外の range を送ったらエラー」なのか「バッファに残す」なのかで方針が変わる)。実装着手前に一次調査で API 契約を確認する。

## 対象外

- `Mp4FileDemuxer` / `Fmp4SegmentDemuxer` への同等テスト (別 issue)
- 実 demuxer にバグが見つかった場合の修正 (発見時に別 issue で切り出す)
- ネットワーク由来の遅延・エラー系のシミュレーション

## 完了条件

- `pbt/tests/prop_fmp4_segment_mux_demux.rs` または新規ファイルに partial input テストが追加されている
- baseline 実装との一致検証が行われている
- coverage gate が exercised されていることが `Cell<usize>` の事後 assert で確認されている
- `cargo test -p pbt` が通る
- `MP4_RS_PBT_SEED` 環境変数で失敗ケースを再現できる
