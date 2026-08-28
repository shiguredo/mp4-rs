# `Mp4FileMuxer` の video/audio/subtitle 真ランダム interleave PBT を追加する

- Created: 2026-08-19
- Completed: {YYYY-MM-DD}
- Branch: feature/add-mp4-file-muxer-interleave-pbt
- Polished: {YYYY-MM-DD}

## 目的

`Mp4FileMuxer::append_sample` を video / audio / subtitle の真にランダムな順序で呼び出す PBT を追加し、track 種別を跨いだ状態相互依存や moov 生成の順序依存を検証する。

現行テストは track 種別ごとに固定順序 (video → audio → subtitle) で呼ぶパターンのみで、interleave 順序に依存する moov 生成の非決定性を検出できない。

## 現状

- `pbt/tests/prop_mux_demux.rs::mux_demux_video_audio_with_advance_position_roundtrip`: `for i in 0..max_len` で「video[i] → audio[i]」の zip 順のみ
- `pbt/tests/prop_mux_demux.rs::mux_demux_video_audio_subtitle_roundtrip`: 「全 video → 全 audio → 全 subtitle」の連続ブロック順で追加
- 「video, video, audio, video, subtitle, audio, ...」のようなランダム interleave は未検証
- `Mp4FileMuxer` の `trak` 順は「先に登場した TrackKind が先」の規則で決まる (`0068` で移行済みの `mux_demux_video_audio_subtitle_roundtrip` のコメント参照)。ランダム interleave はこの規則の boundary を叩ける

## 設計方針

### 生成する操作列

- 操作列の長さ: 5-30 (境界値 5 / 10 / 30 を境界化)
- 各操作は `sample_weighted_index` で 4 択:
  - `AppendVideoSample { keyframe, duration, data_size, has_new_entry }`
  - `AppendAudioSample { duration, data_size }`
  - `AppendSubtitleSample { duration, data_size }`
  - `AdvancePosition(gap)`
- 重み付けは video : audio : subtitle : advance = 3 : 3 : 1 : 1 程度 (subtitle は現実的にレア、advance は数を絞る)

### 検証手順

1. 操作列を順に append/advance
2. `finalize()` → `Mp4FileDemuxer` で demux
3. 全 sample を append 順で照合 (track_id / duration / data_size / keyframe / composition_time_offset)
4. moov の trak 順が「先に登場した TrackKind が先」ルールに従っていることを検証
5. tkhd の track_id が 1 から順に振られていることを検証

### coverage gate

`Cell<usize>` で以下 3 分岐が exercised されたことを事後検証:

1. 3 track 種別すべてが操作列に登場したケース
2. `AdvancePosition` を含む操作列
3. 音声トラックが映像トラックより先に登場したケース (現状の video-first 前提が boundary で破れる)

## 想定される検出対象

- track_id 割り当ての順序依存バグ
- mvhd の timescale 選択 (先着トラック優先の logic) の boundary
- interleave された data_offset の連続性 (advance_position を挟むケース)

## 対象外

- `Fmp4SegmentMuxer` への同等テスト (別 issue)
- append_sample の順序でバグが見つかった場合の修正 (発見時に別 issue で切り出す)
- 3 tracks を超えるマルチトラック (現状の trak 順ルール検証には 3 種で十分)

## 完了条件

- `pbt/tests/prop_mux_demux.rs` にランダム interleave テストが追加されている
- coverage gate 3 分岐が exercised されていることが `Cell<usize>` の事後 assert で確認されている
- `cargo test -p pbt --test prop_mux_demux` が通る
- `MP4_RS_PBT_SEED` 環境変数で失敗ケースを再現できる
