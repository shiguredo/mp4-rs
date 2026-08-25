---
name: shiguredo-mp4
description: 時雨堂の Sans I/O MP4 ライブラリ shiguredo_mp4 の機能・API リファレンス。MP4 / fMP4 ファイルの mux/demux、ボックス群のエンコード・デコード、コーデック別サンプルエントリー、ISO/IEC 14496 系規格準拠に関する質問時に使用。
---

# shiguredo_mp4

Sans I/O 設計に基づく MP4 (ISO Base Media File Format) の mux/demux ライブラリ。

## 特徴

- **依存なし**: 外部依存ゼロ (`core` / `alloc` のみ)
- **`no_std` 対応**: `std` 非依存 (`alloc` クレートは必要)
- **Sans I/O**: I/O を完全に分離した設計。ファイル読み書きは利用側の責務
- **高レベル API**: `Mp4FileMuxer` / `Mp4FileDemuxer` / `Fmp4SegmentMuxer` / `Fmp4SegmentDemuxer` による mux/demux
- **ボックス単位の API**: 全ボックスが `Encode` / `Decode` を実装し、直接エンコード/デコード可能
- **C API / WebAssembly API**: `crates/c-api` / `crates/wasm` で提供

## バージョン情報

- crate 名: `shiguredo_mp4`
- バージョン: 2026.4.0
- Rust Edition: 2024
- 最小 Rust バージョン: 1.93
- ライセンス: Apache-2.0

## 基本概念

MP4 ファイルは `ftyp` + 各種ボックス（`moov` / `mdat` / `moof` 等）で構成される。
すべてのボックスは `BaseBox` トレイトを実装し、`Encode` / `Decode` トレイトでバイト列と相互変換できる。

| 型 | 説明 |
|----|------|
| `BaseBox` | 全ボックスの共通トレイト (`box_type()` / `children()`) |
| `BoxHeader` | ボックスヘッダー (`box_type` + `box_size`)。`decode_header_and_payload()` でヘッダーとペイロードに分解 |
| `BoxSize` | ボックスのサイズ表現。`U32(u32)` / `U64(u64)`。`VARIABLE_SIZE` (`U32(0)`) は「ファイル末尾まで」を意味する |
| `BoxType` | ボックス種別。`Normal([u8; 4])` / `Uuid([u8; 16])` |
| `FullBox` / `FullBoxHeader` | version + flags を持つボックス用トレイト/ヘッダー |
| `Mp4File<B>` | ファイル全体 (`ftyp_box` + `boxes: Vec<B>`) の表現。`RootBox` がデフォルト |
| `Either<A, B>` | `stco`/`co64` のように 2 種のボックスのどちらかを持つ場合に使う |
| `Uint<T, BITS, OFFSET>` | 任意ビット幅の整数 (`Uint<u8, 2, 6>` 等) |
| `FixedPointNumber<I, F>` | 固定小数点数 (16.16 等) |
| `Utf8String` | null 終端 UTF-8 文字列 (`new()` は null を含む文字列を拒否) |
| `LanguageCode` | 3 文字言語コード (`from_ascii("eng")` 等。`0x60..=0x7F` の範囲のみ) |
| `Mp4FileTime` | MP4 時間 (1904/1/1 起点)。`from_unix_time()` で変換 |
| `TrackKind` | `Audio` / `Video` / `Subtitle` |
| `SampleFlags` | fMP4 のサンプルフラグ (`trun` 等で使用)。`from_fields()` / `sample_is_non_sync_sample()` 等 |

### Encode / Decode トレイト

```rust
use shiguredo_mp4::{Decode, Encode};

// エンコード: buf に書き込み、書き込んだバイト数を返す
let n = box.encode(&mut buf)?;

// バイト列に変換
let bytes = box.encode_to_vec()?;

// デコード: (値, 消費バイト数) を返す
let (decoded, size) = BoxType::decode(buf)?;

// オフセットを自動で進めるデコード
let value = u32::decode_at(buf, &mut offset)?;
```

### エラー型

- `Error`: エンコード/デコード時のエラー。`kind: ErrorKind` (`InvalidInput` / `InvalidData` / `InsufficientBuffer` / `Unsupported`) と `reason` / `location` / `box_type` を持つ。`Display` は発生箇所 (`at src/xxx.rs:NNN`) 付きで表示される
- `DemuxError`: デマルチプレックス時のエラー。`DecodeError` / `SampleTableError` / `InvalidState` / `InputRequired(RequiredInput)` (Sans I/O の入力要求)
- `MuxError`: マルチプレックス時のエラー。`EncodeError` / `EmptyTracks` / `EmptySamples` / `PositionMismatch` / `MissingSampleEntry` / `AlreadyFinalized` / `TimescaleMismatch` / `MixedSampleEntries` / `NoSyncSamples` / `Overflow`

## デマルチプレックス (demux)

### `Mp4FileDemuxer` - MP4 ファイルのデマルチプレックス

通常の MP4 ファイルからトラック情報とサンプルを時系列順に取得する。

| メソッド | 説明 |
|---------|------|
| `new()` | インスタンス生成 |
| `required_input()` | 次に必要な入力範囲 `Option<RequiredInput>` を返す (初期化済みなら `None`) |
| `handle_input(Input)` | ファイルデータを供給する。`required_input()` が要求した範囲を包含するデータを一度に渡すこと。部分的なデータ消費は行わない |
| `tracks()` | `&[TrackInfo]` を返す。I/O が必要な場合は `InputRequired` を返す |
| `next_sample()` | 全トラックから最も早いタイムスタンプのサンプルを返す。無ければ `None` |
| `prev_sample()` | 現在位置より前で最も遅いタイムスタンプのサンプルを返す |
| `seek(Duration)` | 指定時刻にシーク。次回 `next_sample()` は指定時刻を含むサンプルから開始 |

`Input` は `{ position: u64, data: &[u8] }`、`RequiredInput` は `{ position: u64, size: Option<usize> }`。
`required_input()` で要求された範囲を供給し、`DemuxError::InputRequired` が返るまで繰り返す。
ストリーミング用途は想定していない（大きな `mdat` は要求されないが、`moov` は全体を渡す必要がある）。

```rust
use shiguredo_mp4::demux::{Input, Mp4FileDemuxer};

let mut demuxer = Mp4FileDemuxer::new();
while let Some(required) = demuxer.required_input() {
    // required.position と required.size に基づいてデータを読み込む
    let data: &[u8] = read_data(required.position, required.size);
    demuxer.handle_input(Input { position: required.position, data });
}
let tracks = demuxer.tracks()?;
while let Ok(Some(sample)) = demuxer.next_sample() {
    // sample.track, sample.timestamp, sample.data_offset, sample.data_size
}
```

### `Fmp4SegmentDemuxer` - fMP4 セグメントのデマルチプレックス

初期化セグメント (`ftyp` + `moov`) とメディアセグメント (`moof` + `mdat`) を個別に処理する。

| メソッド | 説明 |
|---------|------|
| `new()` | インスタンス生成 |
| `handle_init_segment(&[u8])` | 初期化セグメントを処理する (2 回目以降は `InvalidState`) |
| `tracks()` | 初期化済みトラック情報 `&[TrackInfo]` を返す |
| `handle_media_segment(&[u8])` | メディアセグメントを処理し `Vec<Sample>` を返す。先頭の `sidx` は自動スキップ。1 回の呼び出しで 1 つの `moof` + `mdat` ペアのみ |

`Sample.data_offset` は `handle_media_segment()` に渡したバッファ先頭からの相対位置。

```rust
use shiguredo_mp4::demux::Fmp4SegmentDemuxer;

let mut demuxer = Fmp4SegmentDemuxer::new();
demuxer.handle_init_segment(&init_segment)?;
let tracks = demuxer.tracks()?;
let samples = demuxer.handle_media_segment(&media_segment)?;
```

### `Fmp4FileDemuxer` - fMP4 ファイルのインクリメンタルデマルチプレックス

1 つのファイル内に並んだ複数セグメントを順番に処理する。`Mp4FileDemuxer` と同様に
`required_input()` / `handle_input()` の Sans I/O ループで使う。

| メソッド | 説明 |
|---------|------|
| `new()` | インスタンス生成 |
| `required_input()` | 次に必要な入力範囲を返す |
| `handle_input(Input)` | ファイルデータを供給する |
| `tracks()` | トラック情報を返す |
| `next_sample()` | 次のサンプルを返す (全トラック時系列順) |

制限: `tfhd` の `base_data_offset` にファイル先頭からの絶対オフセットを記録した形式には非対応。

### `Mp4FileKindDetector` - MP4 / fMP4 の種別判定

`moov` 内の `mvex` ボックスの有無で MP4 か fragmented MP4 かを incremental に判定する。

```rust
use shiguredo_mp4::demux::{Mp4FileKind, Mp4FileKindDetector};

let mut detector = Mp4FileKindDetector::new();
while let Some(required) = detector.required_input() {
    // データを読み込んで供給
    detector.handle_input(Input { position: required.position, data });
    if let Some(kind) = detector.file_kind()? {
        match kind {
            Mp4FileKind::Mp4 => { /* 通常 MP4 */ }
            Mp4FileKind::FragmentedMp4 => { /* fMP4 */ }
        }
        break;
    }
}
```

### demux の共通型

- `TrackInfo`: `{ track_id: u32, kind: TrackKind, duration: u64, timescale: NonZeroU32 }`
  - `duration` / `timestamp` はタイムスケール単位。秒は `timescale` で割る
  - fMP4 では `duration` は init segment 由来で 0 になることが多い
- `Sample<'a>`: `{ track: &'a TrackInfo, sample_entry: Option<&'a SampleEntry>, keyframe: bool, timestamp: u64, duration: u32, data_offset: u64, data_size: usize, composition_time_offset: Option<i64> }`
  - `sample_entry` は前のサンプルから変更がない場合 `None` (最初のサンプルは常に `Some`)
  - `timestamp` は DTS。PTS は `timestamp + composition_time_offset`
  - `data_offset` は file demuxer ではファイル先頭からの絶対位置、segment demuxer では入力バッファ先頭からの相対位置

## マルチプレックス (mux)

### `Mp4FileMuxer` - MP4 ファイルのマルチプレックス

複数トラックのサンプルを統合して MP4 ファイルを生成する。

基本的な使用フロー:

1. `new()` または `with_options(Mp4FileMuxerOptions)` でインスタンス生成
2. `initial_boxes_bytes()` のバイト列をファイルに書き込む (ftyp + 予約領域 + mdat ヘッダー)
3. サンプルデータをファイルに追記し、`append_sample(&Sample)` でメタデータを通知
4. `finalize()` で完了し、`FinalizedBoxes::offset_and_bytes_pairs()` の内容をファイルの該当オフセットに書き込む

| メソッド | 説明 |
|---------|------|
| `new()` / `with_options(options)` | インスタンス生成 |
| `initial_boxes_bytes()` | 初期ボックス群のバイト列 (ファイル先頭に書く) |
| `append_sample(&Sample)` | サンプルを追加。エラー時は内部状態が変わらない (再呼び出し可能) |
| `advance_position(u64)` | サンプルデータ以外のバイト列 (moof / mdat ヘッダ等) 分だけ書き込み位置を進める。OBS の Hybrid MP4 用 |
| `finalize()` | マルチプレックス完了。`&FinalizedBoxes` を返す |
| `finalized_boxes()` | ファイナライズ結果を後から取得 (`finalize()` 前は `None`) |

`Sample` のフィールド (`mux::Sample`):

| フィールド | 説明 |
|-----------|------|
| `track_kind: TrackKind` | トラック種別 |
| `sample_entry: Option<SampleEntry>` | コーデック情報。最初のサンプルでは必須、以降は省略可 (前のサンプルを引き継ぐ) |
| `keyframe: bool` | キーフレームか。`stss` ボックスの生成に使われる |
| `timescale: NonZeroU32` | タイムスケール。同一トラック内で統一必須 (不一致は `TimescaleMismatch`) |
| `duration: u32` | サンプルの尺 (タイムスケール単位) |
| `composition_time_offset: Option<i64>` | PTS - DTS。負値は `i32::MIN..=-1`、非負値は `0..=u32::MAX` の範囲のみ |
| `data_offset: u64` | ファイル内でのサンプルデータの開始位置。直前の `append_sample()` の位置と一致必須 (`PositionMismatch`) |
| `data_size: usize` | サンプルデータのサイズ |

注意点:

- MP4 はサンプルのタイムスタンプを直接指定できず、累積尺で表現する。タイムスタンプのギャップは利用側で補完する (映像: 黒画像や尺調整 / 音声: 無音補完)
- 映像トラックの全サンプルが `keyframe = false` だと `finalize()` が `NoSyncSamples` を返す
- `composition_time_offset` は `ctts` ボックスに書き出される
- 詳細な制御が必要な場合は muxer を使わずボックスを直接構築する

```rust
use shiguredo_mp4::mux::{Mp4FileMuxer, Sample};

let mut muxer = Mp4FileMuxer::new()?;
let initial_bytes = muxer.initial_boxes_bytes();
// file.write_all(initial_bytes)?;

let sample = Sample {
    track_kind: TrackKind::Video,
    sample_entry: Some(sample_entry),
    keyframe: true,
    timescale: NonZeroU32::MIN.saturating_add(30 - 1),
    duration: 1,
    composition_time_offset: None,
    data_offset: initial_bytes.len() as u64,
    data_size: 1024,
};
muxer.append_sample(&sample)?;

let finalized = muxer.finalize()?;
for (offset, bytes) in finalized.offset_and_bytes_pairs() {
    // file.seek(SeekFrom::Start(offset))?;
    // file.write_all(bytes)?;
}
```

#### faststart 対応

`Mp4FileMuxerOptions::reserved_moov_box_size` に予約サイズを指定すると、moov をファイル先頭付近に配置する
faststart 形式になる (`FinalizedBoxes::is_faststart_enabled()` で判定)。
`estimate_maximum_moov_box_size(&[sample_count_per_track])` で概算サイズを計算できる。
予約サイズより moov が大きい場合は moov はファイル末尾に配置され、faststart は無効になる。

`Mp4FileMuxerOptions` のフィールド:

- `reserved_moov_box_size: usize` - faststart 用の moov 予約サイズ (デフォルト 0 = 無効)
- `creation_timestamp: Duration` - 作成時刻 (デフォルト UNIX エポック)
- `audio_track` / `video_track` / `subtitle_track: TrackMetadata` - `mdhd.language` / `hdlr.name` のメタデータ

`TrackMetadata` は `{ language: LanguageCode, name: Utf8String }`。

### `Fmp4SegmentMuxer` - fMP4 セグメントのマルチプレックス

初期化セグメント (`ftyp` + `moov`) とメディアセグメント (`moof` + `mdat`) を生成する。

| メソッド | 説明 |
|---------|------|
| `new()` / `with_options(SegmentMuxerOptions)` | インスタンス生成 |
| `create_media_segment_metadata(&[Sample])` | メディアセグメントの先頭メタデータ (`moof` + `mdat` ヘッダー) を生成。`mdat` ペイロードは含まない。サンプルの track 情報と sample entry を内部に蓄積する |
| `create_media_segment_metadata_with_sidx(&[Sample])` | 先頭に `sidx` ボックスを付加したメディアセグメントを生成 (DASH 等向け) |
| `init_segment_bytes()` | その時点までに観測した内容を反映した初期化セグメントを返す。未観測なら `EmptyTracks` |
| `mfra_bytes()` | ランダムアクセスインデックス (`mfra`) のバイト列を生成。ファイル末尾に付加 |

注意点:

- `Sample.data_offset` は「ファイル全体の絶対位置」ではなく「今回のセグメントの `mdat` payload 領域の先頭からの相対位置」
- 同一セグメント内の同一トラックのサンプルは `data_offset` 昇順で連続配置する必要がある (1 track = 1 traf = 1 trun 前提)
- `composition_time_offset` は `trun` に書くため `i32::MIN..=i32::MAX` の範囲に限られる。負値と `> i32::MAX` の値の混在はエラー
- 映像トラックの全サンプルが `keyframe = false` でも `NoSyncSamples` にはならない (`trun` の `SampleFlags` と `sidx` の SAP 判定に使われる)
- `mfra_bytes()` は、実際にファイル先頭に配置する init segment を確定させた後で呼ぶこと (tfra の `moof_offset` は init segment のサイズ基準で計算される)

```rust
use shiguredo_mp4::mux::{Fmp4SegmentMuxer, Sample};

let mut muxer = Fmp4SegmentMuxer::new()?;
let segment = muxer.create_media_segment_metadata(&samples)?;
let init_segment = muxer.init_segment_bytes()?;
let mfra = muxer.mfra_bytes()?;
```

## ボックス群 (boxes)

`shiguredo_mp4::boxes` モジュールに全ボックスが定義されている。
各ボックスは `Encode` / `Decode` / `BaseBox` を実装し、フィールドがすべて `pub` なので直接構築・参照できる。

### トップレベルボックス

| ボックス | 説明 |
|---------|------|
| `RootBox` | トップレベルボックスの enum (`Free` / `Mdat` / `Moov` / `Moof` / `Mfra` / `Sidx` / `Unknown`) |
| `FtypBox` | ファイル種別 (`major_brand` / `minor_version` / `compatible_brands`) |
| `MdatBox` | メディアデータ本体 (`payload: Vec<u8>`) |
| `FreeBox` | 空き領域 (`payload: Vec<u8>`) |
| `UnknownBox` | 未知のボックスの受け皿 |
| `Brand` | ブランド定数 (`ISOM` / `ISO2` / `MP41` / `AVC1` / `HEV1` / `HVC1` / `AV01` 等) |

### moov ツリー (`boxes_moov_tree`)

| ボックス | 説明 |
|---------|------|
| `MoovBox` | movie ボックス (`mvhd_box` + `trak_boxes` + `mvex_box: Option` + `unknown_boxes`) |
| `MvhdBox` | movie ヘッダー。`DEFAULT_RATE` / `DEFAULT_VOLUME` / `DEFAULT_MATRIX` 定数あり |
| `TrakBox` | トラック (`tkhd_box` + `edts_box: Option` + `mdia_box`) |
| `TkhdBox` | トラックヘッダー (`track_id` / `duration` / `width` / `height` 等) |
| `EdtsBox` / `ElstBox` / `ElstEntry` | 編集リスト |
| `MdiaBox` | メディア情報 (`mdhd_box` + `hdlr_box` + `minf_box`) |
| `MdhdBox` | メディアヘッダー (`timescale` / `duration` / `language` / `creation_time`) |
| `HdlrBox` | ハンドラー (`HANDLER_TYPE_VIDE` / `HANDLER_TYPE_SOUN` / `HANDLER_TYPE_SUBT` / `HANDLER_TYPE_TEXT` 定数あり) |
| `MinfBox` | メディア情報ボックス (`media_header: Option<MediaHeader>` + `dinf_box` + `stbl_box`) |
| `MediaHeader` | enum (`Smhd` / `Vmhd` / `Sthd` / `Nmhd`) |
| `DinfBox` / `DrefBox` / `UrlBox` | データ参照 (`DinfBox::LOCAL_FILE` 定数あり) |
| `StblBox` | サンプルテーブル |
| `StsdBox` | サンプル説明 (`entries: Vec<SampleEntry>`) |
| `SttsBox` / `SttsEntry` | デコード時間テーブル (`from_sample_deltas()` で構築可能) |
| `CttsBox` / `CttsEntry` | コンポジション時間オフセット |
| `CslgBox` | コンポジション時間シフト |
| `SdtpBox` / `SdtpSampleFlags` | 独立/再利用可能サンプル |
| `StscBox` / `StscEntry` | サンプル-チャンクマッピング |
| `StszBox` | サンプルサイズ。`Fixed { sample_size }` / `Variable { entry_sizes }` の enum |
| `StcoBox` / `Co64Box` | チャンクオフセット (32bit / 64bit)。`Either::A` / `Either::B` で使い分ける |
| `StssBox` | 同期サンプル (不在 = 全サンプル同期) |
| `MvexBox` / `MehdBox` / `TrexBox` | ムービー拡張 (fMP4 用) |
| `EsdsBox` | ES 記述子 (AAC 用) |

### fMP4 ボックス (`boxes_fmp4`)

| ボックス | 説明 |
|---------|------|
| `MoofBox` | ムービーフラグメント (`mfhd_box` + `traf_boxes`) |
| `MfhdBox` | フラグメントヘッダー (`sequence_number`) |
| `TrafBox` | トラックフラグメント (`tfhd_box` + `tfdt_box: Option` + `trun_boxes`) |
| `TfhdBox` | トラックフラグメントヘッダー。`FLAG_BASE_DATA_OFFSET_PRESENT` 等のフラグ定数あり。`default_base_is_moof` / `base_data_offset` / `default_sample_*` フィールド |
| `TfdtBox` | フラグメントデコード時間 (`base_media_decode_time`) |
| `TrunBox` | トラックフラグメントラン (`data_offset: Option<i32>` + `samples: Vec<TrunSample>`) |
| `TrunSample` | `duration` / `size` / `flags` / `composition_time_offset` (すべて `Option`) |
| `SidxBox` / `SidxReference` | セグメントインデックス (DASH 用) |
| `MfraBox` / `TfraBox` / `TfraEntry` / `MfroBox` | ムービーフラグメントランダムアクセス |

`TrunBox` のフィールドに `Option` を渡すと、対応するフラグが立っていない場合はフィールドが省略される。

### サンプルエントリー (`boxes_sample_entry`)

`SampleEntry` は以下の enum:

| variant | ボックス種別 | コーデック |
|---------|-------------|-----------|
| `Avc1(Avc1Box)` | `avc1` | H.264 / AVC |
| `Hev1(Hev1Box)` | `hev1` | H.265 / HEVC (パラメータセット in-band) |
| `Hvc1(Hvc1Box)` | `hvc1` | H.265 / HEVC (パラメータセット out-of-band) |
| `Vp08(Vp08Box)` | `vp08` | VP8 |
| `Vp09(Vp09Box)` | `vp09` | VP9 |
| `Av01(Av01Box)` | `av01` | AV1 |
| `Opus(OpusBox)` | `Opus` | Opus |
| `Mp4a(Mp4aBox)` | `mp4a` | AAC |
| `Flac(FlacBox)` | `fLaC` | FLAC |
| `Stpp(StppBox)` | `stpp` | XML 字幕 (TTML / IMSC) |
| `Wvtt(WvttBox)` | `wvtt` | WebVTT 字幕 |
| `Tx3g(Tx3gBox)` | `tx3g` | 3GPP タイムドテキスト |
| `Unknown(UnknownBox)` | - | 未知のサンプルエントリー |

`SampleEntry` のヘルパー:

- `audio_channel_count()` / `audio_sample_rate()` / `audio_sample_size()` - 音声のチャンネル数 / サンプリングレート / ビット深度
- `video_resolution()` - `(幅, 高さ)` を返す

コーデック設定ボックス (`avcC` / `hvcC` / `vpcC` / `av1C` / `dOps` / `dfLa` 等) も同モジュールで提供される。
`Avc1Box` 等の映像系は `VisualSampleEntryFields` (幅・高さ・解像度等)、音声系は `AudioSampleEntryFields` を持つ。

## 補助モジュール

### aux (サンプルテーブルアクセサ)

`StblBox` をラップしてサンプル情報を簡単に取り出すための構造体。

- `SampleTableAccessor<T>`: `new(stbl_box)` / `sample_count()` / `chunk_count()` / `get_sample(NonZeroU32)` / `get_sample_by_timestamp(u64)` / `get_chunk(NonZeroU32)` / `samples()` / `chunks()` / `stbl_box()`
- `SampleAccessor<'a, T>`: `index()` / `duration()` / `timestamp()` / `data_size()` / `data_offset()` / `is_sync_sample()` / `sync_sample()` / `composition_time_offset()` / `chunk()`
- `ChunkAccessor<'a, T>`: `index()` / `offset()` / `sample_entry()` / `sample_entry_index()` / `sample_count()` / `samples()`

`SampleTableAccessor::new()` は stts / stsz / stsc / stco(または co64) / stss / ctts の整合性を検証し、
不整合 (`InconsistentSampleCount` / `FirstChunkIndexIsNotOne` / `ChunkIndicesNotMonotonicallyIncreasing` 等) を
`SampleTableAccessorError` で報告する。

```rust
use shiguredo_mp4::aux::{SampleTableAccessor, SampleAccessor};

let table = SampleTableAccessor::new(&stbl_box)?;
for sample in table.samples() {
    println!(
        "sample {}: ts={}, duration={}, offset={}, size={}",
        sample.index(),
        sample.timestamp(),
        sample.duration(),
        sample.data_offset(),
        sample.data_size(),
    );
}
```

### descriptors (ES 記述子)

`EsDescriptor` / `DecoderConfigDescriptor` / `DecoderSpecificInfo` / `SlConfigDescriptor` を提供。
`OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3` (0x40) 等の定数あり。

### docs

- `docs::hybrid_mp4` - OBS の Hybrid MP4 の取り扱い
- `docs::subtitle` - 字幕トラックの取り扱い

## 対応コーデック

- 音声: AAC (`mp4a`) / Opus (`Opus`) / FLAC (`fLaC`)
- 映像: VP8 (`vp08`) / VP9 (`vp09`) / AV1 (`av01`) / H.264 (`avc1`) / H.265 (`hev1`, `hvc1`)
- 字幕: `stpp` / `wvtt` / `tx3g`

## 規格書

- ISO/IEC 14496-1 (MPEG-4 システム)
- ISO/IEC 14496-12 (ISO Base Media File Format)
- ISO/IEC 14496-14 (MP4 ファイル形式)
- ISO/IEC 14496-15 (AVC / HEVC のファイル形式)
- ISO/IEC 14496-30 (Timed Text 系)
- 3GPP TS 26.245 (タイムドテキスト)
- VP Codec ISO Media File Format Binding
- RFC 6386: VP8 Data Format and Decoding Guide
- VP9 Bitstream and Decoding Process Specification
- AV1 Codec ISO Media File Format Binding
- AV1 Bitstream & Decoding Process Specification
- Encapsulation of Opus in ISO Base Media File Format
- Encapsulation of FLAC in ISO Base Media File Format
