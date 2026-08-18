# AAC ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/add-aac-bitstream-utilities
- Polished: {YYYY-MM-DD}

## 目的

AAC の AudioSpecificConfig と ADTS ヘッダーを解析し、`mp4a` / `esds` の構築、および ADTS <-> raw AAC 相互変換を `shiguredo_mp4` の汎用ユーティリティとして提供する。

これらは MP4 ボックス自体の処理ではないが、AAC ストリームから `Mp4aBox` を構築する場合と、MP4 サンプルをデコーダーへ渡せる形式 (ADTS) に変換する場合の双方で必要になる。

## 現状

- `src/boxes_sample_entry.rs` の `Mp4aBox` および `src/descriptors.rs` の `EsDescriptor` / `DecoderConfigDescriptor` / `DecoderSpecificInfo` は既に存在するが、`DecoderSpecificInfo::payload` に格納される AudioSpecificConfig 本体を解析する API はない
- ADTS ヘッダー解析、および ADTS <-> raw AAC 相互変換の API もない
- `DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3` (`0x40`) や `STREAM_TYPE_AUDIO` (`0x05`) の定数は既に定義されているが、AudioSpecificConfig の中身 (audio object type、サンプリング周波数、チャンネル構成) は呼び出し側が生バイト列として組み立てる必要がある
- `shiguredo/hisui` や `shiguredo/sora-rust-sdk` など利用側で同種の解析を重複実装している可能性がある (要確認)

参照仕様は [ISO/IEC 14496-3 (MPEG-4 Audio)](https://www.iso.org/standard/76383.html) および ISO/IEC 13818-7 (ADTS) とする。

## 設計方針

### モジュール構成

`src/lib.rs` から公開する `bitstream` モジュール配下に AAC 用のサブモジュールを追加する。`mod.rs` は使わない。

```text
src/bitstream.rs
src/bitstream/aac.rs
```

`src/bitstream.rs` が既に他 issue (0062〜0066) の実装で追加済みであれば `pub mod aac;` を追記するだけで足りる。未追加なら本 issue で新設する。AAC の解析処理は NAL 層 (`src/bitstream/nal.rs`) に依存せず、`bitstream::aac` 単独で完結する。

### AudioSpecificConfig 解析 API

`bitstream::aac` は AudioSpecificConfig (以下 ASC) を解析する API を公開する。ASC は ISO/IEC 14496-3 で規定されるビットストリームで、少なくとも次の情報を返す。

- audio object type (AOT。5 ビットまたはエスケープ形式で 11 ビット)
- sampling frequency index (4 ビット。0xF はエスケープで 24 ビットの明示サンプリング周波数が続く)
- 実効サンプリング周波数 (index から表引き、または明示値から取得)
- channel configuration (4 ビット)
- AOT 2 (AAC-LC) の場合の GASpecificConfig の必須部分 (frameLengthFlag / dependsOnCoreCoder / extensionFlag)

対応する AOT の範囲は少なくとも AAC-LC (AOT 2) を含む。SBR (HE-AAC v1、explicit / implicit 双方) と PS (HE-AAC v2) の扱いは実装時に決めるが、少なくとも「AOT 2 以外の入力を黙って読み替えない」ことを保証する。

短すぎる入力、reserved AOT、reserved sampling frequency index、reserved channel configuration、境界超過は `crate::Error` を返す。

### ADTS 解析 API と ADTS <-> raw AAC 相互変換

`bitstream::aac` は ADTS ヘッダー (7 バイトまたは CRC 付き 9 バイト) を解析する API と、ADTS フレーム列と raw AAC フレーム列を相互変換する API を公開する。

- ADTS ヘッダーから、AOT (profile + 1)、sampling_frequency_index、channel_configuration、frame_length、number_of_raw_data_blocks_in_frame を取得する
- ADTS フレーム列を raw AAC フレーム列に変換する。number_of_raw_data_blocks_in_frame が 0 (1 raw data block) 以外の入力の扱いを明示する
- raw AAC フレーム列と ASC から ADTS フレーム列を組み立てる。ADTS ヘッダーの MPEG バージョン、protection_absent、home、original_copy などの固定/選択値を呼び出し側が明示する

syncword 不一致、frame_length フィールドが入力境界を超える、切り詰められた ADTS 入力は `crate::Error` を返す。

### サンプルエントリー構築 API

解析済み ASC と呼び出し側の設定から、具体的な `Mp4aBox` および `EsdsBox` を構築する API を追加する。

- `Mp4aBox::audio` (`AudioSampleEntryFields`) の channelcount / samplerate は ASC から反映する
- `EsdsBox` / `EsDescriptor` / `DecoderConfigDescriptor` / `DecoderSpecificInfo` の各定数 (`OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3` / `STREAM_TYPE_AUDIO` / `UP_STREAM_FALSE`) を使用する
- `DecoderSpecificInfo::payload` には ASC の生バイト列を格納する (解析 API から得た構造化値から再エンコードした結果と入力バイト列が一致することを保証する)
- `es_id`、`buffer_size_db`、`max_bitrate`、`avg_bitrate` は呼び出し側が明示する値として受け取る

公開 API は `no_std` を維持し、crate 本体 (`shiguredo_mp4`) に新しい外部依存は追加せず、エラーを既存の `crate::Error` に統合する。

### テスト

- 単体テスト (`tests/test_bitstream_aac.rs`): AOT 2 の代表的な AudioSpecificConfig (44.1 kHz stereo、48 kHz mono など)、ADTS 7 バイトヘッダーの決定的テスト。境界エラー (short input、reserved 値、境界超過、syncword 不一致) の拒否確認
- PBT (`pbt/tests/prop_bitstream_aac.rs`): `noprop` サンプラーで生成した ASC / ADTS のラウンドトリップと不変条件を検証する (`pbt/Cargo.toml` に `noprop` 依存を追加する。既存 proptest との共存は 0068 で解消される)
- 実データ fixture: `tests/testdata/beep-aac-audio.mp4` が既に存在するので、その `esds` から抽出した ASC バイト列を fixture として利用する。追加で ADTS ストリームの実データも同ディレクトリに置く
- Fuzzing: `fuzz/fuzz_targets/fuzz_bitstream_aac.rs` に AudioSpecificConfig パーサーと ADTS パーサーの fuzz target を追加し、`fuzz/Cargo.toml` に `[[bin]]` エントリを追加する

### 対象外

- SBR / PS の詳細構文解析 (存在検出だけ行い、詳細解析は別 issue とする可能性を残す)
- LATM / LOAS 形式の解析
- Hisui / Sora Rust SDK 側の呼び出し置換と依存バージョン更新
- RTP / SDP、デコーダーやエンコーダー固有のポリシー、コーデック文字列生成 (RFC 6381 の `codecs=` パラメータ生成など)
- C API / WASM バインディング。利用要件が明確になった時点で別 issue とする

## 完了条件

- `bitstream::aac` が公開され、AudioSpecificConfig 解析、ADTS ヘッダー解析、ADTS <-> raw AAC 相互変換、`Mp4aBox` 構築が利用できること
- AOT 2 (AAC-LC) の解析と構築が確実に動作すること。それ以外の AOT を「AAC-LC として黙って扱う」ことをしないこと
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と crate 本体の依存ライブラリ 0 を維持すること
- 決定的テスト、`noprop` PBT、実データ fixture、fuzz target が追加され、`fuzz/Cargo.toml` に `[[bin]]` エントリが登録されていること
- 公開 API の rustdoc に入力形式、返すバイト範囲、エラー条件が記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
