# MP4 ライブラリ WebAssembly API

MP4 ファイルの読み込み（デマルチプレックス）と書き込み（マルチプレックス）を行うための WebAssembly API です。

JavaScript / TypeScript から直接呼び出すことができます。

## アーキテクチャ

`crates/wasm` は `shiguredo_mp4` クレートを `wasm32-unknown-unknown` ターゲット向けにビルドし、[wasm-bindgen](https://github.com/wasm-bindgen/wasm-bindgen) で JavaScript / TypeScript 向けのバインディングを生成します。

- 生ポインタの受け渡しは行わず、文字列・バイト列・ハンドル型だけを境界に出す
- メモリの確保と解放は wasm-bindgen が管理する
- `.d.ts` は wasm-bindgen が生成するため、手書きの型定義は持たない

トラック情報・サンプル情報・サンプルエントリーは JSON 文字列として受け渡します。バイナリデータ（SPS / PPS / NALU 等）は数値配列として表現します。

## ビルド方法

```bash
# WebAssembly ターゲットをインストール（初回のみ）
rustup target add wasm32-unknown-unknown

# wasm-bindgen の CLI をインストール（初回のみ）
# バージョンは Cargo.lock に固定された wasm-bindgen クレートと完全に一致させる必要がある
cargo install wasm-bindgen-cli --version 0.2.130

# WebAssembly をビルド
cargo build -p wasm --target wasm32-unknown-unknown --profile release-wasm

# Node.js 向けのバインディングを生成
wasm-bindgen --target nodejs \
  --out-dir crates/wasm/pkg/node \
  target/wasm32-unknown-unknown/release-wasm/mp4_wasm.wasm

# ブラウザ向けのバインディングを生成
wasm-bindgen --target web \
  --out-dir crates/wasm/pkg/web \
  target/wasm32-unknown-unknown/release-wasm/mp4_wasm.wasm

# サイズを最適化する（生成した .wasm に対して行う）
wasm-opt -Oz --strip-debug --enable-bulk-memory \
  -o crates/wasm/pkg/node/mp4_wasm_bg.wasm \
  crates/wasm/pkg/node/mp4_wasm_bg.wasm
```

### release-wasm プロファイル

`release-wasm` プロファイルはルートの `Cargo.toml` に定義されており、以下の最適化が有効になっています:

- `lto = true`: リンク時最適化
- `codegen-units = 1`: 単一コード生成ユニット
- `opt-level = "z"`: サイズ最適化
- `panic = "abort"`: パニック時に即座に終了

`strip = true` は指定できません。wasm-bindgen が `target_features` カスタムセクションを読んで有効な wasm 機能を判定するため、strip するとその情報が失われます。デバッグ情報の削除はビルド後の `wasm-opt --strip-debug` が行います。

### サイズ

2026.6.0-canary.2 の時点で、`wasm-opt -Oz --strip-debug` 適用後の `.wasm` は約 514 KB、gzip 後は約 187 KB です。これに加えて Node.js 向けの `.js` が約 25 KB、`.d.ts` が約 8 KB になります。

## 使い方

### Node.js

```js
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { Mp4FileDemuxer } = require('./pkg/node/mp4_wasm.js');
```

### ブラウザ

```js
import init, { Mp4FileDemuxer } from './pkg/web/mp4_wasm.js';

// 同梱の .wasm を読み込んで初期化する
await init();
```

### デマルチプレックス

Sans I/O のため、ファイルの読み込みは呼び出し側が行います。`requiredInput()` が要求するデータを `handleInput()` に渡します。

```js
const demuxer = new Mp4FileDemuxer();
try {
  for (;;) {
    const requiredJson = demuxer.requiredInput();
    if (requiredJson === undefined) break;

    const { position, size } = JSON.parse(requiredJson);
    // size が null の場合はファイル末尾までのデータが必要であることを意味する
    const end = size === null ? fileBytes.length : position + size;
    demuxer.handleInput(position, fileBytes.subarray(position, end));
  }

  const tracks = JSON.parse(demuxer.tracks());
  for (;;) {
    const sampleJson = demuxer.nextSample();
    if (sampleJson === undefined) break;
    const sample = JSON.parse(sampleJson);
  }
} finally {
  // wasm 側のメモリを解放する
  demuxer.free();
}
```

### マルチプレックス

```js
const muxer = new Mp4FileMuxer(0);
try {
  // 1. 初期ボックスをファイル先頭に書き込む
  const initial = muxer.initialBoxes();
  // file は任意の書き込み先 (fs など)
  write(file, 0, initial);
  let offset = initial.length;

  // 2. サンプルデータを書き込み、その位置とサイズを伝える
  for (const sampleData of sampleDataList) {
    write(file, offset, sampleData);
    muxer.appendSample(JSON.stringify({
      track_kind: 'video',
      keyframe: true,
      timescale: 30,
      duration: 1,
      composition_time_offset: null,
      data_offset: offset,
      data_size: sampleData.length,
      sample_entry: sampleEntry,
    }));
    offset += sampleData.length;
  }

  // 3. 完了処理を行い、出力ボックスを書き込む
  muxer.finalize();
  for (;;) {
    const output = muxer.nextOutput();
    if (output === undefined) break;
    write(file, output.offset, output.data);
  }
} finally {
  muxer.free();
}
```

## API

### `Mp4FileDemuxer`

MP4 ファイルを段階的に読み込んでメディアサンプルを取り出します。

| メソッド | 説明 |
|---|---|
| `constructor()` | デマルチプレクサを作成する |
| `requiredInput(): string \| undefined` | 次に必要な入力データを JSON で返す。不要になったら `undefined` |
| `handleInput(position: number, data: Uint8Array)` | 入力データを供給する |
| `tracks(): string` | トラック情報を JSON の配列で返す |
| `nextSample(): string \| undefined` | 時系列順に次のサンプルを JSON で返す |
| `prevSample(): string \| undefined` | 時系列順に前のサンプルを JSON で返す |
| `seek(timestamp: number, timescale: number)` | `timestamp / timescale` 秒の位置へシークする |

### `Mp4FileMuxer`

MP4 ファイルを構築します。

| メソッド | 説明 |
|---|---|
| `constructor(reservedMoovBoxSize: number)` | マルチプレクサを作成する。0 を指定すると faststart は無効 |
| `initialBoxes(): Uint8Array` | ファイル先頭に書き込むべきバイト列を返す |
| `appendSample(sampleJson: string)` | サンプルデータを書き込んだ位置とサイズを伝える |
| `advancePosition(size: number)` | サンプル以外のデータを書き込んだ分だけ位置を進める |
| `finalize()` | マルチプレックス処理を完了する |
| `nextOutput(): Mp4FileMuxerOutput \| undefined` | 未取得の出力ボックスを 1 つ返す |
| `isFaststartEnabled(): boolean` | 確定したファイルで faststart が有効かどうかを返す |
| `moovBoxSize(): number` | 確定した moov ボックスのサイズを返す |

`Mp4FileMuxerOutput` は `offset: number` と `data: Uint8Array` を持ちます。

### `Fmp4SegmentMuxer`

fMP4 の初期化セグメントとメディアセグメントを生成します。

| メソッド | 説明 |
|---|---|
| `constructor(creationTimestampSecs: number)` | マルチプレクサを作成する |
| `createMediaSegmentMetadata(samplesJson: string, sampleData: Uint8Array): Uint8Array` | メディアセグメントを返す |
| `createMediaSegmentMetadataWithSidx(samplesJson: string, sampleData: Uint8Array): Uint8Array` | `sidx` 付きのメディアセグメントを返す |
| `initSegment(): Uint8Array` | 初期化セグメントを返す |
| `mfra(): Uint8Array` | ランダムアクセス用の `mfra` ボックスを返す |

`sampleData` はサンプルの出現順に連結したペイロードです。muxer が要求する並び順への並べ替えはこの関数が行うため、返り値をそのまま書き込めば完全なメディアセグメントになります。

### `Fmp4SegmentDemuxer`

fMP4 の初期化セグメントとメディアセグメントを読み取ります。

| メソッド | 説明 |
|---|---|
| `constructor()` | デマルチプレクサを作成する |
| `handleInitSegment(data: Uint8Array)` | 初期化セグメントを読み込む |
| `tracks(): string` | トラック情報を JSON の配列で返す |
| `handleMediaSegment(data: Uint8Array): string` | メディアセグメントを読み込み、サンプル情報を JSON の配列で返す |

### `Mp4FileKindDetector`

ファイル先頭部分から MP4 の種類を判定します。

| メソッド | 説明 |
|---|---|
| `constructor()` | 判定器を作成する |
| `requiredInput(): string \| undefined` | 次に必要な入力データを JSON で返す |
| `handleInput(position: number, data: Uint8Array)` | 入力データを供給する |
| `fileKind(): string \| undefined` | `"mp4"` か `"fragmented-mp4"` を返す |

### `version()`

ライブラリのバージョンを返します。ルートの `Cargo.toml` の `version` と同じ値になるため、npm パッケージのバージョンと一致しているかを利用側で検証できます。

### リソースの解放

wasm-bindgen が生成したクラスは `free()` で明示的に解放するか、`Symbol.dispose` に対応した構文（`using` 宣言）で自動解放します。

## JSON スキーマ

### 入力要求

`requiredInput()` が返す JSON です。

```json
{ "position": 0, "size": 32 }
```

`size` が `null` の場合はファイル末尾までのデータが必要であることを表します。

### トラック情報

`tracks()` が返す配列の要素です。

```json
{ "track_id": 1, "kind": "video", "duration": 512, "timescale": 12800 }
```

`kind` は `"audio"` / `"video"` / `"subtitle"` のいずれかです。

### サンプル情報

`nextSample()` / `prevSample()` / `handleMediaSegment()` が返す JSON です。

```json
{
  "track_id": 1,
  "sample_entry": { "kind": "avc1", "width": 640, "height": 480, "..." : "..." },
  "keyframe": true,
  "timestamp": 0,
  "duration": 512,
  "composition_time_offset": null,
  "data_offset": 48,
  "data_size": 796
}
```

- `sample_entry` は、前のサンプルから内容が変化しない場合には省略されます。省略された場合は直前のサンプルの値が引き継がれます
- `composition_time_offset` は `null` か整数です。`null` は `ctts` / `trun` にコンポジション時間オフセットが存在しないことを意味します
- `data_offset` は file demuxer ではファイル先頭からの絶対位置、segment demuxer では渡したメディアセグメント先頭からの相対位置です

### マルチプレックス用サンプル

`appendSample()` に渡す JSON です。

```json
{
  "track_kind": "video",
  "keyframe": true,
  "timescale": 90000,
  "duration": 3000,
  "composition_time_offset": null,
  "data_offset": 0,
  "data_size": 12345,
  "sample_entry": { "kind": "avc1", "width": 1920, "height": 1080, "..." : "..." }
}
```

- `sample_entry` は最初のサンプルでは必須です。以降は省略可能で、省略した場合は前のサンプルと同じ値が使われます
- `composition_time_offset` は省略するか `null` を指定すると、コンポジション時間オフセットなしとして扱われます

### fMP4 メディアセグメント用サンプル

`createMediaSegmentMetadata()` に渡す JSON 配列の要素です。

```json
[
  {
    "track_kind": "video",
    "timescale": 90000,
    "duration": 3000,
    "keyframe": true,
    "composition_time_offset": null,
    "data_size": 1024,
    "sample_entry": { "kind": "avc1", "width": 1920, "height": 1080, "..." : "..." }
  }
]
```

`data_size` の合計は `sampleData` の長さと一致している必要があります。

### サンプルエントリー

サンプルエントリーは `kind` フィールドでコーデックの種類を識別します。未対応のコーデックは `{ "kind": "unknown", "boxType": "xxxx" }` として出力されます。

#### AVC1 (H.264)

```json
{
  "kind": "avc1",
  "width": 1920,
  "height": 1080,
  "avcProfileIndication": 100,
  "profileCompatibility": 0,
  "avcLevelIndication": 40,
  "lengthSizeMinusOne": 3,
  "sps": [[103, 100, 0, 40]],
  "pps": [[104, 238, 60, 128]]
}
```

`chromaFormat` / `bitDepthLumaMinus8` / `bitDepthChromaMinus8` は、`avcC` に該当フィールドがある場合のみ出力されます。

#### HEV1 / HVC1 (H.265/HEVC)

```json
{
  "kind": "hev1",
  "width": 1920,
  "height": 1080,
  "generalProfileSpace": 0,
  "generalTierFlag": 0,
  "generalProfileIdc": 1,
  "generalProfileCompatibilityFlags": 1610612736,
  "generalConstraintIndicatorFlags": 144115188075855872,
  "generalLevelIdc": 120,
  "chromaFormatIdc": 1,
  "bitDepthLumaMinus8": 0,
  "bitDepthChromaMinus8": 0,
  "minSpatialSegmentationIdc": 0,
  "parallelismType": 0,
  "avgFrameRate": 0,
  "constantFrameRate": 0,
  "numTemporalLayers": 1,
  "temporalIdNested": 1,
  "lengthSizeMinusOne": 3,
  "naluArrays": [
    { "naluType": 32, "units": [[64, 1, 12, 1]] },
    { "naluType": 33, "units": [[66, 1, 1, 1]] },
    { "naluType": 34, "units": [[68, 1, 0]] }
  ]
}
```

`kind` が `hev1` か `hvc1` かだけが異なり、他のフィールドは共通です。

#### VP8

```json
{
  "kind": "vp08",
  "width": 1920,
  "height": 1080,
  "bitDepth": 8,
  "chromaSubsampling": 1,
  "videoFullRangeFlag": false,
  "colourPrimaries": 1,
  "transferCharacteristics": 1,
  "matrixCoefficients": 1
}
```

#### VP9

```json
{
  "kind": "vp09",
  "width": 1280,
  "height": 720,
  "profile": 0,
  "level": 31,
  "bitDepth": 8,
  "chromaSubsampling": 1,
  "videoFullRangeFlag": false,
  "colourPrimaries": 1,
  "transferCharacteristics": 1,
  "matrixCoefficients": 1
}
```

#### AV1

```json
{
  "kind": "av01",
  "width": 3840,
  "height": 2160,
  "seqProfile": 0,
  "seqLevelIdx0": 13,
  "seqTier0": 0,
  "highBitdepth": 0,
  "twelveBit": 0,
  "monochrome": 0,
  "chromaSubsamplingX": 1,
  "chromaSubsamplingY": 1,
  "chromaSamplePosition": 0,
  "configObus": [10, 11, 0, 0]
}
```

`initialPresentationDelayMinusOne` は、`av1C` に該当フィールドがある場合のみ出力されます。

#### Opus

```json
{
  "kind": "opus",
  "channelCount": 2,
  "sampleRate": 48000,
  "sampleSize": 16,
  "preSkip": 312,
  "inputSampleRate": 48000,
  "outputGain": 0
}
```

#### MP4A (AAC)

```json
{
  "kind": "mp4a",
  "channelCount": 2,
  "sampleRate": 44100,
  "sampleSize": 16,
  "bufferSizeDb": 0,
  "maxBitrate": 128000,
  "avgBitrate": 128000,
  "decSpecificInfo": [17, 144, 109, 0]
}
```

#### FLAC

```json
{
  "kind": "flac",
  "channelCount": 2,
  "sampleRate": 44100,
  "sampleSize": 16,
  "streaminfoData": [0, 16, 0, 16]
}
```

`streaminfoData` は `dfLa` の STREAMINFO メタデータブロックの内容です。

#### STPP (TTML 字幕)

```json
{
  "kind": "stpp",
  "namespace": "http://www.w3.org/ns/ttml",
  "schemaLocation": "https://example.com/ttml.xsd",
  "auxiliaryMimeTypes": ""
}
```

#### WVTT (WebVTT 字幕)

```json
{ "kind": "wvtt", "config": "WEBVTT\n\nSTYLE" }
```

#### TX3G (3GPP 字幕)

```json
{
  "kind": "tx3g",
  "display_flags": 0,
  "horizontal_justification": 0,
  "vertical_justification": 0,
  "background_color_rgba": [0, 0, 0, 255],
  "default_text_box": [0, 0, 240, 320],
  "default_style": {
    "start_char": 0,
    "end_char": 0,
    "font_id": 1,
    "face_style_flags": 0,
    "font_size": 12,
    "text_color_rgba": [255, 255, 255, 255]
  },
  "ftab": [{ "font_id": 1, "font_name": [65, 114, 105, 97, 108] }]
}
```

## Examples

- デマルチプレックスの例:
  - `node crates/wasm/examples/demux.js /path/to/input.mp4`
  - MP4 ファイルを読み込み、トラック情報とサンプルデータを抽出
- マルチプレックスの例:
  - `node crates/wasm/examples/mux.js /path/to/output.mp4`
  - Opus オーディオトラック（無音）を含む MP4 ファイルを作成
