# `Mp4FileMuxer` に字幕トラック（`TrackKind::Subtitle`）の受け入れを追加する

- Priority: Low
- Created: 2026-07-21
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-mp4-file-muxer-subtitle
- Polished: 2026-07-24

## 目的

`Mp4FileMuxer` に字幕トラック（`TrackKind::Subtitle`）の受け入れ経路を実装し、非フラグメント MP4 で字幕トラックを mux できるようにする。

0042（`issues/closed/0042-add-subtitle-track-common.md`）では `Mp4FileMuxer` 内部が Audio / Video 専用フィールドで構造化されていることを理由に受け入れをスコープ外にし、`MuxError::UnsupportedTrackKind` を返す拒否経路のみを実装した。本 issue はその制限を解消する。

## 優先度根拠

Low。バグ由来ではなく緊急要求も無い。0043 / 0044 / 0045 の各完了条件「MP4 のラウンドトリップができる」は本 issue の完了を前提とするが、いずれも Low で緊急性がないため、本 issue も Low で足りる。

## 依存関係

- 0042 (`issues/closed/0042-add-subtitle-track-common.md`) は完了済み。以下を利用する
  - `TrackKind::Subtitle` バリアント（`src/basic_types.rs:677-691`）
  - `MediaHeader` enum と `SthdBox` / `NmhdBox`（`src/boxes_moov_tree.rs`）
  - `HdlrBox::HANDLER_TYPE_SUBT` (`subt`) / `HdlrBox::HANDLER_TYPE_TEXT` (`text`)
  - `MuxError::UnsupportedTrackKind` バリアント
- 0043 (`issues/closed/0043-add-subtitle-stpp.md`) は完了済み。`SampleEntry::Stpp(StppBox)` バリアントを利用する
- 0044 (`issues/closed/0044-add-subtitle-wvtt.md`) は完了済み。`SampleEntry::Wvtt(WvttBox)` バリアントを利用する
- 0045 (`issues/closed/0045-add-subtitle-tx3g.md`) は完了済み。`SampleEntry::Tx3g(Tx3gBox)` バリアントを利用する。また `src/mux_fmp4_segment.rs:956-1005` の `derive_trak_attributes` で `Stpp` / `Wvtt` / `Tx3g` 3 バリアントの明示 arm と `SampleEntry::Unknown` 向け防御的 fallback（`subt` + `sthd`）が完成しているため、本 issue ではこれを共有利用する

## 現状

`Mp4FileMuxer` は内部が Audio / Video 専用フィールドで 2 系統ハードコードされており、`TrackKind::Subtitle` を渡しても Chunk の格納先が無い。以下は 0045 完了時点（コミット `047aac3`）の実ファイル行番号:

- `src/mux_mp4_file.rs:434-437` `audio_chunks: Vec<Chunk>` / `video_chunks: Vec<Chunk>` / `audio_track_timescale: NonZeroU32` / `video_track_timescale: NonZeroU32` の 2 系統フィールド
- `src/mux_mp4_file.rs:568-572` `append_sample` 冒頭の Subtitle 早期拒否 arm（0042 で追加された）
- `src/mux_mp4_file.rs:591-628` `append_sample` の kind ごとの `chunks` 参照 match（末尾に 0042 で追加された Subtitle 早期 return 防御 arm がある。行 623-627）
- `src/mux_mp4_file.rs:656-679` `is_new_chunk_needed` の同 match（末尾に 0042 で追加された Subtitle 防御 arm がある。行 669）
- `src/mux_mp4_file.rs:724-758` `build_final_ftyp_box` は `for chunk in self.audio_chunks.iter().chain(self.video_chunks.iter())`（行 730）で 2 系統を直接走査する
- `src/mux_mp4_file.rs:820-852` `build_moov_box` は `audio_chunks.is_empty()` / `video_chunks.is_empty()` を直接チェックして audio → video の順で push する（従来挙動は audio 先 → video 後の固定順）
- `src/mux_mp4_file.rs:854-1005` `build_audio_trak_box` / `build_video_trak_box` / `build_audio_mdia_box` / `build_video_mdia_box` の Audio / Video 専用ビルダー群
- `src/mux_mp4_file.rs:894-907` `build_video_trak_box` 内の映像解像度計算は `self.video_chunks.iter().filter_map(|c| c.sample_entry.video_resolution()).fold(...)` で全チャンクの最大 width / height を採用する（複数 sample_entry でも最大値ベースで tkhd に埋め込む既存仕様）
- `src/mux_mp4_file.rs:1101-1123` `calculate_total_duration` は `audio_chunks` / `video_chunks` / `audio_track_timescale` / `video_track_timescale` の 4 フィールドを直接参照する

`Fmp4SegmentMuxer` は既に `Vec<TrackEntry>` の汎用構造で Subtitle を受け入れる形になっており、本 issue はそことの設計整合も取る。ただし `Fmp4SegmentMuxer::TrackEntry`（`src/mux_fmp4_segment.rs:91-99`）はフラグメント固有フィールド（`track_id` / `decode_time` / `current_sample_entry_index`）を持つため、`Mp4FileMuxer` では別構成の同名 private 型として定義する（詳細は「### 内部フィールド構造の一般化」節）。

### 更新が必要な doc コメント（実装時に必ず追随する）

本 issue の実装で「Mp4FileMuxer は字幕未対応」を前提とした既存 doc コメントが 7 箇所ある。すべて更新する:

- `src/mux_mp4_file.rs:1-5` モジュール doc の「複数のメディアトラック（音声・映像）から」→「複数のメディアトラック（音声・映像・字幕）から」に書き換える
- `src/mux_fmp4_segment.rs:1-5` モジュール doc の「複数のメディアトラック（音声・映像）から」→「複数のメディアトラック（音声・映像・字幕）から」に書き換える
- `src/mux_fmp4_segment.rs:22-25` モジュール doc の 4 行構成を書き換える。1 行目「現時点では同一 [`TrackKind`] のトラックは 1 本までに制限している（音声 / 映像 / 字幕 各 1 本）。」と 4 行目「将来、同種複数トラックに対応する場合は file muxer と合わせて拡張する想定である。」はそのまま維持する。2-3 行目の「[`Mp4FileMuxer`] は現時点で字幕未対応のため、字幕トラックは [`Fmp4SegmentMuxer`] 経由でのみ扱える。」の 2 行を削除する（本 issue 完了により [`Mp4FileMuxer`] も字幕対応となり、両 muxer で字幕トラックを扱えるようになるため）
- `src/mux_mp4_file.rs:311-318` `MuxError::UnsupportedTrackKind` バリアントの doc「例えば `Mp4FileMuxer` は現状 [`TrackKind::Subtitle`] を受け付けないため」を次の文面に書き換える: 「サポートされていないトラック種別が指定された場合のエラーを表す。両 muxer からは現時点では投げられないが、C API `MP4_ERROR_UNSUPPORTED` マッピングと将来の `TrackKind` バリアント追加時の拡張余地としてバリアント自体は保持する」
- `src/mux_mp4_file.rs:409-411` `Mp4FileMuxer` 構造体 doc の「複数のメディアトラック（音声・映像）から」→「複数のメディアトラック（音声・映像・字幕）から」に書き換える
- `src/mux_mp4_file.rs:552-559` `append_sample` doc を汎用構造化後の実態に合わせて全面書き換え。特に「エラーを返した場合、内部状態 (`next_position` / `audio_chunks` / `video_chunks` / `last_sample_kind`) は変更されない…」および「`TrackKind::Subtitle` のサンプルは受け付けず…」を削除し、以下 2 点を明記する:
  - フィールド名を `tracks` に統一（`audio_chunks` / `video_chunks` / `audio_track_timescale` / `video_track_timescale` は消滅）
  - エラー返却時の内部状態不変性は「### `append_sample` の失敗パス契約」節に従う（後述）
- `src/mux_mp4_file.rs:564-567` `append_sample` 内の Subtitle 早期拒否のインラインコメントは、拒否経路除去に伴い arm ごと削除する

## 設計方針

### スコープ

含むもの:

- `Mp4FileMuxer` の内部フィールドを Audio / Video / Subtitle を統一的に扱う汎用構造（`Vec<TrackEntry>`）に置き換える
- `TrackKind::Subtitle` を渡した際の拒否 arm を除去し、通常受入経路に置き換える
- Audio + Video + Subtitle の 3 トラック mux が可能な `build_moov_box` および `build_trak_box` / `build_mdia_box` の汎用化
- Subtitle トラックの handler_type・Media Header 決定は `Fmp4SegmentMuxer::derive_trak_attributes` を Audio / Subtitle 用として共有利用する（Video 用は `Mp4FileMuxer` 側で個別実装。「### tkhd / handler_type / Media Header の決定と共通ヘルパ」節参照）
- PBT に Subtitle トラックを含む `Mp4FileMuxer` ラウンドトリップテストを追加する
- 0043 / 0044 / 0045 側で「0046 完了後に別途追加」と保留された `stpp_sample_entry_via_mp4_file_demuxer` / `wvtt_sample_entry_via_mp4_file_demuxer` / `tx3g_sample_entry_via_mp4_file_demuxer` の 3 経路テストを本 issue のスコープに取り込む（closed 済み issue には追加できないため）
- 0042 で追加した `test_unsupported_track_kind_error_for_subtitle` 単体テストの削除
- 上述「### 更新が必要な doc コメント」の 7 箇所の追随更新

含まないもの:

- **同一 `TrackKind` のトラックを複数本許容する対応**。`Fmp4SegmentMuxer` の現行方針（`src/mux_fmp4_segment.rs:22-25` の「同一 `TrackKind` のトラックは 1 本まで」）に揃え、`Mp4FileMuxer` でも `Vec<TrackEntry>` 構造でありつつ、`ensure_track_entry` 相当のヘルパで同一 kind を検索して既存 entry に合流させる（明示エラーは出さない。既存 Audio / Video 挙動を維持）
- **字幕系ブランドの追加**（`msubs` 等）。0043 / 0044 / 0045 の各「### `compatible_brands` の方針」節と揃え、本 issue でも `build_final_ftyp_box` で字幕系ブランドは追加しない
- **`MuxError::UnsupportedTrackKind` バリアント本体の削除**、および `test_unsupported_track_kind_display_contains_subtitle`（`src/mux_mp4_file.rs:1307-1321`）と `crates/c-api/src/error.rs` の `test_unsupported_track_kind_maps_to_mp4_error_unsupported` の削除。詳細は「### 拒否経路の除去と `MuxError::UnsupportedTrackKind` の存置」節

### 内部フィールド構造の一般化

`audio_chunks` / `video_chunks` / `audio_track_timescale` / `video_track_timescale` の 2 系統フィールドを、`TrackKind` を切り口とする汎用構造 `Vec<TrackEntry>` に置き換える（`Fmp4SegmentMuxer` の設計と揃える）。

`Mp4FileMuxer::TrackEntry` の完全なフィールド仕様（新規追加、`mux_mp4_file.rs` 内 private 型）:

```rust
#[derive(Debug, Clone)]
struct TrackEntry {
    track_kind: TrackKind,
    timescale: NonZeroU32,
    chunks: Vec<Chunk>,
}
```

- `Fmp4SegmentMuxer::TrackEntry`（`src/mux_fmp4_segment.rs:91-99`）と同名の別型（別モジュールの private 型で衝突しない）。フラグメント固有のフィールド（`track_id` / `decode_time` / `current_sample_entry_index` / `sample_entries`）は Mp4FileMuxer では不要
- `track_id` は `build_moov_box` で `trak_boxes.len() as u32 + 1` として振る（既存 `src/mux_mp4_file.rs:824, 829` の慣習を維持）。空 chunks の `TrackEntry` は build_moov_box で skip するため（「### `build_moov_box` の汎用化」節参照）、`track_id` は「空でない TrackEntry の出現順」になる
- `sample_entries` は既存 `Chunk.sample_entry: SampleEntry`（`src/mux_mp4_file.rs:401-406`）で Chunk ごとに個別に持つパターンを継続する。1 トラック内で複数 sample_entry を許容する既存挙動を維持し、`build_stbl_box` の `stsd` / `stsc` 経路もそのまま動く

`Mp4FileMuxer` のフィールド差し替え後:

```rust
#[derive(Debug, Clone)]
pub struct Mp4FileMuxer {
    options: Mp4FileMuxerOptions,
    initial_boxes_bytes: Vec<u8>,
    mdat_box_offset: u64,
    next_position: u64,
    last_sample_kind: Option<TrackKind>,
    finalized_boxes: Option<FinalizedBoxes>,
    tracks: Vec<TrackEntry>,
}
```

`audio_chunks` / `video_chunks` / `audio_track_timescale` / `video_track_timescale` を削除し、`tracks: Vec<TrackEntry>` に統合する。

### `ensure_track_entry` ヘルパの追加

`Fmp4SegmentMuxer::ensure_track_entry`（`src/mux_fmp4_segment.rs:905-935`）と同じ規約で、`Mp4FileMuxer` に private ヘルパ `ensure_track_entry(&mut self, track_kind: TrackKind, timescale: NonZeroU32) -> Result<usize, MuxError>` を追加する。

- 同一 kind の既存 entry があれば timescale 一致を検証（不一致なら `MuxError::TimescaleMismatch` を返す）し、その index を返す
- 無ければ新規 `TrackEntry`（chunks 空）を push して末尾の index を返す

`Fmp4SegmentMuxer` は free fn パターン（`tracks: &mut Vec<TrackEntry>` を第一引数に取る）だが、`Mp4FileMuxer` では method パターンを採る。`Fmp4SegmentMuxer` は `create_media_segment_metadata` で clone-then-swap の rollback パターン（`src/mux_fmp4_segment.rs:316-317, 415`）を取っているため free fn が便利だが、`Mp4FileMuxer` の `append_sample` は rollback を取らない設計のため method で十分。

### `append_sample` の失敗パス契約

汎用構造化後の `append_sample` は以下の順序で処理する。**この順序と失敗パス契約は doc（`src/mux_mp4_file.rs:552-559`）に明記する**:

1. `PositionMismatch` チェック（既存挙動を維持）
2. `SampleMetadata` 構築（既存の `u32::MAX` チェック等）
3. `is_new_chunk_needed(sample)` を先に計算する（`&self` で `self.tracks.iter().find(|t| t.track_kind == sample.track_kind)` を辿る。「### `is_new_chunk_needed` の汎用化」節参照）
4. **`sample_entry` を解決**: `is_new_chunk_needed == true` の場合のみ以下で `resolved_sample_entry: SampleEntry`（非 Option）を得る。`is_new_chunk_needed == false` の場合は本ステップを完全にスキップする（変数は導入しない）。**この時点でエラーが返るなら `self.tracks` は不変**（`ensure_track_entry` はまだ呼ばない）
   ```rust
   // if is_new_chunk_needed 分岐の内側でのみ実行
   let resolved_sample_entry: SampleEntry = sample.sample_entry.clone()
       .or_else(|| self.tracks.iter().find(|t| t.track_kind == sample.track_kind)
                    .and_then(|t| t.chunks.last().map(|c| c.sample_entry.clone())))
       .ok_or(MuxError::MissingSampleEntry { track_kind: sample.track_kind })?;
   ```
5. `let track_index = self.ensure_track_entry(sample.track_kind, sample.timescale)?;` を呼ぶ（新規 kind ならこの時点で `TrackEntry` を push、既存 kind で timescale 不一致ならエラー返却）
6. `self.tracks[track_index].chunks` を操作する。**`is_new_chunk_needed == true` の場合は Step 4 で解決した `resolved_sample_entry` を持つ新規 `Chunk { offset: sample.data_offset, sample_entry: resolved_sample_entry, samples: Vec::new() }` を push する**。その後 `is_new_chunk_needed` の値に関わらず、共通で `self.tracks[track_index].chunks.last_mut().expect("bug").samples.push(sample_metadata)` を実行する（既存 `src/mux_mp4_file.rs:646` と同じパターン）
7. `next_position` / `last_sample_kind` を更新（`next_position.checked_add(sample.data_size as u64)` で `MuxError::Overflow` が返り得る）

**エラー返却時の内部状態不変性の doc 記載**（`append_sample` の doc `src/mux_mp4_file.rs:552-559` に書き換えで反映）:

- Step 1 の `PositionMismatch`、Step 2 の `EncodeError`（`u32::MAX` 超過）、Step 4 の `MissingSampleEntry`、Step 5 の `TimescaleMismatch` の場合: `self` の内部状態は完全に不変（`self.tracks` に新規 push 済みの状態は残らない）
- Step 7 の `Overflow` の場合: **Step 5 の TrackEntry push（新規 kind の場合）と Step 6 の Chunk push（`is_new_chunk_needed == true` の場合）と `samples` への metadata push（常に発生）の 3 段の副作用が残る**。`next_position` / `last_sample_kind` は未更新。既存実装（`src/mux_mp4_file.rs:648-651`）も同じ副作用パターンで、本 issue ではこの挙動を維持する（次サンプルで同一 `data_offset` を再試行すると `PositionMismatch` が返り、事実上 rollback 不能）

`Fmp4SegmentMuxer` のような clone-then-swap rollback は本 issue では採用しない（単純化のため。`Mp4FileMuxer` の append_sample は元々 rollback を取らない挙動）。

**既存挙動からの改善点**（doc に明記する）: 現状の実装（`src/mux_mp4_file.rs:592-616`）は kind ごとの match arm 冒頭で `audio_track_timescale` / `video_track_timescale` を `sample.timescale` に代入するため、その直後の Step 4 相当で `MissingSampleEntry` が返ると **timescale だけが記録済み** の副作用が残っていた。新実装では Step 4 の `sample_entry` 解決を Step 5 の `ensure_track_entry` より前に実施するため、`MissingSampleEntry` エラー時に `self.tracks` は完全に不変となる。この挙動変化は CHANGES.md の `[CHANGE]` エントリの子項目に明記する（「## CHANGES.md」節参照）。

### `is_new_chunk_needed` の汎用化

`src/mux_mp4_file.rs:656-679` を以下に書き換える:

- `last_sample_kind != Some(sample.track_kind)` の早期リターン（true 返却）を残す
- **`sample.sample_entry` が `None` の場合は `false` を早期返却する**（既存 `src/mux_mp4_file.rs:672-674` の `let Some(sample_entry) = &sample.sample_entry else { return false; };` を維持。`Sample::sample_entry` doc `src/mux_mp4_file.rs:189-193` の「省略した場合は前のサンプルと同じ sample_entry が使用される」契約を保つため必須）
- 上記 2 段の早期リターン通過後、kind 依存の chunks 参照を `self.tracks.iter().find(|t| t.track_kind == sample.track_kind)` で辿る
- 早期リターンを通過している場合 `find` は `Some` になる想定だが、Rust の型システム上 `Option` を経由するため `map(|t| t.chunks.last().is_none_or(|c| c.sample_entry != *sample_entry)).unwrap_or(true)` パターンで安全に扱う（`find` が `None` を返した場合は防御的に `true` を返す）

Subtitle 早期 return 防御 arm（`src/mux_mp4_file.rs:669`）は削除する。

### `build_trak_box` / `build_mdia_box` への汎用化（1 関数集約）

`build_audio_trak_box` / `build_video_trak_box` / `build_audio_mdia_box` / `build_video_mdia_box`（`src/mux_mp4_file.rs:854-1005`）を廃止し、`TrackEntry` を第一引数に取る 1 関数へ集約する:

```rust
fn build_trak_box(&self, entry: &TrackEntry, track_id: u32) -> Result<TrakBox, MuxError>
fn build_mdia_box(&self, entry: &TrackEntry) -> Result<MdiaBox, MuxError>
```

- `build_stbl_box`（`src/mux_mp4_file.rs:1007-` 現在既に `&[Chunk]` を取る kind 中立実装）と `build_ctts_box`（`src/mux_mp4_file.rs:1126-` 同じく `&[Chunk]` を取る kind 中立）は既存のまま流用する
- tkhd 属性（volume / width / height）と handler type / Media Header の決定は「### tkhd / handler_type / Media Header の決定と共通ヘルパ」節参照
- `build_trak_box` は `!entry.chunks.is_empty()` を invariant として要求する（呼び出し側 `build_moov_box` が空 chunks の TrackEntry を skip するため）。空 chunks で呼ばれた場合は `expect("bug: build_trak_box called with empty chunks")` で panic して良い。**この invariant は `build_trak_box` の doc コメントにも明記する**（将来別経路から呼ばれた際に invariant 違反が読み手に伝わるように）

### tkhd / handler_type / Media Header の決定と共通ヘルパ

Audio / Subtitle kind については `Fmp4SegmentMuxer::derive_trak_attributes` を `Mp4FileMuxer` からも共有利用する。以下の refactor を先に行う:

1. `derive_trak_attributes` のシグネチャを `pub(crate) fn derive_trak_attributes(track_kind: TrackKind, sample_entry: &SampleEntry) -> Result<TrakDerivation, MuxError>` に変更する（現状の `entry: &TrackEntry` 引数から `entry.track_kind` しか使っていないため、`TrackKind` を直接受け取る形にする）
2. `TrakDerivation` の struct 本体と **すべてのフィールド（`volume` / `width` / `height` / `handler_type` / `media_header`）** を `pub(crate)` に格上げする（struct だけ格上げしてもフィールドはデフォルト private のままなので、`Mp4FileMuxer` からのフィールドアクセスがコンパイルできなくなるため必ずセットで格上げする）
3. `derive_trak_attributes` の doc コメント（`src/mux_fmp4_segment.rs:953-955`）は「`entry.track_kind` と `sample_entry` から…」の "`entry.`" を削除して「`track_kind` と `sample_entry` から…」に書き換える
4. `Fmp4SegmentMuxer::build_init_trak`（`src/mux_fmp4_segment.rs:595-682`）の呼び出し側は `derive_trak_attributes(entry.track_kind, sample_entry)` に書き換える。挙動変化なし
5. `extract_video_dimensions`（`src/mux_fmp4_segment.rs:1011`）と `TkhdDimensions`（`src/mux_fmp4_segment.rs:938`）は `Mp4FileMuxer` から直接呼ばないため `pub(crate)` に格上げしない（`derive_trak_attributes` 内部でのみ呼ばれる関数）
6. `src/mux_mp4_file.rs` の use 宣言に `use crate::mux_fmp4_segment::{derive_trak_attributes, TrakDerivation};` を追加する（`Mp4FileMuxer::build_trak_box` の Audio / Subtitle kind で `derive_trak_attributes` を呼び、Video kind で `TrakDerivation` を直接組み立てるため）

`Mp4FileMuxer::build_trak_box` での kind ごとの分岐は以下:

- **Audio kind / Subtitle kind**: `let first_sample_entry = &entry.chunks.first().expect("bug: build_trak_box called with empty chunks").sample_entry;` を取り出し、`derive_trak_attributes(entry.track_kind, first_sample_entry)?` を呼び、返り値 `TrakDerivation` の `volume` / `width` / `height` / `handler_type` / `media_header` をそのまま tkhd / hdlr / minf.media_header に埋め込む
- **Video kind**: `Mp4FileMuxer` は 1 トラック内で複数 sample_entry を許容し、tkhd width / height は全 chunk の最大値（`src/mux_mp4_file.rs:894-907` 既存挙動）を採用するため、`derive_trak_attributes` は呼ばず **`Mp4FileMuxer::build_trak_box` 内で `TrakDerivation` を直接組み立てる**。手順:
  1. `let (max_width_u16, max_height_u16) = entry.chunks.iter().filter_map(|c| c.sample_entry.video_resolution()).fold((0u16, 0u16), |(mw, mh), (w, h)| (mw.max(w), mh.max(h)));`（既存 `src/mux_mp4_file.rs:894-900` を維持）
  2. `let width_i16 = i16::try_from(max_width_u16).map_err(|_| MuxError::EncodeError(Error::invalid_data("video width exceeds i16::MAX")))?;` 同様に `height_i16` を得る（既存 `src/mux_mp4_file.rs:902-907` の i16 変換エラーメッセージを維持し、既存テスト `test_finalize_video_width_exceeds_i16_max`（`src/mux_mp4_file.rs:1562-1575`）/ `test_finalize_video_height_exceeds_i16_max`（同 1577-1591）が pass するようにする）
  3. `TrakDerivation { volume: TkhdBox::DEFAULT_VIDEO_VOLUME, width: FixedPointNumber::new(width_i16, 0), height: FixedPointNumber::new(height_i16, 0), handler_type: HdlrBox::HANDLER_TYPE_VIDE, media_header: MediaHeader::Vmhd(VmhdBox::default()) }` を組み立てる

  Video kind で `derive_trak_attributes` を呼ばない理由は、`derive_trak_attributes` の Video 分岐が内部で `extract_video_dimensions(first_sample_entry)` を呼び第一 chunk の sample_entry のみから width / height を決めてしまい、`Mp4FileMuxer` の「全 chunk の最大値」既存挙動と食い違うため。重複計算を避けるための設計判断

### `build_moov_box` の汎用化

- `src/mux_mp4_file.rs:820-852` `build_moov_box` の `audio_chunks.is_empty()` / `video_chunks.is_empty()` 分岐を、`self.tracks.iter()` で走査する形に書き換える
- **空 chunks の `TrackEntry` は skip する**（現状の `if !self.audio_chunks.is_empty()` / `if !self.video_chunks.is_empty()` の invariant を維持。`MissingSampleEntry` エラー後に finalize を呼ばれても現状は 0 trak の MP4 が生成される既存挙動を保持する）
- `track_id` は `trak_boxes.len() as u32 + 1` として振る（既存 `src/mux_mp4_file.rs:824, 829` 慣習）
- `mvhd_box.next_track_id` は既存の式 `trak_boxes.len() as u32 + 1` を維持する（`src/mux_mp4_file.rs:843`）。内訳は空 chunks skip 後の実 trak 数 + 1 になる
- **trak_box の出力順は「追加順」に固定する**。従来の Mp4FileMuxer は audio → video の固定順（`src/mux_mp4_file.rs:823-831`）で push していたが、`Vec<TrackEntry>` は `ensure_track_entry` の呼び出し順（＝最初の当該 kind の Sample 到着順）で並ぶため、結果として「追加順」になる
- **既存挙動変化**: 例えば Video を先に append し Audio を後に append した場合、従来は Audio が `track_id=1` / Video が `track_id=2` で trak_box[0] が Audio だったが、新実装では Video が `track_id=1` / Audio が `track_id=2` になり trak_box[0] が Video になる。既存 PBT (`pbt/tests/prop_mux_demux.rs:503-591` `mux_demux_video_audio_roundtrip` 等) は tracks の len と各 track の sample count のみを検証し trak 順の絶対値には依存しないため、pass する。既存単体テスト（`test_audio_and_video_tracks` `src/mux_mp4_file.rs:1594-1631`）も `finalized.moov_box_bytes` が非空か否かのみを検証するため、pass する
- この挙動変化は CHANGES.md の `[CHANGE]` エントリの子項目として明記する（「## CHANGES.md」節参照）

### `calculate_total_duration` の汎用化

- `src/mux_mp4_file.rs:1101-1123` を `self.tracks` を走査する形に書き換える
- 「正規化した duration が最長のトラックの `(timescale, duration)` を返す」既存ロジックを 3 種類以上のトラックに一般化する
- 空 `tracks`（0 トラック）時は `(NonZeroU32::MIN, 0)` を返す（この関数は `build_moov_box` 内 `src/mux_mp4_file.rs:834` で呼ばれ、`mvhd_box.timescale` / `mvhd_box.duration` に埋め込まれる。空 tracks 時は `trak_boxes` が空の MoovBox として finalize は成功するため、任意の初期値で十分）

### `build_final_ftyp_box` の汎用化

- `src/mux_mp4_file.rs:724-758` `build_final_ftyp_box` の `for chunk in self.audio_chunks.iter().chain(self.video_chunks.iter())`（行 730）を `for track in &self.tracks { for chunk in &track.chunks {` の入れ子ループに書き換える
- 字幕系ブランドは追加しない方針を継続する（「### スコープ」節「含まないもの」参照）

### 拒否経路の除去と `MuxError::UnsupportedTrackKind` の存置

- `src/mux_mp4_file.rs:568-572, 623-627, 669` の `TrackKind::Subtitle` 早期拒否 / 防御 arm をすべて除去する
- `MuxError::UnsupportedTrackKind` バリアント自体は `mux_mp4_file.rs:311-318` に残す。C API `Mp4Error::MP4_ERROR_UNSUPPORTED` マッピング（`crates/c-api/src/error.rs`）と将来の拡張余地を維持するため
- `test_unsupported_track_kind_display_contains_subtitle`（`src/mux_mp4_file.rs:1307-1321`）は削除しない。バリアント本体の Display 検証で `Mp4FileMuxer::append_sample` の Subtitle 拒否経路に依存しないため
- `crates/c-api/src/error.rs` の `test_unsupported_track_kind_maps_to_mp4_error_unsupported` も削除しない（C API マッピング検証で維持）

### 公開 API シグネチャへの影響

- `Mp4FileMuxer` の pub フィールドは 0 個（すべて private）。内部フィールド構造化は semver 上の破壊的変更ではない
- `Mp4FileMuxer` の pub メソッド（`new` / `with_options` / `initial_boxes_bytes` / `advance_position` / `append_sample` / `finalize` / `finalized_boxes`）はすべてシグネチャ・返り値ともに変更なし
- 周辺 pub 型（`Sample` / `MuxError` / `FinalizedBoxes` / `Mp4FileMuxerOptions` / `estimate_maximum_moov_box_size`）にも pub シグネチャ変更なし
- `TrackKind::Subtitle` を渡した際の挙動が「エラー返却」から「受入」に変わる
- trak_box の出力順が「Audio → Video の固定順」から「`append_sample` 呼び出し順」に変わる。「### `build_moov_box` の汎用化」節参照
- `MissingSampleEntry` エラー時の副作用が消滅する（現状は当該 kind の timescale が記録済みで残ったが、新実装では `self` の内部状態は完全に不変になる）
- `derive(Debug)` の出力形式が変化する（フィールド構造化に伴う自然な変化。semver は保証しない範囲）

## 完了条件

### 実装完了

- `Mp4FileMuxer` の内部フィールドを `tracks: Vec<TrackEntry>` に置き換える（`TrackEntry` は `track_kind` / `timescale` / `chunks` を持つ private 型）
- `ensure_track_entry` private ヘルパを追加する
- `append_sample` を「### `append_sample` の失敗パス契約」節の順序に書き換える（Subtitle 拒否 / 防御 arm をすべて除去）
- `is_new_chunk_needed` を汎用化する
- `build_trak_box(entry, track_id)` / `build_mdia_box(entry)` の 2 関数へ集約する（Audio / Video 専用の 4 関数を廃止）
- Video kind の tkhd width / height は既存の fold ロジックを維持する（`Mp4FileMuxer::build_trak_box` 内で直接 `TrakDerivation` を組み立て、`derive_trak_attributes` を呼ばない）
- Audio / Subtitle kind は `derive_trak_attributes(track_kind, first_sample_entry)` を呼んで結果をそのまま使う
- `build_moov_box` を `self.tracks.iter()` 走査に書き換え、空 chunks の TrackEntry を skip し、trak_box を追加順で出力する
- `calculate_total_duration` を `self.tracks` 走査に書き換える
- `build_final_ftyp_box` の 2 系統チャンク走査ループを `self.tracks` の入れ子ループに書き換える
- `Fmp4SegmentMuxer::derive_trak_attributes` と `TrakDerivation`（フィールド全 5 個含む）を `pub(crate)` に格上げし、シグネチャを `derive_trak_attributes(track_kind: TrackKind, sample_entry: &SampleEntry)` に変更する。`Fmp4SegmentMuxer::build_init_trak` の呼び出し側と `derive_trak_attributes` の doc コメントも同期更新する
- `Mp4FileMuxer` が `TrackKind::Subtitle` の `Sample` を `append_sample` で受け入れ、`Mp4FileMuxer::finalize` で MP4 バイト列を生成できる
- 生成された MP4 の Subtitle トラックが `Mp4FileDemuxer` でラウンドトリップできる
- 既存の Audio / Video トラック mux の挙動が変わらない（trak 順を除いて。既存 PBT / 単体テストが pass）
- Audio + Video + Subtitle の 3 トラック mux が可能（`build_moov_box` が 3 種類の trak_box を含める）
- 「### 更新が必要な doc コメント」節の 7 箇所すべてを追随更新する
- 「### PBT / 単体テスト追加（`Mp4FileMuxer` 経路）」節および「### `Mp4FileDemuxer` 経路の合成ラウンドトリップテスト追加」節に列挙されたすべてのテスト・ヘルパ（`create_stpp_sample_entry` / `arb_subtitle_sample_info` / `SubtitleSampleInfo` / `mux_demux_subtitle_only_roundtrip` / `mux_demux_video_audio_subtitle_roundtrip` / `test_subtitle_track_append_and_finalize` / `test_audio_video_subtitle_tracks` / `test_missing_sample_entry_error_leaves_tracks_unchanged` / `stpp_sample_entry_via_mp4_file_demuxer` / `wvtt_sample_entry_via_mp4_file_demuxer` / `tx3g_sample_entry_via_mp4_file_demuxer` / `build_stpp_mp4_file_bytes` / `build_wvtt_mp4_file_bytes` / `build_tx3g_mp4_file_bytes`）が追加される
- `cargo clippy --all-targets --all-features` が warning なしで通る
- `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が warning なしで通る（CI で実行されるコマンド）
- `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` がすべて pass する（0045 の解決方法と揃える）

### テスト削除

- `src/mux_mp4_file.rs:1279-1305` `test_unsupported_track_kind_error_for_subtitle`（Subtitle 拒否の単体テスト）を削除する。**「## 解決方法」の Subtitle 拒否 arm 除去と同一コミット** で削除する（分離すると cargo test が pass しなくなるため）

### PBT / 単体テスト追加（`Mp4FileMuxer` 経路）

以下を `pbt/tests/prop_mux_demux.rs` に追加する:

- `create_stpp_sample_entry()`: 引数なしで最小構成の `SampleEntry::Stpp(StppBox { data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX, namespace: Utf8String::new("http://www.w3.org/ns/ttml").expect("null 文字を含まない"), schema_location: Utf8String::EMPTY, auxiliary_mime_types: Utf8String::EMPTY, unknown_boxes: vec![] })` を返すヘルパ。既存 `create_avc1_sample_entry(width, height)`（`pbt/tests/prop_mux_demux.rs:22`）/ `create_opus_sample_entry(channel_count)`（同 139）と同じ場所に追加する。expect メッセージは既存 `build_stpp_fmp4_segments`（`pbt/tests/prop_container_boxes.rs:1008`）と同じ「null 文字を含まない」に揃える
- `pbt/tests/prop_mux_demux.rs:9-19` の top-level use 宣言に `Utf8String` と `boxes::StppBox` を追加する（現状は `Decode, FixedPointNumber, TrackKind, Uint` および `boxes::{AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, Brand, DopsBox, FtypBox, Hev1Box, Hvc1Box, HvccBox, OpusBox, SampleEntry, VisualSampleEntryFields}` のみ）
- `arb_subtitle_sample_info()` Strategy: 既存 `arb_video_sample_info`（`pbt/tests/prop_mux_demux.rs:247-255`）/ `arb_audio_sample_info`（同 258-263）と同型の struct `SubtitleSampleInfo { duration: u32, data_size: usize }` を新設して生成する。`keyframe` フィールドは持たせない（`Sample` doc `src/mux_mp4_file.rs:179-183` の推奨値「字幕サンプルは通常すべて独立サンプル」に従い、`mux_demux_subtitle_only_roundtrip` 内で `Sample.keyframe = true` を固定値として使う設計）。値域は `duration in 1u32..100` / `data_size in 100usize..2000`（字幕サンプルは音声より小さい想定）。payload バイト列は Strategy では保持せず、既存 `build_file_data` パターンと同じく `data_size` 分の zero-filled バッファを `append_sample` の直前に書き出すだけとする（既存 Video / Audio と同型）
- `mux_demux_subtitle_only_roundtrip`: Subtitle トラック 1 本のみ（`SampleEntry::Stpp` 固定）で `Mp4FileMuxer` → `Mp4FileDemuxer` ラウンドトリップを検証。既存 `mux_demux_audio_only_roundtrip` / `mux_demux_video_only_roundtrip` の隣に追加する。demuxer 側の集計 match は Subtitle のみカウントし、Audio / Video は本テストの対象外として `unreachable!("音声・映像トラックは本テストの対象外")` にする（既存 `mux_demux_video_audio_roundtrip`（`pbt/tests/prop_mux_demux.rs:586`）の Subtitle arm `unreachable!` パターンに倣う）
- `mux_demux_video_audio_subtitle_roundtrip`: Audio + Video + Subtitle の 3 トラック mux → demux ラウンドトリップを検証。既存 `mux_demux_video_audio_roundtrip`（`pbt/tests/prop_mux_demux.rs:503-591`）の隣に追加する。サンプル追加順は Video → Audio → Subtitle の順にする。demuxer 側の集計 match で 3 種類とも arm を追加し `unreachable!` は使わない

以下を `src/mux_mp4_file.rs` の tests モジュールに追加する:

- tests モジュールの use 宣言（`src/mux_mp4_file.rs:1190-1195`）に `StppBox` を追加する（`Utf8String` は親モジュール `src/mux_mp4_file.rs:60-61` の `use crate::{ ..., Utf8String, ...};` および tests モジュール冒頭 `use super::*;` で既に見えているため不要）
- ローカルヘルパ `fn create_stpp_sample_entry() -> SampleEntry` を tests モジュール内に定義する（内容は `pbt/tests/prop_mux_demux.rs` に追加するものと同じ。integration test 側の関数は src からは参照できないためコピーする）
- `test_subtitle_track_append_and_finalize`: Subtitle トラック 1 本で `create_stpp_sample_entry()` を渡し、`append_sample` → `finalize` の smoke test（既存 `test_missing_sample_entry_error`（`src/mux_mp4_file.rs:1325`）と同じ場所）
- `test_audio_video_subtitle_tracks`: 追加順は既存 `test_audio_and_video_tracks`（`src/mux_mp4_file.rs:1594-1631`）と揃え Video → Audio、続いて Subtitle の順に append する。SampleEntry は Video が `create_avc1_sample_entry`、Audio が `create_opus_sample_entry`、Subtitle が `create_stpp_sample_entry` を使う
- `test_missing_sample_entry_error_leaves_tracks_unchanged`: 新規 kind の初回サンプルで `sample_entry = None` を渡して `MissingSampleEntry` を受け取った後、別 `timescale` の Sample を投入しても `TimescaleMismatch` にならないことを検証する。「### `append_sample` の失敗パス契約」節の「既存挙動からの改善点」（`MissingSampleEntry` 時の timescale 副作用消滅）の回帰テストで、CHANGES.md `[CHANGE]` エントリの子項目を担保する

### `Mp4FileDemuxer` 経路の合成ラウンドトリップテスト追加

0043 / 0044 / 0045 側で「0046 完了後に別途追加」と保留された 3 経路 typed テストを、本 issue のスコープに取り込み、`pbt/tests/prop_container_boxes.rs` の `boundary_tests` モジュール内に追加する:

- `stpp_sample_entry_via_mp4_file_demuxer`: `SampleEntry::Stpp(_)` を持つ Subtitle トラックを `Mp4FileMuxer` で mux し、`Mp4FileDemuxer` で `sample.sample_entry` が `Some(SampleEntry::Stpp(_))` として取り出せることを検証
- `wvtt_sample_entry_via_mp4_file_demuxer`: 同上で `SampleEntry::Wvtt(_)` を検証
- `tx3g_sample_entry_via_mp4_file_demuxer`: 同上で `SampleEntry::Tx3g(_)` を検証

追加位置は既存の `stpp_sample_entry_via_fmp4_file_demuxer`（`pbt/tests/prop_container_boxes.rs:1026`）/ `wvtt_sample_entry_via_fmp4_file_demuxer`（同 1172）/ `tx3g_sample_entry_via_fmp4_file_demuxer`（同 1323）の隣とする。sample payload は既存 `build_stpp_fmp4_segments`（1005）/ `build_wvtt_fmp4_segments`（1159）/ `build_tx3g_fmp4_segments`（1306）で使われている以下のバイト列を流用する:

- stpp: `b"<tt xmlns=\"http://www.w3.org/ns/ttml\"/>"`
- wvtt: `b"WEBVTT-cue-payload-placeholder"`
- tx3g: `b"\x00\x05HELLO"`

`Fmp4SegmentMuxer` 経由のヘルパは `Mp4FileMuxer` の `initial_boxes_bytes` / `append_sample` / `finalize` の流れと異なるため直接再利用はできない。`pbt/tests/prop_container_boxes.rs` の import に `Mp4FileMuxer` を追加し（現状 19 行は `mux::{Fmp4SegmentMuxer, Sample}` のみ）、同ファイル内に `build_stpp_mp4_file_bytes` / `build_wvtt_mp4_file_bytes` / `build_tx3g_mp4_file_bytes` の 3 ヘルパを追加する。各ヘルパのシグネチャは `fn build_XXX_mp4_file_bytes() -> Vec<u8>` で、以下の 8 ステップで完成した MP4 バイト列 1 本を返す（既存 `pbt/tests/prop_mux_demux.rs:160-189` の `build_file_data` と同型のフローを inline する。integration test の性質上、他ファイルのヘルパは直接再利用できないためコピーする）:

1. `let mut muxer = Mp4FileMuxer::new()?;`
2. `let mut output: Vec<u8> = muxer.initial_boxes_bytes().to_vec();`
3. `let data_offset = output.len() as u64;`
4. `let sample_entry = SampleEntry::Stpp(StppBox { ... });`（方式ごとに最小構成。stpp は `create_stpp_sample_entry()` と同じ初期値パターン、wvtt は既存 `build_wvtt_fmp4_segments` と同じ `WvttBox { data_reference_index: WvttBox::DEFAULT_DATA_REFERENCE_INDEX, vttc_box: VttCBox { config: String::from("WEBVTT") }, unknown_boxes: vec![] }`、tx3g は既存 `build_tx3g_fmp4_segments` と同じ `Tx3gBox { data_reference_index: Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX, display_flags: 0, horizontal_justification: 0, vertical_justification: 0, background_color_rgba: [0, 0, 0, 0], default_text_box: BoxRecord::default(), default_style: StyleRecord::default(), ftab_box: FtabBox::default(), unknown_boxes: vec![] }`）
5. `let payload: &[u8] = b"...";` として上述の対応表の payload を選び、`output.extend_from_slice(payload);` で append する
6. `let timescale = NonZeroU32::new(1000).expect("non-zero");` `let duration = 1000u32;` を導入し、`Sample { track_kind: TrackKind::Subtitle, sample_entry: Some(sample_entry), keyframe: true, timescale, duration, composition_time_offset: None, data_offset, data_size: payload.len() }` を構築して `muxer.append_sample(&sample)?;`（既存 `build_subtitle_fmp4_segments`（`pbt/tests/prop_container_boxes.rs:975-988`）と同じ timescale / duration パターン）
7. `let finalized = muxer.finalize()?;`
8. `finalized.offset_and_bytes_pairs()` は `(offset: u64, bytes: &[u8])` の列を返す。デフォルトの `Mp4FileMuxer` は faststart を有効にできないため moov は mdat の末尾（現状の `output.len()`）以降に書き戻される。以下の手順で書き戻す（既存 `pbt/tests/prop_mux_demux.rs:160-189` の `build_file_data` パターンに揃える）:
   ```rust
   // moov などの書き戻し範囲を先に計算して output を事前拡張する
   let max_end = finalized.offset_and_bytes_pairs()
       .map(|(offset, bytes)| offset as usize + bytes.len())
       .max()
       .unwrap_or(output.len());
   if max_end > output.len() {
       output.resize(max_end, 0);
   }
   for (offset, bytes) in finalized.offset_and_bytes_pairs() {
       let start = offset as usize;
       output[start..start + bytes.len()].copy_from_slice(bytes);
   }
   // 完成した output: Vec<u8> を返す
   ```

### 既存 PBT の `unreachable!` arm の扱い

`pbt/tests/prop_mux_demux.rs:586` および `pbt/tests/prop_mux_demux.rs:894` の `TrackKind::Subtitle => unreachable!("字幕トラックは本テストの対象外")` は本 issue でも維持する。既存 PBT の Strategy 側は Audio / Video のみを生成しており、Subtitle は混入しないため `unreachable!` が発火することはない。Subtitle 対応は上述「### PBT / 単体テスト追加」の新規テスト（`mux_demux_subtitle_only_roundtrip` / `mux_demux_video_audio_subtitle_roundtrip`）で担保する。

## 解決方法

以下の順で実装する見込み。相互依存で「単独では cargo build / cargo test が通らない」手順は同一コミット単位でまとめる。

1. `Fmp4SegmentMuxer::derive_trak_attributes` と `TrakDerivation`（フィールド全 5 個含む）を `pub(crate)` に格上げし、`derive_trak_attributes` のシグネチャを `(track_kind: TrackKind, sample_entry: &SampleEntry)` に変更する。`Fmp4SegmentMuxer::build_init_trak` の呼び出し側と `derive_trak_attributes` の doc コメントも同期更新する（挙動不変。単独で cargo build / cargo test が通る）
2. **同一コミット単位で実施**: `Mp4FileMuxer::TrackEntry` を追加し、`Mp4FileMuxer` の 4 フィールド (`audio_chunks` / `video_chunks` / `audio_track_timescale` / `video_track_timescale`) を `tracks: Vec<TrackEntry>` に置き換える。`ensure_track_entry` ヘルパを追加する。`append_sample` を「### `append_sample` の失敗パス契約」節の順序に書き換え（Subtitle 拒否 arm を削除）、`is_new_chunk_needed` を汎用化する。`build_moov_box` / `build_trak_box` / `build_mdia_box` / `calculate_total_duration` / `build_final_ftyp_box` を汎用化する（`build_audio_trak_box` / `build_video_trak_box` / `build_audio_mdia_box` / `build_video_mdia_box` を廃止）。`test_unsupported_track_kind_error_for_subtitle`（`src/mux_mp4_file.rs:1279-1305`）を **同一コミットで削除** する（分離すると cargo test が pass しなくなるため）。さらに **本コミット内で「### 更新が必要な doc コメント」節の 7 箇所（`src/mux_mp4_file.rs:1-5, 311-318, 409-411, 552-559, 564-567` と `src/mux_fmp4_segment.rs:1-5, 22-25`）の doc / インラインコメントも同時に書き換える**（本体書き換えと doc を分離すると cargo doc が warning を出す中間状態を経由し、完了条件「`cargo doc` が warning なしで通る」と食い違うため）。フィールド差し替えで関数群がすべて同時にコンパイルエラーになるため 1 コミットで実施する
3. `test_subtitle_track_append_and_finalize` / `test_audio_video_subtitle_tracks` / `test_missing_sample_entry_error_leaves_tracks_unchanged` を `src/mux_mp4_file.rs` の tests モジュールに追加する
4. `create_stpp_sample_entry` / `arb_subtitle_sample_info` ヘルパを追加し、`mux_demux_subtitle_only_roundtrip` / `mux_demux_video_audio_subtitle_roundtrip` を `pbt/tests/prop_mux_demux.rs` に追加する。既存 PBT の demuxer 側 match に Subtitle arm 追加も行う（新テスト内でのみ）
5. `build_stpp_mp4_file_bytes` / `build_wvtt_mp4_file_bytes` / `build_tx3g_mp4_file_bytes` 相当のヘルパを `pbt/tests/prop_container_boxes.rs` の `boundary_tests` モジュール内に追加し、`stpp_sample_entry_via_mp4_file_demuxer` / `wvtt_sample_entry_via_mp4_file_demuxer` / `tx3g_sample_entry_via_mp4_file_demuxer` を追加する
6. `cargo clippy --all-targets --all-features` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` で最終検証する

## CHANGES.md

以下 1 エントリで記載する（担当者行 `- @ユーザー名` は実装時に補う）:

- `[ADD]` `Mp4FileMuxer` で `TrackKind::Subtitle` トラックの受け入れと mux に対応する
  - Audio + Video + Subtitle の 3 トラック mux が可能になる
  - `stpp` / `wvtt` / `tx3g` の各 `SampleEntry` を含む Subtitle トラックを `Mp4FileMuxer` → `Mp4FileDemuxer` でラウンドトリップできる
  - `MuxError::UnsupportedTrackKind` バリアント自体は残す（`Mp4FileMuxer` からは投げなくなる。C API の `MP4_ERROR_UNSUPPORTED` マッピングも維持）
- `[CHANGE]` `Mp4FileMuxer` の内部フィールド構造化に伴う 2 つの挙動変化
  - `finalize` で生成される MP4 の trak_box 出力順が「Audio → Video の固定順」から「`append_sample` 呼び出し順（先に登場した `TrackKind` が先）」に変わる。既存 Rust API シグネチャは維持されるが、生成バイト列を順序に依存して検証しているダウンストリームは影響する
  - `MuxError::MissingSampleEntry` エラー返却時の副作用が消滅する（現状は当該 kind の timescale が記録済みで残ったが、新実装では `self` の内部状態は完全に不変になる）
