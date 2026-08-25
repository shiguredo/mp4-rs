# H.264 ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/add-h264-bitstream-utilities
- Polished: 2026-08-24

## 目的

H.264 の Annex B / length-prefixed NAL ユニット列の解析、SPS の解析、パラメータセットの抽出、`avc1` / `avcC` の構築を `shiguredo_mp4` の汎用ユーティリティとして提供する。

これらは MP4 ボックス自体の処理ではないが、H.264 ストリームから `Avc1Box` を構築する場合と、MP4 サンプルをデコーダーへ渡せる形式に変換する場合の双方で必要になる。利用側ごとの重複実装をなくし、MP4 のサンプルエントリーと整合する解析結果を 1 箇所で提供する。

## 現状

- `src/boxes_sample_entry.rs` には `Avc1Box` と `AvccBox` があるが、NAL ユニット列や SPS を解析してこれらを構築する API はない
- `src/bitstream.rs` は既に公開されており、`vp8` / `vp9` だけを公開している。`h264` サブモジュールも非公開の `nal` モジュールも無い。`pbt/Cargo.toml` の `noprop` は 0068 で追加済み
- `shiguredo/hisui` の `src/video/h264.rs` には Annex B の走査、SPS の解析、Annex B からのサンプルエントリー構築、Annex B から length-prefixed 形式への変換がある。NAL 長 4 バイト固定、`profile_idc` の許可リスト、戻り値を `SampleEntry` に包む、といった利用側固有の契約が混ざっている。本 crate には移植しない
- `shiguredo/sora-rust-sdk` の `src/video_codecs/mp4.rs` には length-prefixed 形式から Annex B への変換が独立に実装されている。ISO/IEC 14496-15 に合わせ長さ 1 / 2 / 4 だけを受理する一方、切り詰められた NAL ユニットを `break` で黙って打ち切る。後者は汎用パーサーの契約として適切ではない。SDP の `profile-level-id` 組み立ては本 crate の対象外である
- H.264 と H.265 は Annex B の開始コード探索と length-prefixed 形式の境界検証を共有できるが、NAL ヘッダーの長さ、種別のビット配置、妥当性条件は異なる。0063 は本 issue が追加する非公開 `nal` 層を再利用する前提である

参照仕様は [ITU-T H.264 (06/2026)](https://www.itu.int/rec/T-REC-H.264-202606-I/en) とする。ISO/IEC 14496-15 の `AVCDecoderConfigurationRecord` (`avcC`) は本リポジトリに一次資料が無いため、現行 `AvccBox` の encode 契約（`src/boxes_sample_entry.rs`）と、下記で固定する crate 契約に合わせる。

## 設計方針

### モジュール構成

`src/lib.rs` から公開済みの `bitstream` モジュール配下に H.264 用サブモジュールを追加する。`mod.rs` は使わない。

```text
src/bitstream.rs
src/bitstream/h264.rs
src/bitstream/nal.rs
```

`src/bitstream.rs` に `pub mod h264;` と非公開の `mod nal;` を追記する。本体の公開 API は `src/bitstream/h264.rs` に置く。open の 0063 / 0064 / 0069 と実装が並列する場合、`src/bitstream.rs` の追記が競合し得る。

`src/bitstream/nal.rs` は crate 内部だけで使う非公開モジュールとする。公開の `bitstream::nal`、コーデック共通の公開 NAL 型、共通化のためのトレイトは追加しない。

非公開 NAL 層の責務は次に限定する。

- 3 バイト (`0x000001`) および 4 バイト (`0x00000001`) の開始コードを認識し、Annex B の NAL ユニット境界を走査する
- length-prefixed NAL ユニット列を指定された長さフィールド幅で走査し、境界超過、切り詰め、長さのオーバーフローを検出する
- Annex B と length-prefixed 形式の間で、コーデックに依存しないフレーミング変換を行う
- NAL ユニット本体をバイト列として呼び出し側へ渡し、H.264 / H.265 のヘッダー解釈は行わない

H.264 側は 1 バイトの NAL ヘッダーを検証し、`forbidden_zero_bit` と `nal_unit_type` を H.264 固有の型・API として扱う。`nal_ref_idc` はヘッダーの一部として保持してよいが、公開 API の主契約にはしない。

### Annex B と length-prefixed の契約

返す NAL バイト列は、1 バイトの NAL ヘッダーを含み、開始コードと長さプレフィックスを含まない。ペイロードは emulation prevention byte を残したままとする (ISO/IEC 14496-15 の `sequenceParameterSetNALUnit` / `pictureParameterSetNALUnit`、および現行 `AvccBox::sps_list` / `pps_list` と同じ EBSP)。RBSP 化は SPS 解析の内部だけで行い、公開の列挙・変換・ `AvccBox` 格納には使わない。除去の規範は ITU-T H.264 7.3.1 の構文ループと 7.4.1 の規定（`emulation_prevention_three_byte` はデコード処理が破棄する）である。7.4.1.1 は SODB を RBSP に包む側（挿入）の informative 説明であり、除去手順の一次根拠にはしない。

Annex B (ITU-T H.264 Annex B):

- 空入力は NAL ユニット 0 個の成功とする (開始コード欠落とは区別する)
- 非空入力に開始コードが 1 つも無い場合は `crate::Error` とする
- 最初の開始コードより前の `leading_zero_8bits`、NAL 間のゼロ詰め (`trailing_zero_8bits` / 次の開始コードの `leading_zero_8bits`)、および最後の NAL より後の `trailing_zero_8bits` は境界の詰め物として捨て、NAL 本体に含めない。NAL 本体の終端は ITU-T H.264 Annex B B.2 どおり、後続のバイトアラインされた `0x000000` / `0x000001`、またはストリーム終端の直前までとする
- 開始コードの直後に次の開始コードまたは入力終端が来る空 NAL は `crate::Error` とする。黙って読み飛ばさない
- 3 バイトと 4 バイトの開始コードの混在を受理する。4 バイト開始コードを 3 バイト開始コード + 先行ゼロに誤分割しない

length-prefixed:

- 長さフィールド幅は 1 / 2 / 4 のみ受理する。ISO/IEC 14496-15 の `lengthSizeMinusOne` は 0 / 1 / 3 が正当で、2 (幅 3) は reserved である。Hisui の `convert_annexb_to_nalu` は幅 1..=4（3 を含む）を受理するが、crate 契約にはしない
- `AvccBox::length_size_minus_one` (`Uint<u8, 2>`、0..=3) から幅を取るときは、0 / 1 / 3 を 1 / 2 / 4 に写し、2 は `crate::Error` とする
- 長さフィールドが入力末尾を超える、宣言長が残バイトを超える、宣言長が 0 の NAL は `crate::Error` とする。Sora のように切り詰めを `break` で黙って打ち切らない
- 空入力は NAL ユニット 0 個の成功とする
- Annex B から length-prefixed への変換では、開始コードを除いた NAL 本体の前に指定幅の長さフィールドを付ける
- length-prefixed から Annex B への変換では、開始コードを常に 4 バイト (`0x00000001`) で書く。解析側は 3 / 4 バイト混在を受理する。NAL type に応じた `zero_byte` の SHALL はアクセスユニット検出が対象外のため実装しない

H.264 ヘッダー (ITU-T H.264 7.3.1 / 7.4.1):

- `forbidden_zero_bit` が 1 なら `crate::Error` とする
- ヘッダー 1 バイトに満たない NAL は `crate::Error` とする
- 予約・未指定の `nal_unit_type` はフレーミングでは不透明な NAL として通す。SPS 解析は type 7、構築時の PPS は type 8 を要求する

### SPS 解析

入力は NAL ヘッダー付き EBSP とする。内部でヘッダーを検証し、残バイトから emulation prevention byte を除いて RBSP を得て、ITU-T H.264 7.3.2.1.1 / 7.4.2.1.1 の Exp-Golomb (`ue(v)` / `se(v)`) で読む。

SPS 追加構文 (`chroma_format_idc` 以降) を読む `profile_idc` は、同 7.3.2.1.1 の条件節どおり次に限る。

`100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135`

これ以外の `profile_idc` (Baseline / Main / Extended の 66 / 77 / 88 を含む) では、同 7.4.2.1.1 の不在時推論どおり `chroma_format_idc = 1` (4:2:0)、`bit_depth_luma_minus8 = 0`、`bit_depth_chroma_minus8 = 0` とする。同節が特定の `profile_idc` に別の推論値を定めている場合（02/2014 系の文面では `profile_idc == 183` なら `chroma_format_idc` は 0）は、ITU-T H.264 (06/2026) の当該文を一次としてそれに従う。Hisui の許可リストで `profile_idc` 自体を拒否しない。

寸法は同 7.4.2.1.1 に従う。

- `PicWidthInSamplesL = (pic_width_in_mbs_minus1 + 1) * 16`
- サンプルエントリーの高さはフレームの輝度高さ `16 * FrameHeightInMbs` とする。`FrameHeightInMbs = (2 - frame_mbs_only_flag) * PicHeightInMapUnits`、`PicHeightInMapUnits = pic_height_in_map_units_minus1 + 1`。仕様上の `PicHeightInSamplesL` は 7.4.3 で `field_pic_flag` に依存するため、スライスを読まない本 API では使わない
- `ChromaArrayType` は `separate_colour_plane_flag == 1` なら 0、さもなくば `chroma_format_idc`
- `ChromaArrayType == 0` のとき `CropUnitX = 1`、`CropUnitY = 2 - frame_mbs_only_flag`
- それ以外は `CropUnitX = SubWidthC`、`CropUnitY = SubHeightC * (2 - frame_mbs_only_flag)`。`SubWidthC` / `SubHeightC` は同仕様 Table 6-1 (`chroma_format_idc` 1 なら 2 / 2、2 なら 2 / 1、3 なら 1 / 1)
- `frame_cropping_flag == 1` のとき、幅から `CropUnitX * (left + right)`、高さから `CropUnitY * (top + bottom)` を引く
- クロップ後の幅または高さが 0、クロップが符号化サイズを食いつぶす、結果が `u16::MAX` を超える場合は `crate::Error` とする。`VisualSampleEntryFields::width` / `height` へ写すとき飽和しない

寸法に到達するために、`pic_order_cnt_type` 0 / 1 の追加構文は全 `profile_idc` でビット位置を進めるためだけに読み飛ばす。SPS 追加構文を読む `profile_idc` では、その前に `qpprime_y_zero_transform_bypass_flag` (`u(1)`) と `seq_scaling_matrix_present_flag` 配下の scaling list も同様に読み飛ばす。公開結果に scaling list や VUI は載せない。`vui_parameters_present_flag` 以降は読まない。

`chroma_format_idc > 3`、`bit_depth_luma_minus8 > 6`、`bit_depth_chroma_minus8 > 6` は同 7.4.2.1.1 の値域外として拒否する。切り詰められた SPS、Exp-Golomb の途中終端も拒否する。

### サンプルエントリー構築 API

SPS / PPS の EBSP リストと呼び出し側設定から `Avc1Box` を 1 つ返す。先頭 SPS を関数内で解析して代表値にする。`SampleEntry` には包まない。Annex B 入力からの構築は、列挙して type 7 / 8 を入力順で集め、同じ構築関数に渡す薄いラッパーとする。SEI / IDR / AUD 等は無視する。SPS または PPS が 0 個なら `crate::Error` とする。複数 SPS / PPS は `AvccBox` のリストに入力順で全部残し、profile / level / 寸法 / chroma / bit depth は先頭 SPS だけから取る。

PPS は NAL type 8 であることと非空であることだけを検証し、PPS 構文は解析しない。SPS extension (type 13) の抽出と `sps_ext_list` への格納は対象外とし、構築結果の `sps_ext_list` は空 `Vec` とする。

`AvccBox::encode` は `avc_profile_indication` が 66 / 77 / 88 以外のとき `chroma_format` / `bit_depth_luma_minus8` / `bit_depth_chroma_minus8` を必須とする。この条件は SPS 追加構文の `profile_idc` リストと一致しない。66 / 77 / 88 以外では、SPS 追加構文から読めた値があればそれを入れ、無ければ上記の推論値 (`chroma_format_idc = 1`、bit depth minus8 = 0) を入れて encode が失敗しないようにする。66 / 77 / 88 ではこれらのフィールドは `None` のままにする。

#### 固定値 (関数側で埋める)

- `VisualSampleEntryFields` の `horizresolution` / `vertresolution` / `frame_count` / `compressorname` / `depth`: 同構造体の `DEFAULT_HORIZRESOLUTION` / `DEFAULT_VERTRESOLUTION` / `DEFAULT_FRAME_COUNT` / `NULL_COMPRESSORNAME` / `DEFAULT_DEPTH`
- `VisualSampleEntryFields::data_reference_index` = `VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX`
- `Avc1Box::unknown_boxes` = 空 `Vec`
- `AvccBox::sps_ext_list` = 空 `Vec`
- `AvccBox` の configurationVersion は既存の `AvccBox::encode` が書く 1 に任せる

#### ストリーム導出値 (先頭 SPS から写す)

- `AvccBox::avc_profile_indication` / `profile_compatibility` / `avc_level_indication` (SPS の `profile_idc`、constraint フラグ 1 バイト全体、`level_idc`)
- `AvccBox::sps_list` / `pps_list` (呼び出し側が渡した EBSP を、開始コードを付けず、emulation prevention byte を残したまま格納する)
- `AvccBox::chroma_format` / `bit_depth_luma_minus8` / `bit_depth_chroma_minus8` (上記の 66 / 77 / 88 分岐)
- `VisualSampleEntryFields::width` / `height` (クロップ適用後。`u16` に収まらない値は拒否)

SPS / PPS の個数が現行 `AvccBox::encode` の上限 (SPS 31、PPS 255) を超える、または各 NAL が `u16` 長に収まらない場合は、ボックス encode に渡す前に `crate::Error` とする。

#### 呼び出し側指定値

- NAL 長フィールド幅: 1 / 2 / 4。`AvccBox::length_size_minus_one` には幅 - 1 を入れる（結果は 0 / 1 / 3）。幅 3（`length_size_minus_one == 2`）は拒否する。Hisui のように 4 に固定しない

公開 API は `no_std` を維持し、crate 本体 (`shiguredo_mp4`) に新しい外部依存は追加しない。入力から読んだサイズやカウントを信頼した `Vec::with_capacity` は行わない (実入力長に比例する確保までは禁じない)。エラーは新しい公開エラー体系を増やさず、既存の `crate::Error` / `ErrorKind` に統合する。

以下は骨格例である。型名・関数名は実装時に既存 API (`parse_frame_header` / `build_vp08_box` 等) と整合させて調整してよい。

```rust
pub struct H264NalUnit<'a> {
    pub nal_unit_type: u8, // 下位 5 ビット
    pub data: &'a [u8],    // NAL ヘッダー込み、開始コード / 長さプレフィックス無し、EBSP
}

pub fn parse_annexb_nal_units(input: &[u8]) -> Result<...>; // 借用ベースの列挙
pub fn parse_length_prefixed_nal_units(input: &[u8], length_size: u8) -> Result<...>;

pub fn annexb_to_length_prefixed(input: &[u8], length_size: u8) -> Result<Vec<u8>>;
pub fn length_prefixed_to_annexb(input: &[u8], length_size: u8) -> Result<Vec<u8>>;

pub fn collect_nal_units<'a>(nals: impl IntoIterator<Item = H264NalUnit<'a>>, nal_unit_type: u8) -> Vec<&'a [u8]>;

pub struct H264Sps {
    pub profile_idc: u8,
    pub constraint_set_flags: u8,
    pub level_idc: u8,
    pub chroma_format_idc: u8,
    pub bit_depth_luma_minus8: u8,
    pub bit_depth_chroma_minus8: u8,
    pub width: u16,  // クロップ後
    pub height: u16, // クロップ後
}

pub fn parse_sps(nal_unit: &[u8]) -> Result<H264Sps>;

pub struct H264SampleEntryConfig {
    pub length_size: u8, // 1 / 2 / 4
}

pub fn build_avc1_box(
    sps_list: &[Vec<u8>],
    pps_list: &[Vec<u8>],
    config: &H264SampleEntryConfig,
) -> Result<Avc1Box>;

pub fn build_avc1_box_from_annexb(input: &[u8], config: &H264SampleEntryConfig) -> Result<Avc1Box>;
```

`parse_length_prefixed_nal_units` と相互変換の `length_size` は、呼び出し側が `AvccBox::length_size_minus_one` から換算した 1 / 2 / 4 を渡す。換算ヘルパーを置く場合も、`length_size_minus_one == 2`（幅 3）は Error にする。

### テスト

- 単体テスト (`tests/test_bitstream_h264.rs`): 3 バイト / 4 バイト / 混在開始コード。空入力は 0 個で成功、非空で開始コード無しは Error、空 NAL は Error。先頭・末尾のゼロ詰めが NAL 本体に混ざらないこと。length-prefixed の幅 1 / 2 / 4、幅 3 の拒否、切り詰め・長さ 0・長さ超過の Error。`forbidden_zero_bit`。EBSP の `0x000003` を含む SPS が RBSP 化後に正しく読めること。Baseline (66) / Main (77) / High (100) / High 10 (110)。クロップ無しとクロップ後 1920x1080。`frame_mbs_only_flag == 0`。66 / 77 / 88 で `chroma_format` が `None`、100 で `Some`。SPS / PPS 空リストの構築拒否。幅 0 と `u16::MAX` 超過の拒否
- PBT (`pbt/tests/prop_bitstream_h264.rs`): Annex B と length-prefixed のラウンドトリップ (NAL 境界が入力を重複なく覆うこと)。構築した正当な SPS の不変条件 (profile / 寸法 / avcC 欄)。`pbt/Cargo.toml` の noprop は 0068 で追加済みなので依存は足さない。PBT 専用 SPS ビルダーはテスト内に留め、公開 API にしない
- 実データ fixture: `tests/testdata/` に小さな Annex B または length-prefixed 列を置く。既存の `black-h264-video.mp4` / `black-h264-fmp4.mp4` から抽出してもよい。ネットワークや外部コマンドなしでテストが完結すること
- Fuzzing: `fuzz/fuzz_targets/fuzz_bitstream_h264.rs` に公開の Annex B 列挙・length-prefixed 列挙・`parse_sps` を対象とするターゲットを追加し、`fuzz/Cargo.toml` に `[[bin]]` エントリを追加する

### 対象外

- H.265 / AV1 / VP8 / VP9 のコーデック固有処理
- 非公開 NAL 層を公開 API に昇格すること、H.264 と H.265 のヘッダー型や SPS 型の共通化
- Hisui / Sora Rust SDK 側の呼び出し置換と依存バージョン更新
- RTP / SDP (`profile-level-id` の hex 化を含む)、デコーダーやエンコーダー固有のポリシー、コーデック文字列生成
- C API / WASM バインディング。利用要件が明確になった時点で別 issue とする
- PPS 構文解析、SPS extension、VUI、スライスヘッダー、アクセスユニット検出
- PBT 専用 SPS ビルダーの公開 API 化
- length-prefixed の 3 バイト長（Hisui の `convert_annexb_to_nalu` が受理する幅）のサポート

## 完了条件

- `bitstream::h264` が公開され、Annex B 走査、length-prefixed 形式との相互変換、SPS 解析、SPS / PPS 抽出、`Avc1Box` 構築が利用できること
- `src/bitstream/nal.rs` が非公開であり、コーデック共通の公開 NAL 型やトレイトが追加されていないこと
- 空の Annex B / length-prefixed 入力は 0 個の成功、非空 Annex B の開始コード欠落・空 NAL・切り詰め・長さ 0・幅 3 は `crate::Error` であること
- 返す NAL バイト列がヘッダー込み・開始コード無しの EBSP であり、RBSP 化が SPS 解析内部に閉じていること
- 正当な `profile_idc` を Hisui 固有の許可リストで制限していないこと。66 / 77 / 88 以外の `AvccBox` 追加欄は SPS 値または推論値で埋め、encode が必須欄欠落で失敗しないこと
- `build_avc1_box` が `Avc1Box` を返し `SampleEntry` に包まないこと。固定値 / ストリーム導出 / 呼び出し側指定が設計方針の三分類どおりであること
- クロップ後寸法の式が ITU-T H.264 7.4.2.1.1 に従い、0 と `u16` 超過を飽和せず拒否すること
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と crate 本体の依存ライブラリ 0 を維持すること (pbt 側の noprop は crate 本体の依存ではない)
- 決定的テスト (`tests/test_bitstream_h264.rs`)、`noprop` PBT (`pbt/tests/prop_bitstream_h264.rs`)、実データ fixture (`tests/testdata/` 配下)、fuzz target (`fuzz/fuzz_targets/fuzz_bitstream_h264.rs`) が追加され、`fuzz/Cargo.toml` に `[[bin]]` エントリが登録されていること
- 公開 API の rustdoc に入力形式、返す NAL バイト列へヘッダーを含むか、長さ幅 1 / 2 / 4 の契約、固定値 / ストリーム導出値 / 呼び出し側指定値の分類、エラー条件が記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
