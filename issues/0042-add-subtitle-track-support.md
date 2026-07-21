# 字幕トラック（Timed Text 系）のサンプルエントリー対応を追加する

- Priority: Low
- Created: 2026-07-21
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-subtitle-track-support
- Polished: YYYY-MM-DD

## 目的

音声認識などで生成した字幕テキストを MP4 に格納できるようにするための、字幕トラック（Timed Text 系サンプルエントリー）の decode / encode 対応を追加する。

現状の `shiguredo_mp4` は映像・音声トラックのみを扱う設計になっており、`TrackKind` に字幕バリアントが無く、`SampleEntry` にも Timed Text 系のバリエーションが存在しない。そのため、字幕トラックを含む MP4 の生成・解析はいずれもできない。

## 優先度根拠

Low。既存機能を壊すバグ由来ではなく、外部からの緊急要求があるわけでもない。想定ユースケース（自動生成字幕の埋め込み）は存在するが、実装スコープの確定に方式選定（後述）の設計判断が必要な段階にあり、Medium/High に上げる根拠は無い。

## 現状

トラック種別と関連ボックスは以下の 2 種類だけをサポートしている。

- `src/basic_types.rs:677` `TrackKind` は `Audio` と `Video` のみ
- `src/boxes_moov_tree.rs:920-923` `HdlrBox` の handler type 定数は `HANDLER_TYPE_SOUN` (`soun`) と `HANDLER_TYPE_VIDE` (`vide`) のみ
- `src/boxes_moov_tree.rs:986` `MinfBox::smhd_or_vmhd_box: Option<Either<SmhdBox, VmhdBox>>` で音声/映像の 2 種類の Media Header にしか対応していない
- `src/boxes_sample_entry.rs:17` `SampleEntry` の既知バリアントは Avc1 / Hev1 / Hvc1 / Vp08 / Vp09 / Av01 / Opus / Mp4a / Flac のみ（+ `Unknown` フォールバック）

デマルチプレクサ側では handler type が `soun` / `vide` 以外だとトラックそのものを skip している。

- `src/demux_mp4_file.rs:511-514` の `_ => continue`
- `src/demux_fmp4_file.rs:320-323` 同様
- `src/demux_fmp4_segment.rs:145-148` 同様

マルチプレクサ側は `TrackKind` から handler type と Media Header を分岐で決めているため、字幕系を通す口が無い。

- `src/mux_fmp4_segment.rs:655-668`
- `src/mux_mp4_file.rs:908,944`

C API (`crates/c-api/`) と WASM API (`crates/wasm/`) にも字幕系の露出は無い。

## 設計方針

まず「どの Timed Text 系サンプルエントリーをサポートするか」を確定させるところから設計する。候補は以下の通り。**この issue の中で最終決定する**（着手前に polish して詰める）。

- `stpp`: XML 系字幕（TTML / IMSC 等）。ISO/IEC 14496-30 で規定。柔軟だが XML 前提でパーサ側の負担が大きい
- `wvtt`: WebVTT。ISO/IEC 14496-30 で規定。Web 系プレイヤーとの親和性が高い
- `tx3g`: 3GPP Timed Text（3GPP TS 26.245）。Apple 系プレイヤーで慣習的に使われるがフォーマットは古く表現力が限定的

判断の観点として少なくとも以下を明確化してから採用可否を決める。

1. 想定する生成元（音声認識結果）のデータ表現から見て、自然に載せられる形式は何か
2. 想定する再生側（Web プレイヤー / QuickTime / ffmpeg 等）の対応状況
3. 各方式で必要な追加ボックス（`stpp` は名前空間宣言と MIME、`wvtt` は `vttC` 等）と実装コストの比較
4. 一度に複数方式を入れるか、まず 1 方式に絞ってから拡張していくか

方式選定と並行して必要になる共通変更（採用方式によらず必要）は以下の通り。

- `TrackKind` に字幕バリアントを追加する
- `HdlrBox` に字幕用 handler type 定数を追加する（`subt` / `sbtl` / `text` のいずれか。方式に応じて選ぶ）
- `MinfBox` の Media Header を字幕用（`sthd` / `nmhd`）にも対応できるように拡張する（`Either<SmhdBox, VmhdBox>` の 2 択構造は見直しが必要）
- `SampleEntry` に選定した方式のバリアントを追加する
- デマルチプレクサの handler type 分岐に字幕を追加し、字幕トラックを skip せず取り出す
- マルチプレクサに字幕トラックの生成経路を追加する（Media Header の切り替えを含む）
- C API・WASM API に字幕系サンプルエントリーの型を露出する（対応する場合）

### 後方互換性への影響

- `TrackKind` は現状 `#[non_exhaustive]` ではなく通常の enum のため、バリアント追加はコンシューマ側の網羅 match を破壊する。SemVer 上のブレイキング扱いになる想定
- `MinfBox::smhd_or_vmhd_box` を Timed Text の Media Header にも対応する形へ拡張する場合、フィールド型（`Either<SmhdBox, VmhdBox>`）が変わるため公開 API の破壊的変更になる。フィールド名も含めた命名見直しが必要（例: `smhd_or_vmhd_box` → `media_header_box` など）
- `SampleEntry` へのバリアント追加も破壊的変更（`Unknown` があるため decode 側の未知バリアントに対する後方互換は保たれるが、コンシューマ側の網羅 match は破壊する）

## 完了条件

- 決定した 1 方式以上の Timed Text 系サンプルエントリーの decode / encode ができる
- 字幕サンプルを含む MP4 / fMP4 の demux / mux ができる（トラックが skip されない）
- ラウンドトリップテスト（PBT）と単体テストが通る
- 既存の映像・音声トラックの demux / mux 挙動が変わらない
- `cargo clippy` が通る
- C API・WASM API での露出方針を決めて、決めた方針通りに実装するか、明示的に対象外とする

## 解決方法

方式選定後に確定する。少なくとも以下の順で作業する見込み。

1. サポート方式を決定し、必要なサンプルエントリー・付随ボックス（`vttC` / 名前空間宣言等）を列挙する
2. `TrackKind` に字幕バリアントを追加し、`HdlrBox` に handler type 定数を追加する
3. `MinfBox` の Media Header 取り扱いを字幕用にも対応させる
4. `SampleEntry` に対応バリアントを追加し、`Encode` / `Decode` 実装を書く
5. デマルチプレクサ・マルチプレクサに字幕トラック経路を通す
6. C API・WASM API を必要に応じて拡張する
7. PBT・単体テスト・実サンプル（既存の tests/ の資産があれば流用、無ければ最小の合成データ）で回帰確認する

## CHANGES.md

`[ADD]` として記載する。ただし、上記「後方互換性への影響」にある型変更を伴う部分は `[CHANGE]` として合わせて記載する。
