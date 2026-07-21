# tx3g (TX3GSampleEntry) サンプルエントリー対応を追加する

- Priority: Low
- Created: 2026-07-21
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-subtitle-tx3g
- Polished: YYYY-MM-DD

## 目的

3GPP Timed Text 形式の字幕を格納する `tx3g` サンプルエントリー（`TX3GSampleEntry`、3GPP TS 26.245）の decode / encode 対応を追加する。旧 QuickTime / iTunes 系のワークフローで作られた MP4 に現存する形式で、ffmpeg / VLC / mpv も現役で対応する。

## 優先度根拠

Low。緊急要求は無いが、旧 QuickTime / iTunes 由来の MP4 を読める側で書けないと mp4 汎用ライブラリとして片手落ちになるため、`stpp` / `wvtt` と並べて追加する。

## 現状

- `src/boxes_sample_entry.rs:17` `SampleEntry` に `tx3g` バリアントは存在しない
- 字幕トラック自体を扱う共通基盤も未整備（0042 で対応する）

## 設計方針

3GPP TS 26.245 に従い、`TX3GSampleEntry` (`tx3g`) を追加する。

サンプルエントリー本体の主要フィールド:

- `displayFlags` (u32)
- `horizontal-justification` (i8)
- `vertical-justification` (i8)
- `background-color-rgba` ([u8; 4])
- `default-text-box` (BoxRecord: [i16; 4])
- `default-style` (StyleRecord)
- 子ボックス: `ftab` (FontTableBox、必須)

サンプルデータの扱い方針:

- サンプルデータは `text_length` (u16) + テキスト本体 + 任意の modifier boxes（`styl` / `hlit` / `hclr` / `krok` / `dlay` / `href` / `tbox` / `blnk` / `twrp`）で構成される
- 本 issue では **サンプルデータ全体は不透明なバイト列** として扱い、内部構造の parse / build は consumer 側に委ねる
- 理由: 既存の映像・音声サンプルの扱いと一貫させ、実装スコープを抑えるため
- 追加で modifier boxes を型付きで扱いたくなった場合は別 issue とする

### 後方互換性への影響

- `SampleEntry` へのバリアント追加は網羅 match を破壊するため SemVer 上のブレイキング扱い。ただし `Unknown` フォールバックがあるため decode 側の未知バリアント互換は維持される

## 依存関係

- 0042（共通基盤）の完了が前提
- 0046（`Mp4FileMuxer` の Subtitle 受け入れ）は「MP4 のラウンドトリップ」検証で前提となる。0046 未完了時は `Fmp4SegmentMuxer` 経由の fMP4 ラウンドトリップのみで完了と判断する

## 完了条件

- `tx3g` サンプルエントリーの decode / encode ラウンドトリップができる
- `ftab` サブボックスの decode / encode ができる
- 実サンプルデータ（不透明バイト列扱い）を含む fMP4 のラウンドトリップができる（`Fmp4SegmentMuxer` / `Fmp4SegmentDemuxer` 経由）
- 0046 完了後、`Mp4FileMuxer` / `Mp4FileDemuxer` 経由の MP4 ラウンドトリップも検証する
- 既存の SampleEntry の動作が変わらない
- `cargo clippy` が通る

## 解決方法

0042 の完了後に着手する。

1. `FtabBox` を実装（`Encode` / `Decode` / `BaseBox`）
2. `Tx3gBox` を実装（子として `FtabBox` を含む）
3. `SampleEntry::Tx3g(Tx3gBox)` を追加
4. handler type / Media Header の対応方針を 0042 側と揃える（0042 の対応表 `tx3g → text + nmhd` に従う。QuickTime 系の `gmhd` 慣習は 0042 のスコープ外のため、必要になれば別途起票する）
5. C API / WASM API に必要な露出を追加
6. PBT・単体テストを追加

## CHANGES.md

`[ADD]` として記載する。`SampleEntry` バリアント追加による網羅 match への影響がある点は明記する。
