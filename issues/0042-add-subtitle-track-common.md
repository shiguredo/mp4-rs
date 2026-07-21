# 字幕トラックの共通基盤（TrackKind / handler type / Media Header）を追加する

- Priority: Low
- Created: 2026-07-21
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-subtitle-track-common
- Polished: 2026-07-21

## 目的

音声認識などで生成した字幕テキストを MP4 に格納できるようにするための、字幕トラック共通基盤（`TrackKind::Subtitle` バリアント、handler type 定数、Media Header ボックス、`MinfBox` の型刷新、demux / mux 経路の穴あけ、C API / WASM 露出）を追加する。方式固有のサンプルエントリー（`stpp` / `wvtt` / `tx3g`）は別 issue（0043 / 0044 / 0045）で扱い、本 issue は前提となる共通部分のみを扱う。

## 優先度根拠

Low。バグ由来ではなく緊急要求も無い。後続 3 方式（0043 / 0044 / 0045）はいずれも本 issue の完了を待つ blocker 構造にあるが、方式実装自体が Low で緊急性がないため、本 issue も Low で足りる（依存元より格上げする根拠が無い）。

## 依存関係

- 本 issue はどの issue にも依存しない
- 本 issue は 0043 / 0044 / 0045 の依存元となる（各 issue の「依存関係」節で明記済み）
- 「## 実装着手前の準備」節の 3 件を先に完了させる必要がある（詳細は同節）

## 現状

トラック種別と関連ボックスは 2 種類だけをサポートしている。

- `src/basic_types.rs:677-683` `TrackKind` は `Audio` と `Video` のみ（`#[non_exhaustive]` なし。shiguredo-rust スキル規約に従い今後も付与しない）
- `src/boxes_moov_tree.rs:920,923` `HdlrBox` の handler type 定数は `HANDLER_TYPE_SOUN` (`soun`) と `HANDLER_TYPE_VIDE` (`vide`) のみ
- `src/boxes_moov_tree.rs:985-986` `MinfBox::smhd_or_vmhd_box: Option<Either<SmhdBox, VmhdBox>>` で音声/映像の 2 種類の Media Header にしか対応していない。`Option` は 2025.4.0 の `[CHANGE]` で「メディアトラック以外を含む MP4」対応のため意図的に付与された経緯があり、本 issue でも維持する

デマルチプレクサ側は handler type が `soun` / `vide` 以外だとトラックそのものを skip する。

- `src/demux_mp4_file.rs:511-514`
- `src/demux_fmp4_file.rs:320-323`
- `src/demux_fmp4_segment.rs:145-148`

`Fmp4SegmentMuxer` は `TrackKind` から handler type と Media Header を 2 バリアント網羅 match で決めているため、字幕系を通す口が無い。

- `src/mux_fmp4_segment.rs:655-658, 665-668` handler type / Media Header 分岐
- `src/mux_fmp4_segment.rs:22-24` doc に「同時に扱えるトラックは Audio 1 本と Video 1 本まで」との制限（3 行構成: 1 行目 = 制限文、2 行目 = 「将来、同種複数トラックに対応する場合は file muxer と合わせて拡張する想定」）

`Mp4FileMuxer` は内部が Audio / Video 専用フィールドで構造化されており、`TrackKind` バリアント追加だけでは Subtitle トラックを受け付けられない。本 issue では拒否経路のみを実装し、実際の受け入れは別 issue で対応する。

decode 側の `SampleEntry::Unknown` フォールバックは既に機能しており（`src/boxes_sample_entry.rs:137`）、追加実装は不要。

### `TrackKind` の網羅 match 箇所（バリアント追加で必ずコンパイル修正が必要）

- `src/mux_mp4_file.rs:561-590` `append_sample` の match
- `src/mux_mp4_file.rs:623-626` `is_new_chunk_needed` の match
- `src/mux_fmp4_segment.rs:655-658, 665-668`
- `crates/c-api/src/basic_types.rs:16-32` の双方向 `From` impl
- `crates/wasm/src/demux.rs:102-105`
- `crates/wasm/src/fmp4_segment_demux.rs:154-157`
- `pbt/tests/prop_mux_demux.rs:583-584, 886-890`

### 網羅ではないが更新が必要な箇所

- `crates/wasm/src/mux.rs:73-77` の JSON パーサ（`"audio"` / `"video"` のみ受理、エラーメッセージ本文も更新）
- `crates/wasm/src/fmp4_segment_mux.rs:286-299` の JSON パーサ（同）
- `crates/wasm/src/fmp4_segment_mux.rs:33-47` `same_track_kind`（`matches!` の非網羅版のためコンパイルは通るが、Subtitle バリアント追加後に `Subtitle vs Subtitle` の判定が常に false になる潜在バグ）
- `pbt/tests/prop_fmp4_segment_mux_demux.rs:695` `prop_assert_eq!(track_kind, TrackKind::Video)` の等値アサーション（コンパイルは通るが、テスト網羅性のため Subtitle 版のテスト追加を検討する箇所）
- `crates/c-api/src/error.rs:72-87` の `impl From<MuxError> for Mp4Error` は末尾に `_ => Self::MP4_ERROR_OTHER` フォールバックがあり、`MuxError` が `#[non_exhaustive]` のため新規バリアント追加でコンパイルエラーにならない。`UnsupportedTrackKind` の arm を `_` より前に明示追加する必要がある

### `MinfBox` フィールド型変更で更新が必要な箇所

- `src/boxes_moov_tree.rs:985-986` フィールド定義（`pub` フィールドのため利用者コード側の直接アクセスも影響を受ける）およびフィールドに付くコメント
- `src/boxes_moov_tree.rs:997-1015` `Encode`
- `src/boxes_moov_tree.rs:1017-1062` `Decode`（`SmhdBox::TYPE` / `VmhdBox::TYPE` 以外の match arm 追加が必要）
- `src/boxes_moov_tree.rs:1069-1077` `children`
- `src/mux_mp4_file.rs:912-916, 948-953` `smhd_or_vmhd_box:` フィールド初期化
- `src/mux_fmp4_segment.rs:699-702` 同
- `examples/transcode_wasm/src/mp4.rs:149-153` の直接フィールドアクセス
- `pbt/tests/prop_container_boxes.rs:150-158, 281-327, 500`
- `pbt/tests/prop_error_paths.rs:964-1012, 1887-1897`

## 設計方針

字幕方式に依存しない共通部分だけを実装する。方式固有の `SampleEntry` バリアントは本 issue の対象外（0043 / 0044 / 0045 で対応）。

### スコープ

含むもの:

- `TrackKind::Subtitle` の追加
- 字幕用 handler type 定数の追加（`subt` / `text`）
- `SthdBox` / `NmhdBox` の実装
- `MinfBox` の Media Header 保持構造の刷新（新規 enum `MediaHeader` 導入）
- デマルチプレクサ 3 系統の handler type 分岐拡張
- `Fmp4SegmentMuxer` の字幕トラック受け入れ経路（tkhd 属性・handler type / Media Header 固定選択・doc 更新を含む）
- `Mp4FileMuxer` の Subtitle 拒否経路（新規エラー `MuxError::UnsupportedTrackKind` を返す）
- C API / WASM の `TrackKind` / 新規エラー種別の露出
- PBT / 単体テスト・`examples/transcode_wasm` の更新

含まないもの:

- **`Mp4FileMuxer` (`src/mux_mp4_file.rs`) の字幕トラック受け入れ**。`audio_chunks` / `video_chunks` の 2 系統ハードコードを解消するには内部リファクタが必要で、字幕対応本体よりリファクタ判断のスコープが大きい。本 issue では `MuxError::UnsupportedTrackKind` で明示拒否するだけとし、受け入れは別 issue で扱う（起票運用は「## 実装着手前の準備」節を参照）
- **サンプルエントリー種別（`stpp` / `wvtt` / `tx3g`）に基づく handler type / Media Header 分岐関数**。0042 の時点では方式固有 `SampleEntry` バリアントが未実装のため決定表 (`stpp→subt+sthd`, `wvtt→text+sthd`, `tx3g→text+nmhd`) を実装できない。0042 では Subtitle 全体で **単一の handler type / Media Header (`subt` + `sthd`)** を固定選択する暫定実装とし、0043 の実装時にこの暫定を **完全置換する** 分岐関数を新設する（暫定の fallback は残さない）
- **`gmhd` (GenericMediaHeaderBox) 系ボックスの実装**。tx3g の QuickTime 系レガシー慣習で使う場合があるが、ISO/IEC 14496-12 の主流は `sthd` / `nmhd`。まず 14496-12 準拠に絞る
- **方式固有の `SampleEntry` バリアント**（0043 / 0044 / 0045 側）
- **`SampleEntry::Unknown` の C API / WASM 露出**。字幕トラックのサンプルエントリー内容へのアクセスは 0043-0045 の担当

### `TrackKind` の拡張

- バリアント名は `Subtitle` を採用する
- 本 issue で `#[non_exhaustive]` は付けない（shiguredo-rust スキルの規約に従う）

### handler type 定数の追加

以下の 2 種類を `HdlrBox` に追加する。0043 / 0044 / 0045 の要求から逆算した確定値。

- `HANDLER_TYPE_SUBT: [u8; 4] = *b"subt"` — stpp（ISO/IEC 14496-30 XMLSubtitleSampleEntry の handler type）
- `HANDLER_TYPE_TEXT: [u8; 4] = *b"text"` — wvtt（ISO/IEC 14496-30 WVTTSampleEntry の handler type）および tx3g（3GPP TS 26.245 の handler type）

`subt` / `text` のどちらを持つトラックも `TrackKind::Subtitle` に射影する。

### `SthdBox` / `NmhdBox` の実装

ISO/IEC 14496-12 に従って以下を追加する。

- `SthdBox` (`sthd`, SubtitleMediaHeaderBox): 追加ペイロードなしの FullBox
- `NmhdBox` (`nmhd`, NullMediaHeaderBox): 追加ペイロードなしの FullBox

いずれも既存 `SmhdBox` / `VmhdBox`（`src/boxes_moov_tree.rs:1080-1219`）と揃えて以下を実装する。

- `#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]`（既存 `SmhdBox` は `Default` を derive しているため揃える）
- `Encode` / `Decode` / `BaseBox` / `FullBox`
- doc コメント形式は既存の `[ISO/IEC 14496-12] <ClassName> class (親: [MinfBox]）`（全角閉じ括弧）に揃える

`NmhdBox` は字幕トラック専用ではなく「メディアハンドラーに対応する Media Header が特にない汎用ボックス（ヒントトラック等でも使われる）」であることを doc に明記する。

### `MinfBox` の Media Header 保持構造の刷新

- `MinfBox::smhd_or_vmhd_box: Option<Either<SmhdBox, VmhdBox>>` を `MinfBox::media_header: Option<MediaHeader>` に置き換える（型・フィールド名の同時変更。`pub` フィールドのため利用者コードにも影響）
- 新規 enum `MediaHeader` を導入し、以下のバリアントを持つ
  - `Smhd(SmhdBox)`
  - `Vmhd(VmhdBox)`
  - `Sthd(SthdBox)`
  - `Nmhd(NmhdBox)`
- `MediaHeader` の派生 / 実装は既存 `SampleEntry` enum（enum 定義は `src/boxes_sample_entry.rs:15-28`、`Encode` / `Decode` / `BaseBox` 実装は 107-154）と揃える。`#[derive(Debug, Clone, PartialEq, Eq, Hash)]` + `BaseBox` / `Encode` / `Decode` を実装し、`FullBox` は各 inner box 側で実装済みのため enum 側では実装しない
- `MediaHeader::decode` の実装方針
  - `SampleEntry::decode`（`src/boxes_sample_entry.rs:124-140`）と同じく BoxHeader を先読みして `SmhdBox::TYPE` / `VmhdBox::TYPE` / `SthdBox::TYPE` / `NmhdBox::TYPE` から dispatch する
  - `MinfBox::decode` 側で 4 種の box_type を事前判別してから呼ぶ運用のため、既知でない box_type は通常到達しない。ただし `MediaHeader::decode` は公開 API のため防衛的に `Err(Error::invalid_data("unexpected box type for MediaHeader"))` を返す（`SampleEntry::decode` の `Unknown` フォールバックとは異なる）
- `MinfBox::decode` の while ループ内 match arm を更新し、`SmhdBox::TYPE` / `VmhdBox::TYPE` / `SthdBox::TYPE` / `NmhdBox::TYPE` のいずれかを見つけた時点で `MediaHeader::decode_at(...)` を呼び、最初に見つかったものを `Option<MediaHeader>` に代入する（現状の smhd 優先ロジックは廃止する。仕様上 minf 直下には Media Header は 1 種類しか出ないため実害はない）
- `Option` ラップは維持する（「現状」セクションの経緯を尊重する）
- `MinfBox::encode` / `MinfBox::children` も新型に合わせて更新
- `MinfBox::media_header` フィールドに付くコメント（現状 `// 音声・映像トラック以外の場合は None になる`）も新型に合わせて更新する
- 既存 `Either<A, B>` 型（`src/basic_types.rs:587`）は `stco_or_co64_box` 等で引き続き使われているため削除しない

### `Fmp4SegmentMuxer` の字幕トラック対応

- `build_init_trak` の handler type / Media Header 分岐 (`src/mux_fmp4_segment.rs:655-668`) に `TrackKind::Subtitle` の arm を追加する
- 本 issue の暫定実装では Subtitle 全体で **handler type = `subt`、Media Header = `sthd`** を固定選択する（0043-0045 で方式ごとの分岐関数に完全置換する）
- `visual` 分岐 (`src/mux_fmp4_segment.rs:604-636`) の tkhd 属性決定を刷新する
  - 現状は `visual = match sample_entry { Avc1(b) => Some(&b.visual), ... _ => None }` の結果で `Some => (DEFAULT_VIDEO_VOLUME, w, h)` / `None => (DEFAULT_AUDIO_VOLUME, 0, 0)` を選んでいる。Audio と Subtitle の両方で `None` に落ちるため、Subtitle でも Audio 用 volume が採用される不整合がある
  - 新実装では **`entry.track_kind` を外側で明示 match** して以下のように決定する。既存の `visual = match sample_entry { ... }` は Video arm の内側でローカルに書き直す形にする
    - `TrackKind::Video`: 内側で `sample_entry` から `visual` を取得。`visual = Some(v)` なら `volume = TkhdBox::DEFAULT_VIDEO_VOLUME`、`width = v.width`、`height = v.height`。`visual = None`（Video トラックに映像系以外の `SampleEntry` が渡された変則ケース）でも `volume = TkhdBox::DEFAULT_VIDEO_VOLUME`、`width = 0`、`height = 0` を採用する（従来の暗黙的な audio volume 誤決定を修正する副次効果あり）
    - `TrackKind::Audio`: `volume = TkhdBox::DEFAULT_AUDIO_VOLUME`、`width = 0`、`height = 0`
    - `TrackKind::Subtitle`: `volume = TkhdBox::DEFAULT_VIDEO_VOLUME`（値は 0）、`width = 0`、`height = 0`（字幕トラック用の tkhd 慣習に合わせる）
- `same_track_kind` (`crates/wasm/src/fmp4_segment_mux.rs:33-47`) に Subtitle 対応 arm を追加（`Subtitle vs Subtitle` の判定が true になるよう明示）
- `Fmp4SegmentMuxer` の doc コメント (`src/mux_fmp4_segment.rs:22-24`) を以下に書き換える（3 行構成の 1 行目を書き換え、2 行目「将来、同種複数トラックに対応する場合は file muxer と合わせて拡張する想定」はそのまま維持する — 依然として同種複数の未対応は残るため）
  - 旧: 「現時点では `Mp4FileMuxer` と同様に、同時に扱えるトラックは Audio 1 本と Video 1 本までに制限している。」
  - 新: 「現時点では同一 `TrackKind` のトラックは 1 本までに制限している（Audio / Video / Subtitle 各 1 本）。`Mp4FileMuxer` は現時点で Subtitle 未対応のため、Subtitle トラックは `Fmp4SegmentMuxer` 経由でのみ扱える。」
- `build_ftyp` の `compatible_brands` (`src/mux_fmp4_segment.rs:501-538`) は本 issue の範囲では字幕系ブランド（`msubs` 等）を追加しない。本 issue 完了時点の生成物は syntactic なラウンドトリップ担保に留まり、実プレイヤーでの字幕再生（DASH.js / Safari / VLC 等での認識）は 0043-0045 完了後の担当

### `Mp4FileMuxer` の Subtitle 拒否

- `MuxError` に新規バリアント `UnsupportedTrackKind { track_kind: TrackKind }` を追加する（`MuxError` は `#[non_exhaustive]` のため非破壊）。命名は `Unsupported*` パターンを採用（既存 `Missing*` / `Mixed*` パターンでは意味が取りづらいため）
- `Display` 実装の文言は既存パターン（`Mp4FileMuxer` などの実装型名を含めない、`{track_kind:?}` を接続する形）に揃え、`"Unsupported track kind: {track_kind:?}"` を採用する
- `src/mux_mp4_file.rs:561-590` および `src/mux_mp4_file.rs:623-626` の網羅 match に `TrackKind::Subtitle =>` arm を追加し、いずれも `return Err(MuxError::UnsupportedTrackKind { track_kind: TrackKind::Subtitle })` を返す
- C API 側の `Mp4Error` マッピング（`crates/c-api/src/error.rs:72-87` の `impl From<MuxError> for Mp4Error`）に `MuxError::UnsupportedTrackKind { .. } => Self::MP4_ERROR_UNSUPPORTED` の arm を追加する（意味論的に「操作またはデータ形式がサポートされていない」を表す既存の `MP4_ERROR_UNSUPPORTED` を採用。既存の `MP4_ERROR_INVALID_INPUT` は「入力値そのものの不正」に振られており、意味軸が異なる）。arm は末尾の `_ => Self::MP4_ERROR_OTHER` フォールバックより前に明示追加する（`MuxError` が `#[non_exhaustive]` のため、フォールバックに落ちてもコンパイルは通ってしまう）

### `Sample` 構造体

- `src/mux_mp4_file.rs:179` に定義された `Sample` の doc コメントに、字幕トラック時の推奨値（`keyframe: true`、`composition_time_offset: None`）と「本 issue の時点では `Fmp4SegmentMuxer` 経由のみで受理される（`Mp4FileMuxer` は `MuxError::UnsupportedTrackKind` を返す）」を明記する

### C API / WASM 露出

- `crates/c-api/src/basic_types.rs` の `Mp4TrackKind` に `MP4_TRACK_KIND_SUBTITLE = 2` を追加（`#[repr(C)]` のため整数値を明示的に割り当てる）
- `From<TrackKind>` / `From<Mp4TrackKind>` の双方向 match に Subtitle arm を追加
- `crates/wasm/src/mux.rs` / `fmp4_segment_mux.rs` の JSON パーサに `"subtitle"` 文字列を追加（エラーメッセージ本文も `"audio", "video", "subtitle"` に更新）
- `crates/wasm/src/demux.rs` / `fmp4_segment_demux.rs` の JSON 出力に `"subtitle"` 文字列を追加
- `crates/c-api/examples/demux.c` の `get_track_kind_name` にも Subtitle 対応 `case` を追加
- `HdlrBox::name` は既存 Audio / Video と同様に空文字列（`Utf8String::EMPTY.into_null_terminated_bytes()`）を書き出す
- C ヘッダ (`crates/c-api/include/mp4.h`) は cbindgen で自動生成される（`crates/c-api/build.rs`）。実装時に `cargo build` 後の再生成でヘッダが更新されることを確認する

## 完了条件

- `TrackKind::Subtitle` が追加され、既存の映像・音声トラックの demux / mux 挙動が変わらない（既存 PBT が pass）
- `SthdBox` / `NmhdBox` の decode / encode ラウンドトリップテスト（PBT）が pass する。追加先は `pbt/tests/prop_container_boxes.rs`（既存の `SmhdBox` / `VmhdBox` PBT の隣に追加）
- `MediaHeader` enum を通じた `MinfBox` の decode / encode ラウンドトリップテスト（PBT）が 4 バリアントすべてで pass する。追加先は `pbt/tests/prop_container_boxes.rs`（既存 `minf_box_audio_roundtrip` / `minf_box_video_roundtrip` の隣に `minf_box_subtitle_sthd_roundtrip` / `minf_box_subtitle_nmhd_roundtrip` を追加）
- `HdlrBox::HANDLER_TYPE_SUBT` / `HANDLER_TYPE_TEXT` の定数が追加される
- 字幕系 handler type を持つトラックが `Mp4FileDemuxer` / `Fmp4FileDemuxer` / `Fmp4SegmentDemuxer` の 3 経路すべてで skip されず取り出せる。テスト用の合成 MP4 は既存 `pbt/tests/prop_container_boxes.rs:150-158` の `minimal_minf_box_audio` および `pbt/tests/prop_container_boxes.rs:171-` の `minimal_trak_box_audio` を模して以下のヘルパーを追加する
  - `minimal_minf_box_subtitle`: `media_header: Some(MediaHeader::Sthd(SthdBox::default()))`、他は既存 audio 版に準拠
  - `minimal_trak_box_subtitle(track_id: u32, handler_type: [u8; 4], sample_entry_box_type: [u8; 4])`: 引数で `subt` / `text` の handler type、および stsd 内 `SampleEntry::Unknown(UnknownBox { box_type: BoxType::Normal(sample_entry_box_type), payload: vec![] })` の box_type を切り替えられる形。テストケースは対応表に従い `(subt, "stpp")` / `(text, "wvtt")` / `(text, "tx3g")` の 3 組で境界値テスト（`#[test]`）を書く
  - この 2 ヘルパで組み立てた `MoovBox` を encode → decode し、3 経路のデマルチプレクサすべてで `TrackKind::Subtitle` として取り出せることを確認する
- `Fmp4SegmentMuxer` で `TrackKind::Subtitle` の init segment と media segment が生成できる（`SampleEntry::Unknown` を渡す合成テスト、tkhd の `volume=0` / `width=0` / `height=0` を確認）
- `Mp4FileMuxer::append_sample` に `TrackKind::Subtitle` の Sample を渡すと `MuxError::UnsupportedTrackKind` が返る（単体テスト。既存 `test_missing_sample_entry_error` パターンを模して、`Sample { track_kind: TrackKind::Subtitle, sample_entry: None, ... }` を構築して `append_sample` 呼び出しで検証する）
- `MuxError::UnsupportedTrackKind { track_kind: TrackKind::Subtitle }` を直接構築した際の `Display` 出力に `"Subtitle"` が含まれる（単体テスト）
- `MuxError::UnsupportedTrackKind` が C API 側で `MP4_ERROR_UNSUPPORTED` にマッピングされ、`_ => MP4_ERROR_OTHER` フォールバックに落ちないこと（単体テスト）
- `Mp4TrackKind::MP4_TRACK_KIND_SUBTITLE` が追加され、C API のトラック取得 API から字幕トラック種別が判別できる
- WASM の JSON API で `"subtitle"` の入出力が可能
- `cargo clippy --all-targets --all-features` が通る
- 既存 PBT（`pbt/tests/prop_container_boxes.rs` / `prop_error_paths.rs` / `prop_mux_demux.rs` / `prop_fmp4_segment_mux_demux.rs`）の網羅 match および `smhd_or_vmhd_box` フィールドアクセス箇所が新型に沿って修正される
- `examples/transcode_wasm/src/mp4.rs:149-153` の `smhd_or_vmhd_box` 直接アクセス箇所が新型に沿って修正される

`Mp4FileMuxer` の Subtitle トラック **受け入れ** は本 issue の完了条件に含めない（別 issue で対応。本 issue では **拒否経路のみ** 実装する）。

## 解決方法

以下の順で実装する。相互依存で「単独では cargo build が通らない」手順があるため、指示された束は 1 コミット単位でまとめて実施する。

1. `MuxError` に `UnsupportedTrackKind` バリアントを追加、`Display` を実装、C API `Mp4Error` マッピングに `MP4_ERROR_UNSUPPORTED` の arm を追加
2. **同一コミット単位で実施**: `Mp4TrackKind` に `MP4_TRACK_KIND_SUBTITLE = 2` を追加、`TrackKind::Subtitle` を追加、コンパイルエラーになる網羅 match（「現状」セクションで列挙した箇所）を機械的に修正する。`TrackKind` と `Mp4TrackKind` の双方向 `From` impl は互いの新バリアントを参照するため、片方だけ追加すると必ずコンパイルエラーになる。`Mp4FileMuxer` の 2 箇所は手順 1 で追加した `MuxError::UnsupportedTrackKind` を返す arm を追加
3. `HdlrBox` に `HANDLER_TYPE_SUBT` / `HANDLER_TYPE_TEXT` を追加
4. `SthdBox` / `NmhdBox` を実装し、対応する PBT を追加
5. **同一コミット単位で実施**: `MediaHeader` enum の導入、`MinfBox` のフィールド型・`Encode` / `Decode` / `children` 刷新、`smhd_or_vmhd_box` 直接アクセス箇所（`src/mux_mp4_file.rs:912-916, 948-953`、`src/mux_fmp4_segment.rs:699-702`、`examples/transcode_wasm/src/mp4.rs:149-153`）の同時修正、`MinfBox::media_header` コメントの更新。`MinfBox` のフィールド名変更で lib+example の 4 箇所が同時にコンパイルエラーになるため、これらを分離できない（`cargo build --workspace` はこの手順で通る）。なお `pbt/tests/prop_container_boxes.rs:153, 290, 301, 314, 323, 500` と `pbt/tests/prop_error_paths.rs:964-1012, 1889` の PBT 側 9 箇所は本手順のコミットで `cargo test` が壊れる状態のまま残る。PBT の同時修正は 1 コミットに含めても良いし、手順 9 で修正する形にしても良い（本 issue 完成時点で `cargo test --workspace` が pass することを完了条件で担保する）
6. デマルチプレクサ 3 系統の handler type 分岐に Subtitle を追加
7. `Fmp4SegmentMuxer` に Subtitle トラック生成経路を通す（`same_track_kind` / `build_init_trak` / tkhd 属性の外側 match / doc 更新）
8. WASM の `"subtitle"` JSON 対応・エラーメッセージ更新、`crates/c-api/examples/demux.c` の switch 更新、`Sample` doc 更新
9. PBT・単体テスト・合成 MP4（handler_type=`subt` / `text` + `SampleEntry::Unknown` を持つトラックの生成と分解）でラウンドトリップを検証

## CHANGES.md

機能単位に以下 6 エントリで記載する（各エントリの担当者行 `- @ユーザー名` は実装時に補う）。エントリ間の実装上の関連（例: `MediaHeader` と `MinfBox` 刷新）は各エントリ配下の補足箇条書きで相互参照する。

- `[CHANGE]` `TrackKind` に `Subtitle` バリアントを追加する（C API `Mp4TrackKind` の `MP4_TRACK_KIND_SUBTITLE = 2` および WASM JSON API の `"subtitle"` 文字列対応を含む）
- `[CHANGE]` `MinfBox` の `smhd_or_vmhd_box: Option<Either<SmhdBox, VmhdBox>>` フィールドを `media_header: Option<MediaHeader>` に置き換える（フィールド名と型の同時変更。新規 `MediaHeader` enum は別エントリで追加）
- `[ADD]` ISO/IEC 14496-12 の `SthdBox` (`sthd`) と `NmhdBox` (`nmhd`) を追加する
- `[ADD]` `MediaHeader` enum を追加する（`Smhd` / `Vmhd` / `Sthd` / `Nmhd` の 4 バリアント。`MinfBox` の Media Header 保持構造刷新のために導入）
- `[ADD]` `HdlrBox` に `HANDLER_TYPE_SUBT` (`subt`) と `HANDLER_TYPE_TEXT` (`text`) の定数を追加する
- `[ADD]` `MuxError::UnsupportedTrackKind` を追加し、`Fmp4SegmentMuxer` で `TrackKind::Subtitle` トラックを受け入れる（`Mp4FileMuxer` は当該バリアントで拒否する）
- `[FIX]` `Fmp4SegmentMuxer::build_init_trak` で `TrackKind::Video` に非映像系 `SampleEntry`（`SampleEntry::Unknown` 等）が渡されたときに `TkhdBox::DEFAULT_AUDIO_VOLUME` が採用されていた不整合を修正し、`DEFAULT_VIDEO_VOLUME` を採用するようにする（visual 分岐の刷新の副次効果）

## 実装着手前の準備

本 issue のコード変更に着手する前に、以下 3 件を **手動で** 完了させる必要がある。さもないと実装者が古い 0045 の記述と 0042 の判断を突き合わせて混乱する。

- **`Mp4FileMuxer` の Subtitle 受け入れの別 issue を事前に起票する**（`/create-issue` を人手で呼び出す）。番号を確定させて 0043 / 0044 / 0045 の「依存関係」節から参照できるようにする。0043-0045 の完了条件「MP4 のラウンドトリップができる」は当該別 issue の完了も前提となる
- **0043 / 0044 / 0045 の依存関係セクションを追随修正する**（`/refresh-issue` を人手で呼び出す）。上記別 issue の番号を「依存関係」節に追記し、完了条件「MP4 のラウンドトリップができる」を fMP4 経由に絞るか、または当該別 issue の完了も前提とする旨を明示する
- **0045 (tx3g) の記述を追随修正する**（`/refresh-issue` を人手で呼び出す）。0045 側に「解決方法 4: 手順の gmhd 系の慣習も含めて 0042 で調整」との記述があるが、本 issue で「gmhd はスコープ外」と決めたため、0045 の該当行を「本 issue 0042 の対応表に従う」等に更新する。書き換え文面の具体は 0045 の refresh 時に判断する（本節では文面まで規定しない）
