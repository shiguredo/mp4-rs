# `Mp4FileMuxer` の video/audio/subtitle 真ランダム interleave PBT を追加する

- Created: 2026-08-19
- Completed: {YYYY-MM-DD}
- Branch: feature/add-mp4-file-muxer-interleave-pbt
- Polished: 2026-10-06

## 目的

`Mp4FileMuxer::append_sample` を video / audio / subtitle の真にランダムな順序で呼び出す PBT を追加し、track 種別を跨いだ状態相互依存や、append 順序に依存する moov 生成規則 (trak 順・track_id 割り当て・mvhd の timescale 選択) を検証する。

現行テストは track 種別ごとに固定順序 (video → audio → subtitle) で呼ぶパターンのみで、interleave 順序に依存する moov 生成規則の boundary (音声が映像より先に登場した場合など) を検出できない。

## 現状

- `pbt/tests/prop_mux_demux.rs::mux_demux_video_audio_with_advance_position_roundtrip`: `for i in 0..max_len` で「video[i] → audio[i]」の zip 順のみ
- `pbt/tests/prop_mux_demux.rs::mux_demux_video_audio_subtitle_roundtrip`: 「全 video → 全 audio → 全 subtitle」の連続ブロック順で追加
- 「video, video, audio, video, subtitle, audio, ...」のようなランダム interleave は未検証
- `Mp4FileMuxer` の `trak` 順は「先に登場した TrackKind が先」の規則で決まる (`0068` で移行済みの `mux_demux_video_audio_subtitle_roundtrip` のコメント参照)。ランダム interleave はこの規則の boundary を叩ける

## 設計方針

### 生成する操作列

- 操作列の長さ: 5-30 (`sample_with_boundaries` で境界値 5 / 30 と代表値 10 を優先)
- 各操作は `sample_weighted_index` で 4 択:
  - `AppendVideoSample { keyframe, duration, data_size, composition_time_offset, has_new_entry }`
  - `AppendAudioSample { duration, data_size }`
  - `AppendSubtitleSample { duration, data_size }`
  - `AdvancePosition(gap)`
- 重み付けは video : audio : subtitle : advance = 3 : 3 : 1 : 1 程度 (subtitle は現実的にレア、advance は数を絞る)
- `Runner::run` のケース数は既存の `CASES_MAIN` (20) に合わせる

### 各操作の生成規則

- duration / data_size は既存の `arb_video_sample_info` / `arb_audio_sample_info` / `arb_subtitle_sample_info` と同じ値域を使う (video 100..10000、audio 100..5000、subtitle 100..2000、duration 1..100)
- `composition_time_offset` は既存テストと同じ生成式 (半々で `None` / 0..=6000 から 3000 を引いた値) を使い、video のみに付与する
- `keyframe` は video のみランダム。**操作列内で最初の `AppendVideoSample` は必ず `keyframe = true` にする** (全サンプル false だと `finalize()` が `NoSyncSamples` を返すため。既存テストの「最初の映像サンプルは必ず keyframe」と同じ方針)
- audio / subtitle の `keyframe` は true 固定とする。false を混ぜると stss 省略時に demux 側で true に正規化され照合に例外処理が必要になるため固定にする (音声全 false による stss 省略契約の検証は既存テストが担う)
- 各 TrackKind の `timescale` はそのトラックの初回操作時に 1 回だけ生成し以降は固定する (既存テストと同じ値域。同一トラック内で一致しないと `TimescaleMismatch`)
- `sample_entry` は各 TrackKind の初回操作では必ず `Some` (video はランダム解像度の Avc1、audio は Opus、subtitle は stpp)
  - video は `has_new_entry` (初回は必ず true、以降は random) が false のとき `None` を渡し、直前チャンクの entry を引き継がせる。audio / subtitle は 2 回目以降は常に `None`
  - 初回に `None` を渡すと `MissingSampleEntry` になるため、この制約は必須
  - video は異なる解像度の entry を混在させてよい (既存挙動は最大幅・高さを採用するためエラーにならない)。subtitle は entry 種別固定なので混在しない (混在すると `MixedSampleEntries`)
- `AdvancePosition(gap)` の gap は 1..=256 程度 (0 は no-op で単一トラックの既存テストと重複するため対象外)
  - 後続の `append_sample` の `data_offset` は gap 加算後の位置とし、gap 分は `build_hybrid_file_data` の regions にも記録する

### 検証手順

1. 操作列を順に `append_sample` / `advance_position` し、各サンプルの `data_offset` / regions を累積で記録する
2. `finalize()` し、regions 付きの `build_hybrid_file_data` でファイルデータを構築 → `Mp4FileDemuxer` で demux
   - ギャップを含むため `build_file_data` (連続配置前提) では moov が切り詰められてしまう。既存の `build_hybrid_file_data` を使うこと
3. 全 sample の照合: `Mp4FileDemuxer::next_sample()` は全トラックのうち「正規化タイムスタンプ (timestamp / timescale) が最小のもの」を返すため、**出力順は append 順と一致しない** (同着は moov の trak 順)。そのためトラック種別ごとにグループ化して「そのトラックへ append した順」で照合する。照合フィールド: track_id / duration / data_size / keyframe / composition_time_offset / data_offset (出現していない TrackKind は対象外)。あわせてトラック内の `timestamp` が duration の累積と一致することも検証する。`composition_time_offset` の照合は既存テストと同じ正規化を使う (同一トラック内で 1 つでも Some があれば None は `Some(0)` に正規化し、全 None なら `None`。`ctts` は全サンプル分を書き出すため)
4. moov の trak 順が「先に登場した TrackKind が先」ルールに従っていることを検証 (期待順は操作列の最初の出現順から導出)
5. tkhd の track_id が 1 から順に振られていることを検証
6. mvhd の timescale が操作列から導出した期待値 (正規化した尺が最長のトラック、同着なら先に登場したトラックの timescale) と一致することを検証する。あわせて既存の `assert_moov_duration_invariants` で各トラックの mdhd / tkhd duration の整合を検証する

### coverage gate

`Cell<usize>` で以下 3 分岐が exercised されたことを事後検証 (いずれかが 0 件なら fail):

1. 3 track 種別すべてが操作列に登場したケース
2. サイズ > 0 の `AdvancePosition` を含む操作列
3. 音声トラックが映像トラックより先に登場したケース (現状の video-first 前提が boundary で破れる)

## 想定される検出対象

- track_id 割り当ての順序依存バグ
- mvhd の timescale 選択 (正規化尺最長トラック優先、同着は先着トラック優先) の boundary
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
