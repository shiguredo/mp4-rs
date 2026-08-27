# SampleEntry からコーデック文字列を生成する

- Created: 2026-08-27
- Completed: {YYYY-MM-DD}
- Branch: feature/add-codec-strings
- Polished: {YYYY-MM-DD}

## 目的

MP4 の `SampleEntry` が保持するコーデック設定から、RFC 6381 および各コーデックの ISOBMFF binding に準拠した `codecs` パラメーター文字列を生成できるようにする。

HLS / DASH などの利用側が `avcC` / `hvcC` / `av1C` / `vpcC` / `esds` を個別に再解釈する重複をなくし、MP4 に格納した設定とマニフェストの表記が食い違うことを防ぐ。

## 現状

- `src/boxes_sample_entry.rs` の `SampleEntry` はコーデック別の設定ボックスを保持するが、`codecs` パラメーター文字列へ変換する API はない
- `src/bitstream/h264.rs` の `H264ProfileLevelId::to_hex` は H.264 の 3 バイトを文字列化できるが、`Avc1Box` から文字列を生成する処理は利用側に残る
- Hisui の `src/codec_string.rs` は H.264 / H.265 / AV1 / VP8 / VP9 / AAC / Opus の文字列生成を独自実装しており、同じボックスを mp4-rs と Hisui の 2 箇所で解釈している
- Hisui の AAC 実装は `DecoderSpecificInfo` がない場合に AAC-LC を仮定するが、サンプルエントリーから確認できない情報を汎用 API が補完すべきではない
- Hisui は VP8 を `vp8`、Opus を `opus` としているが、ISOBMFF の sample entry 4CC はそれぞれ `vp08`、`Opus` である。既存文字列をそのまま移植せず、コンテナの規格に合わせる必要がある

参照仕様は次のとおり。

- RFC 6381: <https://www.rfc-editor.org/rfc/rfc6381>
- ISO/IEC 14496-15 Annex E（HEVC の `codecs` パラメーター）
- AV1 Codec ISO Media File Format Binding v1.3.0 Section 5: <https://aomediacodec.github.io/av1-isobmff/v1.3.0.html>
- VP Codec ISO Media File Format Binding の Codecs Parameter String: <https://www.webmproject.org/vp9/mp4/>
- MP4 Registration Authority の codec 登録: <https://mp4ra.org/registered-types/codecs>

## 設計方針

### モジュールと公開 API

`src/codec_string.rs` を新設し、`src/lib.rs` から `pub mod codec_string;` として公開する。ビットストリームそのものではなく、構築済みの `SampleEntry` を解釈する機能なので `bitstream` 配下には置かない。

公開 API は次の 1 関数に限定する。コーデックごとの書式処理は private 関数に分け、公開トレイトや re-export は追加しない。

```rust
pub fn from_sample_entry(entry: &SampleEntry) -> Result<String>;
```

既知の `SampleEntry` は次の規則で文字列化する。

- `Avc1`: `avc1.PPCCLL`。`AvccBox` の `avc_profile_indication` / `profile_compatibility` / `avc_level_indication` をこの順の 6 桁 hex にする
- `Hev1` / `Hvc1`: ISO/IEC 14496-15 Annex E の profile space / profile IDC / bit reverse した compatibility flags / tier / level / constraint bytes を使う。constraint bytes は末尾のゼロバイトを省略する
- `Av01`: AV1 binding で必須の `av01.P.LLT.DD` までを `Av1cBox` から生成する。色特性は `Av1cBox` だけでは決定できないため、任意フィールドは付けない
- `Vp08` / `Vp09`: VP binding で必須の `<4CC>.PP.LL.DD` を `VpccBox` から生成する。任意の色特性フィールドは付けない
- `Mp4a`: `DecoderConfigDescriptor::object_type_indication` を 2 桁 hex にする。値が `0x40` の場合は `DecoderSpecificInfo::payload` 先頭の `audioObjectType` を読み、`mp4a.40.AOT` とする。AOT 31 のエスケープ形式も 11 ビットから復元する
- `Opus` / `Flac` / `Stpp` / `Wvtt` / `Tx3g`: 登録済み sample entry 4CC を返す

`SampleEntry::Unknown` は、未知ボックスの意味や RFC 6381 の追加部分を判断できないため `ErrorKind::Unsupported` とする。

`Mp4aBox` で object type `0x40` に必要な `DecoderSpecificInfo` がない、または AOT のビット列が切り詰められている場合は、AAC-LC を仮定せず `ErrorKind::InvalidData` とする。ASC 全体の対応可否は `bitstream::aac::parse_audio_specific_config` の AAC-LC 制限と分離し、ここでは `codecs` 文字列に必要な AOT だけを読む。

### テスト

- `tests/test_codec_string.rs` に各 `SampleEntry` の仕様例、HEVC constraint bytes の末尾ゼロ省略、AAC の通常 AOT / エスケープ AOT / 欠落エラー、`Unknown` のエラーを追加する
- `pbt/tests/prop_codec_string.rs` に任意の H.264 3 バイトが常に 6 桁 hex へ保存されることと、HEVC constraint bytes の非ゼロ末尾が失われないことを確認する PBT を追加する
- mock / stub、外部 command、ネットワークは使用しない

### 変更履歴

`CHANGES.md` の develop に `[ADD]` として、`SampleEntry` から RFC 6381 と各 binding のコーデック文字列を生成する API を追加したことを記載する。

## 完了条件

- `codec_string::from_sample_entry` が公開される
- H.264 / H.265 / AV1 / VP8 / VP9 / AAC の構造化フィールドから仕様どおりの文字列が生成される
- Opus は ISOBMFF の sample entry 4CC と同じ `Opus` になる
- AAC の情報欠落を AAC-LC として補完しない
- `SampleEntry::Unknown` が `ErrorKind::Unsupported` になる
- 決定的テストと PBT が追加される
- `CHANGES.md` が更新される
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る
