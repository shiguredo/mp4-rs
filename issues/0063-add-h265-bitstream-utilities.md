# H.265 ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: 2026-08-26
- Branch: feature/add-h265-bitstream-utilities
- Polished: 2026-08-25

## 目的

H.265 の Annex B / length-prefixed NAL ユニット列の解析、SPS の解析、VPS / SPS / PPS の抽出、`hev1` / `hvc1` / `hvcC` の構築を `shiguredo_mp4` の汎用ユーティリティとして提供する。

H.264 と共有できるのは NAL ユニットを区切る外側のフレーミング処理だけに限定し、H.265 固有のヘッダーとパラメータセットの意味を独立した公開 API で表現する。

## 現状

- `src/boxes_sample_entry.rs` には `Hev1Box`、`Hvc1Box`、`HvccBox`、`HvccNalUintArray` があるが、H.265 ストリームから構築する API はない
- `src/bitstream.rs` は既に公開されており、`aac` / `av1` / `h264` / `vp8` / `vp9` を公開している。非公開の `mod nal` (`src/bitstream/nal.rs`) も 0062 で追加済み。`h265` サブモジュールは無い。`pbt/Cargo.toml` の `noprop` は 0068 で追加済み
- `LengthSize` (`OneByte` / `TwoBytes` / `FourBytes`) は `src/bitstream/nal.rs` に定義され、公開面では `bitstream::h264` が `pub use` している。ISO/IEC 14496-15:2022 8.3.2.1.3 の `lengthSizeMinusOne` は 0 / 1 / 3 (幅 1 / 2 / 4) だけが shall であり、2 (幅 3) は型で表現できない
- `shiguredo/hisui` の `src/video/h265.rs` には Annex B の走査、SPS の解析、VPS / SPS / PPS からのサンプルエントリー構築がある。H.264 側の開始コード探索を再利用しており、外側のフレーミング処理には実際の共通性がある。一方で、2 バイトの H.265 NAL ヘッダー、NAL 種別、SPS 構文は H.264 と異なる
- Hisui 固有のプロファイル許可リスト (`general_profile_idc` の `{1, 2, 3, 4, 5, 6, 7, 9}`)、固定の 4 バイト長、`FrameRate` から切り上げた `avg_frame_rate` と `constant_frame_rate = 1`、常に `SampleEntry::Hvc1` を返す方針、常に `array_completeness = 1` は汎用 crate の契約にはできない
- `shiguredo/sora-rust-sdk` の `src/video_codecs/mp4.rs` は `HvccBox` から VPS / SPS / PPS を 4 バイト開始コード付き Annex B へ結合し、`lengthSizeMinusOne` は 0 / 1 / 3 だけを受理する。SDP やコーデック文字列生成は本 crate の対象外である

参照仕様は [ITU-T H.265 (V11) (01/2026)](https://www.itu.int/rec/T-REC-H.265-202601-I/en) (ISO/IEC 23008-2 と技術的に整合) および ISO/IEC 14496-15:2022 の 8.3 / 8.4 (`HEVCDecoderConfigurationRecord` / `'hvc1'` / `'hev1'` / `'hvcC'`) とする。H.265 ビットストリーム構文は ITU-T H.265、ファイル格納は 14496-15:2022 を一次とする。

## 設計方針

### モジュールと共通化の境界

公開 API は `bitstream::h265` に配置する。既存の非公開 `src/bitstream/nal.rs` を、Annex B の境界走査と length-prefixed 形式の境界検証・変換だけに再利用する。`src/bitstream.rs` に `pub mod h265;` を追記する。`mod.rs` は使わない。

```text
src/bitstream.rs
src/bitstream/h265.rs
src/bitstream/nal.rs  (既存。本 issue では新設しない)
```

H.265 側は 2 バイトの NAL ヘッダーを独自に解析する。H.264 の公開型 (`H264NalUnit` / `H264NalUnitType` / `H264Sps` / `H264SampleEntryConfig`)、公開関数、ヘッダー解釈を流用しない。共通トレイトや公開 `bitstream::nal` モジュールも追加しない。

`LengthSize` は `nal.rs` の既存型を `bitstream::h265` からも `pub use` する。`bitstream::h264::LengthSize` に依存して公開 API を組まない (同じ非公開型を両モジュールが再公開する形にする)。長さフィールド幅を `u8` では表さない。

### NAL バイト列の契約

返す NAL バイト列は、2 バイトの NAL ヘッダーを含み、開始コードと長さプレフィックスを含まない。ペイロードは emulation prevention byte を残したままとする (現行 `HvccNalUintArray::nalus` と同じ EBSP)。RBSP 化は SPS 解析の内部だけで行い、公開の列挙・変換・ `HvccBox` 格納には使わない。除去の規範は ITU-T H.265 7.3.1.1 の `nal_unit()` 構文ループ (ヘッダー 2 バイトのあと `i = 2` から `0x000003` を捨てる) と 7.4.2.1 の規定 (`emulation_prevention_three_byte` はデコード処理が破棄する) である。7.4.2.3 は SODB を RBSP に包む側 (挿入) の informative 説明であり、除去手順の一次根拠にはしない。

### Annex B と length-prefixed の契約

Annex B の境界走査と length-prefixed の境界検証・相互変換は `nal.rs` の既存契約に従う (ITU-T H.265 Annex B B.2 は H.264 Annex B と同型の開始コード / `leading_zero_8bits` / `trailing_zero_8bits`)。

- 空入力は NAL ユニット 0 個の成功とする (開始コード欠落とは区別する)
- 非空入力に開始コードが 1 つも無い場合、最初の開始コードより前の非ゼロ、空 NAL、切り詰め、長さ 0 は `crate::Error` とする
- 3 バイトと 4 バイトの開始コードの混在を受理する
- length-prefixed の長さフィールド幅は `LengthSize` (`OneByte` / `TwoBytes` / `FourBytes`)。ISO/IEC 14496-15:2022 8.3.2.1.3 は `lengthSizeMinusOne` を 0 / 1 / 3 に限る。Hisui の 4 バイト固定は移植しない
- Annex B から length-prefixed への変換では、開始コードを除いた NAL 本体の前に指定幅の長さフィールドを付ける
- length-prefixed から Annex B への変換では、開始コードを常に 4 バイト (`0x00000001`) で書く。NAL type に応じた `zero_byte` の shall (VPS / SPS / PPS、AU 先頭) はアクセスユニット検出が対象外のため実装しない
- 相互変換自体はフレーミングのみで、NAL ヘッダー検証は行わない (0062 の `annexb_to_length_prefixed` と同じ)

H.265 ヘッダー (ITU-T H.265 7.3.1.2 / 7.4.2.2) は列挙 API で検証する。

- ヘッダー 2 バイトに満たない NAL は `crate::Error` とする
- `forbidden_zero_bit` が 1 なら `crate::Error` とする
- `nuh_temporal_id_plus1` が 0 なら `crate::Error` とする (shall not be equal to 0。`TemporalId = nuh_temporal_id_plus1 - 1`)
- `nuh_layer_id` は 0..=62 だけ受理し、63 は `crate::Error` とする (7.4.2.2 の値域。63 は将来拡張)。`nuh_layer_id != 0` は拒否しない。Annex A 非 INBLD デコーダーの ignore は本 API の契約にしない
- `nal_unit_type` は `H265NalUnitType` 型で表す。Table 7-1 の VPS (32) / SPS (33) / PPS (34) と、実装時に既存 API へ揃える主要種別は名前付きバリアント、それ以外は `Other(u8)` としてフレーミングでは不透明に通す
- VPS / SPS の `TemporalId == 0` (7.4.2.2) は、SPS 解析とサンプルエントリー構築時の VPS / SPS 検証で要求する。列挙 API では `nuh_temporal_id_plus1 != 0` だけを全 NAL に課し、PPS や VCL まで `TemporalId == 0` にしない (PPS は NOTE 9 どおり 0 でなくてよい)

### SPS 解析

入力は NAL ヘッダー付き EBSP とする。内部でヘッダーを検証し (`nal_unit_type` が SPS、`TemporalId == 0`)、残バイトから emulation prevention byte を除いて RBSP を得て、ITU-T H.265 7.3.2.2.1 / 7.3.3 / 7.4.3.2.1 の `u(n)` と Exp-Golomb (`ue(v)`) で読む。VPS 構文と PPS 構文と VUI は読まない。

読む範囲は寸法と `hvcC` へ写す値に到達するまでとする。

- `sps_video_parameter_set_id` (`u(4)`) は読み飛ばす
- `sps_max_sub_layers_minus1` (`u(3)`、0..=6) と `sps_temporal_id_nesting_flag` (`u(1)`) を読む。0..=6 以外は拒否する。`sps_max_sub_layers_minus1 == 0` のとき 7.4.3.2.1 は `sps_temporal_id_nesting_flag` を 1 と定めるので、0 なら拒否する
- `profile_tier_level(1, sps_max_sub_layers_minus1)` (7.3.3) を読み、`general_profile_space` / `general_tier_flag` / `general_profile_idc` / `general_profile_compatibility_flags` / `general_constraint_indicator_flags` (48 bit) / `general_level_idc` を公開結果に載せる。sub-layer の present flag と、flag が 1 のときの sub-layer profile / level、および `sps_max_sub_layers_minus1 > 0` のときの `reserved_zero_2bits` はビット位置を進めるためだけに読み飛ばす。公開結果に sub-layer profile は載せない
- `sps_seq_parameter_set_id` (`ue(v)`) は読み飛ばす
- `chroma_format_idc` (`ue(v)`、0..=3)。3 のときだけ `separate_colour_plane_flag` (`u(1)`)。不在時は 0
- `pic_width_in_luma_samples` / `pic_height_in_luma_samples` (`ue(v)`)
- `conformance_window_flag` と、1 のときの 4 オフセット (`ue(v)`)
- `bit_depth_luma_minus8` / `bit_depth_chroma_minus8` (`ue(v)`)

これ以降 (`log2_max_pic_order_cnt_lsb_minus4` 以後、VUI を含む) は読まない。寸法に必要な構文が途中で終わる SPS、Exp-Golomb の途中終端は拒否する。以降の欠落は成功とする。

Hisui の `H265_ALLOWED_PROFILE_IDCS` で `general_profile_idc` 自体を拒否しない。5 ビット値はそのまま受理する。

`chroma_format_idc > 3`、`sps_max_sub_layers_minus1 > 6` は 7.4.3.2.1 の値域外として拒否する。

`bit_depth_luma_minus8` / `bit_depth_chroma_minus8` は ITU-T H.265 7.4.3.2.1 では 0..=8 だが、ISO/IEC 14496-15:2022 8.3.2.1.2 の `HEVCDecoderConfigurationRecord` はどちらも `unsigned int(3)` (0..=7) である。現行 `HvccBox` も `Uint<u8, 3>`。`Uint::new` は範囲を検証せず、8 を渡すと `HvccBox::encode` の `0b1111_1000 | to_bits()` が下位 3 ビット 0 (8-bit) として黙って書き出す。したがって **0..=7 以外は `crate::Error`** とする。H.265 として合法な 16-bit (`minus8 == 8`) も `hvcC` に載せられないため拒否する。Hisui が 0..=7 で切っているのはこのフィールド幅に合わせたものであり、許可リストとは別件として維持する。

寸法は同 7.4.3.2.1 と Table 6-1 に従う。H.264 の `CropUnitY = SubHeightC * (2 - frame_mbs_only_flag)` は使わない (H.265 に `frame_mbs_only_flag` は無い)。

- `ChromaArrayType` は `separate_colour_plane_flag == 1` なら 0、さもなくば `chroma_format_idc`
- `SubWidthC` / `SubHeightC` は Table 6-1。`chroma_format_idc` 0 なら 1 / 1、1 なら 2 / 2、2 なら 2 / 1、3 なら 1 / 1。`separate_colour_plane_flag == 1` の行も 1 / 1
- クロップ後幅 = `pic_width_in_luma_samples - SubWidthC * (conf_win_left_offset + conf_win_right_offset)`
- クロップ後高さ = `pic_height_in_luma_samples - SubHeightC * (conf_win_top_offset + conf_win_bottom_offset)`
- `conformance_window_flag == 0` のオフセットは 0
- 仕様は `SubWidthC * (left + right)` が `pic_width_in_luma_samples` 未満、高さ側も同様であることを要求する。クロップが符号化サイズ以上、結果が 0、結果が `u16::MAX` を超える場合は `crate::Error` とする。`VisualSampleEntryFields::width` / `height` へ写すとき飽和しない

### サンプルエントリー構築 API

VPS / SPS / PPS の EBSP リストと呼び出し側設定から、`Hev1Box` または `Hvc1Box` を 1 つ返す。fourcc の選択は真偽値にせず、`build_hev1_box` / `build_hvc1_box` の専用関数にする (内部の組み立ては共有してよい)。戻り値を `SampleEntry` に包まない。戻り値型をまとめる enum も公開しない。

Annex B 入力からの構築は、列挙して type 32 / 33 / 34 を入力順で集め、同じ構築関数に渡す薄いラッパーとする (`build_hev1_box_from_annexb` / `build_hvc1_box_from_annexb`)。VCL / SEI 等は無視する。

VPS / SPS / PPS のいずれかが 0 個なら `crate::Error` とする。ISO/IEC 14496-15:2022 8.3.1 は `'hev1'` でパラメータセットをサンプル側にも置けるとするが、本構築 API は代表 SPS から `hvcC` 欄と寸法を埋めるため、`hev1` でも 3 種を 1 個以上要求する。全ての VPS は非空・NAL type 32・`TemporalId == 0`、全ての SPS は非空・NAL type 33・`TemporalId == 0`、全ての PPS は非空・NAL type 34 であることを検証する (`forbidden_zero_bit`、`nuh_temporal_id_plus1 != 0`、`nuh_layer_id` 0..=62 も適用。PPS の `TemporalId` は 0 でなくてよい)。構文解析は先頭 SPS だけ。複数 VPS / SPS / PPS は `nalu_arrays` に入力順で全部残し、profile / level / width / height / chroma / bit depth / temporal 欄は先頭 SPS だけから取る。ISO/IEC 14496-15:2022 8.3.2.1.1 は活性化される全パラメータセットで `chroma_format_idc` / `bit_depth_*_minus8` 等が同一であること等を shall とするが、全件一致の検証は対象外とする (0062 の先頭 SPS 代表値と同じ)。

`nalu_arrays` は VPS / SPS / PPS の 3 配列をこの順で持つ。ISO/IEC 14496-15:2022 8.3.2.1.1 は配列を VPS、SPS、PPS、prefix SEI、suffix SEI の順にすることを recommended とし、NAL 種別はこれらに限る。本 issue は VPS / SPS / PPS だけを載せ、SEI 配列は対象外とする。各 `nalus` には呼び出し側が渡した EBSP を開始コード無し・emulation prevention byte 残しで格納する (同 8.3.2.1.3 の `nalUnit` は ISO/IEC 23008-2 の NAL ユニット)。

`HvccNalUintArray::array_completeness` は fourcc に連動させる。ISO/IEC 14496-15:2022 8.4.1.1.1 は `'hvc1'` でパラメータセット配列の completeness を 1 に必須 (default and mandatory)、その他配列は 0 とする。`'hev1'` では全配列の default が 0。8.3.1 は `'hvc1'` の VPS / SPS / PPS をサンプルエントリーのみ、`'hev1'` はサンプルエントリーとサンプルの両方を許可する。したがって `build_hvc1_box` は completeness 1、`build_hev1_box` は completeness 0 とする。Hisui の常時 1 は移植しない。呼び出し側の completeness 引数は置かない。

`HvccBox::encode` は `nalu_arrays` 個数を `u8`、各配列の NAL 個数と各 NAL 長を `u16` で書く。上限を超える入力はボックス encode に渡す前に `crate::Error` とする。

#### 固定値 (関数側で埋める)

- `VisualSampleEntryFields` の `horizresolution` / `vertresolution` / `frame_count` / `compressorname` / `depth`: 同構造体の `DEFAULT_HORIZRESOLUTION` / `DEFAULT_VERTRESOLUTION` / `DEFAULT_FRAME_COUNT` / `NULL_COMPRESSORNAME` / `DEFAULT_DEPTH`。ISO/IEC 14496-15:2022 8.4.1.1.3 の compressorname `"\013HEVC Coding"` は recommended であり shall ではない。H.264 / AV1 / VP8 構築に合わせ DEFAULT を使う
- `VisualSampleEntryFields::data_reference_index` = `VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX`
- `Hev1Box::unknown_boxes` / `Hvc1Box::unknown_boxes` = 空 `Vec`
- `HvccBox` の configurationVersion は既存の `HvccBox::encode` が書く 1 に任せる (ISO/IEC 14496-15:2022 8.3.2.1.2 の `configurationVersion = 1`)
- `HvccBox::min_spatial_segmentation_idc` = 0。VUI を読まないため追加制限を付けない。ISO/IEC 14496-15:2022 8.3.2.1.1 は、活性化される全パラメータセットの空間分割の最低以下であることを shall とする (0 はその下限)
- `HvccBox::parallelism_type` = 0。ISO/IEC 14496-15:2022 8.3.2.1.3 は、混合または不明なら 0 に should とする。値は PPS の `tiles_enabled_flag` / `entropy_coding_sync_enabled_flag` から推論できるが、PPS 構文は対象外
- `HvccBox::avg_frame_rate` = 0。同 8.3.2.1.3 は 0 を unspecified average frame rate とする。Hisui の `FrameRate` は移植しない
- `HvccBox::constant_frame_rate` = 0。同 8.3.2.1.3 は 1 を定フレームレート、2 を temporal layer 単位の定フレームレート、0 を「定フレームレートであるとは限らない」とする。Hisui の CFR=1 は移植しない。現行 `HvccBox` の rustdoc は 0 を VBR と書いており仕様文面とずれるが、ボックス rustdoc の修正は対象外

#### ストリーム導出値 (先頭 SPS から写す)

- `HvccBox::general_profile_space` / `general_tier_flag` / `general_profile_idc` / `general_profile_compatibility_flags` / `general_constraint_indicator_flags` / `general_level_idc`
- `HvccBox::chroma_format_idc` / `bit_depth_luma_minus8` / `bit_depth_chroma_minus8` (上記の 0..=7 検証済み)
- `HvccBox::num_temporal_layers` = `sps_max_sub_layers_minus1 + 1` (1..=7。`Uint<u8, 3>` に収まる)。ISO/IEC 14496-15:2022 8.3.2.1.3 は 1 を非 temporal scalable、0 を不明、2 以上を層数とする。単層 SPS (`minus1 == 0`) は 1 になる
- `HvccBox::temporal_id_nested` = 先頭 SPS の `sps_temporal_id_nesting_flag`。同 8.3.2.1.3 は活性化される全 SPS が 1 のとき 1 とするが、全 SPS の一致検証は対象外
- `VisualSampleEntryFields::width` / `height` (クロップ適用後。`u16` に収まらない値は拒否)
- `HvccBox::nalu_arrays` (VPS / SPS / PPS の EBSP)

#### 呼び出し側指定値

- NAL 長フィールド幅: `LengthSize`。`HvccBox::length_size_minus_one` には `LengthSize::length_size_minus_one` の値 (0 / 1 / 3) を入れる (ISO/IEC 14496-15:2022 8.3.2.1.3)。Hisui のように 4 に固定しない
- `hev1` / `hvc1` の選択: 呼び出す関数 (`build_hev1_box` または `build_hvc1_box`)

公開 API は `no_std` を維持し、crate 本体 (`shiguredo_mp4`) に新しい外部依存は追加しない。入力から読んだサイズやカウントを信頼した `Vec::with_capacity` は行わない (実入力長に比例する確保までは禁じない)。エラーは新しい公開エラー体系を増やさず、既存の `crate::Error` / `ErrorKind` に統合する。

以下は骨格例である。型名・関数名は実装時に既存 API (`parse_annexb_nal_units` / `build_avc1_box` 等) と整合させて調整してよい。

```rust
pub enum H265NalUnitType {
    Vps,       // 32
    Sps,       // 33
    Pps,       // 34
    Aud,       // 35
    PrefixSei, // 39
    SuffixSei, // 40
    Other(u8), // それ以外を不透明に通す
}

pub struct H265NalUnit<'a> {
    pub nal_unit_type: H265NalUnitType,
    pub nuh_layer_id: u8,            // 0..=62
    pub nuh_temporal_id_plus1: u8,   // 1..=7
    pub data: &'a [u8], // 2 バイト NAL ヘッダー込み、開始コード / 長さプレフィックス無し、EBSP
}

pub fn parse_annexb_nal_units(input: &[u8]) -> Result<...>;
pub fn parse_length_prefixed_nal_units(input: &[u8], length_size: LengthSize) -> Result<...>;

pub fn annexb_to_length_prefixed(input: &[u8], length_size: LengthSize) -> Result<Vec<u8>>;
pub fn length_prefixed_to_annexb(input: &[u8], length_size: LengthSize) -> Result<Vec<u8>>;

pub fn collect_nal_units<'a, I>(nals: I, nal_unit_type: H265NalUnitType) -> Vec<&'a [u8]>
where
    I: IntoIterator<Item = H265NalUnit<'a>>;

pub struct H265Sps {
    pub general_profile_space: u8,
    pub general_tier_flag: u8,
    pub general_profile_idc: u8,
    pub general_profile_compatibility_flags: u32,
    pub general_constraint_indicator_flags: u64, // 下位 48 bit
    pub general_level_idc: u8,
    pub sps_max_sub_layers_minus1: u8, // 0..=6
    pub sps_temporal_id_nesting_flag: u8,
    pub chroma_format_idc: u8,
    pub bit_depth_luma_minus8: u8,    // 0..=7
    pub bit_depth_chroma_minus8: u8,  // 0..=7
    pub width: u16,  // クロップ後
    pub height: u16, // クロップ後
}

pub fn parse_sps(nal_unit: &[u8]) -> Result<H265Sps>;

pub struct H265SampleEntryConfig {
    pub length_size: LengthSize,
}

pub fn build_hev1_box(
    vps_list: &[Vec<u8>],
    sps_list: &[Vec<u8>],
    pps_list: &[Vec<u8>],
    config: &H265SampleEntryConfig,
) -> Result<Hev1Box>;

pub fn build_hvc1_box(
    vps_list: &[Vec<u8>],
    sps_list: &[Vec<u8>],
    pps_list: &[Vec<u8>],
    config: &H265SampleEntryConfig,
) -> Result<Hvc1Box>;

pub fn build_hev1_box_from_annexb(input: &[u8], config: &H265SampleEntryConfig) -> Result<Hev1Box>;
pub fn build_hvc1_box_from_annexb(input: &[u8], config: &H265SampleEntryConfig) -> Result<Hvc1Box>;
```

`parse_length_prefixed_nal_units` と相互変換の `length_size` は、呼び出し側が `HvccBox::length_size_minus_one` の値 (0 / 1 / 3) から `LengthSize` へ写して渡す。2 (幅 3) は ISO/IEC 14496-15:2022 8.3.2.1.3 の shall 集合に含まれないため型で表現できない。

### テスト

- 単体テスト (`tests/test_bitstream_h265.rs`): 3 バイト / 4 バイト / 混在開始コードと 2 バイト NAL ヘッダー。空入力は 0 個で成功、非空で開始コード無しは Error、空 NAL は Error。length-prefixed の幅 1 / 2 / 4、切り詰め・長さ 0・長さ超過の Error。`forbidden_zero_bit`、`nuh_temporal_id_plus1 == 0`、`nuh_layer_id == 63`。`nuh_layer_id != 0` は列挙では成功。EBSP の `0x000003` を含む SPS が RBSP 化後に正しく読めること。Main (`general_profile_idc = 1`) / Main 10 (`= 2`)。Hisui 許可リスト外の `general_profile_idc` (例: 8) も受理。conformance window 無しと、1920x1088 に `conf_win_bottom_offset = 4` (4:2:0 で `SubHeightC = 2`) を掛けて 1920x1080 になること。`bit_depth_*_minus8 == 8` の拒否。SPS / VPS / PPS 空リストの構築拒否。幅 0 と `u16::MAX` 超過の拒否。`build_hev1_box` の `array_completeness` が 0、`build_hvc1_box` が 1。`nalu_arrays` がヘッダー込み EBSP であること
- PBT (`pbt/tests/prop_bitstream_h265.rs`): Annex B と length-prefixed のラウンドトリップ。構築した正当な SPS の不変条件 (profile / 寸法 / hvcC 欄)。`pbt/Cargo.toml` の noprop は追加済みなので依存は足さない。PBT 専用 SPS ビルダーはテスト内に留め、公開 API にしない
- 実データ fixture: `tests/testdata/` に小さな Annex B または length-prefixed 列を置く。既存の `black-h265-video.mp4` / `black-h265-hvc1-video.mp4` から抽出してもよい。ネットワークや外部コマンドなしでテストが完結すること
- Fuzzing: `fuzz/fuzz_targets/fuzz_bitstream_h265.rs` に公開の Annex B 列挙・length-prefixed 列挙・`parse_sps` を対象とするターゲットを追加し、`fuzz/Cargo.toml` に `[[bin]]` エントリを追加する

### 対象外

- 非公開 NAL 層を公開 API に昇格すること
- H.264 と H.265 の NAL ヘッダー型、SPS 型、サンプルエントリー構築 API の共通化
- Hisui / Sora Rust SDK 側の呼び出し置換と依存バージョン更新
- RTP / SDP、フレームレート推定、デコーダー固有のプロファイル制限
- C API / WASM バインディング
- VPS 構文解析、PPS 構文解析、VUI、スライスヘッダー、アクセスユニット検出
- `min_spatial_segmentation_idc` / `parallelism_type` を VUI / PPS から導出すること
- prefix SEI / suffix SEI を `nalu_arrays` に載せる (ISO/IEC 14496-15:2022 8.3.2.1.1 は許可するが、本 issue は VPS / SPS / PPS のみ)
- 活性化される全パラメータセット間の profile / chroma / bit depth 一致の検証 (ISO/IEC 14496-15:2022 8.3.2.1.1 の shall。先頭 SPS の代表値のみ)
- `nuh_layer_id != 0` の NAL を Annex A の ignore として捨てること
- PBT 専用 SPS ビルダーの公開 API 化
- length-prefixed の 3 バイト長のサポート
- `HvccBox` の rustdoc (`constant_frame_rate` の 0 の意味など) の修正
- ISO/IEC 14496-15:2022 8.4.1.1.3 の compressorname `"\013HEVC Coding"` への変更

## 完了条件

- `bitstream::h265` が公開され、Annex B 走査、length-prefixed 形式との相互変換、SPS 解析、VPS / SPS / PPS 抽出、`Hev1Box` / `Hvc1Box` 構築が利用できること
- 共有処理が既存の `src/bitstream/nal.rs` のフレーミング層に限定され、H.264 / H.265 の公開 API が独立していること。`LengthSize` は `h265` からも再公開し、幅 3 は型で表現できないこと
- 返す NAL バイト列が 2 バイトヘッダー込み・開始コード無しの EBSP であり、RBSP 化が SPS 解析内部に閉じ、`nalu_arrays` にも EBSP を格納すること
- `build_hev1_box` / `build_hvc1_box` がそれぞれ `Hev1Box` / `Hvc1Box` を返し `SampleEntry` に包まないこと。`array_completeness` が hev1 で 0、hvc1 で 1 であること。固定値 / ストリーム導出 / 呼び出し側指定が設計方針の三分類どおりであること
- クロップ後寸法の式が ITU-T H.265 7.4.3.2.1 と Table 6-1 に従い、0 と `u16` 超過を飽和せず拒否すること
- `bit_depth_*_minus8` が 0..=7 以外 (仕様上あり得る 8 を含む) を拒否し、`Uint::new` に範囲外を渡さないこと。Hisui のプロファイル許可リストで `general_profile_idc` を制限していないこと
- Hisui 固有のフレームレート、NAL 長 4 固定、常時 `hvc1`、常時 `array_completeness = 1` を持ち込んでいないこと
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と crate 本体の依存ライブラリ 0 を維持すること (pbt 側の noprop は crate 本体の依存ではない)
- 決定的テスト (`tests/test_bitstream_h265.rs`)、`noprop` PBT (`pbt/tests/prop_bitstream_h265.rs`)、実データ fixture (`tests/testdata/` 配下)、fuzz target (`fuzz/fuzz_targets/fuzz_bitstream_h265.rs`) が追加され、`fuzz/Cargo.toml` に `[[bin]]` エントリが登録されていること
- 公開 API の rustdoc に入力形式、返す NAL バイト列へヘッダーを含むか、`LengthSize` とヘッダー検証、`hev1` / `hvc1` の選択と `array_completeness`、固定値 / ストリーム導出値 / 呼び出し側指定値の分類、エラー条件が記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること

## 解決方法

`bitstream::h265` を追加した。

- Annex B / length-prefixed の NAL ユニット列の解析 (`parse_annexb_nal_units` / `parse_length_prefixed_nal_units`)、相互変換 (`annexb_to_length_prefixed` / `length_prefixed_to_annexb`)、`collect_nal_units` を提供する。共有処理は既存の `src/bitstream/nal.rs` のフレーミング層に限定し、H.264 / H.265 の公開 API は独立させる。`LengthSize` は `bitstream::h265` からも再公開する
- SPS 解析 (`parse_sps`) は ITU-T H.265 7.3.2.2.1 / 7.3.3 / 7.4.3.2.1 に従い、`profile_tier_level` から bit depth までを読み、クロップ後寸法を Table 6-1 で計算する。`bit_depth_*_minus8` は `hvcC` の `unsigned int(3)` に合わせ 0..=7 以外を拒否する
- サンプルエントリー構築 (`build_hev1_box` / `build_hvc1_box` / `build_hev1_box_from_annexb` / `build_hvc1_box_from_annexb`) は、VPS / SPS / PPS の EBSP から `Hev1Box` / `Hvc1Box` を組み立てる。`array_completeness` は hev1 で 0、hvc1 で 1 とし、固定値 / ストリーム導出値 / 呼び出し側指定値を設計方針の三分類に従って埋める
- テストとして、単体テスト (`tests/test_bitstream_h265.rs`)、noprop PBT (`pbt/tests/prop_bitstream_h265.rs`)、実データ fixture (`tests/testdata/h265-vps-sps-pps-annexb.bin`)、fuzz target (`fuzz/fuzz_targets/fuzz_bitstream_h265.rs`) を追加した
- `CHANGES.md` の develop に `[ADD]` として記載した
