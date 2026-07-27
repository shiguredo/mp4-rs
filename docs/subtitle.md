# 字幕トラックの取り扱い

MP4 コンテナ内に格納される字幕トラック（`TrackKind::Subtitle`）について、本 crate が対応する 3 形式（stpp / wvtt / tx3g）の概要と、mux / demux で扱う際の注意事項をまとめる。

## 字幕トラックとは

MP4 における字幕トラックは、音声・映像と並ぶ独立したメディアトラックの一種で、時刻軸に沿って字幕データを配信するためのもの。ソフトサブ（映像に焼き込まず、プレイヤー側で描画する字幕）を配信する標準的な手段として用いられる。

音声・映像トラックと比較して以下のような特徴がある。

- サンプル 1 個 = 字幕 1 表示（cue）と考えて扱うことが多い
- 各サンプルは前後に依存しない独立サンプルとして扱うのが通例
- コンポジション時間オフセット（B フレーム相当）も通常使わない

## 対応する 3 形式

字幕には歴史的経緯から複数のサブタイプ（サンプルエントリー）が存在する。本 crate は以下の 3 種類を型付きで扱える。

### stpp — XML 系字幕（TTML / IMSC）

- 仕様: ISO/IEC 14496-30 `XMLSubtitleSampleEntry`
- サンプルペイロード: XML ドキュメント（TTML / IMSC 1.x など）
- 用途: 放送・配信系で広く使われる
- 特徴: 表現力が高い（スタイル・レイアウト・ルビ等）が、パーサーやレンダラーが重い

TTML（Timed Text Markup Language）は W3C 標準の XML 字幕形式。IMSC（Internet Media Subtitles and Captions）は TTML の相互運用プロファイル。

### wvtt — WebVTT

- 仕様: ISO/IEC 14496-30 `WVTTSampleEntry`
- サンプルペイロード: WebVTT の cue ボックス列（`vttc` / `vtte` / `vtta` 等）
- 用途: HTML5 動画の字幕形式として広く採用。HLS / DASH の字幕にも使われる
- 特徴: シンプルなテキストベース。ブラウザネイティブ対応

WebVTT テキスト形式そのものではなく、MP4 コンテナ用に cue ごとにボックス化した内部形式（ISO BMFF envelope）である点に注意。

### tx3g — 3GPP Timed Text

- 仕様: 3GPP TS 26.245 `TextSampleEntry` (§5.16)
- サンプルペイロード: `text_length: u16 BE` + テキスト本体 + 任意 modifier boxes
- 用途: 3G 携帯電話向けの Timed Text として策定された
- 特徴: 軽量なテキスト + スタイル指定。フォントテーブル（`ftab`）を持つ

modifier box には `styl`（部分スタイル）/ `hlit`（ハイライト）/ `krok`（カラオケ）などがある。

## handler_type と media_header の対応

サンプルエントリー種別ごとに、`hdlr` の handler_type と `minf` 直下のメディアヘッダーが決まる。

| サンプルエントリー | handler_type | media_header |
| --- | --- | --- |
| `stpp` | `subt` | `sthd` |
| `wvtt` | `text` | `sthd` |
| `tx3g` | `text` | `nmhd` |

本 crate では両 muxer がこの対応表に従って `hdlr` と `minf.media_header` を自動的に組み立てるため、利用側が明示的に指定する必要はない。

`hdlr` と `media_header` はトラック単位で 1 つしか持てないため、1 本の字幕トラックに対応表の組が異なる形式（たとえば `stpp` と `tx3g`）を混ぜることはできない。混在するサンプルを渡した場合は `MuxError::MixedSampleEntries` エラーになる。

対応表の組が同じサンプルエントリー同士（たとえば `namespace` が異なる `stpp`）であれば混在してよく、`stsd` に複数のエントリーが並ぶ。

## サンプルペイロードの扱い方針

本 crate は 3 形式とも **サンプルペイロードの内部構造をパースしない**。型付きで扱えるのはサンプルエントリーとその子ボックス（`wvtt` の `vttC`、`tx3g` の `ftab` など）までで、サンプルデータ自体（XML 本文・WebVTT cue ボックス列・tx3g のテキスト + modifier）は生バイト列としてのみ扱う。

用途を考えると:

- 字幕を **中継・保存する** 用途では本 crate だけで完結する
- 字幕を **表示・編集する** 用途では、サンプルペイロードのパースを別途行う必要がある

XML パーサ / WebVTT パーサ / tx3g modifier パーサはそれぞれ独立した専門ライブラリを使うことを想定している。

## mux / demux の対応状況

`Mp4FileMuxer` / `Fmp4SegmentMuxer` の両 muxer と、`Mp4FileDemuxer` / `Fmp4FileDemuxer` / `Fmp4SegmentDemuxer` の 3 つの demuxer がいずれも字幕トラックに対応している。C API では `MP4_TRACK_KIND_SUBTITLE`、WASM の JSON API では `"subtitle"` として扱える。

## 利用側で意識すべきこと

### 同一 `TrackKind` は 1 本まで

音声 / 映像 / 字幕はそれぞれ 1 トラックまでしか扱えない。同じ `TrackKind` のサンプルを別トラックのつもりで渡してもエラーにはならず、すべて 1 本のトラックに合流する。多言語字幕を同時に mux する用途は現時点で未対応。

### サンプル追加時の推奨値

字幕サンプルを `mux::Sample` として渡すときは以下を推奨する。

- `keyframe`: `true`（字幕サンプルは通常すべて独立サンプル）
- `composition_time_offset`: `None`

`keyframe` に `false` を渡すと `stbl` に同期サンプルの一覧（`stss`）が生成される。トラック内の全サンプルが `false` の場合は「同期サンプルが 1 つも存在しないトラック」を意味する空の `stss` が出力されてしまうため、字幕トラックでは `true` を指定すること。

`timescale` と `duration` の意味は音声・映像トラックと同じで、実時間の尺は `duration / timescale` 秒。

### 字幕系 `compatible_brands` は自動追加しない

`ftyp` の `compatible_brands` に字幕向けのブランドを自動追加する処理は入れていない。両 muxer にはブランドを外から指定する API も無いため、追加したい場合は muxer を経由せずに `FtypBox` を直接組み立てる必要がある。

### 表示挙動はプレイヤー依存

字幕トラックの表示位置・タイミング・スタイル解釈は最終的にプレイヤー側の実装に委ねられる。本 crate が扱うのは MP4 コンテナ層での配置と読み書きまでである。

## 骨格コード例

以下は `stpp` を例にした `Mp4FileMuxer` での mux の骨格。

```rust
use std::num::NonZeroU32;

use shiguredo_mp4::{
    TrackKind, Utf8String,
    boxes::{SampleEntry, StppBox},
    mux::{Mp4FileMuxer, MuxError, Sample},
};

fn mux_subtitle() -> Result<Vec<u8>, MuxError> {
    let mut muxer = Mp4FileMuxer::new()?;
    let mut output: Vec<u8> = muxer.initial_boxes_bytes().to_vec();
    let data_offset = output.len() as u64;

    let payload: &[u8] = b"<tt xmlns=\"http://www.w3.org/ns/ttml\"/>";
    output.extend_from_slice(payload);

    let sample = Sample {
        track_kind: TrackKind::Subtitle,
        sample_entry: Some(SampleEntry::Stpp(StppBox {
            data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
            namespace: Utf8String::new("http://www.w3.org/ns/ttml").expect("null 文字を含まない"),
            schema_location: Utf8String::EMPTY,
            auxiliary_mime_types: Utf8String::EMPTY,
            unknown_boxes: vec![],
        })),
        keyframe: true,
        timescale: NonZeroU32::new(1000).expect("non-zero"),
        duration: 1000,
        composition_time_offset: None,
        data_offset,
        data_size: payload.len(),
    };
    muxer.append_sample(&sample)?;

    // 以降 finalize() の結果を output に書き戻して MP4 を完成させる
    Ok(output)
}
```

demux 側は `Mp4FileDemuxer::next_sample()` が返すサンプルの `sample_entry` が `SampleEntry::Stpp(_)` かどうかで字幕を判別でき、`data_offset` と `data_size` がサンプルペイロード（この例では XML 本文）のバイト範囲を指す。

`wvtt` / `tx3g` も基本の流れは同じで、`SampleEntry::Wvtt` / `SampleEntry::Tx3g` を組み立てて渡す。ボックスの詳細フィールドは `StppBox` / `WvttBox` / `Tx3gBox` の rustdoc を参照する。

## 参考仕様

- ISO/IEC 14496-12: ISO Base Media File Format
- ISO/IEC 14496-30: Timed text and other visual overlays in ISO base media file format
- 3GPP TS 26.245: Transparent end-to-end packet switched streaming service (PSS); Timed text format
- W3C TTML2: <https://www.w3.org/TR/ttml2/>
- W3C WebVTT: <https://www.w3.org/TR/webvtt1/>
