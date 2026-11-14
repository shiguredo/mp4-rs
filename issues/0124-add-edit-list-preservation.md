# demux した edts / elst を mux で保持する

- Created: 2026-11-14
- Completed: 2026-11-14
- Branch: feature/add-edit-list-preservation
- Polished: 2026-11-14

## 目的

MP4 の demux → mux の往復で `edts` / `elst`（編集リスト）が失われ、再生開始位置がずれるのを防ぐ。

AAC の priming は `elst` の `media_time` で先頭サンプルを飛ばして表現され、B フレームで生じる先頭 PTS オフセットは `ctts` に加えて `elst` の `media_time` で打ち消される。編集リストが失われると、音声が数 10 ms 手前から始まる、先頭に空白が入るなどのずれになる。

## 現状

### コア

- `src/boxes_moov_tree.rs` の `EdtsBox` / `ElstBox` / `ElstEntry` は parse / encode 済みで、`pbt/tests/prop_boxes.rs` に `elst_box_v0_roundtrip` / `elst_box_v1_roundtrip` / `edts_box_with_elst_roundtrip` がある
- demux の 3 経路はいずれも `TrakBox::edts_box` を参照しない
  - `src/demux_mp4_file.rs` の `Mp4FileDemuxer::read_moov_box`
  - `src/demux_fmp4_segment.rs` の `Fmp4SegmentDemuxer::handle_init_segment`
  - `src/demux_fmp4_file.rs` の `Fmp4FileDemuxer::read_moov_box`
- `TrackInfo`（`src/demux_mp4_file.rs`）は `track_id` / `kind` / `duration` / `timescale` のみを持つ
- mux の 2 経路は `edts_box: None` を固定で書く
  - `src/mux_mp4_file.rs` の `Mp4FileMuxer::build_trak_box`
  - `src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::build_init_trak`
- `mvhd` / `tkhd` の `duration` は、入力では編集リストの尺（`beep-aac-audio.mp4` は `mvhd` duration 1000、`tkhd` duration 1000）だが、現行 muxer はメディア尺（`mdhd` duration 45124）から導出する。編集リストを保持しても、この duration を直さないと出力の尺が入力と食い違う
- `src/mux_mp4_file.rs` の `total_sample_duration` の doc は「`tkhd` / `mdhd` / `mvhd` に入る尺はいずれもこの値から導出される」と述べており、編集リスト付きトラックではこの不変条件が変わる

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
- `crates/wasm/src/mux.rs` の `Mp4FileMuxer::new` は `reservedMoovBoxSize` しか受け取らず、`creationTimestampSecs` も編集リストも渡せない
- `crates/wasm/src/fmp4_segment_mux.rs` の `Fmp4SegmentMuxer::new` は `creationTimestampSecs` しか受け取らず、編集リストを渡せない
- 既存の wasm 結合テストは `edts` / `elst` を比較していないため、この損失を検出できない

## 設計方針

### 解釈はしない（保持のみ）

`Sample::timestamp` の意味は変えない。編集リストの値はそのまま保持し、タイムスタンプへの反映は行わない。タイムスタンプの解釈は `issues/pending/0070-add-edit-list-demux-support.md` の設計選択肢（A〜D）を前提に、方針確定後の実装用 issue で扱う（0070 自体は実装を持たない）。

### コア

- `TrackInfo` に次を追加する
  - `movie_timescale: NonZeroU32`: `moov` の `mvhd` の timescale
  - `edit_list: Option<ElstBox>`: `trak` の `edts_box.elst_box`
- 3 つの demuxer で上記を populate する
- `TrackMetadata` に `edit_list: Option<EditList>` を追加する
  - `EditList { movie_timescale: NonZeroU32, media_timescale: NonZeroU32, elst_box: ElstBox }`
  - `movie_timescale` は `edit_duration` の単位、`media_timescale` は `media_time` の単位を表す
  - `EditList` は `src/mux_mp4_file.rs` に定義し、`src/mux.rs` の `pub use` に追加して `shiguredo_mp4::mux::EditList` として公開する
- mux の `trak` 構築時の換算
  - `edit_duration` だけを、入力 movie timescale から出力 movie timescale へ四捨五入で換算する専用ヘルパを使う
    - `edit_duration` は編集尺そのものであり、`tkhd` / `mvhd` の duration 換算（`convert_duration_to_movie_timescale`、切り上げ）とは目的が異なるため、既存ヘルパは流用しない
  - 換算結果が `u64` を超える場合は `MuxError::EncodeError(Error::invalid_data(...))` を返す（既存の `convert_duration_to_movie_timescale` と同じ方針）
  - 出力トラックの `timescale` が `media_timescale` と一致しない場合は `MuxError::TimescaleMismatch` を返す（`media_time` の意味が変わるため）
  - `media_time` / `media_rate` / エントリー数はそのまま保持する
- `elst` の version は `ElstBox::full_box_version` が値域から再計算する
  - 入力が v1 でも値が 32 bit に収まれば出力は v0 になる
  - 換算で `edit_duration` が `u32::MAX` を超えると v0 → v1 になる
  - version のバイト一致は保証しない（値の一致を保証する）
- `mvhd` / `tkhd` の `duration`
  - 編集リストがあるトラックは、`edit_duration` の合計（換算後）から導出する
    - 合計は `u128` で計算し、`u64` を超える場合は `MuxError::EncodeError(Error::invalid_data(...))` を返す
  - 編集リストがないトラックは現行どおりメディア尺から導出する
  - `Mp4FileMuxer` の `mvhd` の timescale は現行どおり最長トラックの timescale を採用し、長さの比較は presentation の秒で行う
    - `calculate_total_duration` を `Result<(NonZeroU32, u64), MuxError>` に変更し、`build_moov_box` で `?` によりエラーを伝播する
    - 返す duration は、編集リスト付きトラックでは換算後の `edit_duration` の合計、編集リストなしでは現行のメディア尺とする
    - 比較に使う合計は `u128` で計算し、`u64` に収まることを checked に確認してから比較する
    - `total_sample_duration` の doc コメントにある「`tkhd` は `mdhd` の換算値」という不変条件を、編集リスト付きトラックを例外とする記述に更新する
    - `build_moov_box` の「`calculate_total_duration()` は `self.tracks` しか参照しない」というコメントを、`self.options` の編集リストも参照する旨に更新する
- `Fmp4SegmentMuxer` の init segment は、`0109` が扱う `mehd` の有無には立ち入らず、`mvhd` / `tkhd` の `duration` 0 を維持する（編集リストだけを書く。`mehd` は現行どおり出力する）
  - init segment の `mvhd.timescale` は現行の 1000 固定のままとする
  - `edit_duration` は 1000 へ換算する
- `src/boxes_moov_tree.rs` は変更しない（parse / encode は実装済み）

### wasm

- `tracks()` の各トラックに、既存キーと同じ snake_case で次を追加する
  - `movie_timescale: number`（常に出力）
  - `edit_list: { movie_timescale, media_timescale, entries: [{ segment_duration, media_time, media_rate }] }`（編集リストがある場合のみ）
  - 例: `{"track_id":1,"kind":"audio","duration":45124,"timescale":44100,"movie_timescale":1000,"edit_list":{"movie_timescale":1000,"media_timescale":44100,"entries":[{"segment_duration":1000,"media_time":1024,"media_rate":1.0}]}}`
- 編集リストを渡せる静的ファクトリを追加する（`new` は互換のため残す）
  - `Mp4FileMuxer.withOptions(optionsJson)`: `reservedMoovBoxSize`、`creationTimestampSecs`、`video` / `audio` / `subtitle` ごとの `edit_list`
  - `Fmp4SegmentMuxer.withOptions(optionsJson)`: `creationTimestampSecs`、`video` / `audio` / `subtitle` ごとの `edit_list`
  - 例: `{"reservedMoovBoxSize":0,"audio":{"edit_list":{"movie_timescale":1000,"media_timescale":44100,"entries":[{"segment_duration":1000,"media_time":1024,"media_rate":1.0}]}}}`
  - `edit_list` の `movie_timescale` / `media_timescale` は `tracks()` の値をそのまま渡す
  - `optionsJson` は `crate::json_limit::check` で深さと長さを検査する
  - 省略したキーは現行の既定値を使う
- `crates/wasm/README.md` の JSON スキーマとクラス一覧を更新する

### 変更対象

- `src/demux_mp4_file.rs` / `src/demux_fmp4_segment.rs` / `src/demux_fmp4_file.rs`
- `src/mux_mp4_file.rs`（`EditList` 型、`build_trak_box`、`calculate_total_duration`、`total_sample_duration` の doc、換算ヘルパ）/ `src/mux_fmp4_segment.rs`
- `src/mux.rs`（`EditList` の `pub use`、module doc の `TrackMetadata` 例）
- `crates/wasm/src/track.rs` / `crates/wasm/src/mux.rs` / `crates/wasm/src/fmp4_segment_mux.rs`
- `crates/wasm/tests/wasm_api.test.mjs`（編集リストの往復、`withOptions` の上限超過）
- `crates/wasm/README.md`
- `pbt/tests/helpers.rs` / `pbt/tests/prop_mux_demux.rs` / `pbt/tests/prop_fmp4_segment_mux_demux.rs`
  - `TrackMetadata` の struct literal を `..Default::default()` に更新する
  - `arb_track_metadata` に各トラックの media timescale を渡し、`movie_timescale` と `edit_duration` は現実的な範囲（`u32` に収まる範囲）で edit list を生成する
  - `assert_track_metadata(trak_box, expected, output_movie_timescale)` に変更し、mux 側と同じ四捨五入で換算した `edit_duration` を期待値として比較する
  - mux / demux の往復 PBT では、demux 後の `TrackInfo` について `movie_timescale` が `mvhd.timescale` と一致すること、`edit_list` が `trak.edts_box.elst_box` と一致することを検証する
- `skills/shiguredo-mp4/SKILL.md`（`TrackInfo` / `TrackMetadata` のフィールド一覧）
- `CHANGES.md`（`[CHANGE]`: `TrackInfo` / `TrackMetadata` のフィールド追加、mux が `edts` を書くことによる出力変化。`[ADD]`: `EditList` 型、wasm の `withOptions` と JSON フィールド）
- 関連 issue: 0070 の「現状」に本 issue の実測値を参考情報として追記する（pending 理由と完了条件は変更しない）。0071 の関連 issue に本 issue を追記する

## 完了条件

- `tests/testdata` の編集リスト付き 8 本が demux → mux で保持される
- `beep-aac-audio.mp4` の出力が次を満たす
  - `mvhd` timescale 44100、`mvhd` / `tkhd` duration 44100（1.0 秒）
  - `elst` が `segment_duration=44100`、`media_time=1024`、`media_rate=1.0`
- `black-h265-video.mp4` の出力が次を満たす
  - `mvhd` timescale 12800、`mvhd` / `tkhd` duration 12800（1.0 秒）
  - `elst` が `segment_duration=12800`、`media_time=1024`、`media_rate=1.0`
- 複数トラックのケースが次の 2 パターンで検証される
  - (a) 編集リスト付き音声 1.0 秒（`edit_duration=1000`、input movie timescale 1000、media timescale 44100）と編集リストなし映像 0.5 秒（timescale 12800、duration 6400）: 音声が最長。`mvhd` は timescale 44100 / duration 44100、音声 `tkhd` 44100、映像 `tkhd` 22050（`ceil(6400 * 44100 / 12800)`）
  - (b) 同じ音声と編集リストなし映像 2.0 秒（timescale 12800、duration 25600）: 映像が最長。`mvhd` は timescale 12800 / duration 25600、音声 `tkhd` 12800、音声 `elst` の `segment_duration` 12800
- 編集リスト付きトラックの `edit_duration` の合計が `u64::MAX` 近傍のとき、オーバーフローせずエラーになる
- `elst` の version が値域から再計算される（入力 v1・小さい値 → 出力 v0、換算で 32 bit 超 → 出力 v1）
- `media_timescale` と出力トラックの `timescale` が一致しない場合はエラーになる
- empty edit（`media_time = -1`）、複数エントリー、`media_rate != 1` が保持される
- wasm の `tracks()` の `edit_list` をそのまま `withOptions()` に渡す往復が成立する
  - `media_time` / `media_rate` / エントリー数は保持される
  - `edit_duration` は出力 `mvhd.timescale` に換算される（JSON の `movie_timescale` と `segment_duration` は出力側の値になる）
  - `beep-aac-audio.mp4` の場合、再取得した `tracks()` は `movie_timescale=44100`、`segment_duration=44100` になる
- `withOptions()` に深さ 128 超または 8 MiB 超の `optionsJson` を渡すと例外になり、wasm が停止しない
- fMP4 の init segment に `edts` が入り、`mvhd.timescale` は 1000、`mvhd` / `tkhd` の duration は 0 のまま（0109）
- `pbt/tests/helpers.rs` と doc 例が更新され、mux / demux の往復 PBT に編集リストが含まれる
- 往復 PBT で mux 出力の `trak.edts_box` と demux 後の `TrackInfo.edit_list` の双方が検証される
- `TrackMetadata` に `EditList` を構築する doctest が `shiguredo_mp4::mux::EditList` としてコンパイルできる
- `CHANGES.md`、`crates/wasm/README.md`、`skills/shiguredo-mp4/SKILL.md` が更新されている
- `cargo fmt --all -- --check`、`cargo clippy --workspace --exclude c-api --all-targets -- -D warnings`、`cargo test --workspace --exclude c-api`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude c-api --no-deps` が成功する
- CI の Node 24 の wasm テストと Miri が成功する

## 解決方法

- `TrackInfo` に `movie_timescale` と `edit_list` を追加し、`Mp4FileDemuxer` / `Fmp4SegmentDemuxer` / `Fmp4FileDemuxer` で populate する
- `EditList` 型を追加して `TrackMetadata.edit_list` で受け取り、`Mp4FileMuxer::build_trak_box` と `Fmp4SegmentMuxer::build_init_trak` で `edts` を書き出す
- `edit_duration` は `convert_edit_duration` で出力 movie timescale へ四捨五入で換算する
- 編集リスト付きトラックの `tkhd` の duration は `EditList::total_duration` から導出し、`mvhd.duration` は全トラックの `tkhd.duration` の最大値から導出する。movie timescale は presentation 秒で最長トラックを選ぶ
- wasm の `tracks()` に `movie_timescale` / `edit_list` を追加し、`Mp4FileMuxer.withOptions()` / `Fmp4SegmentMuxer.withOptions()` を追加する
- PBT の `arb_track_metadata` / `assert_track_metadata` を media timescale と出力 movie timescale に対応させ、demux 後の `TrackInfo` も検証する
- `tests/test_mux_mp4_file.rs` に換算・empty edit・version 再計算・桁溢れ・複数トラックのテスト、`wasm_api.test.mjs` に往復と JSON 上限のテストを追加する

## 関連 issue

- `issues/pending/0070-add-edit-list-demux-support.md`: 編集リストを demux のタイムスタンプに反映する場合の設計選択肢。0070 自身は実装を持たず、方針確定後に実装用の別 issue を切る
- `issues/0071-other-position-timing-boxes-inventory.md`: 位置・時刻調整系ボックスの棚卸し。本 issue が `edts` / `elst` の mux 生成の個別 issue に該当する
- `issues/0109-bug-fmp4-mehd-zero-duration.md`（open）: fMP4 の `mehd` を省略する方針。`mehd` の扱いは 0109 の担当であり、本 issue は `mvhd` / `tkhd` の duration 0 を維持するだけで、`mehd` の有無は変更しない

## 対象外

- 編集リストを解釈して `Sample::timestamp` を presentation timeline に変えること（0070 の選択肢確定後に別 issue で扱う）
- 編集リストをバイト単位で一致させること（`elst` の version は再計算する）
- `edts` 内の未知ボックス（`elst` 以外）
- `cslg` / `prft` など他の位置・時刻調整系ボックス（0071 の棚卸し対象）
- `crates/c-api`（0122 で削除予定であり、対象外）
