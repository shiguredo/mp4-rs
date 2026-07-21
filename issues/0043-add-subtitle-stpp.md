# stpp (XMLSubtitleSampleEntry) サンプルエントリー対応を追加する

- Priority: Low
- Created: 2026-07-21
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-subtitle-stpp
- Polished: YYYY-MM-DD

## 目的

XML 形式の字幕（TTML / IMSC 等）を格納する `stpp` サンプルエントリー（`XMLSubtitleSampleEntry`、ISO/IEC 14496-30）の decode / encode 対応を追加する。DASH の IMSC / TTML 系ワークフローで実装標準となっており、broadcast / OTT 系のファイルで実際に使われている。

## 優先度根拠

Low。緊急要求は無いが、DASH IMSC 系のファイルを読み書きするうえで必要な最小構成の一つ。

## 現状

- `src/boxes_sample_entry.rs:17` `SampleEntry` に `stpp` バリアントは存在しない
- 字幕トラック自体を扱う共通基盤も未整備（0042 で対応する）

## 設計方針

ISO/IEC 14496-30 に従い、`XMLSubtitleSampleEntry` (`stpp`) を追加する。

サンプルエントリー本体のフィールド:

- `namespace`: null-terminated string（XML 名前空間 URI のスペース区切り）
- `schema_location`: null-terminated string（対応するスキーマの URL、任意）
- `auxiliary_mime_types`: null-terminated string（任意）

サンプルデータ自体は XML ドキュメント（TTML / IMSC）で、可変長のバイト列として不透明に扱う（parse は consumer 側の責務）。既存の映像・音声サンプルの扱いと一貫させる。

想定される付随ボックスは以下（存在する場合のみ格納）:

- `btrt` (BitRateBox): 既存の対応と合わせる
- 名前空間関連の任意ボックス

### 後方互換性への影響

- `SampleEntry` へのバリアント追加は網羅 match を破壊するため SemVer 上のブレイキング扱い。ただし `Unknown` フォールバックがあるため decode 側の未知バリアント互換は維持される
- 0042 の共通基盤変更に伴うブレイクは 0042 側でカバーする

## 依存関係

- 0042（共通基盤）の完了が前提

## 完了条件

- `stpp` サンプルエントリーの decode / encode ラウンドトリップができる
- 実サンプルデータ（TTML / IMSC の XML）を含む MP4 のラウンドトリップができる
- 既存の SampleEntry の動作が変わらない
- `cargo clippy` が通る

## 解決方法

0042 の完了後に着手する。

1. `StppBox` を実装（`Encode` / `Decode` / `BaseBox`）
2. `SampleEntry::Stpp(StppBox)` を追加
3. handler type / Media Header の対応方針を 0042 側と揃える
4. C API / WASM API に必要な露出を追加
5. PBT・単体テストを追加

## CHANGES.md

`[ADD]` として記載する。`SampleEntry` バリアント追加による網羅 match への影響がある点は明記する。
