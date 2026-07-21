# 字幕トラックの共通基盤（TrackKind / handler type / Media Header）を追加する

- Priority: Low
- Created: 2026-07-21
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-subtitle-track-common
- Polished: YYYY-MM-DD

## 目的

音声認識などで生成した字幕テキストを MP4 に格納できるようにするための、字幕トラック共通基盤を追加する。

具体的な Timed Text 系サンプルエントリー（`stpp` / `wvtt` / `tx3g`）の対応は別 issue（0043 / 0044 / 0045）に分離し、本 issue はそれらの前提となる、トラック種別・handler type・Media Header・demux / mux 経路の共通部分のみを扱う。

## 優先度根拠

Low。バグ由来ではなく緊急要求も無い。後続 3 方式の実装（0043 / 0044 / 0045）はいずれも本 issue に依存するため方式実装より先に着手する必要があるが、方式実装自体が Low のため本 issue も Low とする。

## 現状

トラック種別と関連ボックスは以下の 2 種類だけをサポートしている。

- `src/basic_types.rs:677` `TrackKind` は `Audio` と `Video` のみ
- `src/boxes_moov_tree.rs:920-923` `HdlrBox` の handler type 定数は `HANDLER_TYPE_SOUN` (`soun`) と `HANDLER_TYPE_VIDE` (`vide`) のみ
- `src/boxes_moov_tree.rs:986` `MinfBox::smhd_or_vmhd_box: Option<Either<SmhdBox, VmhdBox>>` で音声/映像の 2 種類の Media Header にしか対応していない

デマルチプレクサ側では handler type が `soun` / `vide` 以外だとトラックそのものを skip している。

- `src/demux_mp4_file.rs:511-514` の `_ => continue`
- `src/demux_fmp4_file.rs:320-323` 同様
- `src/demux_fmp4_segment.rs:145-148` 同様

マルチプレクサ側は `TrackKind` から handler type と Media Header を分岐で決めているため、字幕系を通す口が無い。

- `src/mux_fmp4_segment.rs:655-668`
- `src/mux_mp4_file.rs:908,944`

## 設計方針

字幕方式に依存しない共通部分だけを実装する。方式固有の `SampleEntry` バリアントは本 issue の対象外（0043 / 0044 / 0045 で対応）。本 issue の範囲では `SampleEntry::Unknown` フォールバック経由で decode / encode ラウンドトリップできる状態までを担保する。

- `TrackKind` に字幕バリアントを追加する（例: `Subtitle`）
- `HdlrBox` に字幕用 handler type 定数を追加する
  - 0043 (stpp) / 0044 (wvtt) / 0045 (tx3g) の各方式が要求する handler type を仕様書に沿って洗い出し、必要なものを追加する（少なくとも `subt` と `text` を想定するが、実装着手前に一次資料で確認する）
- 字幕用 Media Header ボックスを実装する
  - `SthdBox` (`sthd`, SubtitleMediaHeaderBox, ISO/IEC 14496-12)
  - `NmhdBox` (`nmhd`, NullMediaHeaderBox, ISO/IEC 14496-12)
- `MinfBox` の Media Header 保持フィールドを、字幕用にも対応できる形へ拡張する
  - 現状の `smhd_or_vmhd_box: Option<Either<SmhdBox, VmhdBox>>` は 2 択構造のため拡張困難
  - 新規に enum（例: `MediaHeaderBox { Smhd, Vmhd, Sthd, Nmhd }`）を導入するのが素直
  - フィールド名も見直す（例: `smhd_or_vmhd_box` → `media_header_box`）
- デマルチプレクサ側の handler type 分岐で字幕トラックを skip せず取り出す経路を作る
- マルチプレクサ側で `TrackKind::Subtitle` から handler type と Media Header を決める経路を作る
- C API / WASM API に `TrackKind` の字幕バリアントを露出する（`Mp4TrackKind` 相当の列挙値を追加する）

### 後方互換性への影響

- `TrackKind` は現状 `#[non_exhaustive]` ではなく通常の enum のため、バリアント追加はコンシューマ側の網羅 match を破壊する。SemVer 上のブレイキング扱い
- `MinfBox` の Media Header 保持フィールドの型と名前が変わるため公開 API の破壊的変更になる
- C API / WASM の `TrackKind` 相当の列挙値追加も ABI 変更として明示する必要がある

いずれも `CHANGES.md` では `[CHANGE]` で記載する。

## 依存関係

- 本 issue は 0043 / 0044 / 0045 の前提。方式実装より先に完了させる

## 完了条件

- `TrackKind` に字幕バリアントが追加され、既存の映像・音声トラックの demux / mux 挙動が変わらない
- `SthdBox` / `NmhdBox` の decode / encode ラウンドトリップテストが通る
- 字幕系 handler type を持つトラックが、`SampleEntry::Unknown` 経由で demux / mux ラウンドトリップできる
- C API / WASM API から字幕トラック種別が判別できる
- `cargo clippy` が通る

## 解決方法

方式選定と並行して詰める。少なくとも以下の順で作業する見込み。

1. 0043 / 0044 / 0045 が要求する handler type を仕様書で洗い出す
2. `TrackKind` に字幕バリアントを追加する
3. `HdlrBox` に必要な handler type 定数を追加する
4. `SthdBox` / `NmhdBox` を実装し、`MinfBox` の Media Header 保持構造を拡張する
5. デマルチプレクサの handler type 分岐で字幕トラックを取り出す
6. マルチプレクサで字幕トラックの生成経路を通す
7. C API / WASM API に `TrackKind` の字幕バリアントを露出する
8. PBT・単体テスト・`Unknown` フォールバック経由の合成 MP4 でラウンドトリップを検証する

## CHANGES.md

`[CHANGE]` として記載する（`TrackKind` バリアント追加および `MinfBox` フィールド型の変更を伴うため）。
