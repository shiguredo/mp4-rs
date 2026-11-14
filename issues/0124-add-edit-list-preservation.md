# demux した edts / elst を mux で保持する

- Created: 2026-11-14
- Completed: {YYYY-MM-DD}
- Branch: feature/add-edit-list-preservation
- Polished: {YYYY-MM-DD}

## 目的

MP4 の demux → mux の往復で `edts` / `elst`（編集リスト）が失われ、再生開始位置がずれるのを防ぐ。

AAC の priming や B フレームの PTS オフセットは編集リストで表現されており、失われると音声が数 10 ms 手前から始まる、先頭に空白が入るなどの A/V 同期ずれになる。

## 現状

### コア

- `src/boxes_moov_tree.rs` の `EdtsBox` / `ElstBox` / `ElstEntry` は parse / encode 済みで、`pbt/tests/prop_boxes.rs` にラウンドトリップ PBT もある
- demux の 3 経路はいずれも `TrakBox::edts_box` を参照しない
  - `src/demux_mp4_file.rs` の `Mp4FileDemuxer::read_moov_box`
  - `src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_init_segment`
  - `src/demux_fmp4_file.rs` の `Fmp4FileDemuxer::read_moov_box`
- `TrackInfo`（`src/demux_mp4_file.rs`）は `track_id` / `kind` / `duration` / `timescale` のみを持つ
- mux の 2 経路は `edts_box: None` を固定で書く
  - `src/mux_mp4_file.rs` の `Mp4FileMuxer::build_trak_box`
  - `src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::build_init_trak`

### 実データ

`tests/testdata/*.mp4` の 9 本中 8 本に `elst` がある（バイト列を直接パースして確認）。`mvhd` はすべて 1000。

| ファイル | mdhd | elst の内容 | 意味 |
|---|---:|---|---|
| `beep-aac-audio.mp4` | 44100 | segment=1000, media_time=1024 | AAC priming 1024 サンプル = 23.2 ms |
| `black-h265-video.mp4` | 12800 | segment=1000, media_time=1024 | 2 フレーム = 80.0 ms |
| `black-h265-hvc1-video.mp4` | 12800 | segment=1000, media_time=1024 | 80.0 ms |
| `black-h264-video.mp4` | 12800 | segment=40, media_time=0 | 実質 no-op |
| `beep-opus-audio.mp4` | 48000 | segment=8248, media_time=0 | no-op |
| `beep-flac-audio.mp4` | 44100 | segment=1000, media_time=0 | no-op |
| `black-av1-video.mp4` | 12800 | segment=1000, media_time=0 | no-op |
| `black-vp9-video.mp4` | 12800 | segment=1000, media_time=0 | no-op |
| `black-h264-fmp4.mp4` | 12800 | なし | fMP4 |

`beep-aac-audio.mp4` と `black-h265-video.mp4` の `elst` は同一バイト列である。

```
size=28 "elst" v0 count=1 segment_duration=1000 media_time=1024 media_rate=1.0
```

### 再現

現行 API で `beep-aac-audio.mp4` と `black-h265-video.mp4` を demux → mux すると、出力から `edts` / `elst` が消える。あわせて出力の `mvhd` の timescale が入力と変わる（muxer は最長トラックの timescale を `mvhd` に使う）。

| ファイル | 入力 | 出力 |
|---|---|---|
| `beep-aac-audio.mp4` | `edts` 1 / `elst` 1 / mvhd 1000 | `edts` 0 / `elst` 0 / mvhd 44100 |
| `black-h265-video.mp4` | `edts` 1 / `elst` 1 / mvhd 1000 | `edts` 0 / `elst` 0 / mvhd 12800 |

### 単純コピーが使えない理由

`elst` の `edit_duration` は movie timescale 単位、`media_time` は media timescale 単位である（`ElstEntry` の doc コメントにも明記されている）。出力の movie timescale は入力と異なるため、生の値をそのままコピーすると次のように壊れる。

| 入力の値 | 出力 mvhd=44100 での解釈 | 出力 mvhd=12800 での解釈 | 正しい解釈 |
|---|---|---|---|
| 1000 (1.0 秒) | 22.7 ms | 78.1 ms | 1000 ms |

### wasm

- `crates/wasm/src/track.rs` の `fmt_track_info` は `track_id` / `kind` / `duration` / `timescale` のみを出力する
- `crates/wasm/src/mux.rs` の `Mp4FileMuxer::new` は `reservedMoovBoxSize` しか受け取らず、編集リストを渡す口がない
- 結合テストは JSON の `deepEqual` のみで、JSON に `elst` が無いため損失を検出できない

## 設計方針

### 解釈はしない（保持のみ）

`Sample::timestamp` の意味は変えない。編集リストの値はそのまま保持し、タイムスタンプへの反映は行わない。タイムスタンプの解釈は `issues/pending/0070-add-edit-list-demux-support.md` の設計選択肢（A〜D）が決まってから別 issue で扱う。

### コア

- `TrackInfo` に次を追加する
  - `movie_timescale: NonZeroU32`: `moov` の `mvhd` の timescale
  - `edit_list: Option<ElstBox>`: `trak` の `edts_box.elst_box`
- 3 つの demuxer で上記を populate する
- `TrackMetadata` に `edit_list: Option<EditList>` を追加する
  - `EditList { movie_timescale: NonZeroU32, elst_box: ElstBox }`
  - `movie_timescale` は入力側の `edit_duration` の単位を表す
- mux の `trak` 構築時に `edit_duration` だけを出力側の movie timescale に四捨五入で換算する
  - `media_time` / `media_rate` / エントリー数 / `elst` の version はそのまま保持する
  - ファイル muxer は動的な movie timescale、fMP4 セグメント muxer は固定 1000

### wasm

- `tracks()` の各トラックに `movieTimescale` を、`edit_list` がある場合は `editList`（`segmentDuration` / `mediaTime` / `mediaRate`）を追加する
- 編集リストを渡せるように `Mp4FileMuxer.withOptions(optionsJson)` と `Fmp4SegmentMuxer.withOptions(optionsJson)` を追加する
  - `reservedMoovBoxSize` / `creationTimestampSecs` と、`video` / `audio` / `subtitle` ごとの `editList` を受け取る
  - 既存の `new(...)` は互換のため残す

## 完了条件

- `tests/testdata` の `elst` が demux → mux で保持される（出力の `elst` が入力と意味的に一致する）
- 出力の `mvhd` に合わせて `edit_duration` が換算される（1000 → 44100 / 12800）
- empty edit（`media_time = -1`）、複数エントリー、`media_rate != 1` が保持される
- wasm の `tracks()` → `withOptions()` で往復できる
- fMP4 の init segment にも編集リストを書ける
- 既存のテスト（コア・wasm・Node 24 / 26・Miri）がすべて通る

## 解決方法

{実装後に記載する}

## 関連 issue

- `issues/pending/0070-add-edit-list-demux-support.md`: 編集リストを demux のタイムスタンプに反映する場合の設計選択肢
- `issues/0071-other-position-timing-boxes-inventory.md`: 位置・時刻調整系ボックスの棚卸し

## 対象外

- 編集リストを解釈して `Sample::timestamp` を presentation timeline に変えること（0070 で扱う）
- `edts` 内の未知ボックス（`elst` 以外）
- `cslg` / `prft` など他の位置・時刻調整系ボックス（0071 の棚卸し対象）
