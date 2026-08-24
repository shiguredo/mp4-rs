# AAC ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/add-aac-bitstream-utilities
- Polished: 2026-08-21

## 目的

AAC-LC の AudioSpecificConfig と ADTS ヘッダーを解析し、`mp4a` / `esds` の構築、および ADTS と raw AAC の相互変換を `shiguredo_mp4` の汎用ユーティリティとして提供する。

これらは MP4 ボックス自体の処理ではないが、AAC-LC ストリームから `Mp4aBox` を構築する場合と、MP4 サンプルをデコーダーへ渡せる形式 (ADTS) に変換する場合の双方で必要になる。

## 現状

- `src/boxes_sample_entry.rs` の `Mp4aBox`、`src/descriptors.rs` の `EsDescriptor` / `DecoderConfigDescriptor` / `DecoderSpecificInfo` / `SlConfigDescriptor`、`src/boxes_moov_tree.rs` の `EsdsBox` は既にある。`DecoderSpecificInfo::payload` に入る AudioSpecificConfig (以下 ASC) を解析する API はない
- ADTS ヘッダー解析、および ADTS と raw AAC の相互変換 API もない
- `DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3` (`0x40`) と `STREAM_TYPE_AUDIO` (`0x05`) は定義済みで、ASC 本体は呼び出し側が生バイト列として組み立てている。既存テストの代表値は `tests/test_boxes_sample_entry.rs` の `payload: vec![0x11, 0x90]` (コメント: AAC-LC、48 kHz、stereo)
- `src/bitstream.rs` は公開済みで `vp8` / `vp9` だけを公開している。`pbt/Cargo.toml` の `noprop` は 0068 で追加済み
- Hisui の AAC モジュールは同種処理を自前実装している (`parse_audio_specific_config` / `create_audio_specific_config` / `create_mp4a_sample_entry`、HLS の ADTS 付与、SRT の ADTS ヘッダー解析)。AOT を読まず先頭 2 バイトだけから周波数 index を取る、チャンネル 1 / 2 以外を拒否する、`buffer_size_db` / `max_bitrate` / `avg_bitrate` を固定する、戻り値を `SampleEntry::Mp4a` に包む、ADTS を MPEG-4・CRC なし・1 raw data block にハードコードする、といった利用側固有の契約が混ざっている。本 crate には移植しない
- Sora Rust SDK の `src` に ASC / ADTS 解析は無い

参照仕様は ISO/IEC 14496-3 (MPEG-4 Audio、ASC) と ISO/IEC 13818-7 (ADTS) とする。本リポジトリに `refs/` は無い。下記の表と拒否条件は crate の契約として固定し、実装時に一次資料と突き合わせる。

## 設計方針

本 issue の対象は **AOT 2 (AAC-LC) のみ**。SBR / PS の存在検出も含め、HE-AAC は対象外とする。AOT 2 以外、および AOT 2 でも GASpecificConfig 必須 3 フラグの後ろで入力が終端していない場合 (後続バイトや explicit SBR / PS 拡張) は、AAC-LC として黙って読み替えず `crate::Error` を返す。ゼロ埋めの後続バイトも拒否する。

### モジュール構成

`src/lib.rs` から公開する `bitstream` モジュール配下に AAC 用サブモジュールを追加する。`mod.rs` は使わない。

```text
src/bitstream.rs
src/bitstream/aac.rs
```

`src/bitstream.rs` は既にあるので `pub mod aac;` を追記する。本体は `src/bitstream/aac.rs`。open の 0062 / 0063 / 0064 と並列実装する場合、`src/bitstream.rs` の追記が競合し得る。AAC の解析は NAL 層に依存せず、`bitstream::aac` 単独で完結する。ビット読み取りは `aac.rs` 内の非公開実装とし、`vp9.rs` の `BitReader` を共有しない。

### AudioSpecificConfig 解析 API

`bitstream::aac` は ASC を解析する API と、受理した構造化値から正規形バイト列をエンコードする API を公開する。返す情報は次に限定する。

- audio object type (常に 2。5 ビット値が 31 のエスケープ形式は拒否)
- sampling frequency index (4 ビット)
- 実効サンプリング周波数 (Hz)
- channel configuration (4 ビット、値は 1..=7)

サンプリング周波数の crate 契約 (ISO/IEC 14496-3 の表に対応):

- index 0..=12: 96000 / 88200 / 64000 / 48000 / 44100 / 32000 / 24000 / 22050 / 16000 / 12000 / 11025 / 8000 / 7350
- index 13 / 14: reserved。拒否
- index 15 (`0xF`): 後続 24 ビットを明示周波数 (Hz) として読む。 0 は拒否する

channel configuration の crate 契約 (index 7 だけチャンネル数と一致しない):

- 0: PCE で定義。本 issue では拒否
- 1..=6: そのままチャンネル数
- 7: 8 チャンネル
- 8..=15: reserved。拒否

GASpecificConfig の必須 3 フラグ (`frameLengthFlag` / `dependsOnCoreCoder` / `extensionFlag`) はすべて 0 のみ受理する。 1 つでも 1 なら、後続の `coreCoderDelay` や PCE を読まずに拒否する。フラグを公開型の可変フィールドにはしない。

入力バイト列は、上記を読み切った位置で終端していなければならない。後続バイトがある入力 (例: `tests/testdata/beep-aac-audio.mp4` の `DecoderSpecificInfo::payload` である 5 バイト `12 08 56 e5 00`。先頭 16 ビットは AOT 2 / 44.1 kHz / mono だが、余りが SBR の `syncExtensionType` `0x2B7`) は拒否する。短すぎる入力も拒否する。

エンコードは受理条件を満たす構造化値だけを正規形バイト列にする。フラグ 3 つは 0 で書き、明示周波数以外は 2 バイト、index `0xF` のときは 5 バイトになる。受理した入力に対する `encode(parse(input))` は入力と一致する。

実装では上記の受理条件を型で保証する方針にした。`audio_object_type` は `AudioObjectType` (単一 variant)、`channel_configuration` は `ChannelConfiguration` (7 variant)、サンプリング周波数は `SamplingFrequency` (priv field の不透明 struct) で表し、index 0..=12 と Hz の対応・明示周波数 1..=16777215 (0 は拒否) は `SamplingFrequency::from_hz` と parse が保証する。このため「index と Hz が食い違う手組み」は表現できず、encode / `build_mp4a_box` / `wrap_raw_aac_in_adts` の入力検証は不要になった (`encode_audio_specific_config` は非 Result)。構築 API の `DecoderSpecificInfo::payload` にはこの正規形を格納する (入力生バイトを「余りごと」コピーしない)。

以下は公開 API の骨格を示す (型名・関数名は実装時に既存 API と整合させて調整可)。

```rust
/// AAC の Audio Object Type (現状は AOT 2 のみ)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioObjectType {
    /// AOT 2 (AAC-LC)
    AacLc,
}

/// ASC のサンプリング周波数 (不透明 struct)
///
/// 標準テーブル (index 0..=12) に一致する Hz は index 形式、それ以外は明示形式
/// (24 ビット) になり、形式は `SamplingFrequency::from_hz` が自動選択する
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SamplingFrequency {
    // 非公開フィールド
}

impl SamplingFrequency {
    pub fn from_hz(hz: u32) -> Result<Self>; // 0 と 24 ビット超過は Err
    pub fn hz(self) -> u32;                  // 実効周波数 (Hz)。常に有効
}

/// AAC のチャンネル構成 (1..=7。7 は 8 チャンネル)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelConfiguration {
    Mono,          // 1
    Stereo,        // 2
    Channels3,     // 3
    Channels4,     // 4
    Channels5,     // 5
    FivePointOne,  // 6 (5.1)
    SevenPointOne, // 7 (8 チャンネル / 7.1)
}

/// 受理した AAC-LC の AudioSpecificConfig
///
/// 全フィールドが型で受理条件を保証されるため、不正な手組みは表現できない
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AudioSpecificConfig {
    pub audio_object_type: AudioObjectType, // 常に AacLc
    pub sampling_frequency: SamplingFrequency,
    pub channel_configuration: ChannelConfiguration,
}

pub fn parse_audio_specific_config(input: &[u8]) -> Result<AudioSpecificConfig>;
pub fn encode_audio_specific_config(config: &AudioSpecificConfig) -> Vec<u8>; // 非 Result
```

### ADTS 解析 API と ADTS と raw AAC の相互変換

`bitstream::aac` は ADTS フレーム (ヘッダー + raw data block) を解析する API と、raw AAC と ASC から ADTS フレームを組み立てる API を公開する。

ADTS 側のフィールド幅は ASC と異なる。

- `profile` は 2 ビット。AOT は `profile + 1`。本 issue では AOT 2 のみなので `profile` は 1 以外を拒否する
- `sampling_frequency_index` は 4 ビット。0..=12 以外 (13 / 14 / 15) は拒否する。ADTS に 24 ビット明示周波数は無い。ASC の index `0xF` から ADTS への変換は拒否する
- `channel_configuration` は **3** ビット。値 1..=7 以外を拒否する。載せるのは ASC の index (1..=7) であり、`channelcount` へ展開したチャンネル数 (index 7 のとき 8) ではない。ASC の 4 ビット値 8..=15 はもともと拒否済みなので、受理した ASC からの変換では 3 ビットに載る

`number_of_raw_data_blocks_in_frame` は 0 (raw data block 1 個) のみ受理する。0 以外は `crate::Error`。このときヘッダーは `protection_absent == 1` なら 7 バイト、`== 0` なら CRC 16 ビット付き 9 バイト。CRC 値の検証はしない (バイトを読み飛ばすだけ)。CRC 生成は対象外のため、組み立て側の `protection_absent` は常に 1 (CRC なし) に固定する。

その他の組み立て固定値:

- `layer` = 0
- `private_bit` = 0
- copyright 識別ビット = 0
- `adts_buffer_fullness` = `0x7FF` (VBR 慣習値)
- `number_of_raw_data_blocks_in_frame` = 0

呼び出し側が明示する組み立て値は MPEG バージョン (ID ビット)、`original_copy`、`home` に限定する。解析結果はこれらの再指定に必要なフィールドを返す。ヘッダー全体のビット一致までは要求しない (`buffer_fullness` 等は固定値で書き直す)。意味の往復は AOT / 周波数 index / channel configuration / MPEG バージョン / `original_copy` / `home` / raw AAC ペイロードで保証する。

`frame_length` はヘッダー込みの 13 ビット値。ヘッダー長未満、入力末尾を超える、または raw AAC + ヘッダーが 13 ビットに収まらない組み立ては拒否する。 syncword (`0xFFF`) 不一致、`layer != 0`、切り詰めも拒否する。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdtsMpegVersion {
    Mpeg4, // ID = 0
    Mpeg2, // ID = 1
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdtsHeader {
    pub mpeg_version: AdtsMpegVersion,
    pub protection_absent: bool,
    pub audio_object_type: AudioObjectType, // 常に AacLc
    pub sampling_frequency_index: u8, // 0..=12
    pub channel_configuration: ChannelConfiguration,
    pub frame_length: u16,
    pub original_copy: bool,
    pub home: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdtsEncodeConfig {
    pub mpeg_version: AdtsMpegVersion,
    pub original_copy: bool,
    pub home: bool,
}

pub fn parse_adts_frame(input: &[u8]) -> Result<(AdtsHeader, &[u8])>; // 第 2 値は raw AAC
pub fn wrap_raw_aac_in_adts(
    raw: &[u8],
    asc: &AudioSpecificConfig,
    config: &AdtsEncodeConfig,
) -> Result<Vec<u8>>;
```

### サンプルエントリー構築 API

解析済み ASC と呼び出し側設定から `Mp4aBox` を 1 つ返す。`SampleEntry` には包まない。`EsdsBox` は `Mp4aBox::esds_box` として中に置く (公開 API を `Mp4aBox` と `EsdsBox` の 2 関数にはしない)。定義場所は `EsdsBox` が `src/boxes_moov_tree.rs`、それ以外は既存の再エクスポートを使う。`bitstream/aac.rs` 側で組み立て、descriptors / moov_tree の型定義は変えない。

#### 固定値 (関数側で埋める)

- `AudioSampleEntryFields::data_reference_index` = `DEFAULT_DATA_REFERENCE_INDEX`
- `AudioSampleEntryFields::samplesize` = `DEFAULT_SAMPLESIZE` (16)
- `Mp4aBox::unknown_boxes` = 空 `Vec`
- `EsDescriptor::stream_priority` = `LOWEST_STREAM_PRIORITY`
- `EsDescriptor::depends_on_es_id` / `url_string` / `ocr_es_id` = `None`
- `EsDescriptor::sl_config_descr` = `SlConfigDescriptor` (既存 encode が `predefined = 2` を書く)
- `DecoderConfigDescriptor::object_type_indication` = `OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3`
- `DecoderConfigDescriptor::stream_type` = `STREAM_TYPE_AUDIO`
- `DecoderConfigDescriptor::up_stream` = `UP_STREAM_FALSE`
- `DecoderConfigDescriptor::dec_specific_info` = `Some` (`payload` は `encode_audio_specific_config` の正規形)

#### ストリーム導出値 (ASC から写す)

- `AudioSampleEntryFields::channelcount` = 上表のチャンネル数 (`u16`)。 index 7 は 8 チャンネル
- `AudioSampleEntryFields::samplerate`: 実効周波数が `u16` に収まる (1..=65535) ときは `FixedPointNumber::new(hz as u16, 0)`。収まらないとき (96000 / 88200、および明示周波数が 65535 超) は切り捨てず `FixedPointNumber::new(0, 0)` とし、真値は ASC payload 側に残す。Hisui の `SampleRate::as_u16` エラーを crate の契約にしない

#### 呼び出し側指定値

- `es_id`: 0 は予約なので拒否 (`EsDescriptor::MIN_ES_ID` 以上)
- `buffer_size_db`: 24 ビット (0..=16777215) に収まる値。収まらなければ `Uint::new` に渡さず `crate::Error` (`DecoderConfigDescriptor::encode` は `to_bits()` の上位 1 バイトを捨てるため、黙って切り捨てない)
- `max_bitrate` / `avg_bitrate`

Hisui のビットレート固定値 (`buffer_size_db = 65536`、`max_bitrate = 256000`、`avg_bitrate = 128000`) と、モノラル / ステレオ以外を拒否する制限は移植しない。

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Mp4aSampleEntryConfig {
    pub es_id: u16,
    pub buffer_size_db: u32,
    pub max_bitrate: u32,
    pub avg_bitrate: u32,
}

pub fn build_mp4a_box(
    asc: &AudioSpecificConfig,
    config: &Mp4aSampleEntryConfig,
) -> Result<Mp4aBox>;
```

公開 API は `no_std` を維持し、crate 本体に新しい外部依存は追加せず、エラーは既存の `crate::Error` に統合する。

### テスト

- 単体テスト (`tests/test_bitstream_aac.rs`): 正規形 ASC (`0x11 0x90` の 48 kHz stereo、`0x12 0x10` の 44.1 kHz stereo など)、ADTS 7 バイト (nrdb = 0)。拒否: 短い入力、AOT 2 以外、sfi 13 / 14、channel 0 / 8..=15、3 フラグのいずれかが 1、後続バイトあり (fixture の `12 08 56 e5 00`)、`buffer_size_db` の 24 ビット超過、ADTS の syncword 不一致 / nrdb != 0 / profile != 1 / 境界超過。96 kHz (index 0) の構築で `samplerate.integer == 0` かつ payload に 96000 が残ること。「index と Hz が食い違う手組み」は型で表現できないため拒否テストは存在しない (型による保証の確認は `SamplingFrequency::from_hz` の往復で行う)
- PBT (`pbt/tests/prop_bitstream_aac.rs`): 受理条件内の ASC の `encode` / `parse` 往復、ADTS の意味往復。`pbt/Cargo.toml` の noprop は追加済みなので依存は足さない
- 実データ: `beep-aac-audio.mp4` の 5 バイト ASC は成功例にしない (SBR 拡張の拒否例)。成功の実データとして ADTS ストリームを `tests/testdata/` に追加する。ネットワークや外部コマンドなしでテストが完結すること
- Fuzzing: `fuzz/fuzz_targets/fuzz_bitstream_aac.rs` に ASC パーサーと ADTS パーサーを対象とするターゲットを追加し、`fuzz/Cargo.toml` に `[[bin]]` を追加する

### 対象外

- SBR / PS (explicit / implicit) の解析と存在検出。implicit SBR は 2 バイト AAC-LC ASC からは判別できないので、デコーダー側の方針として扱わない
- `dependsOnCoreCoder == 1` / `extensionFlag == 1` / PCE (`channelConfiguration == 0`) の構文解析
- `number_of_raw_data_blocks_in_frame != 0` の複数 block、ADTS CRC の生成と値の検証
- LATM / LOAS
- Hisui / Sora Rust SDK 側の呼び出し置換と依存バージョン更新
- RTP / SDP、デコーダーやエンコーダー固有のポリシー、コーデック文字列生成
- C API / WASM バインディング

## 完了条件

- `bitstream::aac` が公開され、AAC-LC の ASC 解析と正規形エンコード、ADTS フレーム解析、ADTS 組み立て、`Mp4aBox` 構築が利用できること
- AOT 2 以外、GASpecificConfig 必須 3 フラグの非ゼロ、後続の SBR/PS 拡張、channel configuration 0 / 8..=15、sfi 13 / 14、ADTS の `number_of_raw_data_blocks_in_frame != 0` を `crate::Error` として拒否し、AAC-LC として黙って読み替えないこと
- `build_mp4a_box` が `Mp4aBox` を返し `SampleEntry` に包まないこと。固定値 / ストリーム導出 / 呼び出し側指定が設計方針の三分類どおりであること。`channelcount` は index 7 を 8 に写し、`samplerate` は `u16` に収まらない周波数を切り捨てず 0 にすること。`buffer_size_db` が 24 ビットに収まらない値は `build_mp4a_box` で `crate::Error` であること。`sampling_frequency_index` と `sampling_frequency` の食い違いは `AudioSpecificConfig` の型設計により構築不能であること
- Hisui の AOT 無視、モノラル / ステレオ以外の拒否、ビットレート固定、`SampleEntry` ラップを移植していないこと
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と crate 本体の依存ライブラリ 0 を維持すること (pbt 側の noprop は crate 本体の依存ではない)
- 決定的テスト、`noprop` PBT、実データ fixture、fuzz target が追加され、`fuzz/Cargo.toml` に `[[bin]]` エントリが登録されていること
- 公開 API の rustdoc に受理する入力、拒否条件、`samplerate` が 0 になる周波数、ADTS 組み立ての固定ビットが記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
