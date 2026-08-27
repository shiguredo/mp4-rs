# SampleEntry からコーデック文字列を生成する

- Created: 2026-08-27
- Completed: {YYYY-MM-DD}
- Branch: feature/add-codec-strings
- Polished: 2026-08-27

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

既知の `SampleEntry` は次の規則で文字列化する。hex の大小文字は各コーデックの慣例に合わせる（H.264 / AAC の OTI は小文字、HEVC は大文字）。

- `Avc1`: `avc1.` + `AvccBox` の `avc_profile_indication` / `profile_compatibility` / `avc_level_indication` をこの順に並べた 6 桁の小文字 hex（例: `avc1.640028`）。`H264ProfileLevelId::to_hex` と同じ表記でよい
- `Hev1` / `Hvc1`: ISO/IEC 14496-15 Annex E に基づき、次の完成形とする（プレフィックスは sample entry 4CC。`hev1` / `hvc1`）

  `{4CC}.{space}{profile_idc}.{compatHex}.{tier}{level}(.{XX})*`

  - `space`: `HvccBox::general_profile_space` が 0 なら空、1→`A`、2→`B`、3→`C`
  - `profile_idc`: `general_profile_idc` の十進（先頭ゼロなし）
  - `compatHex`: `general_profile_compatibility_flags` を bit-reverse した 32 bit 値の大文字 hex（先頭ゼロは省略可。値が 0 なら `0`）
  - `tier`: `general_tier_flag` が 0 なら `L`、1 なら `H`
  - `level`: `general_level_idc` の十進（先頭ゼロなし）
  - constraint: `general_constraint_indicator_flags`（48 bit）を 6 バイトの大文字 2 桁 hex（`.` 区切り）にする。末尾のゼロバイトは省略する。全バイトがゼロのときは最低 1 バイト `00` を残す（サフィックスごと省略しない）
  - 例: `hev1.1.6.L93.B0`
- `Av01`: AV1 Codec ISO Media File Format Binding v1.3.0 Section 5 の必須形 `av01.<profile>.<level><tier>.<bitDepth>` のみを `Av1cBox` から生成する。任意フィールド（monochrome / chroma / CICP / range）は付けない

  - `profile`: `seq_profile` の十進 1 桁
  - `level`: `seq_level_idx_0` の 2 桁十進（ゼロ埋め）
  - `tier`: `seq_tier_0` が 0 なら `M`、1 なら `H`
  - `bitDepth`: AV1 の `BitDepth` と同じ導出（`bitstream::av1` の `color_config` 解釈と一致させる）。`seq_profile == 2` かつ `high_bitdepth` なら `twelve_bit` で 12 / 10、それ以外で `high_bitdepth` なら 10、そうでなければ 8。2 桁十進（ゼロ埋め）
  - 例: `av01.0.01M.08`
- `Vp08` / `Vp09`: VP Codec ISO Media File Format Binding の必須形 `<4CC>.<profile>.<level>.<bitDepth>` のみを `VpccBox` から生成する。任意欄（`chromaSubsampling` / `colourPrimaries` / `transferCharacteristics` / `matrixCoefficients` / `videoFullRangeFlag`）は相互包含のため、色特性に限らず一切付けない。各数値は 2 桁十進（ゼロ埋め）。4CC は `vp08` / `vp09`
  - 例: `vp09.00.31.08`
- `Mp4a`: 常に `mp4a.` + `DecoderConfigDescriptor::object_type_indication` の 2 桁小文字 hex とする（例: `mp4a.40`）。OTI が `0x40` のときだけ `DecoderSpecificInfo::payload` 先頭から `audioObjectType` を読み、`mp4a.40.<AOT>` とする。`<AOT>` は十進（先頭ゼロなし。例: `mp4a.40.2`）。AOT が 31 のエスケープ形式は先頭 5 bit が 31 のとき続き 6 bit を読み、`32 + その値` を十進で出す
- `Opus` / `Flac` / `Stpp` / `Wvtt` / `Tx3g`: 各ボックスの `TYPE`（登録済み sample entry 4CC）をそのまま返す。返す値は `Opus` / `fLaC` / `stpp` / `wvtt` / `tx3g`（大文字小文字を含む）

`SampleEntry::Unknown` は、未知ボックスの意味や RFC 6381 の追加部分を判断できないため `ErrorKind::Unsupported` とする。

`Mp4aBox` で object type `0x40` に必要な `DecoderSpecificInfo` がない、または AOT のビット列が切り詰められている場合は、AAC-LC を仮定せず `ErrorKind::InvalidData` とする。ASC 全体の対応可否は `bitstream::aac::parse_audio_specific_config` の AAC-LC 制限と分離し、ここでは `codecs` 文字列に必要な AOT だけを読む。

### テスト

- `tests/test_codec_string.rs` に各 `SampleEntry` の仕様例（H.264 / HEVC / AV1 / VP / AAC / 4CC のみ）、HEVC constraint の末尾ゼロ省略と全ゼロ時の `00`、AAC の通常 AOT / エスケープ AOT / 欠落エラー、`Unknown` のエラーを追加する
- `pbt/tests/prop_codec_string.rs` に任意の H.264 3 バイトが常に 6 桁小文字 hex へ保存されることと、HEVC constraint bytes の非ゼロ末尾が失われないことを確認する PBT を追加する
- mock / stub、外部 command、ネットワークは使用しない

### 変更履歴

`CHANGES.md` の develop に `[ADD]` として、`SampleEntry` から RFC 6381 と各 binding のコーデック文字列を生成する API を追加したことを記載する。

## 完了条件

- `codec_string::from_sample_entry` が公開される
- H.264 / H.265 / AV1 / VP8 / VP9 / AAC の構造化フィールドから、上記設計方針どおりの文字列が生成される
- Opus / FLAC は ISOBMFF の sample entry 4CC と同じ `Opus` / `fLaC` になる
- AAC の情報欠落を AAC-LC として補完しない
- `SampleEntry::Unknown` が `ErrorKind::Unsupported` になる
- 決定的テストと PBT が追加される
- `CHANGES.md` が更新される
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る
