# `Mp4FileMuxer` に字幕トラック（`TrackKind::Subtitle`）の受け入れを追加する

- Priority: Low
- Created: 2026-07-21
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-mp4-file-muxer-subtitle
- Polished: YYYY-MM-DD

## 目的

`Mp4FileMuxer` に字幕トラック（`TrackKind::Subtitle`）の受け入れ経路を実装し、非フラグメント MP4 で字幕トラックを mux できるようにする。

0042（`issues/0042-add-subtitle-track-common.md`）では `Mp4FileMuxer` 内部が Audio / Video 専用フィールドで構造化されていることを理由に受け入れをスコープ外にし、`MuxError::UnsupportedTrackKind` を返す拒否経路のみを実装している。本 issue はその制限を解消する。

## 優先度根拠

Low。バグ由来ではなく緊急要求も無い。0043 / 0044 / 0045 の各完了条件「MP4 のラウンドトリップができる」は本 issue の完了を前提とするが、いずれも Low で緊急性がないため、本 issue も Low で足りる。

## 依存関係

- 0042（`issues/0042-add-subtitle-track-common.md`）の完了が前提。以下を利用する
  - `TrackKind::Subtitle` バリアント
  - `MuxError::UnsupportedTrackKind`（既存の拒否経路の除去に伴い削除ないし別の未サポート種別用に振り替える）
  - `MediaHeader` enum と `SthdBox` / `NmhdBox`
  - `HdlrBox::HANDLER_TYPE_SUBT` / `HANDLER_TYPE_TEXT`
- 0043 / 0044 / 0045 のいずれかの完了は必須ではない。ただし 0043 で新設される予定のサンプルエントリー種別に基づく handler type / Media Header 分岐関数を、本 issue が完了しているタイミングでは共有利用する（未完了なら 0042 の暫定 `subt` + `sthd` 固定選択をそのまま流用する）

## 現状

`Mp4FileMuxer` は内部が Audio / Video 専用フィールドで 2 系統ハードコードされており、`TrackKind::Subtitle` を渡しても Chunk の格納先が無い。

- `src/mux_mp4_file.rs:417-420` `audio_chunks: Vec<Chunk>` / `video_chunks: Vec<Chunk>` / `audio_track_timescale: NonZeroU32` / `video_track_timescale: NonZeroU32` の 2 系統フィールド
- `src/mux_mp4_file.rs:561-590` `append_sample` の 2 バリアント + 0042 で追加された `TrackKind::Subtitle => Err(MuxError::UnsupportedTrackKind)` arm
- `src/mux_mp4_file.rs:623-626` `is_new_chunk_needed` の同 match（0042 で Subtitle arm 追加済み）
- `src/mux_mp4_file.rs:776-808` `build_moov_box` が `audio_chunks.is_empty()` / `video_chunks.is_empty()` を直接チェック
- `src/mux_mp4_file.rs:810-961` `build_audio_trak_box` / `build_video_trak_box` / `build_audio_mdia_box` / `build_video_mdia_box` の Audio / Video 専用ビルダー関数

なお `Fmp4SegmentMuxer` は既に `Vec<TrackEntry>` の汎用構造で Subtitle を受け入れる形になっており（0042 で `TrackKind::Subtitle` arm を追加済み）、本 issue はそことの設計整合も取る。

## 設計方針

### 内部フィールド構造の一般化

`audio_chunks` / `video_chunks` / `audio_track_timescale` / `video_track_timescale` の 2 系統フィールドを、`TrackKind` を切り口とする汎用構造に置き換える。候補:

- 案 A: `HashMap<TrackKind, TrackState>`（順序保証のため `IndexMap` 相当が必要になり、`no_std` 環境で追加依存が発生する）
- 案 B: `Vec<(TrackKind, TrackState)>` + トラック追加順を要素順で保持
- 案 C: `Fmp4SegmentMuxer` の `Vec<TrackEntry>` パターンに揃える（`TrackEntry` は `track_kind` / `timescale` / `sample_entries` / `chunks` を持つ）

案 C が既存 `Fmp4SegmentMuxer` の設計と揃うため一貫性が高い。ただし `Mp4FileMuxer` は `Fmp4SegmentMuxer` と違い `TrackEntry` に `Chunk` 群を持たせるためのフィールド追加が必要。案の最終決定は実装時に行うが、既存 `Fmp4SegmentMuxer` との一貫性を最優先に検討する。

トラック追加順は `trak` ボックスの出力順に反映する（従来は video → audio の固定順で push していた `build_moov_box:779-787` の挙動を、追加順ないし決定的な kind 順に統一する）。

### 拒否経路の除去

- `src/mux_mp4_file.rs:561-590, 623-626` の `TrackKind::Subtitle =>` arm を通常受入経路に置き換える
- 0042 で追加した「`Mp4FileMuxer::append_sample` に Subtitle を渡すと `MuxError::UnsupportedTrackKind` が返る」単体テストは削除する（本 issue で挙動が変わるため）
- `MuxError::UnsupportedTrackKind` 自体は残す（`Mp4FileMuxer` 以外の将来の未サポート kind 追加時に再利用する余地を残す。本 issue で該当 arm を除去した結果、当該バリアントが未使用になっても API から削除しない）

### `build_subtitle_trak_box` / `build_subtitle_mdia_box`

Audio / Video 用の既存関数と揃えた形で Subtitle 用ビルダーを追加する。あるいは、汎用化リファクタと合わせて `build_trak_box(track_kind, ...)` / `build_mdia_box(track_kind, ...)` の 1 関数に集約するかを実装時に判断する。

Subtitle トラックの handler type と Media Header は `Fmp4SegmentMuxer` と同じロジックで決める。

- 0043 / 0044 / 0045 のいずれかで方式ごとの分岐関数が新設されていれば、それを共有利用する
- そうでなければ 0042 の暫定選択（handler type = `subt`、Media Header = `sthd`）をそのまま使う

### tkhd 属性

`Fmp4SegmentMuxer::build_init_trak` (`src/mux_fmp4_segment.rs:604-636` を 0042 で刷新したもの) と同様に、`track_kind` 外側 match で決定する。

- `TrackKind::Video`: `volume = TkhdBox::DEFAULT_VIDEO_VOLUME`、`width` / `height` は visual から取得（visual が None の場合は 0）
- `TrackKind::Audio`: `volume = TkhdBox::DEFAULT_AUDIO_VOLUME`、`width` / `height` は 0
- `TrackKind::Subtitle`: `volume = TkhdBox::DEFAULT_VIDEO_VOLUME`（値は 0）、`width` / `height` は 0

`Fmp4SegmentMuxer` 側との重複コードが多くなる場合は共通ヘルパを抽出する（本 issue の範囲内で判断）。

### `build_ftyp` / `compatible_brands`

`Mp4FileMuxer::build_final_ftyp_box` (`src/mux_mp4_file.rs:686-694` 前後) は `SampleEntry` から compatible_brands を決める。字幕系ブランド（`msubs` 等）を追加するかどうかは 0043-0045 完了状況に応じて判断する（本 issue の範囲では追加しない方向を第一候補とし、必要になれば別途対応）。

### 後方互換性への影響

- 公開 API シグネチャ（`Mp4FileMuxer::append_sample` の `Sample` 引数と `Result` 返り値）は変わらない
- `Mp4FileMuxer` の pub フィールド／pub メソッドの構成に変更がある場合は `[CHANGE]` として明示（実装時に確認）
- `TrackKind::Subtitle` を渡した際の挙動が「エラー返却」から「受入」に変わる。これは semver 上の破壊ではない（今まで動かなかったものが動くようになるだけ）が、CHANGES.md では `[ADD]` として明示する
- 0042 で追加した「Subtitle 拒否テスト」の削除／振替は非公開テストコードの変更なので破壊的ではない

## 完了条件

- `Mp4FileMuxer` が `TrackKind::Subtitle` の `Sample` を `append_sample` で受け入れ、`Mp4FileMuxer::finalize` で MP4 バイト列を生成できる（合成テスト、`SampleEntry::Unknown` を含む）
- 生成された MP4 の subtitle トラックが `Mp4FileDemuxer` でラウンドトリップできる
- 既存の Audio / Video トラック mux の挙動が変わらない（既存 PBT / 単体テストが pass）
- 0042 で追加した `Mp4FileMuxer` の Subtitle 拒否テストは削除される
- `MuxError::UnsupportedTrackKind` 自体は削除せず残す
- Audio + Video + Subtitle の 3 トラック mux が可能（`build_moov_box` が 3 種類の trak_box を含める）
- `cargo clippy --all-targets --all-features` が通る
- PBT に Subtitle トラック含む `Mp4FileMuxer` ラウンドトリップテストを追加する

## 本 issue 完了後の追随タスク

本 issue の完了により、以下の姉妹字幕方式 issue で保留された `Mp4FileDemuxer` 経路検証テストの追加が可能となる。本 issue のスコープには含めないが、本 issue のマージ後に該当 issue 側で追加する運用とする。

- **0043 (`issues/0043-add-subtitle-stpp.md`)**: `stpp_sample_entry_via_mp4_file_demuxer` PBT の追加（`pbt/tests/prop_container_boxes.rs`）。0043 では `Mp4FileMuxer` が Subtitle 拒否のため合成データを Muxer 経由で吐かせられず、本 issue 完了を待つ形になった（0043 の「### 3 経路デマルチプレクサ検証と合成ラウンドトリップ」節参照）
- **0044 (`issues/0044-add-subtitle-wvtt.md`) / 0045 (`issues/0045-add-subtitle-tx3g.md`)**: 同種の `wvtt_sample_entry_via_mp4_file_demuxer` / `tx3g_sample_entry_via_mp4_file_demuxer` PBT を同じ理由で保留する可能性がある。各 issue の refresh 状況に応じて同時に追加する

## 解決方法

0042 完了後に着手する。以下の順で実装する見込み。

1. 内部フィールド構造の一般化方針を確定し（案 C の `Vec<TrackEntry>` 相当を第一候補として検討）、フィールドを差し替える
2. `append_sample` / `is_new_chunk_needed` を汎用構造に対応させる。Subtitle arm は拒否から受入に変更する
3. `build_moov_box` を 3 種類（Audio / Video / Subtitle）以上に対応するよう汎用化する。trak_box の出力順ポリシーを決める
4. `build_subtitle_trak_box` / `build_subtitle_mdia_box` を実装（または `build_trak_box(track_kind, ...)` 等の汎用関数への統合）。tkhd 属性は `track_kind` 外側 match で決定
5. handler type / Media Header は `Fmp4SegmentMuxer` と共通のロジックを使う（可能なら共通ヘルパに抽出する）
6. `build_final_ftyp_box` の compatible_brands ポリシーを確認する（本 issue では字幕系ブランドを追加しない方向）
7. 0042 で追加した Subtitle 拒否テストを削除する
8. PBT に Subtitle トラック含む `Mp4FileMuxer` ラウンドトリップテストを追加する（既存 Audio / Video 版に揃える）
9. `cargo clippy` / 全テストで検証する

## CHANGES.md

- `[ADD]` `Mp4FileMuxer` で `TrackKind::Subtitle` トラックの受け入れと mux に対応する
- `[CHANGE]`（実装時に判断）`Mp4FileMuxer` の内部フィールド構成を汎用化した際に pub フィールドの型が変わる場合は独立エントリで記載する（変更がない場合は本項目は不要）
