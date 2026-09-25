# `default_base_is_moof = false` かつ `base_data_offset` なしのときの基準位置が ISO/IEC 14496-12:2022 の 8.8.7.1 の箇条と異なる

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-implicit-base-data-offset
- Polished: {YYYY-MM-DD}

## 目的

`tfhd` の `base-data-offset-present` フラグと `default-base-is-moof` フラグがどちらも 0 のときに、`trun` の `data_offset` の基準とする位置を決める。判断の材料は ISO/IEC 14496-12:2022 の規定と既存の実装である。決めた方針に実装と doc を合わせる。

今の実装は、2022 年版の 8.8.7.1 の箇条とは異なる規則で基準位置を決めている。一方で、同じ 2022 年版の 3.1.21 Note 1 と、確認できた既存の実装は、どれも今の規則と整合する。また doc は根拠として `trun` の節（8.8.8）を挙げていて、`tfhd` の規定がある 8.8.7.1 を指していない。

issue 0094 の対応（a82209c でマージ済み）で、`moof` より前にボックスがあるメディアセグメントを受け付けるようになった。そのため、`moof` の先頭と入力の先頭が一致しない入力が増え、規則の違いが結果に表れやすくなっている。

## 現状

### 実装

`src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_media_segment` は、両方のフラグが 0 のとき次の位置を基準にする:

- 最初の `traf`: `moof` の先頭
- 2 番目以降の `traf`: トラックを問わず、直前の `traf` のデータ末尾

doc の「# サポートする `base_data_offset` モード」は、この規則を実装どおりに書いている。ただし根拠として「ISO 14496-12 Section 8.8.8」を挙げている。

`Fmp4FileDemuxer`（`src/demux_fmp4_file.rs`）は、`moof` の先頭から切り出したデータを `Fmp4SegmentDemuxer::handle_media_segment` に渡し、返ってきた `data_offset` に `moof` のファイル上の位置を足している。このため、同じ規則で動く。`tfhd` に `base_data_offset` が明示されている場合は、ファイル先頭からの絶対オフセットに対応していないため、`read_moof_box` で拒否している（モジュール doc の「# 制限事項」）。

2 つの規則の違いの例: 入力が、前に置かれたボックス（P バイト）、`moof`（M バイト）、`mdat` のヘッダー（8 バイト）、映像データ（V バイト）、音声データの順に並び、`traf` が映像、音声の順にある場合、各 `traf` の基準位置は次のようになる。

| 規則 | 映像の `traf` の基準 | 音声の `traf` の基準 | 書き手が `trun` に書く `data_offset`（映像 / 音声） |
|---|---|---|---|
| 今の規則 | P（`moof` の先頭） | P+M+8+V（映像データの末尾） | M+8 / 0 |
| 8.8.7.1 を字義どおりに読んだ場合 | 0（入力の先頭） | 0（入力の先頭） | P+M+8 / P+M+8+V |

### 仕様（ISO/IEC 14496-12:2022）

8.8.7.1 の箇条では、両方のフラグが 0 のとき、基準は次のように決まる:

- 同じ `moof` の中で、同じトラックの 2 番目以降の `traf`: `base_data_offset` は 1 と推定され、同じトラックの直前の `traf` のデータ末尾からの相対値になる。ほかの箇条はいずれも 0 と推定しており、この「1」は誤記の可能性がある
- それ以外で、データ参照が `DataEntryImdaBox` / `DataEntrySeqNumImdaBox` の場合: 0 と推定され、対応する `imda` のペイロードの先頭からの相対値になる
- それ以外: データ参照が指すファイルからの相対値になる。この箇条には値の推定が書かれていない。「ファイルの先頭を基準にする」は解釈である

3.1.21（movie-fragment relative addressing）の Note 1 は、`default-base-is-moof` を 1 にする意味があるのは、run が 2 つ以上ある movie fragment だけだとしている。run が 1 つの movie fragment で両方のフラグが 0 のときに、基準が `moof` の先頭でなければ、この注記は成り立たない。つまり Note 1 は今の規則と整合し、8.8.7.1 の箇条とは食い違う。

手元で確認したのは 2022 年版だけで、以前の版の文面は確認していない。

### 既存の実装（一次ソースで確認）

- FFmpeg の読み込み（`libavformat/mov.c`）: 今の規則と同じ。`mov_read_moof` で `moof` の先頭を初期値にし、`mov_read_tfhd` で両方のフラグが 0 なら直前のデータ末尾（`implicit_offset`）を基準にする。`mov_read_trun` はトラックを問わず、その値を run のデータ末尾に更新する
- FFmpeg の書き込み（`libavformat/movenc.c`）: `-movflags omit_tfhd_offset`（`default_base_moof` なし）で、両方のフラグが 0 のファイルを書く。`mov_write_trun_tag` は、最初の `trun` には `moof` の先頭からの相対値を、以降の `trun` には 0 を書く（直前のトラックのデータに続く前提）。`doc/muxers.texi` は `default_base_moof` を、直前の track fragment の末尾を基準にしないためのフラグと説明している
- GPAC の読み込み（`src/isomedia/track.c` の `MergeTrack`）: 今の規則と同じ。最初の `traf` は `moof` の位置を基準にし、`traf` ごとにデータ末尾へ更新する。書き込み側は、両方のフラグが 0 のファイルを出力しない
- Bento4（`Source/C++/Core/Ap4FragmentSampleTable.cpp` の `AddTrun`）: `base_data_offset` の明示がなければ、各 `traf` の基準を `moof` の先頭にする
- Chromium（`media/formats/mp4`）: `trun` の `data_offset` を、すべて `moof` の先頭からの相対値として扱う
- W3C の MSE ISO BMFF Byte Stream Format（https://www.w3.org/TR/mse-byte-stream-format-isobmff/）: `moof` に `traf` が 1 つだけあり、その `tfhd` に `base-data-offset-present` フラグが立っていなければ、movie-fragment relative addressing（`moof` の先頭が基準）とみなす

2022 年版の 8.8.7.1 を字義どおりに実装しているもの（各トラックの最初の `traf` がファイル基準）は見つからなかった。

## 設計方針

次の 2 案のどちらかを採る。

- 案 A: 8.8.7.1 の箇条に合わせる
  - 各トラックの最初の `traf` は、`Fmp4SegmentDemuxer` では入力データの先頭を基準にする。同じトラックの 2 番目以降は、同じトラックの直前の `traf` のデータ末尾を基準にする
  - 推定値の「1」を足すか、誤記とみなして 0 とするかを決める
  - `Fmp4FileDemuxer` の扱いを決める。内部の `Fmp4SegmentDemuxer` は、渡されたデータのファイル上の位置を知らない。候補は次の 2 つ
    - `moof` のファイル上の位置を内部の処理に渡して、ファイルの先頭を基準に解く。`trun` の `data_offset` は `signed int(32)`（8.8.8.2）なので、ファイルの先頭から 2^31-1 バイトより先は指せない
    - 明示された `base_data_offset` と同じく未対応として拒否し、制限事項に追記する
  - FFmpeg の `omit_tfhd_offset` などの出力を、正しく読めなくなる。`Fmp4SegmentDemuxer` は `mdat` の末尾を超えるかしか検査しないため、エラーにならずに誤ったデータを返す
  - 2026.2.0 で追加した `default_base_is_moof = false` の `traf` への対応の挙動を変えることになる
- 案 B: 今の規則を維持する
  - 実装は変えない
  - doc の根拠を 8.8.7.1 に直す。そのうえで、8.8.7.1 の箇条とは異なること、3.1.21 Note 1 と既存の実装に整合することを書く

関連 issue:

- issue 0097: PBT で `trun` の `data_offset` を補正する対象（`default_base_is_moof` が false なら今の実装では最初の `traf` だけ）が、今の規則を前提にしている
- issue 0100: 読み飛ばす `traf` についてもデータ末尾を計算する方針が、今の規則を前提にしている。案 A では、読み飛ばす `traf` のデータ末尾はほかのトラックの基準に影響しない
- 案 A を採る場合は、0097 と 0100 の該当する記述を更新する

## 完了条件

- 両方のフラグが 0 で `base_data_offset` がないときの基準位置が、採用した案の規則どおりになる
- doc の説明が採用した規則と一致し、根拠の節番号が 8.8.7.1 になっている。8.8.7.1 の箇条との関係（準拠している、または異なる理由）が書かれている
- 案 A の場合: `Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` の基準（入力の先頭、ファイルの先頭、または未対応のエラー）が、決めたとおりになる

## 解決方法

- `src/demux_fmp4_segment.rs` の `handle_media_segment` の doc（案 A の場合は基準位置の計算も）を、採用した案に合わせる
- 案 A の場合
  - `src/demux_fmp4_file.rs` の扱いを、決めたとおりに変える
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs` の既存テストが今の規則を正としているため、書き直す
    - `rewrite_media_segment_default_base_is_moof_false`（issue 0094 で追加済み）の doc と計算
    - `leading_boxes_before_moof_are_skipped` の doc と期待値（`default_base_is_moof` が false のとき、最初の `traf` は `moof` の位置を基準にすると確認している）
  - 同じトラックの `traf` が 2 つ以上ある場合を検証するテストを追加する。`Fmp4SegmentMuxer` はトラックごとに `traf` を 1 つしか出力しないため、`moof` を書き換えて `traf` を分割する
  - `CHANGES.md` に `[CHANGE]` として記載する

## pending にした理由

- 案 A と案 B のどちらを採るかは設計判断であり、判断を保留した（2026-09-25）
- 判断の材料は「現状」に書いた、2022 年版の中の食い違い（8.8.7.1 の箇条と 3.1.21 Note 1）と、既存の実装の調査結果である
- 採用する案が決まったら reopened にする。設計方針・完了条件・解決方法を採用した案だけに書き直し、採用した理由を残す
