# MP4 ライブラリ WebAssembly API

MP4 ファイルの読み込み（デマルチプレックス）と書き込み（マルチプレックス）を行うための WebAssembly API です。

JavaScript/TypeScript から直接呼び出すことができます。

## アーキテクチャ

wasm crate は c-api crate を wasm32-unknown-unknown ターゲット向けにビルドし、wasm 固有の追加機能を提供します。

- **c-api**: デマルチプレクサ・マルチプレクサの本体（`mp4_demuxer_*`, `mp4_muxer_*` 関数群）
- **wasm**: wasm 固有の追加機能（メモリ管理、JSON シリアライズ等）

## ビルド方法

```bash
# WebAssembly ターゲットをインストール（初回のみ）
rustup target add wasm32-unknown-unknown

# ビルド
cargo build -p wasm --target wasm32-unknown-unknown --profile release-wasm

# 出力ファイル: target/wasm32-unknown-unknown/release-wasm/mp4_wasm.wasm
```

### release-wasm プロファイル

`release-wasm` プロファイルはルートの `Cargo.toml` に定義されており、以下の最適化が有効になっています:

- `lto = true`: リンク時最適化
- `codegen-units = 1`: 単一コード生成ユニット
- `opt-level = "z"`: サイズ最適化
- `panic = "abort"`: パニック時に即座に終了
- `strip = true`: シンボル除去

### wasm-opt による最適化

[wasm-opt](https://github.com/WebAssembly/binaryen) (Binaryen) を使用してさらにサイズを最適化できます。

```bash
wasm-opt -Oz --enable-bulk-memory -o mp4_wasm.wasm target/wasm32-unknown-unknown/release-wasm/mp4_wasm.wasm
```

`--enable-bulk-memory` は `release-wasm` プロファイルが bulk memory 命令を使用するため必要です。

## Examples

- デマルチプレックスの例:
  - `node crates/wasm/examples/demux.js /path/to/input.mp4`
  - MP4 ファイルを読み込み、トラック情報とサンプルデータを抽出
- マルチプレックスの例:
  - `node crates/wasm/examples/mux.js /path/to/output.mp4`
  - Opus オーディオトラック（無音）を含む MP4 ファイルを作成

## 提供する機能

### wasm crate が提供する関数

c-api には含まれない、wasm 固有の追加機能です。

#### メモリ管理

- `mp4_alloc`: メモリ確保
- `mp4_free`: メモリ解放
- `mp4_vec_ptr`: Vec のポインタ取得
- `mp4_vec_len`: Vec の長さ取得
- `mp4_vec_free`: Vec の解放

#### マルチプレックス関連

- `mp4_mux_sample_from_json`: `mp4_file_muxer_append_sample` 関数に渡すサンプル構造体を JSON から生成
- `mp4_mux_sample_free`: `mp4_mux_sample_from_json` で確保したサンプル構造体の解放

#### デマルチプレックス関連

- `mp4_demux_sample_to_json`: `mp4_file_demuxer_next_sample` の結果を JSON に変換
- `mp4_demux_track_info_to_json`: `mp4_file_demuxer_get_tracks` の結果を JSON に変換

### c-api が提供する関数

c-api の関数がそのまま利用可能です。詳細は `crates/c-api/README.md` を参照してください。

#### デマルチプレックス

- `mp4_demuxer_new`: デマルチプレクサ作成
- `mp4_demuxer_free`: デマルチプレクサ解放
- `mp4_demuxer_get_last_error`: エラーメッセージ取得
- `mp4_demuxer_get_required_input_position`: 必要な入力位置取得
- `mp4_demuxer_get_required_input_size`: 必要な入力サイズ取得
- `mp4_demuxer_handle_input`: 入力データ供給
- `mp4_demuxer_get_track_count`: トラック数取得
- `mp4_demuxer_get_track`: トラック情報取得
- `mp4_demuxer_next_sample`: 次のサンプル取得

#### マルチプレックス

- `mp4_estimate_maximum_moov_box_size`: moov ボックスサイズ見積もり
- `mp4_muxer_new`: マルチプレクサ作成
- `mp4_muxer_free`: マルチプレクサ解放
- `mp4_muxer_get_last_error`: エラーメッセージ取得
- `mp4_muxer_set_reserved_moov_box_size`: faststart 用 moov サイズ設定
- `mp4_muxer_initialize`: 初期化
- `mp4_muxer_has_output`: 出力データ有無確認
- `mp4_muxer_get_output_offset`: 出力オフセット取得
- `mp4_muxer_get_output_size`: 出力サイズ取得
- `mp4_muxer_get_output_ptr`: 出力ポインタ取得
- `mp4_muxer_advance_output`: 次の出力へ進む
- `mp4_muxer_append_sample`: サンプル追加
- `mp4_muxer_finalize`: 完了処理

## JSON 形式のサンプル

`mp4_mux_sample_from_json` 関数および `mp4_demux_sample_to_json` 関数を使用すると、サンプル構造体の情報を JSON 文字列として扱うことができます。

サンプルは以下のフィールドを持ちます:
- トラック情報（種別、ID など）
- サンプルエントリー（コーデック設定情報）
- キーフレームフラグ
- タイムスタンプと尺
- データの位置とサイズ

バイナリデータ（SPS/PPS/NALU 等）は数値配列として返されます。

### マルチプレックス用サンプルの例

```json
{
  "track_kind": "video",
  "keyframe": true,
  "timescale": 90000,
  "duration": 3000,
  "data_offset": 0,
  "data_size": 12345,
  "sample_entry": {
    "kind": "avc1",
    "width": 1920,
    "height": 1080,
    "avcProfileIndication": 100,
    "profileCompatibility": 0,
    "avcLevelIndication": 40,
    "lengthSizeMinusOne": 3,
    "sps": [[103, 100, 0, 31, ...]],
    "pps": [[104, 235, 227, 96]]
  }
}
```

### デマルチプレックス結果サンプルの例

```json
{
  "track_id": 1,
  "sample_entry": {
    "kind": "opus",
    "channelCount": 2,
    "sampleRate": 48000,
    "sampleSize": 16,
    "preSkip": 312,
    "inputSampleRate": 48000,
    "outputGain": 0
  },
  "keyframe": true,
  "timestamp": 0,
  "duration": 960,
  "data_offset": 1024,
  "data_size": 256
}
```

### サンプルエントリーの種類

サンプルエントリー（`sample_entry` フィールド）は `kind` フィールドでコーデックの種類を識別します。

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
  "sps": [[103, 100, 0, 31, ...]],
  "pps": [[104, 235, 227, 96]]
}
```

#### HEV1/HVC1 (H.265/HEVC)

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
    {"naluType": 32, "units": [[64, 1, ...]]},
    {"naluType": 33, "units": [[66, 1, ...]]},
    {"naluType": 34, "units": [[68, 1, ...]]}
  ]
}
```

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
  "sampleRate": 48000,
  "sampleSize": 16,
  "bufferSizeDb": 0,
  "maxBitrate": 128000,
  "avgBitrate": 128000,
  "decSpecificInfo": [17, 144, 109, ...]
}
```

