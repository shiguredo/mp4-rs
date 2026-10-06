# `Fmp4FileDemuxer` の partial input / 中断再開シーケンス PBT を追加する

- Created: 2026-08-19
- Completed: 2026-10-06
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

## 解決方法

本 issue は対応不要として closed にする。理由は、本 issue 自身が「実装着手前に一次調査で API 契約を確認する」と明記していた調査の結果、前提である「partial input / 中断再開をバッファリングできる」が現行の `Fmp4FileDemuxer` の API 契約に存在しないことが確定したためである。

### API 契約の調査結果

- `src/demux_fmp4_file.rs` の `Fmp4FileDemuxer` には入力データのバッファリング機構が無い。struct のフィールドは `phase` / `inner` / `track_infos` / `track_runtimes` / `pending_samples` / `handle_input_error` のみで、`pending_samples` はデマルチプレックス結果のサンプルであり入力の蓄積ではない
- `handle_input` は、`required_input()` が要求する位置を含む入力を受け取り、その 1 回の呼び出しで処理を進める
- `available_bytes` は、入力が要求サイズに満たない場合にバッファリングせず、入力の終端をファイルの終端とみなして `InvalidData` の `DecodeError`（"input ended before the required range was available"）を返す。部分供給を後から補うための状態は何も残らない
- `input_is_acceptable` は、入力が要求位置を含まない場合（要求されていない range の先出しなど）に `InvalidInput` の `DecodeError` を返す

### 実測結果（2026-10-06、develop の現行実装で確認）

`Fmp4FileDemuxer::new()` 直後（`required_input()` が position=0, size=32 を要求する状態）で、要求範囲を分割して渡す実験をした。

- 要求サイズ 32 バイトのうち 8 バイトだけ渡す `handle_input` は `InvalidData` の `DecodeError` になった
- エラー後に残りの 24 バイトを position=8 から渡す `handle_input` は `InvalidInput` の `DecodeError` になった。最初の 8 バイトは保持されておらず、部分供給の続きを渡す方法は無い
- 同じ要求範囲の全量（0..32）を渡し直せば処理は進むが、これは「要求ごとの全量供給」であり、保留・中断再開ではない

### 既知の記録

`issues/closed/0099-bug-fmp4-file-demuxer-whole-file-input.md`（2026-09-25）の「関連 issue」に次のとおり既に記録されていた。

> issue 0074 は、要求より少ない入力を渡す PBT を扱う。今の実装は要求位置から始まる短い入力をファイルの終端とみなしており、この issue はその扱いを広げる。0074 の前提（短い入力を後から補える）とは今も食い違っており、0074 側で API 契約を確認する必要がある

今回の調査で、API 契約は「短い入力 = ファイル終端の合図」であり「後から補えない」ことが確定した。

### 既存のカバレッジと本 issue の操作列

- `pbt/tests/prop_fmp4_segment_mux_demux.rs` の `fmp4_file_demuxer_roundtrip` と `fmp4_file_demuxer_accepts_whole_file_input` が、本 issue の目的のうち実現可能な範囲を既に検証している。後者は、要求どおりの供給と位置 0 からファイル全体の供給で結果が一致すること、途中で切れた入力のエラー一致、`mdat` size=0、`moof` と `mdat` の間に読み飛ばすボックスがあるファイルを含む
- 本 issue の操作列（`SupplyPartial` / `SupplyExtraRange` / `SupplyExact`）のうち、`SupplyExact` は既存の `feed_fmp4_file_demuxer` と同じ操作であり、`SupplyPartial` と `SupplyExtraRange` は現行 API では常にエラーになる操作で検証対象にならない

### その後の方針

部分入力のバッファリングを `Fmp4FileDemuxer` に追加する場合は、API 変更を伴う新規の機能 issue として切り出す必要がある。`Fmp4FileDemuxer` は「完全な fMP4 ファイルを段階的に読む」設計で、部分入力の意味論はファイル終端の合図として定義されているため、機能追加の妥当性の判断を含めて別途検討する。
