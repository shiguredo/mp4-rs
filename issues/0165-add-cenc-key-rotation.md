# CENC の鍵ローテーション (`sbgp` / `sgpd` / `seig`) に対応する

- Created: 2026-11-25
- Completed: {YYYY-MM-DD}
- Branch: feature/add-cenc-key-rotation
- Polished: 2026-11-25

## 目的

fMP4 のサンプル補助情報 (`senc` / `saiz` / `saio`) には対応済みだが、鍵ローテーション (サンプルグループごとに KID を切り替える構成) には対応していない。

現状、`traf` に `seig` のサンプルグループがあると `Fmp4SegmentDemuxer` が `Unsupported` を返すため、鍵ローテーションを使う CMAF / DASH のストリームを demux できない。mux 側も `tenc` の既定 KID しか書けず、鍵が切り替わるセグメントを生成できない。

本 issue では、`sbgp` / `sgpd` の `seig` サンプルグループを扱い、サンプルごとの実効 KID を mux / demux の両方で往復できるようにする。

## 現状

- `src/boxes_cenc.rs` に `TencBox` / `SencBox` / `SaizBox` / `SaioBox` / `Subsample` / `SampleEncryptionInfo` がある。`SencBox::samples` の要素型は `SampleEncryptionInfo` である。
- `SampleEncryptionInfo` は `iv` と `subsamples` だけを持ち、KID を持たない。KID は `tenc.default_kid` に固定されている。
- `src/demux_fmp4_segment.rs` の `has_seig_sample_group` が `traf` の `sbgp` / `sgpd` に `grouping_type` が `seig` のものがあるかを調べ、あれば `Error::unsupported` を返す。`StblBox` 側の `seig` は検査しておらず、黙って無視される。
- `StblBox` と `TrafBox` は `unknown_boxes` を持ち、`sbgp` / `sgpd` は生のペイロードのまま保持されている。
- `Fmp4SegmentDemuxer` の `TrackRuntime` は `sample_entries` / `trex` / `current_sample_description_index` を持ち、トラック単位のサンプルグループを保持する場所がない。
- `SbgpBox` / `SgpdBox` / `SgpdEntry` / `SeigEntry` は未実装である。

## 設計方針

### 1. 対象とする構成

次の構成だけを対象とする。

- per-sample IV を使うトラック (`tenc.default_per_sample_iv_size` が 8 または 16)
- `tenc` にパターン暗号化パラメーター (`default_crypt_byte_block` / `default_skip_byte_block`) を持たないトラック (`cenc` / `cbc1`)
- `seig` サンプルグループで **KID だけが切り替わる**構成
- `senc` の version 0 (sample_count がサンプル数と一致し、IV サイズがトラック内で一定)

`seig` は仕様上 `is_protected` / IV サイズ / constant IV / パターン暗号化パラメーターも上書きできるが、本 issue では KID 以外の上書きを検出したらエラーにする (後述)。

`cens` / `cbcs` の `tenc` はパターン値が必須であり、`seig` に 0 を書くと `tenc` のパターンを上書きして復号結果が変わる。そのため、パターン値を持つトラックは mux で `kid: Some` を拒否し、demux でも `seig` のパターン値が `tenc` と異なる場合は `Unsupported` とする。

### 2. `SeigEntry` (`seig` エントリー)

ISO/IEC 23001-7:2023 6 の `CencSampleEncryptionInformationGroupEntry` のうち、`multi_key_flag == 0` の形式だけを扱う。ワイヤ形式は次の 20 バイトの固定前置に、条件付きで constant IV が続く。

| フィールド | サイズ | 内容 |
|---|---|---|
| `multi_key_flag` | 1 bit | 0 のみ対応 |
| reserved | 7 bit | 0 |
| `crypt_byte_block` | 4 bit | 0 のみ対応 |
| `skip_byte_block` | 4 bit | 0 のみ対応 |
| `is_protected` | 8 bit | 0 または 1 |
| `Per_Sample_IV_Size` | 8 bit | 0 / 8 / 16 |
| `KID` | 16 バイト | |
| `constant_IV_size` | 8 bit | `is_protected == 1 && Per_Sample_IV_Size == 0` のときだけ |
| `constant_IV` | `constant_IV_size` バイト | 同上 |

- `multi_key_flag == 1` は `Unsupported`
- `crypt_byte_block` / `skip_byte_block` が 0 以外は `Unsupported` (パターン暗号化の上書きは対象外)。`tenc` にパターン値が無いトラックでは、0 / 0 を「パターン無し」とみなす
- `is_protected` が 0 / 1 以外、`Per_Sample_IV_Size` が 0 / 8 / 16 以外は `InvalidData`
- 定義済みフィールドの後ろに追加バイトがあっても無視し、消費バイト数を返す (仕様上、将来の拡張を読み飛ばす必要がある)
- `SgpdBox` のエントリー長は外側から与えられるため、`Encode` / `Decode` ではなく `SeigEntry::decode(payload) -> Result<(Self, usize)>` と `SeigEntry::encode_to_vec() -> Vec<u8>` を用意する

### 3. `SbgpBox` / `SgpdBox`

`SbgpBox` (ISO/IEC 14496-12:2022 8.9.2)

- `version: u8` (0 / 1)、`grouping_type: [u8; 4]`、`grouping_type_parameter: Option<u32>` (version 1 のみ)、`entries: Vec<SbgpEntry>`
- `SbgpEntry { sample_count: u32, group_description_index: u32 }`
- version 0 / 1 の両方を encode / decode できるようにする

`SgpdBox` (8.9.3)

- `version: u8`、`grouping_type: [u8; 4]`、`default_length: Option<u32>` (version 1 のみ)、`entries: Vec<SgpdEntry>`
- version 0: `grouping_type` と `entry_count` だけを持ち、`default_length` は存在しない
- version 1: `default_length` を持ち、`default_length == 0` の場合は各エントリーの直前に 32 ビットの `description_length` が入る
- version 2: `default_group_description_index` を持つ。本 issue では `seig` の version 2 を `Unsupported` として拒否する
- version 0 で `grouping_type` が `seig` 以外の場合は、エントリー長を決められないため `Unsupported`
- `SgpdEntry::{Seig(SeigEntry), Unknown(Vec<u8>)}` とし、`seig` 以外は version 1 の `default_length` / `description_length` で切り出した生バイトを保持する

### 4. `sbgp` の `group_description_index` の解決

ISO/IEC 14496-12:2022 8.9.2.3 と 8.9.4 に従い、次の 3 通りに解釈する。

- `0`: このグループに属さない。`tenc` の既定値を使う
- `1..=0x10000`: **track-level** の `sgpd` (初期化セグメントの `stbl`) の 1 始まりエントリー番号
- `0x10001` 以上: **fragment-local** の `sgpd` (同じ `traf`) のエントリー番号。`index - 0x10000` が 1 始まりのエントリー番号になる

解決の優先順位は次のとおり。

1. `traf` に `grouping_type` が `seig` の `sbgp` があれば、それを使う
2. `traf` に `sbgp` が無ければ `tenc` の既定値を使う
3. 初期化セグメントの `stbl` にある `grouping_type` が `seig` の track-level の `sbgp` は、トラック先頭からの通算サンプル番号が必要になるため **対象外** とし、`Unsupported` を返す。`seig` 以外の track-level `sbgp` は従来どおり生バイトのまま無視する。track-level の `seig` の `sgpd` は、`traf` の `sbgp` が `1..=0x10000` で参照する場合に使う

`sbgp` の `sample_count` の合計が `traf` のサンプル数より少ない場合、余りのサンプルは index `0` として扱う。

### 5. demux

- `handle_init_segment` で、`StblBox::unknown_boxes` から `grouping_type` が `seig` の `sgpd` を取り出して `TrackRuntime` に保持する。`seig` の track-level `sbgp` があれば `Unsupported`
- `handle_media_segment` で、`TrafBox::unknown_boxes` から `sbgp` / `sgpd` を取り出して解決する
- 各サンプルの実効 KID を決め、`SampleEncryptionInfo::kid` に設定する。SAI のデコード自体は IV サイズにしか依存しないため、KID の解決は SAI の解釈とは独立に行う
- `has_seig_sample_group` による `Unsupported` を削除する (削除は demux の解決処理と同じ変更で行う)
- エラーにする入力
  - `sbgp` の `sample_count` の合計が `traf` のサンプル数より多い (`InvalidData`)
  - `group_description_index` が範囲外、または参照先の `sgpd` が無い (`InvalidData`)
  - `seig` の `sbgp` / `sgpd` が同じ `traf` に複数ある (`InvalidData`)
  - `sbgp` の version 1 で `grouping_type_parameter` が 0 以外 (`Unsupported`)
  - `sgpd` のエントリーが切り詰められている (`InvalidData`)

### 6. サンプル単位の暗号化情報

- `SencBox::samples` の要素型を `SencSample { iv: Vec<u8>, subsamples: Vec<Subsample> }` に分離する。`senc` は KID を持たないため、`SencBox` に KID を入れない
- `SampleEncryptionInfo` に `kid: Option<[u8; 16]>` を追加する。`None` は「`tenc` の既定 KID を使う」を表す
- `kid` の意味を「実効 KID が `tenc.default_kid` と異なる場合だけ `Some`」と定義し、`Some(tenc.default_kid)` は `None` に正規化する (mux の入力・demux の出力の両方)
- `parse_sample_aux_info` など SAI を読む処理は `SencSample` を返し、KID は解決処理で後から付ける

### 7. mux

- `SampleEncryptionInfo::kid` が `Some` で `tenc.default_kid` と異なるサンプルがある場合、`traf` に `sgpd` と `sbgp` を書く
  - `sgpd`: version 1、`grouping_type = seig`、`default_length = 20`、KID ごとに 1 エントリー (`is_protected = 1`、`Per_Sample_IV_Size` は `tenc` と同じ、`crypt_byte_block` / `skip_byte_block` は 0)
  - `sbgp`: version 0、連続する同一 KID ごとに 1 エントリー。`group_description_index` は `0x10000 + (1 始まりのエントリー番号)` とし、1 番目が `0x10001` になる
- `tenc.default_kid` と同じ KID のサンプルは index `0` とする
- 全サンプルが `tenc.default_kid` と同じ場合は `sgpd` / `sbgp` を書かない
- 次の場合は `MuxError::EncodeError(Error::invalid_input(...))` を返す
  - `tenc` を持たないトラックに `kid: Some` を指定した場合
  - `tenc.default_per_sample_iv_size` が 0 の場合 (constant IV)
  - `tenc` にパターン暗号化パラメーターがある場合 (`seig` の 0 で上書きしてしまうため)
- `senc` / `saiz` / `saio` の書き出しは既存の処理を使う。`SencBox` の要素型が `SencSample` になるため、`SampleEncryptionInfo` から `SencSample` への変換だけを追加する

### 8. 実装順序

1. `SbgpBox` / `SgpdBox` / `SgpdEntry` / `SeigEntry` と `SencSample` の実装と PBT
2. `SampleEncryptionInfo::kid` の追加と、`SampleEncryptionInfo` の全リテラルの更新 (コンパイルを通す)
3. demux の解決処理 (track-level `sgpd` の保持、`traf` の `sbgp` / `sgpd` の解決、`has_seig_sample_group` の削除)
4. mux の書き出し
5. テスト、`CHANGES.md`、`skills/shiguredo-mp4/SKILL.md`、`issues/0071-other-position-timing-boxes-inventory.md` の更新

## 完了条件

- `SbgpBox` / `SgpdBox` / `SgpdEntry` / `SeigEntry` / `SencSample` が実装され、PBT と異常系テストが通る
- `seig` エントリーの 20 バイトの固定前置を、規格から作った既知バイト列で decode / encode できる
- `sgpd` version 0 (`seig` のみ)、version 1 (`default_length` あり / `default_length == 0` + `description_length`)、version 2 (`seig` は `Unsupported`) をそれぞれテストする
- `sbgp` の `group_description_index` が `0` / `1..=0x10000` (track-level) / `0x10001` 以上 (fragment-local) の 3 通りで正しく解決される
- fragment-local の `sgpd` を参照する `sbgp` を持つセグメントが demux でき、サンプルごとに `kid` が設定される
- track-level の `sgpd` を参照する `sbgp` を持つセグメントが demux でき、サンプルごとに `kid` が設定される
- `grouping_type` が `seig` の track-level `sbgp` を持つ入力が `Unsupported` になり、`seig` 以外の track-level `sbgp` は無視される
- `sbgp` の `sample_count` の合計がサンプル数より少ない場合、余りのサンプルが `tenc` の既定 KID (`kid: None`) になる
- `sample_count` の合計がサンプル数より多い入力、`group_description_index` が範囲外の入力、参照先の `sgpd` が無い入力、`seig` の `sbgp` / `sgpd` が重複する入力が `InvalidData` になる
- `multi_key_flag == 1`、`crypt_byte_block` / `skip_byte_block` が 0 以外の `seig` が `Unsupported` になる
- `tenc` にパターン暗号化パラメーターを持つトラックに `kid: Some` を渡すと `MuxError::EncodeError` になる
- `tenc` にパターン暗号化パラメーターを持つトラックで、`seig` のパターン値が `tenc` と異なる入力が `Unsupported` になる
- `tenc` を持たないトラックに `kid: Some` を渡すと `MuxError::EncodeError` になる
- `tenc.default_per_sample_iv_size` が 0 のトラックに `kid: Some` を渡すと `MuxError::EncodeError` になる
- `sbgp` の version 1 で `grouping_type_parameter` が 0 以外の入力が `Unsupported` になる
- `sgpd` のエントリーが切り詰められている入力が `InvalidData` になる
- `seig` の `per_sample_iv_size` / `is_protected` が `tenc` と食い違う入力、`constant_iv` を持つ入力が `Unsupported` になる
- mux が、KID が切り替わるサンプルに対して `sgpd` / `sbgp` を書き出し、`sbgp` の 1 番目の index が `0x10001`、2 番目が `0x10002` になる (mux → demux の往復だけに依存せず、デコードした値で検証する)
- 全サンプルが `tenc` の既定 KID と同じ場合は `sgpd` / `sbgp` を書かない
- mux → demux の往復で、サンプルごとの **実効 KID** (`kid: None` を `tenc.default_kid` とみなした値) が一致する
- `Some(tenc.default_kid)` を入力すると `None` に正規化される
- `SencBox` の往復 (`SencSample`) と、`senc` / `saiz` / `saio` の既存テストが通り続ける
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz --no-deps` が成功する
- `CHANGES.md`、`skills/shiguredo-mp4/SKILL.md`、`issues/0071-other-position-timing-boxes-inventory.md` が更新されている

## 解決方法

- `src/boxes_cenc.rs` に `SbgpBox` / `SgpdBox` / `SgpdEntry` / `SeigEntry` / `SencSample` を追加する。各ボックスの実装コメントに、根拠 (14496-12 8.9.2 / 8.9.3 / 8.9.4、23001-7 6) と将来変更される可能性を書く
- `SencBox::samples` の要素型を `SencSample` に変更し、`SampleEncryptionInfo` に `kid` を追加する
- `src/boxes.rs` の再エクスポートに新しい型を追加する
- `src/demux_fmp4_segment.rs` の `TrackRuntime` に track-level の `sgpd` を保持するフィールドを追加し、`handle_init_segment` で `stbl` の `unknown_boxes` から取り出す。`grouping_type` が `seig` の track-level `sbgp` があれば `Unsupported` を返し、`seig` 以外は無視する
- `src/demux_fmp4_segment.rs` で、`traf` の `sbgp` の index からサンプルごとの `seig` を引き、実効 KID を `SampleEncryptionInfo::kid` に設定する。`has_seig_sample_group` を削除する
- `src/mux_fmp4_segment.rs` で、KID ごとに `sgpd` / `sbgp` を組み立てて `traf` に書く
- `SampleEncryptionInfo` の全リテラル (`src/boxes_cenc.rs` / `src/demux_fmp4_segment.rs` / `tests/test_boxes_cenc.rs` / `tests/test_demux_fmp4_segment.rs` / `tests/test_mux_fmp4_segment.rs` / `tests/test_mux_mp4_file.rs` / `pbt/tests/prop_boxes_cenc.rs` / `pbt/tests/prop_fmp4_segment_mux_demux.rs`) に `kid` を追加する。`Sample` の `encryption_info: None` リテラルは変更不要である
- テストは `tests/test_boxes_cenc.rs` / `pbt/tests/prop_boxes_cenc.rs` / `tests/test_demux_fmp4_segment.rs` / `tests/test_mux_fmp4_segment.rs` / `pbt/tests/prop_fmp4_segment_mux_demux.rs` に追加する
- `tests/test_demux_fmp4_segment.rs` の `seig_sample_group_is_unsupported` を、鍵ローテーションの正常系テストと対象外構成の異常系テストに置き換える
- `CHANGES.md` の `## develop` にある同じ開発サイクルの `[ADD]` エントリー (新しいボックと型) を更新し、`SbgpBox` / `SgpdBox` / `SeigEntry` / 鍵ローテーション対応と、`SencBox` が `SencSample` を使うことを含める。`SampleEncryptionInfo` は未リリースのため `[CHANGE]` は追加しない
- `skills/shiguredo-mp4/SKILL.md` の「`seig` サンプルグループは非対応」という記述を削除・修正し、新しいボックス、`Sample` のフィールド表、`SampleEncryptionInfo` の説明に `kid` を追加する。`SencBox::samples` の要素型が `SencSample` になったことに合わせて、`SencBox` と `SampleEncryptionInfo` の説明も更新する
- `issues/0071-other-position-timing-boxes-inventory.md` の `sbgp` / `sgpd` の行を実装済みに更新する

## 対象外

- `sgpd` の version 2 (`default_group_description_index`)
  - `seig` の version 2 は、既定グループの KID を解決できないまま `tenc` の既定値に落ちると誤った鍵で解釈するため、`Unsupported` として拒否する
- track-level の `sbgp`
  - トラック先頭からの通算サンプル番号が必要になり、複数セグメントにまたがる状態管理が別途必要になる。必要になった時点で別 issue にする
- `seig` による `is_protected` の上書き (選択的暗号化)
  - ISO/IEC 23001-7:2023 3.1.9 の選択的暗号化であり、`senc` の version 0 では表現できない
- `seig` による IV サイズの上書き
  - `senc` の version 0 はトラック内で IV サイズが一定であることを前提とするため、上書きがあると `senc` では表現できない
- `seig` による constant IV の上書き
  - 本 issue は per-sample IV を使うトラックだけを対象とする
- `seig` によるパターン暗号化パラメーターの上書き
  - 本 issue は KID の切り替えだけを対象とする。`crypt_byte_block` / `skip_byte_block` が 0 以外のエントリーは `Unsupported`
- 複数鍵 (`multi_key_flag == 1`、`senc` の version 1、`aux_info_type_parameter = 1` の複数 IV 形式)
  - 1 サンプルに複数の IV を持つ構成であり、本 issue の単一 IV 形式とは別の表現になる
- `seig` 以外のサンプルグループ種別
  - version 1 の `description_length` で切り出した生バイトとして保持するだけにし、意味の解釈はしない

## 関連 issue

- `issues/closed/0162-add-cenc-sample-aux-info.md`: `senc` / `saiz` / `saio` と `SampleEncryptionInfo`。本 issue はその上に KID の切り替えを追加する
- `issues/closed/0161-add-cenc-init-boxes.md`: 初期化データ向けの CENC ボックス。`tenc` の既定 KID はこちらで実装済み
- `issues/closed/0163-add-fmp4-segment-boxes.md`: `styp` とセグメント出力
- `issues/0071-other-position-timing-boxes-inventory.md`: ボックスの棚卸し
