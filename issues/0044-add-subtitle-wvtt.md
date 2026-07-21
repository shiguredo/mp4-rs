# wvtt (WVTTSampleEntry) サンプルエントリー対応を追加する

- Priority: Low
- Created: 2026-07-21
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-subtitle-wvtt
- Polished: YYYY-MM-DD

## 目的

WebVTT を ISO BMFF に格納する `wvtt` サンプルエントリー（`WVTTSampleEntry`、ISO/IEC 14496-30）の decode / encode 対応を追加する。HLS の fMP4 プロファイルと DASH で現役の標準であり、Web 系プレイヤーとの親和性が最も高い。

## 優先度根拠

Low。緊急要求は無いが、Web 系プレイヤー向けに字幕付き fMP4 を書き出すうえで本命となる方式。

## 現状

- `src/boxes_sample_entry.rs:17` `SampleEntry` に `wvtt` バリアントは存在しない
- 字幕トラック自体を扱う共通基盤も未整備（0042 で対応する）

## 設計方針

ISO/IEC 14496-30 に従い、`WVTTSampleEntry` (`wvtt`) を追加する。

サンプルエントリー本体は以下の子ボックスを持つ:

- `vttC` (WebVTTConfigurationBox): WebVTT ヘッダー（"WEBVTT" 行を含む文字列。必須）
- `vlab` (WebVTTSourceLabelBox): 任意
- `btrt` (BitRateBox): 任意

サンプルデータの扱い方針:

- サンプルデータは `vttc`（cue）/ `vtte`（empty cue）/ `vtta`（additional cue text）等のボックス列で構成される
- 本 issue では **サンプルデータ全体は不透明なバイト列** として扱い、内部構造の parse / build は consumer 側に委ねる
- 理由: 既存の映像・音声サンプルの扱いと一貫させ、実装スコープを抑えるため
- 追加で内部構造の型付き対応が必要になった場合は別 issue とする

### 後方互換性への影響

- `SampleEntry` へのバリアント追加は網羅 match を破壊するため SemVer 上のブレイキング扱い。ただし `Unknown` フォールバックがあるため decode 側の未知バリアント互換は維持される

## 依存関係

- 0042（共通基盤）の完了が前提
- 0046（`Mp4FileMuxer` の Subtitle 受け入れ）は「MP4 のラウンドトリップ」検証で前提となる。0046 未完了時は `Fmp4SegmentMuxer` 経由の fMP4 ラウンドトリップのみで完了と判断する

## 完了条件

- `wvtt` サンプルエントリーの decode / encode ラウンドトリップができる
- `vttC` サブボックスの decode / encode ができる
- 実サンプルデータ（不透明バイト列扱い）を含む fMP4 のラウンドトリップができる（`Fmp4SegmentMuxer` / `Fmp4SegmentDemuxer` 経由）
- 0046 完了後、`Mp4FileMuxer` / `Mp4FileDemuxer` 経由の MP4 ラウンドトリップも検証する
- 既存の SampleEntry の動作が変わらない
- `cargo clippy` が通る

## 解決方法

0042 の完了後に着手する。

1. `VttCBox` を実装（`Encode` / `Decode` / `BaseBox`）
2. `WvttBox` を実装（子として `VttCBox` を含む）
3. `SampleEntry::Wvtt(WvttBox)` を追加
4. handler type / Media Header の対応方針を 0042 側と揃える
5. C API / WASM API に必要な露出を追加
6. PBT・単体テストを追加

## CHANGES.md

`[ADD]` として記載する。`SampleEntry` バリアント追加による網羅 match への影響がある点は明記する。
