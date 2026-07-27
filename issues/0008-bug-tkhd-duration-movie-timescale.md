# mux_mp4_file.rs の tkhd.duration が movie timescale 単位ではなく media timescale 単位で書かれている

- Priority: High
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-tkhd-duration-movie-timescale
- Polished: 2026-07-27

## 目的

`Mp4FileMuxer` が生成する MP4 の `tkhd.duration` が ISO/IEC 14496-12 の仕様に違反しており、音声と映像で timescale が異なる場合に、AVFoundation ベースの環境でトラックのサンプルが打ち切られて読めなくなる問題を修正する。

## 優先度根拠

`Mp4FileMuxer` の実出力で、AVFoundation がトラックのサンプルを読み出せなくなることを実測した（詳細は「実測による影響確認」）。

被害は timescale の組み合わせで決まり、最悪ケースはトラックの実質的な全消失である。映像 30 fps（timescale 30）と音声 48 kHz を約 10 秒ずつ mux した MP4 では、**映像 300 サンプルのうち 1 サンプルしか読めない**。映像 30 / 音声 48000 は `Sample::timescale` の doc（`src/mux_mp4_file.rs:203-204`）が並べて例示している組み合わせで、録画系で普通に現れる。

深刻なのは欠落がサイレントである点である。ffprobe / MediaInfo / mp4box はいずれも正しい尺を報告するため、生成側でファイルを検証しても異常を検出できない。Apple 系プラットフォームで再生して初めて発覚する。

検証は macOS の AVFoundation で行った。iOS / Safari / QuickTime Player は同一フレームワーク上にあるため同様と考えられるが、直接は確認していない。

## 現状

### コードの現状

`build_audio_trak_box` / `build_video_trak_box` は `tkhd.duration` に各トラックの media timescale 単位の合計をそのまま書き込んでいる。

```rust
// src/mux_mp4_file.rs:854-877 (build_audio_trak_box)
let total_duration = self
    .audio_chunks
    .iter()
    .flat_map(|c| c.samples.iter().map(|s| s.duration as u64))
    .sum::<u64>();
// ...
    duration: total_duration,
```

映像側（`src/mux_mp4_file.rs:887-933`）も同じ構造である。

一方 `calculate_total_duration` は 2 トラックの尺を比較して片方の timescale と duration を `mvhd` に採用する。

```rust
// src/mux_mp4_file.rs:1101-1123
fn calculate_total_duration(&self) -> (NonZeroU32, u64) {
    // ...
    let normalized_audio_duration =
        Duration::from_secs(audio_duration) / self.audio_track_timescale.get();
    let normalized_video_duration =
        Duration::from_secs(video_duration) / self.video_track_timescale.get();

    if normalized_audio_duration < normalized_video_duration {
        (self.video_track_timescale, video_duration)
    } else {
        (self.audio_track_timescale, audio_duration)
    }
}
```

採用されなかった側のトラックでは、`tkhd.duration` の単位と movie timescale が食い違う。

`calculate_total_duration` の挙動のうち、以下を前提とする。

- `Duration::from_secs(d) / timescale` は厳密に `floor(d * 10^9 / timescale)` ナノ秒になる。同じナノ秒バケットに落ちる 2 トラックは同値と判定されるため、真の尺の大小と判定結果が食い違うことがある
- 採用されるのは正規化値が大きい方であり、同値のときは音声側である（`<` による比較なので else 側に落ちる）

### 仕様

ISO/IEC 14496-12 8.3.2.3 (TrackHeaderBox semantics) では `duration` は Movie Header Box (`mvhd`) の timescale 単位で表すと定められており、edit list がない場合はサンプル duration の合計を movie timescale に換算した値になる。`mdhd.duration` は 8.4.2.3 で media timescale 単位と定められているため、現状の `mdhd` 側は正しい。

`Mp4FileMuxer` は `edts_box: None` 固定で出力する（`src/mux_mp4_file.rs:881, 929`）ため、edit list による補正も入らない。

本リポジトリに `refs/` は存在せず、上記の節番号と文面は一次資料で照合していない。実装時に原典で確認すること。

### 発生条件

音声と映像の timescale が異なると、採用されなかった側の `tkhd.duration` には media timescale 単位の値がそのまま書かれる。換算値と偶然一致する場合（duration 総和が 0 のとき、および切り上げ換算の結果が生値と同じになるとき）を除き、仕様違反の値になる。ずれの向きで被害が変わる。

- 非採用側の timescale が movie timescale より **小さい**: `tkhd.duration` が過小に解釈され、実尺の `media_timescale / movie_timescale` の割合まで打ち切られる。この割合が小さいほど被害が大きい
- 非採用側の timescale が movie timescale より **大きい**: `tkhd.duration` が過大に解釈される。サンプルの欠落は起きないが、AVFoundation が報告するトラックの尺は実尺より長くなる。過大量は timescale 比に等しく、2 倍程度とは限らない

`Fmp4SegmentMuxer` は `tkhd.duration = 0` 固定だが、fMP4 では尺がフラグメントの `trun` 側で決まるため初期化セグメントとしてこれが慣行であり、本 issue の変更対象外である。

### 実測による影響確認

`Mp4FileMuxer` の実出力で確認した。ffmpeg で素材を生成し、`Mp4FileDemuxer` で読んで `Mp4FileMuxer` で re-mux したものを対象とする。素材は H.264 映像と AAC-LC 音声である。

**ケース A: 映像 timescale 30 / 10 秒、音声 timescale 48000 / 10.021 秒**（最悪ケース）

`finalized.moov_box()` の内容は次のとおりで、音声側が movie timescale に採用され、映像の `tkhd.duration = 300` が movie timescale 48000 では 0.006 秒（実尺 10 秒の 0.06 パーセント）と解釈される。

```
mvhd.timescale = 48000, mvhd.duration = 481024
  trak id=1 handler=soun tkhd.duration=481024 mdhd.timescale=48000 mdhd.duration=481024
  trak id=2 handler=vide tkhd.duration=300    mdhd.timescale=30    mdhd.duration=300
```

**ケース B: 映像 timescale 90000 / 11 秒、音声 timescale 48000 / 10.021 秒**

映像側が採用され、音声の `tkhd.duration = 481024` が movie timescale 90000 では 5.345 秒（実尺 10.021 秒の 53 パーセント）と解釈される。

いずれも、壊れているトラックの `tkhd.duration` だけを正しい換算値に書き換えたもの（修正後相当）と比較した。`AVAssetReader` で最後まで読んだ結果は次のとおりである。

| ケース | 壊れているトラック | バグ版 | 修正後相当 |
| --- | --- | --- | --- |
| A | 映像（全 300 サンプル） | **1 サンプル** | 300 サンプル |
| B | 音声（全 470 サンプル） | 253 サンプル | 470 サンプル |

ケース A では映像トラックが事実上失われる。ケース B では音声 217 サンプルが読めない。いずれも表示上の誤りではなく、実データが取得できない。

AVFoundation が報告するトラックの `timeRange.duration` も `tkhd.duration` に追従する（ケース B の音声で、バグ版 5.301 秒に対し修正後相当は 9.977 秒。tkhd 由来の値より 0.044 秒短いのは AAC のエンコーダディレイ 2112 サンプル分である）。一方 ffprobe / MediaInfo / mp4box はいずれも両者で同じ正しい尺を報告し、差が出ない。

丸め方針の判断材料として、ケース A の構成で次の 2 つも測定した。

- 映像の `tkhd.duration` を 0 にした場合: 読めた映像サンプルは 0 個で、トラックが丸ごと失われる
- 映像の `tkhd.duration` を実尺の 2 倍に改変した場合: 全 300 サンプルが読め、最後のサンプルの終端も変化しない

### 既存テストで検出できない理由

本リポジトリの demuxer がトラックの尺として読むのは `mdia_box.mdhd_box.duration` であり `tkhd.duration` ではない（`src/demux_mp4_file.rs:523`）。そのため mux と demux を往復させる形のテストでは原理的に検出できない。ライブラリ内で `tkhd_box.duration` を読むコードは存在しない。

`pbt/tests/prop_mux_demux.rs:784` の `mux_demux_video_audio_with_advance_position_roundtrip` は音声・映像の timescale を独立にランダム生成しているが、検証が demuxer 経由のサンプル比較に限られ `moov_box()` を見ていないため検出できていない。

`src/mux_mp4_file.rs:1595` の `test_audio_and_video_tracks` はアサーションが `!finalized.moov_box_bytes.is_empty()` だけなので素通りしている（本 issue では変更しない）。

## 設計方針

`calculate_total_duration` が決めた movie timescale に合わせて、各トラックの `tkhd.duration` を `media_duration * movie_timescale / media_timescale` で換算する。`mvhd` 側と `mdhd` 側は変更しない。ずれの向きにかかわらず全トラックを同じ換算で扱う。

### 丸めは切り上げとする

切り捨ては採用しない。換算値が 1 未満になると 0 に潰れ、実測のとおり AVFoundation がトラックを丸ごと破棄するためである。`test_audio_and_video_tracks` の入力（映像 timescale 30 / duration 1、音声 timescale 1000 / duration 20）では movie timescale が 30 になり、音声は `20 * 30 / 1000 = 0.6` で切り捨てると 0 になる。

切り上げなら換算値は必ず真の尺以上になるため打ち切りが起きない。過大側にずれても実データが失われないことは上記の実測で確認済みである。

なお duration 総和が 0 のトラックは切り上げても `tkhd.duration = 0` のままになる。`Sample::duration` は 0 を受け付けるため公開 API から到達可能だが、修正前後で挙動が変わらないため本 issue の範囲外とする。

### mvhd との関係

換算値が `mvhd.duration` を超えることがある。`calculate_total_duration` がナノ秒粒度で比較するため、真の尺がわずかに長いトラックが同値と判定されて非採用になる場合があるためである。超過量の上限は `movie_timescale / 10^9 + 1` tick（整数除算。`u32::MAX` でも高々 5 tick）で、切り上げ固有の現象ではなく切り捨てでも起きる。

これは許容する。過大側のずれで実データが失われないことは実測済みであり、`mvhd` 側を変更するのは本 issue の範囲を超えるためである。

### overflow の扱い

換算は `u128` で行う。`u64 * u32` は最大でも 2 の 96 乗であり `u128` に収まるため、中間結果の overflow は原理的に起きない。切り上げには `u128::div_ceil` を使う。

最終結果が `u64` を超えるには採用側トラックの `Sample::duration`（`u32`）の総和が `u64::MAX` 近傍である必要があり、現実には到達しない。防御として `u64` へ収まらない場合は `MuxError::EncodeError(Error::invalid_data("track duration exceeds u64::MAX"))` を返す。`MuxError::Overflow` ではなく `EncodeError` を選ぶのは、`issues/closed/0001-bug-mux-mp4-file-data-size-truncation.md` が fMP4 側との一貫性を理由に `EncodeError` を採用した先例に従うためである。この経路のテストは書けないため完了条件には含めない。

### その他

音声・映像それぞれの `build_*_trak_box` / `build_*_mdia_box` と `calculate_total_duration` が同じサンプル duration 総和を独立に計算する構造は、本 issue の範囲外とする。

## 依存関係

`issues/0046-add-mp4-file-muxer-subtitle.md`（open、`Priority: Low`、`Polished: 2026-07-24`）が本 issue の変更対象と衝突する。

- 0046 は `build_audio_trak_box` / `build_video_trak_box` / `build_audio_mdia_box` / `build_video_mdia_box` を廃止して `build_trak_box` / `build_mdia_box` へ集約する
- 0046 は `audio_track_timescale` / `video_track_timescale` フィールドを削除し、`calculate_total_duration` を `self.tracks` 走査に書き換える
- 0046 は trak の出力順を「音声固定先頭」から `append_sample` 呼び出し順に変える

Priority の差（High と Low）から本 issue を先に実装する。0046 側の `build_trak_box` シグネチャには movie timescale の引数がないため、0046 を現行の文面どおりに実装すると本修正が失われる。0046 の refresh は本 issue の作業範囲外だが、本 issue が追加する PBT 不変条件が CI で回帰を検出する。

## 完了条件

`Mp4FileMuxer` の出力について、次を満たすこと。

- 全トラックで `tkhd.duration == ceil(mdhd.duration * mvhd.timescale / mdhd.timescale)` が成り立つこと
- `mdhd.duration` が入力サンプルの duration 総和と一致し、`mdhd.timescale` が入力 timescale と一致すること
- 上記 2 点を `pbt/tests/prop_mux_demux.rs` の `mux_demux_video_audio_with_advance_position_roundtrip` に不変条件として追加すること
- `mvhd_box` の各フィールドに入る値が修正前から変わらないこと（レビューで確認する）
- `CHANGES.md` にエントリを追加すること
- `cargo fmt --all --check` / `cargo test --workspace --exclude c-api` / `cargo test -p c-api --lib` / `cargo clippy --workspace --all-targets -- -D warnings` / `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm` が通ること（テストのコマンドは `Makefile` の `test` ターゲットと同形。CI と同じ `cargo test --workspace --exclude dump_wasm ...` は `libmp4.a` の事前ビルドを要求するため単体では失敗する）

## 解決方法

1. `build_moov_box` で `calculate_total_duration` を trak 構築より先に呼び、得られた movie timescale を `build_audio_trak_box` / `build_video_trak_box` に引数として渡す（`calculate_total_duration` は `trak_boxes` に依存しないため順序変更に副作用はない。ただし `mvhd_box` の構築ごと前に動かすと `next_track_id` が `trak_boxes.len()` を正しく反映しなくなるので、動かすのは `calculate_total_duration` の呼び出しだけにする）
2. media timescale 単位の尺を movie timescale 単位へ切り上げ換算するヘルパー関数を追加する。`&self` を必要としないので `build_ctts_box`（`src/mux_mp4_file.rs:1126`）と同じくモジュールレベルの自由関数として置く

```rust
fn convert_duration_to_movie_timescale(
    media_duration: u64,
    media_timescale: NonZeroU32,
    movie_timescale: NonZeroU32,
) -> Result<u64, MuxError>
```

   コードコメントには、根拠資料（`[ISO/IEC 14496-12] TrackHeaderBox class` の形式。節番号は原典で確認できた場合のみ添える）、`tkhd.duration` が movie timescale 単位であること、0 に潰れるとトラックが失われるため切り上げにしていること、仕様改訂で変わりうることを書く（issue 番号は書かない）
3. 両 `build_*_trak_box` で `tkhd.duration` にヘルパーの結果を入れる
4. `pbt/tests/prop_mux_demux.rs` の `mux_demux_video_audio_with_advance_position_roundtrip`（`pbt/tests/prop_mux_demux.rs:784`）に、完了条件の 1 番目と 2 番目を不変条件として追加する。`FinalizedBoxes::moov_box()` は公開 API（`src/mux_mp4_file.rs:171-174`）なのでバイト列のデコードは不要である。トラックの識別は `mdia_box.hdlr_box.handler_type` で行い、`HdlrBox` の import を追加する。`mdhd.duration` の期待値は同テストの `expected_video`（duration はタプルの 2 番目）と `expected_audio`（duration はタプルの 1 番目）の総和を使う。音声と映像でタプル内の位置が異なる点に注意すること

   `tkhd.duration <= mvhd.duration` と「非採用側では修正前の生値と異なること」は、いずれもこの生成範囲内に反例があるため不変条件にしないこと（前者は「mvhd との関係」を、後者は `ceil(d * M / m) == d` が `M / m` の一定範囲で成立することを参照）

   この不変条件は換算式のミラーなので `calculate_total_duration` の movie timescale 選択そのものは検証しないが、本バグの回帰検出には十分である（修正前のコードに対して 20 ケース × 60 回の試行がすべて失敗することを確認済み）。単体テストは追加しない（`shiguredo-rust` の「PBT でカバーできるものを単体テストで書かない」方針に従う）

## 後方互換

- 生成される MP4 のバイト列が変わる。異 timescale の音声・映像を mux している利用者の `tkhd.duration` が変化する。変化の倍率は timescale 比に等しく、映像 30 / 音声 48000 の構成では 1600 倍（300 が 480000）になる
- 公開 API のシグネチャは変わらない。`crates/c-api` / `crates/wasm` も `Mp4FileMuxer` を経由するだけなので、公開ヘッダに変更はなく出力バイト列の変化だけを受ける
- `tkhd` の box version は `creation_time` / `modification_time` / `duration` のいずれかが `u32::MAX` を超えると 1 になる（`src/boxes_moov_tree.rs:481-490`）。換算で version が変わると trak あたり 12 バイト増減するが、`reserved_moov_box_size` には ftyp 更新用の予備が上乗せされ、finalize 時のブランド追加（最大 16 バイト）を差し引いても 56 バイト以上の余白が残るため faststart には影響しない

## CHANGES.md

`## develop` にある既存 `[FIX]` 群の末尾（`### misc` の直前）に記載する（担当者行 `- @ユーザー名` は実装時に補う）。

- [FIX] `Mp4FileMuxer` が生成する `tkhd.duration` を movie timescale 単位に修正する
  - これまでは media timescale 単位の値をそのまま書いていたため、音声と映像で timescale が異なる場合に AVFoundation でトラックが打ち切られていた
  - @ユーザー名

正当な入力に対して出力バイト列が変わる修正だが、仕様違反の是正なので `[CHANGE]` ではなく `[FIX]` とする（`CHANGES.md` の「Mp4FileMuxer が使用した SampleEntry に応じて ftyp の compatible brands を更新する」と同じ扱い）。
