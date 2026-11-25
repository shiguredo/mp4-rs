# fMP4 の暗号化メディアセグメントのサンプル補助情報 (senc / saiz / saio) に対応する

- Created: 2026-11-25
- Completed: 2026-11-25
- Branch: feature/add-cenc-sample-aux-info
- Polished: 2026-11-25

## 目的

`pssh` / `sinf` / `frma` / `schm` / `schi` / `tenc` / `encv` / `enca` は初期化データ向けの CENC 対応として実装済みである。しかし、暗号化されたメディアセグメントが持つサンプル単位の暗号化情報 (初期化ベクトルと subsample の範囲) は扱えていない。

そのため現状では次の問題がある。

- 暗号化された fMP4 を `Fmp4SegmentDemuxer` で demux しても、復号に必要な IV と subsample の範囲を利用側へ渡せない
- demux したサンプルをそのまま `Fmp4SegmentMuxer` で再 mux すると、IV と subsample の情報が失われ、復号できないセグメントになる

本 issue では、CENC のサンプル補助情報 (SAI: Sample Auxiliary Information) を保持する `senc` / `saiz` / `saio` を実装し、fMP4 の mux / demux で往復できるようにする。

## 現状

- `src/boxes_cenc.rs` に `PsshBox` / `SinfBox` / `FrmaBox` / `SchmBox` / `SchiBox` / `TencBox` がある。`SinfBox` / `TencBox` は `EncvBox` / `EncaBox` に保持される。
- `TrafBox` は `unknown_boxes` を持ち、`senc` / `saiz` / `saio` はデコードすると `unknown_boxes` に入る。
- mux の `Sample` (`src/mux_mp4_file.rs`) と demux の `Sample` (`src/demux_mp4_file.rs`) はサンプル単位の暗号化情報を持たない。
- `senc` の IV サイズはボックス自体に含まれず、対応する `tenc` の `default_per_sample_iv_size` から決まる。このため `Decode` トレイトだけでは `senc` を単独でデコードできない。
- `Mp4FileMuxer` / `Mp4FileDemuxer` は非 fragmented MP4 用で、暗号化トラックの出力と、`stbl` 内の `senc` / `saiz` / `saio` の読み取りには対応していない。

## 設計方針

### 1. 対象ボックス

fMP4 の `traf` に置く次の 3 つを実装する。

- `senc` (ISO/IEC 23001-7:2023 7.2.1): CENC SAI の格納先。Container は `TrackFragmentBox` または `TrackBox`
- `saiz` (ISO/IEC 14496-12:2022 8.7.8): SAI のサイズ
- `saio` (ISO/IEC 14496-12:2022 8.7.9、fragment 内の基準は 8.8.14): SAI の位置

mux は `senc` / `saiz` / `saio` の 3 つを書き出す。`senc` だけを書くと、`saiz` / `saio` を参照する実装 (DASH-IF IOP v4.3 7.4.1 の box 構成に従う実装) が SAI を見つけられないため。

demux は `senc` があればそれを優先し、無ければ `saiz` / `saio` から SAI を読む。

### 2. `SencBox` (`senc`)

- version 0 のみ対応する。version 0 以外は `InvalidData`
- flags は `0x000002` (subsample encryption) のみ許可する。他のビットは `InvalidData`
- `samples: Vec<SencSample>` を持ち、`SencSample { iv: Vec<u8>, subsamples: Vec<Subsample> }`、`Subsample { bytes_of_clear_data: u16, bytes_of_protected_data: u32 }`
- constant IV を使うサンプルでは `iv` を空にする
- エンコードは各サンプルの `iv.len()` を書き出す。`subsamples` が 1 つでも非空なら flags に `0x000002` を立て、flag を立てた場合は subsample を持たないサンプルにも `subsample_count = 0` を書く
- IV サイズを決められない `Decode` は `Unsupported` を返す。正式な API は `SencBox::decode_with_iv_size(&[u8], u8)` とし、第 1 引数はボックスヘッダーを含むバイト列、第 2 引数は `tenc` から決まる IV サイズとする
- `TrafBox` には型付きフィールドを追加しない。`senc` は `unknown_boxes` に保持し、demuxer が `UnknownBox::payload` からヘッダーを再構築して `decode_with_iv_size` に渡す。muxer は `SencBox` をエンコードし、`UnknownBox` として `unknown_boxes` に詰める

`Decode` は実装するが、IV サイズを決められないため常に `Unsupported` を返す。この例外を `skills/shiguredo-mp4/SKILL.md` の「すべてのボックスが `Encode` / `Decode` を実装する」記述に追記する。

### 3. `SaizBox` / `SaioBox`

- `saiz`: version 0 固定。`aux_info_type` / `aux_info_type_parameter` (flags `0x000001` がある場合のみ)、`default_sample_info_size`、`sample_count`、`sample_info_sizes`
- `saio`: version 0 (32 bit offset) と version 1 (64 bit offset)。`aux_info_type` / `aux_info_type_parameter`、`offsets`
- `aux_info_type` はトラックの保護スキーム (`schm.scheme_type`) と同じ 4CC を使う。省略時は仕様どおりスキームの 4CC を既定値として扱う
- `aux_info_type_parameter` は 0 のみ対応する。0 以外は `InvalidData`
- `saio` の offset は、8.8.14 に従い `tfhd` の base data offset と同じ基準で解決する。`base_data_offset` がある場合はそれ、`default_base_is_moof` の場合は `moof` 先頭を基準にする
- `saiz` / `saio` を持つ `traf` に `base_data_offset` も `default_base_is_moof` も無い場合は `InvalidData`
- `saio` の offset が 1 個の場合は全 `trun` の SAI が連続、複数ある場合は `trun` ごとに 1 個として解決する

### 4. mux の書き出し

`Fmp4SegmentMuxer` は、暗号化されたサンプルがある `traf` に次のように書く。

- `senc`: `Sample::encryption_info` の `iv` / `subsamples` をそのまま書く
- `saiz`: `sample_info_sizes` に各サンプルの SAI 長を書く (`default_sample_info_size` は使わない)
- `saio`: version 0、`entry_count = 1`、offset は `moof` 先頭から `senc` 内の最初のサンプル SAI 先頭 (サンプル数フィールドの直後) までのバイト数

`moof` のサイズは `senc` / `saiz` / `saio` の内容に依存するため、既存の `build_media_segment_bytes` と同じ 2 パスでサイズを確定させる。

### 5. demux の読み取り

`Fmp4SegmentDemuxer` は、初期化セグメントの `tenc` の IV サイズを使い、次の順で SAI を解決する。

- `senc` があれば `decode_with_iv_size` で読む
- `senc` が無く `saiz` / `saio` があれば、SAI を `CencSampleAuxiliaryDataFormat` として読む。`sample_info_size <= Per_Sample_IV_Size` なら IV のみ、それより大きければ IV の後ろに `subsample_count` と subsample が続く
- `tenc.default_is_protected = 1` のトラック
  - SAI がある場合は上記の手順で復元する
  - SAI が無く `default_per_sample_iv_size = 0` の場合は、constant IV の full-sample 暗号化として `iv` / `subsamples` が空の情報を返す
  - SAI が無く `default_per_sample_iv_size` が 8 または 16 の場合は、per-sample IV が必要なのに SAI が無いため `InvalidData` を返す
- `tenc.default_is_protected = 0` のトラック
  - SAI が無い場合は暗号化情報なしとする
  - SAI がある場合は、既定値と異なる `isProtected` を持つサンプル (選択的暗号化) として `InvalidData` を返す

### 6. 対象外の機能を含む入力の拒否

鍵ローテーション (`sbgp` / `sgpd` の `seig`) と `senc` の version 0 以外は対象外である。対象外の機能に依存する入力を黙って解釈すると、`tenc` の既定値と異なる `isProtected` / KID / IV サイズのサンプルを誤って扱う。

- demux: `traf` に `grouping_type` が `seig` の `sbgp` または `sgpd` がある場合は `Unsupported` を返す
- demux: `senc` の version が 0 以外の場合は `InvalidData` を返す
- mux: これらの入力を作る API は提供しない

### 7. 選択的暗号化の扱い

ISO/IEC 23001-7:2023 3.1.9 は、選択的暗号化を「同じサンプル記述に対する `isProtected` の変化」と定義している。本 issue では選択的暗号化を対象外とし、次の両方向をエラーにする。

- demux: 上の「demux の読み取り」に書いた条件に従い、選択的暗号化と、per-sample IV が必要なのに SAI が無い入力を `InvalidData` にする
- mux: `tenc.default_is_protected = 1` のトラックで `encryption_info` が `None` のサンプルが 1 つでもあれば `InvalidInput`。`tenc.default_is_protected = 0` のトラックで `Some` があれば `InvalidInput`
- mux: `encryption_info` の `iv.len()` が `tenc.default_per_sample_iv_size` と一致しない場合、および `tenc` を持たないトラック (sample entry が `Encv` / `Enca` でない) に `Some` が渡された場合は `InvalidInput`

### 8. サンプル単位の暗号化情報

`src/boxes_cenc.rs` に公開型 `SampleEncryptionInfo` を追加する。

- `iv: Vec<u8>`: 初期化ベクトル。constant IV を使う場合は空
- `subsamples: Vec<Subsample>`: 空ならサンプル全体を暗号化

KID は `tenc` の既定 KID を使うため、サンプル側には持たせない (鍵ローテーションは対象外)。

mux の `Sample` と demux の `Sample` に `encryption_info: Option<SampleEncryptionInfo>` を追加する。この 2 つは公開型であり、フィールド追加は後方互換でない変更になる。

`Mp4FileMuxer` / `Mp4FileDemuxer` は非 fragmented MP4 用であり、次のように扱う。

- `Mp4FileMuxer::append_sample` に `encryption_info: Some` が渡された場合は `MuxError::EncodeError(Error::invalid_input(...))` を返す。黙って無視すると、復号できないファイルを出力する
- `Mp4FileDemuxer` は常に `None` を返す。非 fragmented の暗号化 MP4 には対応しない旨を doc に書く

### 9. 実装順序

1. `SampleEncryptionInfo` と `Sample` のフィールド追加、および全リテラルの更新 (コンパイルを通す)
2. `SencBox` / `SaizBox` / `SaioBox` の実装と PBT
3. `Fmp4SegmentMuxer` の書き出し (senc / saiz / saio)
4. `Fmp4SegmentDemuxer` の読み取りと `encryption_info` の露出
5. `Fmp4FileDemuxer` への委譲、wasm の追従、ドキュメント更新

## 完了条件

- `SencBox` / `SaizBox` / `SaioBox` / `SampleEncryptionInfo` が実装され、PBT と異常系テストが通る
- `SencBox` / `SaizBox` / `SaioBox` の実装コメントに、根拠資料名・節番号 (23001-7 7.2.1、14496-12 8.7.8 / 8.7.9 / 8.8.14)・将来変更される可能性が書かれている
- `SencBox` の `Decode` が常に `Unsupported` を返す例外が `skills/shiguredo-mp4/SKILL.md` に書かれ、`SKILL.md` の `Sample` のフィールド表に `encryption_info` が追加されている
- `senc` の version 0 以外と未知 flags、`saiz` / `saio` の `aux_info_type_parameter` が 0 以外、`saiz` / `saio` を持つ `traf` で base が無い入力が `InvalidData` になる
- `grouping_type` が `seig` の `sbgp` / `sgpd` を持つ `traf` が `Unsupported` になる
- `Fmp4SegmentMuxer` が、暗号化サンプルのある `traf` に `senc` / `saiz` / `saio` を書き出す
- `saio` の offset が、`moof` 先頭から `senc` 内の最初の SAI 先頭までのバイト数と一致する
- `Fmp4SegmentDemuxer` が `senc` から SAI を復元でき、`senc` が無く `saiz` / `saio` だけの入力でも復元できる
- `tenc.default_is_protected = 0` で SAI が無い入力が、`encryption_info: None` として復元される
- `tenc.default_is_protected = 1` かつ `default_per_sample_iv_size = 0` で `senc` / `saiz` / `saio` が無い入力が、空の `iv` / `subsamples` の暗号化情報として復元される
- `tenc.default_is_protected = 1` かつ `default_per_sample_iv_size` が 8 / 16 で SAI が無い入力が `InvalidData` になる
- 選択的暗号化の両方向 (`default_is_protected = 0` で SAI あり、`default_is_protected = 1` で `encryption_info` が `None`) の入力が、demux で `InvalidData`、mux で `InvalidInput` になる
- `tenc` の IV サイズと `encryption_info` の `iv.len()` が一致しない入力が `InvalidInput` になる
- fMP4 の mux → demux 往復で、サンプルごとの `encryption_info` が一致する
- `Mp4FileMuxer` に `encryption_info: Some` を渡すと `MuxError::EncodeError` になる
- `Mp4FileDemuxer` が常に `encryption_info: None` を返す
- `Sample` のフィールド追加に伴い、`examples/` / `tests/` / `pbt/` / `crates/wasm/` / doctest / `fuzz/` の全リテラルが更新されている
- `fuzz` で `cargo check --all-targets` が成功する
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz --no-deps` が成功する
- `CHANGES.md`、`skills/shiguredo-mp4/SKILL.md`、`issues/0071-other-position-timing-boxes-inventory.md` が更新されている

## 解決方法

- `src/boxes_cenc.rs` に `SencBox` / `SaizBox` / `SaioBox` / `Subsample` / `SampleEncryptionInfo` を追加する。各ボックスの実装コメントに、根拠 (23001-7 7.2.1、14496-12 8.7.8 / 8.7.9 / 8.8.14) と将来変更される可能性を書く
- `src/boxes.rs` の再エクスポートに新しい型を追加する
- `src/mux_fmp4_segment.rs` で `senc` / `saiz` / `saio` を書き出す。`moof` のサイズを 2 パスで確定させ、`saio` の offset を計算する
- `src/mux_fmp4_segment.rs` で選択的暗号化の入力検査を追加する
- `src/demux_fmp4_segment.rs` で `traf` の `unknown_boxes` から `senc` / `saiz` / `saio` を取り出して解決し、サンプルごとの `encryption_info` を設定する。`seig` の `sbgp` / `sgpd` と `senc` の version 0 以外を検出した場合は上記のエラーを返す
- `src/mux_mp4_file.rs` の `Sample` に `encryption_info` を追加し、`Mp4FileMuxer::append_sample` で `Some` を拒否する
- `src/demux_mp4_file.rs` のサンプル型に `encryption_info` を追加し、`Mp4FileDemuxer` は常に `None` を返す旨を doc に書く
- `src/demux_fmp4_file.rs` で暗号化情報を委譲する
- `examples/fmp4.rs`、`tests/test_mux_mp4_file.rs`、`tests/test_mux_fmp4_segment.rs`、`tests/test_demux_fmp4_segment.rs`、`tests/test_boxes_moov_tree.rs`、`pbt/tests/*`、`fuzz/fuzz_targets/*`、`crates/wasm/src/mux.rs` / `fmp4_segment_mux.rs` の `Sample` リテラルに `encryption_info: None` を追加する
- テストは `tests/test_boxes_cenc.rs` / `pbt/tests/prop_boxes_cenc.rs` / `tests/test_mux_fmp4_segment.rs` / `tests/test_demux_fmp4_segment.rs` / `pbt/tests/prop_fmp4_segment_mux_demux.rs` に追加する
- `CHANGES.md` に `[ADD]` (新しいボックと型、demux の露出) と `[CHANGE]` (`mux::Sample` と `demux::Sample` のフィールド追加、構造体リテラルに `encryption_info: None` の追加が必要、`Mp4FileMuxer` は暗号化サンプルを拒否すること) を記録する
- `skills/shiguredo-mp4/SKILL.md` に新しいボックスと `SampleEncryptionInfo` を追記し、`SencBox` の `Decode` が常に `Unsupported` を返す例外を明記する。`Sample` のフィールド表 (demux / mux 両方) に `encryption_info` を追加する
- `issues/0071-other-position-timing-boxes-inventory.md` の `saio` / `saiz` の行を実装済みに更新する

## 解決方法

- `src/boxes_cenc.rs` に `SencBox` / `SaizBox` / `SaioBox` / `Subsample` / `SampleEncryptionInfo` を追加した
- `src/boxes.rs` の再エクスポートに追加した
- `src/mux_fmp4_segment.rs` で、暗号化サンプルのある `traf` に `senc` / `saiz` / `saio` を書き出すようにした。`saio` の offset は `moof` 先頭から `senc` 内の最初の SAI 先頭までで、`moof` のレイアウトを 2 パスで確定させてから書く
- `src/demux_fmp4_segment.rs` で、`traf` の `unknown_boxes` から `senc` を優先し、無ければ `saiz` / `saio` から SAI を復元するようにした。サンプル数分の `Vec` を確保しない `TrafEncryptionInfos` を導入し、宣言値だけの巨大な `sample_count` でメモリを確保しないようにした
- 選択的暗号化、`seig` のサンプルグループ、CENC 以外の保護スキームを検出した場合のエラーを追加した
- `mux::Sample` と `demux::Sample` に `encryption_info` を追加し、`Mp4FileMuxer` は暗号化サンプルを拒否、`Mp4FileDemuxer` は常に `None` を返すようにした
- `examples/` / `tests/` / `pbt/` / `crates/wasm/` / `docs/` / `fuzz/` のリテラルを更新した
- `tests/test_boxes_cenc.rs` / `pbt/tests/prop_boxes_cenc.rs` / `tests/test_mux_fmp4_segment.rs` / `tests/test_demux_fmp4_segment.rs` / `pbt/tests/prop_fmp4_segment_mux_demux.rs` / `tests/test_mux_mp4_file.rs` にテストを追加した
- `CHANGES.md`、`skills/shiguredo-mp4/SKILL.md`、`issues/0071-other-position-timing-boxes-inventory.md` を更新した

## 対象外

- 鍵ローテーションと複数鍵 (`sbgp` / `sgpd` / `seig`、`senc` の version 1、`aux_info_type_parameter = 1`)
  - 鍵ローテーションは `sbgp` の `group_description_index` が track-level と fragment-local の 2 系統を指し分ける解決が必要で、本 issue の SAI 往復とは独立した作業になる。SAI の往復が入った後に、必要になった時点で別 issue にする
- 選択的暗号化 (暗号化トラック内にクリアサンプルを混在させる構成)
  - 本 issue では両方向を検出して拒否する
- `tenc` の FullBox flags
  - ISO/IEC 23001-7:2023/Amd 1 (AES-256 対応) で追加されるもので、未発行である。公開後に別 issue で扱う
- `enct` / `encs`
  - ISO/IEC 14496-12 8.12 で `enct` は Text、`encs` は System の保護サンプルエントリーであり、CMAF が CENC の対象とする音声・映像ではない
- トップレベル `pssh`
  - ISO/IEC 23001-7:2023 8.1.1 の Container は `moov` / `moof` / (`moov` が無い場合の) `meta` であり、トップレベルは含まれない
- `moof` 内 `pssh`
  - 仕様上は置けるが、本 issue はサンプル補助情報に限定する。必要になった時点で別 issue にする
- 非 fragmented MP4 の暗号化
  - `Mp4FileMuxer` は暗号化サンプルを拒否し、`Mp4FileDemuxer` は暗号化情報を返さない
- AES による実際の暗号化・復号
  - 鍵管理と暗号処理は DRM システムと利用側の責務である
- PIFF の `uuid` ボックス
  - 独自形式であり、本 issue の対象外とする

## 関連 issue

- `issues/closed/0161-add-cenc-init-boxes.md`: 初期化データ向けの CENC ボックス対応
- `issues/0163-add-fmp4-segment-boxes.md`: `styp` の実装。`src/mux_fmp4_segment.rs` と `src/demux_fmp4_segment.rs` を共有するため、本 issue の後に実装する
- `issues/0164-add-cmaf-segment-brand.md`: CMAF の構造ブランド。`Sample` に `encryption_info` が増えるため、本 issue の後に実装する
- `issues/0071-other-position-timing-boxes-inventory.md`: `saio` / `saiz` の棚卸し
- `issues/pending/0098-bug-fmp4-implicit-base-data-offset.md`: `tfhd` の base data offset を省略した場合の扱い。`saiz` / `saio` を持つ `traf` では 8.8.14 により base が必須なため、本 issue は 0098 の決定に依存しない
