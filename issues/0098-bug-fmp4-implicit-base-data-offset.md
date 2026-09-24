# `default_base_is_moof = false` かつ `base_data_offset` なしのときの基準位置が ISO/IEC 14496-12:2022 と異なる

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-implicit-base-data-offset
- Polished: {YYYY-MM-DD}

## 目的

`tfhd` の `base-data-offset-present` フラグと `default-base-is-moof` フラグがどちらも 0 のときに `trun` の `data_offset` の基準とする位置を、ISO/IEC 14496-12:2022 の規定と照らし合わせる。そのうえで、実装と doc を一致させる。

今の実装は 2022 年版とは異なる規則で基準位置を決めている。doc も根拠として `trun` の節（8.8.8）を挙げていて、`tfhd` の規定がある 8.8.7.1 を指していない。issue 0094 の対応で `moof` より前にボックスがあるメディアセグメントを受け付けるようになるため、`moof` の先頭と入力の先頭が一致しない入力が増え、この違いが結果に表れやすくなる。

## 現状

`src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment` は、両方のフラグが 0 のとき次の位置を基準にする:

- 最初の `traf`: `moof` の先頭
- 2 番目以降の `traf`: トラックを問わず、直前の `traf` のデータ末尾

doc の「# サポートする `base_data_offset` モード」はこの規則を書き、根拠として「ISO 14496-12 Section 8.8.8」を挙げている。

ISO/IEC 14496-12:2022 の 8.8.7.1 では、両方のフラグが 0 のとき、基準は次のように決まる:

- 同じ `moof` の中で、同じトラックの 2 番目以降の `traf`: 同じトラックの直前の `traf` のデータ末尾
- それ以外（各トラックの最初の `traf`）: データ参照が指すファイル（`DataEntryImdaBox` / `DataEntrySeqNumImdaBox` の場合は対応する `imda` のペイロードの先頭）

`Fmp4FileDemuxer` は `moof` の先頭から切り出したデータを `Fmp4SegmentDemuxer::handle_media_segment` に渡し、返ってきた `data_offset` に `moof` のファイル上の位置を足している。このため、同じ規則で動く。

## 設計方針

次の 2 案のどちらにするかを決めてから実装する。

- 案 A: 2022 年版の規定に合わせる
  - 各トラックの最初の `traf` は、`Fmp4SegmentDemuxer` では入力データの先頭、`Fmp4FileDemuxer` ではファイルの先頭を基準にする
  - 2 番目以降は、同じトラックの直前の `traf` のデータ末尾を基準にする
- 案 B: 今の規則を維持する
  - 規則がどの版の規定、またはどの既存実装に合わせたものかを確認して doc に書く
  - 2022 年版と異なることも doc に明記する

どちらの案でも、doc の節番号は `tfhd` の規定がある 8.8.7.1 に直す。判断の材料として、両方のフラグが 0 で `base_data_offset` を省略したファイルを出力する既存の実装と、そのファイルを読む既存の実装がどちらの規則に従っているかを確認する。

## 完了条件

- 両方のフラグが 0 で `base_data_offset` がないときの基準位置が、採用した案の規則どおりになる
- doc の説明と節番号が実装と一致する
- `Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` で規則が一致する

## 解決方法

- `src/demux_fmp4_segment.rs` の `handle_media_segment` の基準位置の計算と doc を、採用した案に合わせる
- 案 A の場合、`src/demux_fmp4_file.rs` の `data_offset` の変換も合わせて確認する
- テスト
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: 両方のフラグが 0 のメディアセグメントを、トラックが複数あり `traf` の並び順が異なる場合も含めて検証する（issue 0094 の対応で追加する `rewrite_media_segment_default_base_is_moof_false` を拡張する）
- 案 A の場合は挙動が変わるため、`CHANGES.md` に `[FIX]` または `[CHANGE]` として記載する
