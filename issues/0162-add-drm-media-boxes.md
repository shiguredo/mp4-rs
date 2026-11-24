# DRM のメディア向けボックス (senc / saiz / saio / sbgp / sgpd / seig) に対応する

- Created: 2026-11-25
- Completed: {YYYY-MM-DD}
- Branch: feature/add-drm-media-boxes
- Polished: {YYYY-MM-DD}

## 目的

`pssh` / `sinf` / `frma` / `schm` / `schi` / `tenc` / `encv` / `enca` は初期化データ向けの CENC 対応として実装済みである。本 issue では、暗号化されたメディアセグメントを生成・解析・再構築するために必要な残りの DRM / CENC ボックスを実装する。

対象:

- `senc` (Sample Encryption Box, ISO/IEC 23001-7 7.2)
- `saiz` / `saio` (Sample Auxiliary Information Sizes / Offsets, ISO/IEC 14496-12 8.7.8 / 8.7.9)
- `sbgp` / `sgpd` (Sample To Group / Sample Group Description, ISO/IEC 14496-12 8.9.2 / 8.9.3)
- `seig` (CencSampleEncryptionInformationGroupEntry, ISO/IEC 23001-7)
- `tenc` の flags (ISO/IEC 23001-7 Amd 1 の subsample / 複数鍵 / senc 使用フラグ)
- `pssh` のトップレベル / `moof` 配置
- `enct` / `encs` (保護された字幕サンプルエントリー。必要なら)
- fMP4 の `traf` と非 fragmented MP4 の `stbl` の両方
- mux / demux でサンプルごとの暗号化情報を扱う API

## 現状

- `src/boxes_cenc.rs` に `PsshBox` / `SinfBox` / `FrmaBox` / `SchmBox` / `SchiBox` / `TencBox` がある。`PsshBox` は `MoovBox` に、`SinfBox` / `TencBox` は `EncvBox` / `EncaBox` に保持される。
- `TrafBox` / `StblBox` は `senc` / `saiz` / `saio` / `sbgp` / `sgpd` を型付きで持たず、未知ボックスとして `unknown_boxes` に保持する。
- `TencBox` は flags を常に 0 として扱う。
- `Fmp4SegmentMuxer` / `Mp4FileMuxer` の `Sample` はサンプルごとの IV / subsample / 鍵情報を持たない。`Fmp4SegmentDemuxer` / `Mp4FileDemuxer` も `senc` / `saiz` / `saio` / `seig` を利用側に露出しない。
- `pssh` は `MoovBox` 内だけを型付きで扱い、トップレベルや `moof` 内の `pssh` は `UnknownBox` に落ちる。
- `senc` の IV サイズはボックス自体に含まれず、対応する `tenc` または `seig` から決まる。このため、`Decode` トレイトだけでは `senc` を単独でデコードできない。

## 設計方針

### 1. ボックス

- `SencBox` (ISO/IEC 23001-7 7.2)
  - version 0、flags の `0x000002` を subsample encryption として扱う
  - `sample_count` と `samples: Vec<SencSample>` を持つ
  - `SencSample { iv: Vec<u8>, subsamples: Vec<Subsample> }`、`Subsample { bytes_of_clear_data: u16, bytes_of_protected_data: u32 }`
  - IV サイズは `tenc` / `seig` から渡す。`SencBox::decode_with_iv_size(buf, iv_size)` と、`iv_size` を保持した `Encode` を用意する。`Decode` トレイト単独では IV サイズを決められないため、`Decode` 実装は `Unsupported` を返すか、コンテキスト付きのデコード API に寄せる
  - `traf` / `stbl` のデコードでは、`tenc` / `seig` を読んだ後に demuxer が `SencBox::decode_with_iv_size` で型付き化する。`TrafBox` / `StblBox` は `senc` を `unknown_boxes` に保持する
- `SaizBox` / `SaioBox` (ISO/IEC 14496-12 8.7.8 / 8.7.9)
  - `saiz`: `aux_info_type` / `aux_info_type_parameter` (flags 0x000001)、`default_sample_info_size`、`sample_count`、`sample_info_sizes`
  - `saio`: `aux_info_type` / `aux_info_type_parameter` (flags 0x000001)、version 0 は 32-bit offset、version 1 は 64-bit offset
  - CENC の `aux_info_type` は `cenc`
- `SbgpBox` / `SgpdBox` (ISO/IEC 14496-12 8.9.2 / 8.9.3)
  - `sbgp`: `grouping_type`、version 1 の `grouping_type_parameter`、`sample_count` / `group_description_index` の列
  - `sgpd`: `grouping_type`、version 1 以降の `default_length`、version 2 の `default_sample_description_index`、エントリー列
  - `sgpd` のエントリーペイロードは grouping_type ごとに異なるため、未知の grouping_type は生バイトで保持する
  - `seig` は `CencSampleEncryptionInformationGroupEntry` として型付きで実装する。`tenc` と同様に `isProtected` / `Per_Sample_IV_Size` / `KID` / constant IV / pattern encryption を持つ
- `TencBox` の flags
  - `use_subsample_encryption` (`0x000003`)、`use_multi_key` (`0x00000C`)、`use_senc` (`0x000030`) を flags として保持する
  - 2 bit の値が予約値 (`3`) の場合はエラーにする
- `PsshBox` の配置
  - `RootBox` にトップレベル `pssh` を追加する
  - `MoofBox` に `pssh_boxes` を追加する
- `enct` / `encs`
  - 字幕の保護サンプルエントリーが必要な場合は `EnctBox` / `EncsBox` を追加し、`stpp` / `wvtt` / `tx3g` に対応する
  - 今回の主目的は映像・音声の CENC なので、字幕保護は独立した実装単位にしてよい

### 2. mux / demux

- サンプルごとの暗号化情報を表す公開型を追加する
  - 例: `SampleEncryptionInfo { iv: Vec<u8>, subsamples: Vec<(u16, u32)> }`
  - `key_id` / `constant_iv` は `tenc` / `seig` 側で表現し、サンプル側は IV と subsample を持つ
- `Fmp4SegmentMuxer` / `Mp4FileMuxer` で `senc` / `saiz` / `saio` を生成する
  - `senc` の IV サイズは `tenc` / `seig` から決める
  - `saiz` はサンプルごとの補助情報サイズ、`saio` は `moof` 基準 / ファイル基準の offsets を計算する
  - 既存の `Sample` 構造体にフィールドを足すか、暗号化情報付きのサンプルを渡す新しい API を追加する。どちらも公開 API の破壊的変更になるため `CHANGES.md` に `[CHANGE]` として記録する
- `Fmp4SegmentDemuxer` / `Mp4FileDemuxer` で `senc` / `saiz` / `saio` を復号し、サンプルごとの暗号化情報を返す
  - `tenc` / `seig` の IV サイズを使って `senc` をデコードする
  - `saio` の offset を `moof` / ファイル先頭からの相対位置として解決する
  - `saiz` と `saio` が複数ある場合は `aux_info_type` / `aux_info_type_parameter` で CENC 用を選ぶ
- `sbgp` / `sgpd` / `seig` を `traf` / `stbl` に統合する
  - フラグメント内の `sgpd` / `sbgp` を保持し、サンプルが属するグループを解決する
  - 鍵ローテーションでは、グループごとに KID / IV / pattern が異なるため、demuxer はサンプルごとの実効的な暗号化パラメーターを返す
  - muxer は複数の鍵グループを指定できる API を追加する
- 非 fragmented MP4 の `stbl` にも同じボックスを書き出し・読み出しできるようにする

### 3. テスト

- 各ボックスの encode / decode の PBT と、境界値・異常系のテスト
- `senc` の subsample あり / なし、IV サイズ 0 / 8 / 16、version / flags 不正
- `saiz` / `saio` の offset 解決、version 0 / 1、`aux_info_type` あり / なし
- `sbgp` / `sgpd` / `seig` の version ごとの往復、鍵ローテーションのグループ解決
- fMP4 と非 fragmented MP4 の mux / demux 往復
- `tenc` flags の 2 bit 値の検証

## 完了条件

- `SencBox` / `SaizBox` / `SaioBox` / `SbgpBox` / `SgpdBox` / `SeigEntry` の encode / decode が実装され、PBT と異常系テストが通る
- `TencBox` の flags と `PsshBox` のトップレベル / `moof` 配置に対応する
- `Fmp4SegmentMuxer` / `Mp4FileMuxer` が `senc` / `saiz` / `saio` を書き出せる
- `Fmp4SegmentDemuxer` / `Mp4FileDemuxer` がサンプルごとの暗号化情報を返せる
- `sbgp` / `sgpd` / `seig` による鍵ローテーションを mux / demux で扱える
- `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz --no-deps` が成功する
- `CHANGES.md` と `skills/shiguredo-mp4/SKILL.md` が更新されている

## 解決方法

- `src/boxes_cenc.rs` に `SencBox` / `SaizBox` / `SaioBox` / `SbgpBox` / `SgpdBox` / `SeigEntry` を追加する
- `src/boxes_fmp4.rs` の `TrafBox` と `src/boxes_moov_tree.rs` の `StblBox` に、必要な `senc` / `saiz` / `saio` / `sbgp` / `sgpd` の保持先を追加する
- `src/boxes_moov_tree.rs` の `TencBox` に flags を追加する
- `src/boxes.rs` / `src/lib.rs` に新しい型を公開する
- `src/mux_fmp4_segment.rs` / `src/mux_mp4_file.rs` にサンプル暗号化情報の API と `senc` / `saiz` / `saio` の書き出しを追加する
- `src/demux_fmp4_segment.rs` / `src/demux_mp4_file.rs` に `tenc` / `seig` を使った `senc` のデコードと offsets 解決を追加する
- `Fmp4FileDemuxer` にも同じ情報を露出する
- テストは `tests/test_boxes_cenc.rs` / `pbt/tests/prop_boxes_cenc.rs` / `tests/test_mux_fmp4_segment.rs` / `tests/test_demux_fmp4_segment.rs` / `tests/test_mux_mp4_file.rs` / `tests/test_demux_mp4_file.rs` に追加する
- `CHANGES.md` と `skills/shiguredo-mp4/SKILL.md` を更新する

## 対象外

- AES による実際の暗号化・復号 (このライブラリの責務ではない)
- AES-256 の追加仕様 (必要になった時点で別 issue)
- PIFF の `uuid` ボックス (`uuid` 形式の `tenc` / `senc` / `pssh`)
- DRM システム固有の `pssh` データの解釈
- `prft` など DRM と無関係なボックス

## 関連 issue

- `issues/closed/0161-add-cenc-init-boxes.md`: 初期化データ向けの CENC ボックス対応
- `issues/0071-other-position-timing-boxes-inventory.md`: `saio` / `saiz` の棚卸し
