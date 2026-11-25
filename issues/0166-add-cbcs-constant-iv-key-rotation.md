# `cbcs` の constant IV を使う鍵ローテーションに対応する

- Created: 2026-11-25
- Completed: {YYYY-MM-DD}
- Branch: feature/add-cbcs-constant-iv-key-rotation
- Polished: 2026-11-25

## 目的

CENC の鍵ローテーション (`sbgp` / `sgpd` の `seig`) には対応済みだが、対象は per-sample IV を使い、`tenc` にパターン暗号化パラメーターを持たないトラック (`cenc` / `cbc1`) に限られている。

`cbcs` は constant IV (per-sample IV サイズ 0) とパターン暗号化を使うスキームであり、`seig` で KID と constant IV がサンプルグループごとに切り替わるストリームを扱えない。現状は `TrafEncryptionInfos::Constant` が全サンプルで 1 つの暗号化情報を共有し、`seig` の KID 切り替えを `Unsupported` にするため demux できず、mux 側も constant IV のトラックに `kid: Some` を渡すとエラーになる。

本 issue では、`cbcs` の constant IV トラックで `seig` による KID と constant IV の切り替えを扱い、mux / demux で往復できるようにする。

## 現状

- `src/boxes_cenc.rs` の `SeigEntry` は `is_protected` / `per_sample_iv_size` / `kid` / `constant_iv` だけで、`crypt_byte_block` / `skip_byte_block` を持たない。`SeigEntry::decode` はパターン値が 0 以外の入力を `Unsupported` にする。
- `SampleEncryptionInfo` は `kid` / `iv` / `subsamples` だけで、constant IV を持たない。
- `src/demux_fmp4_segment.rs` の `check_seig_entry_against_tenc` は `constant_iv` を持つ入力を `Unsupported` にし、`tenc` のパターン値が 0 以外の入力も `Unsupported` にする。
- `src/demux_fmp4_segment.rs` の `TrafEncryptionInfos::Constant` は全サンプルで同じ `SampleEncryptionInfo` を共有する。`apply_seig_sample_runs` は constant IV のトラックで KID が切り替わる入力を `Unsupported` にする。
- `src/mux_fmp4_segment.rs` の `validate_sample_encryption` は、`tenc.default_per_sample_iv_size` が 0 の場合と、`tenc` のパターン値が 0 以外の場合に `kid: Some` を拒否する。
- `src/mux_fmp4_segment.rs` の `build_seig_sample_groups` は KID だけでグループ化し、`sgpd` の `default_length` を 20 固定で書き、`seig` のパターン値を常に 0 / 0 で書く。このため `cbcs` の `tenc` のパターン値を上書きしてしまう。
- `src/mux_fmp4_segment.rs` の `TrackEncryption` は `tenc_box` だけを保持し、保護スキーム (`schm.scheme_type`) を持たない。
- 既存テストは、constant IV のトラックで `seig` が KID を切り替える入力と、パターン値を持つ `seig` を `Unsupported` として固定している。

## 設計方針

### 1. 対象とする構成

`cbcs` の constant IV トラック (`tenc.default_per_sample_iv_size == 0`) のうち、`seig` で KID と constant IV が切り替わる構成だけを対象とする。

- `cbcs` は `tenc` にパターン暗号化パラメーターを持つ。`seig` のパターン値は `tenc` と同じ値でなければならない (上書きは対象外)
- `seig` の `is_protected` と `Per_Sample_IV_Size` は `tenc` の既定値と一致しなければならない (上書きは対象外)
- `cbc1` の constant IV は、ISO/IEC 23001-7:2023 のスキーム定義で認められていないため対象外とし、`cbcs` 以外で constant IV を持つ `seig` は `Unsupported` にする
- `cenc` / `cens` は counter mode であり constant IV を使えないため対象外とする
- per-sample IV を使うトラックの鍵ローテーションは既に対応済みであり、本 issue の対象外とする

### 2. `SeigEntry` のパターン値

`crypt_byte_block: Uint<u8, 4>` / `skip_byte_block: Uint<u8, 4>` を追加する (`TencBox` と同じ型を使う)。

- `decode`: ワイヤの 2 バイト目を上位・下位 4 bit に分けて読む。4 bit を超える値は原理的に現れないため、範囲検査は不要
- `encode_to_vec`: `(crypt << 4) | skip` を書く。`Uint::new` は範囲を検証しないため、`check_seig_entry_for_encode` で 0x0F 超を `InvalidInput` にする (`TencBox` と同じ扱い)
- `multi_key_flag == 1` は引き続き `Unsupported`
- `constant_iv` の長さは 8 / 16 のいずれかでなければならない。`decode` では `InvalidData`、`check_seig_entry_for_encode` では `InvalidInput` を返す
- `is_protected == 1 && per_sample_iv_size == 0` のとき、`constant_iv` は必須である。`decode` で欠けていれば `InvalidData`、encode 側で欠けていれば `InvalidInput` を返す

### 3. `SampleEncryptionInfo` の constant IV

`SampleEncryptionInfo` に `constant_iv: Option<Vec<u8>>` を追加する。

- `None` は `tenc.default_constant_iv` を使うことを表す
- `Some` は `tenc` の既定 constant IV を上書きする実効値であり、長さは 8 / 16 とする
- 実効値が `tenc` の既定値と同じ場合は `None` に正規化する (`kid` と同じ規則)
- `iv` は constant IV のトラックでは空のままとする
- パターン値はサンプルごとに持たない (`seig` のパターン値は `tenc` と一致することを要求するため、mux は `tenc` から書く)

`SencBox` が使う `SencSample` は、KID と constant IV を持たない現状のままとする。

`SampleEncryptionInfo::constant_iv` の `None` は「サンプル API の実効値が `tenc` の既定値と同じ」を意味する。`sgpd` の `seig` エントリーは別の型であり、constant IV のトラックでは実効値を常に `Some` で持つ (両者を混同しない)。

### 4. demux の内部表現

`TrafEncryptionInfos::Constant` は全サンプルで同じ値しか持てないため、run ベースの表現を追加する。

- `TrafEncryptionInfos::Runs { base: SampleEncryptionInfo, runs: Vec<EncryptionRun> }` を追加する
- `EncryptionRun { sample_count: u32, info: SampleEncryptionInfo }` とする
- `base` は `sbgp` の範囲外 (index 0) のサンプルに使う情報であり、`sbgp` の `sample_count` の合計がサンプル数より少ない場合の余りを表現する
- run の数は `sbgp` のエントリー数で上限が決まる。`sbgp` のエントリー数は入力サイズに比例するため、サンプル数分の `Vec` を確保しない
- `apply_seig_sample_runs` は次のすべてを反映する
  - `TrafEncryptionInfos::Constant` で `seig` の KID または constant IV が `tenc` の既定値と異なる場合は `Runs` を構築する
  - `TrafEncryptionInfos::PerSample` の各サンプルにも、対応する run の KID と constant IV を設定する (`senc` を持つ constant IV トラックはこの経路になる)
  - すべて既定値と同じ場合は `Constant` / `PerSample` のままとする
- `handle_media_segment_inner` のサンプル構築は、`None` / `Constant` / `Runs` / `PerSample` の 4 状態を扱う

### 5. demux の検査

- `check_seig_entry_against_tenc` に `scheme_type` を渡す
- `is_protected` と `Per_Sample_IV_Size` の一致は引き続き要求する
- `seig` のパターン値が `tenc` のパターン値と異なる場合は `Unsupported`。`tenc` にパターン値が無い場合は 0 / 0 と比較する
- `seig` の `constant_iv` を持つ入力は、`cbcs` のときだけ受け付ける。それ以外のスキームでは `Unsupported`
- `kid` の切り替えは、per-sample IV かつ `tenc` にパターン値を持たないトラックと、`cbcs` の constant IV トラックだけで受け付ける。それ以外は `Unsupported`
- `seig` の `constant_iv` を持つ場合、`per_sample_iv_size` が 0 でなければ `InvalidData`

### 6. mux の書き出し

- `TrackEncryption` に `scheme_type: [u8; 4]` を追加する
- `validate_sample_encryption` は、`kid: Some` を許可する条件を次の 2 つに限定する
  - per-sample IV (8 / 16) を使い、`tenc` にパターン暗号化パラメーターを持たないトラック (既存の `cenc` / `cbc1` の対応)
  - `cbcs` の constant IV トラック (`tenc.default_per_sample_iv_size == 0`)
- 上記以外で `kid: Some` が指定された場合は `MuxError::EncodeError(Error::invalid_input(...))` を返す。これには次が含まれる
  - `cenc` / `cbc1` / `cens` の constant IV トラック
  - per-sample IV かつ `tenc` にパターン暗号化パラメーターを持つトラック (`cens`、`cbcs` の per-sample IV)
- `constant_iv` は、constant IV のトラック (`tenc.default_per_sample_iv_size == 0`) かつ `cbcs` のときだけ指定できる。それ以外は `MuxError::EncodeError(Error::invalid_input(...))`
- `constant_iv` の長さは 8 / 16 でなければならない
- `build_seig_sample_groups` は (KID, 実効 constant IV) でグループ化し、`seig` エントリーに次を書く
  - `crypt_byte_block` / `skip_byte_block`: `tenc` の値 (`tenc` にパターン値が無い場合は 0)
  - `constant_iv`: constant IV のトラックでは実効値を常に `Some` で書く (`tenc` の既定値と同じでも省略しない)。ワイヤ形式上、`per_sample_iv_size == 0` のときは必須である
- `sgpd` の `default_length` は、全エントリーの長さが同じ場合はその値、異なる場合は 0 (各エントリーに `description_length`) とする。`seig` エントリーの長さは 20 バイトの固定前置 + `constant_IV_size` の 1 バイト + constant IV (8 / 16) であり、constant IV を持つ場合は 29 / 37 になる
- `sgpd` / `sbgp` を書く条件は「いずれかのサンプルで実効 KID または実効 constant IV が `tenc` の既定値と異なる場合」とする

### 7. 実装順序

1. `SeigEntry` のパターン値と constant IV の検査、`check_seig_entry_for_encode` の更新
2. `SampleEncryptionInfo::constant_iv` の追加と全リテラルの更新 (`SampleEncryptionInfo` 38 箇所 / 5 ファイル、`SeigEntry` 13 箇所 / 4 ファイル)
3. demux の `TrafEncryptionInfos::Runs` の追加と `apply_seig_sample_runs` の更新 (`Constant` と `PerSample` の両方)
4. demux の `check_seig_entry_against_tenc` の scheme 対応
5. mux の `TrackEncryption` の scheme 保持、`validate_sample_encryption`、`build_seig_sample_groups` の更新
6. 既存の拒否系テストの置き換え、テスト追加、`CHANGES.md`、`skills/shiguredo-mp4/SKILL.md`、`issues/0071` の更新

## 完了条件

- `SeigEntry` がパターン値を `Uint<u8, 4>` で保持し、encode / decode で往復できる
- `seig` の `constant_iv` の長さが 8 / 16 以外の入力が、decode で `InvalidData`、encode で `InvalidInput` になる
- `is_protected == 1 && per_sample_iv_size == 0` で `constant_iv` が欠けている入力が `InvalidData` になる
- `SampleEncryptionInfo` に `constant_iv` が追加され、`tenc` の既定値と同じ場合は `None` に正規化される
- `cbcs` の constant IV トラックで、`seig` により KID と constant IV が切り替わるセグメントを demux でき、サンプルごとに実効 KID と constant IV が設定される
- `senc` を持つ `cbcs` の constant IV トラックでも、サンプルごとの実効 KID と constant IV が設定される (`PerSample` 経路)
- `sbgp` の `sample_count` の合計がサンプル数より少ない場合、余りのサンプルが `tenc` の既定値 (KID と constant IV) になる
- `seig` の constant IV を持つ入力が、`cbcs` 以外のスキームで `Unsupported` になる
- `cenc` / `cbc1` / `cens` の constant IV トラックで `seig` の KID が切り替わる入力が `Unsupported` になる
- per-sample IV かつ `tenc` にパターン値を持つトラックで `seig` の KID が切り替わる入力が `Unsupported` になる
- `seig` のパターン値が `tenc` と異なる入力が `Unsupported` になる
- `seig` の `is_protected` / `Per_Sample_IV_Size` が `tenc` と異なる入力は `Unsupported` のまま
- per-sample IV のトラックの鍵ローテーション (既存の対応) の挙動が変わらない (`cens` は引き続き対象外)
- mux が、KID または constant IV が切り替わる `cbcs` の constant IV トラックに対して、`tenc` と同じパターン値と実効 constant IV を持つ `sgpd` / `sbgp` を書き出す
- mux の出力を demux すると、サンプルごとの実効 KID と constant IV が一致する
- per-sample IV のトラックで `constant_iv` を指定すると `MuxError::EncodeError` になる
- `cenc` / `cbc1` / `cens` の constant IV トラックで `kid: Some` を指定すると `MuxError::EncodeError` になる
- per-sample IV かつ `tenc` にパターン値を持つトラックで `kid: Some` を指定すると `MuxError::EncodeError` になる
- `cbcs` 以外のスキームで `constant_iv` を指定すると `MuxError::EncodeError` になる
- `constant_iv` の長さが 8 / 16 以外の指定が `MuxError::EncodeError` になる
- `sgpd` の `default_length` が固定長と可変長の両方で正しく書ける
- 既存の拒否系テスト (`seig_constant_iv_is_unsupported` / `muxer_rejects_kid_rotation_with_pattern_tenc` / `muxer_rejects_kid_rotation_with_constant_iv`) が、新しい正常系と対象外の異常系に置き換えられている。`seig_pattern_override_is_unsupported` は期待値を更新して残す
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz --no-deps` が成功する
- `CHANGES.md`、`skills/shiguredo-mp4/SKILL.md`、`issues/0071-other-position-timing-boxes-inventory.md` が更新されている

## 解決方法

- `src/boxes_cenc.rs` の `SeigEntry` に `crypt_byte_block` / `skip_byte_block` (`Uint<u8, 4>`) を追加し、`decode` / `encode_to_vec` / `check_seig_entry_for_encode` を更新する。constant IV の長さと必須検査も追加する。実装コメントに根拠 (ISO/IEC 23001-7:2023 6、`tenc` は同 8.2) と、パターン値の意味が将来の改訂で変わる可能性を書く
- `src/boxes_cenc.rs` の `SampleEncryptionInfo` に `constant_iv` を追加する
- `src/demux_fmp4_segment.rs` に `TrafEncryptionInfos::Runs` と `EncryptionRun` を追加し、`apply_seig_sample_runs` (`Constant` と `PerSample` の両方) とサンプル構築を更新する
- `src/demux_fmp4_segment.rs` の `check_seig_entry_against_tenc` に `scheme_type` を渡し、パターン値の一致検査と constant IV の `cbcs` 限定検査を追加する
- `src/mux_fmp4_segment.rs` の `TrackEncryption` に `scheme_type` を追加し、`validate_sample_encryption` を新しい条件に更新する
- `src/mux_fmp4_segment.rs` の `build_seig_sample_groups` を (KID, 実効 constant IV) の組でグループ化するように変更し、パターン値と constant IV を持つ `sgpd` を書く。`default_length` の決定規則も実装する
- `SampleEncryptionInfo` の全リテラル (38 箇所 / 5 ファイル) と `SeigEntry` の全リテラル (13 箇所 / 4 ファイル) を更新する
- `pbt/tests/prop_boxes_cenc.rs` の `arb_seig_entry` にパターン値と constant IV を、`arb_fixed_length_seig_entry` にパターン値を追加する (固定長の 20 バイトには constant IV を追加できない)
- テストは `tests/test_boxes_cenc.rs` / `pbt/tests/prop_boxes_cenc.rs` / `tests/test_demux_fmp4_segment.rs` / `tests/test_mux_fmp4_segment.rs` / `pbt/tests/prop_fmp4_segment_mux_demux.rs` に追加する
- `CHANGES.md` の同じ開発サイクルの `[ADD]` エントリーを更新する
- `skills/shiguredo-mp4/SKILL.md` の `seig` の非対応記述 (constant IV / パターン値)、mux の `Sample` の `encryption_info` の説明、demux の `Sample` の `encryption_info` の説明、`SampleEncryptionInfo` のフィールド一覧、`SeigEntry` の説明を更新する
- `issues/0071-other-position-timing-boxes-inventory.md` の `sbgp` / `sgpd` の行を、constant IV とパターン値に対応した旨に更新する

## 対象外

- `seig` によるパターン値の上書き
  - `seig` のパターン値は `tenc` と一致することを要求する。パターンがグループごとに切り替わる構成は、必要になった時点で別 issue にする
- `seig` による `is_protected` の上書き (選択的暗号化)
- `seig` による `Per_Sample_IV_Size` の上書き
  - `senc` の version 0 はトラック内で IV サイズが一定であることを前提とするため、上書きがあると `senc` では表現できない
- `cbcs` 以外のスキームの constant IV
  - `cbc1` / `cenc` / `cens` のスキーム定義では constant IV が認められていないため、`seig` の constant IV を持つ入力は `Unsupported` にする
- per-sample IV のトラックの鍵ローテーション (既に対応済み) と `cens` の鍵ローテーション
- 複数鍵 (`multi_key_flag == 1`、`senc` の version 1、`aux_info_type_parameter = 1` の複数 IV 形式)
- `sgpd` の version 2 (`default_group_description_index`)
- track-level の `seig` `sbgp`
- `seig` 以外のサンプルグループ種別の意味の解釈

## 関連 issue

- `issues/closed/0165-add-cenc-key-rotation.md`: per-sample IV の鍵ローテーション。本 issue はその対象外だった constant IV を追加する
- `issues/closed/0162-add-cenc-sample-aux-info.md`: `senc` / `saiz` / `saio` と `SampleEncryptionInfo`
- `issues/closed/0161-add-cenc-init-boxes.md`: `tenc` などの初期化データ向けの CENC ボックス
