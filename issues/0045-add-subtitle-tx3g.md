# tx3g (TX3GSampleEntry) サンプルエントリー対応を追加する

- Priority: Low
- Created: 2026-07-21
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-subtitle-tx3g
- Polished: 2026-07-23

## 目的

3GPP TS 26.245 §5.16 の `TextSampleEntry` (`tx3g`) と必須子 `FontTableBox` (`ftab`) の decode / encode 対応を追加する。旧 QuickTime / iTunes 系ワークフローで作られた MP4 に現存する形式で、ffmpeg / VLC / mpv も現役で対応する。

本 issue は「サンプルエントリー本体と必須子 `ftab` を格納するコンテナ」の対応であって、サンプルデータ内の modifier boxes（`styl` / `hlit` / `hclr` / `krok` / `dlay` / `href` / `tbox` / `blnk` / `twrp`）の型付きパースは行わない。サンプルデータは生バイト列として扱う（詳細は「### サンプルデータの扱い方針」節を参照）。

参照する規格版は 3GPP TS 26.245 v13.0.0（Release 13）を基準とする。第 1 版時点から本 issue で追加する `Tx3gBox` / `FtabBox` の主要フィールドは変わっていない。Release 10 以降で追加された任意子 `DisparityBox` (`dprp`) は型付き実装しない（後述「### 子ボックスの扱い」節）。

## 優先度根拠

Low。バグ由来ではなく緊急要求も無い。0043 / 0044 に続く 3 つ目の字幕方式で、依存元 0042（closed）と依存先 0046（open）もいずれも Low のため、格上げする根拠が無い。

本 issue の完了により、0042 で入った `derive_trak_attributes` の Subtitle 分岐 **暫定固定選択が廃止** され、対応表を持つ 3 方式（stpp / wvtt / tx3g）が明示 arm 化される（0042 の 「## 依存関係」節の予告と、0044 が残した TODO コメントを引き受ける）。未知の Subtitle 系サンプルエントリー（`SampleEntry::Unknown` 経由）向けの防御的 fallback は残る。

## 現状

字幕トラックの共通基盤は 0042 で整備済み、stpp サンプルエントリーは 0043、wvtt サンプルエントリーは 0044 で追加済み（`CHANGES.md` の `## develop` セクション参照）。以下が既に利用可能:

- `src/basic_types.rs:684-690` `TrackKind::Subtitle`
- `src/boxes_moov_tree.rs:926-929` `HdlrBox::HANDLER_TYPE_SUBT` (`subt`) / `HdlrBox::HANDLER_TYPE_TEXT` (`text`)
- `src/boxes_moov_tree.rs:1088-1150` `MediaHeader` enum（`Smhd` / `Vmhd` / `Sthd` / `Nmhd` の 4 バリアント）
- `src/boxes_moov_tree.rs:1297-1347` `SthdBox`
- `src/boxes_moov_tree.rs:1349-1404` `NmhdBox`（本 issue で初めて mux 経路の Media Header として使う）
- `src/boxes_moov_tree.rs:987-999` `MinfBox::media_header: Option<MediaHeader>`
- `src/boxes_sample_entry.rs:17-30` `SampleEntry` enum に `Stpp(StppBox)` / `Wvtt(WvttBox)` バリアントが追加済み（0043 / 0044）
- `src/boxes_sample_entry.rs:1917-2009` `StppBox` 実装、`src/boxes_sample_entry.rs:2011-2098` `WvttBox` 実装、`src/boxes_sample_entry.rs:2100-2148` `VttCBox` 実装（本 issue の参考実装）
- `src/demux_mp4_file.rs:511-517` / `src/demux_fmp4_file.rs:320-326` / `src/demux_fmp4_segment.rs:145-151` で `subt` / `text` を `TrackKind::Subtitle` にマップする分岐
- `src/mux_fmp4_segment.rs:953-1000` `derive_trak_attributes` は現状「`SampleEntry::Wvtt(_) => HANDLER_TYPE_TEXT` / それ以外は暫定 `HANDLER_TYPE_SUBT` fallback」＋ Media Header は match 外で `MediaHeader::Sthd(SthdBox)` 固定。本 issue で対応表を持つ 3 方式（stpp / wvtt / tx3g）を明示 arm 化して暫定固定選択を廃止し、Media Header を match 内に取り込む。未知バリアント（`SampleEntry::Unknown`）向けの防御的 fallback は残す（詳細は「### `derive_trak_attributes` の分岐追加」節）
- `src/mux_mp4_file.rs:557-627` `Mp4FileMuxer::append_sample` は `MuxError::UnsupportedTrackKind` で Subtitle を拒否（本 issue の範囲では変更しない。受け入れは 0046 で対応）

一方、方式固有のサンプルエントリーとしては `tx3g` は未実装で、`tx3g` box_type のサンプルエントリーは `SampleEntry::Unknown` にフォールバックしている。

- `src/boxes_sample_entry.rs:130-147` `SampleEntry::decode` で `tx3g` は `Unknown` へフォールバック
- `pbt/tests/prop_container_boxes.rs:205-238` `minimal_stsd_box_subtitle` は 3 方式（stpp / wvtt / tx3g）のうち `stpp` / `wvtt` は型付き `SampleEntry::Stpp` / `SampleEntry::Wvtt`、未実装の `tx3g` のみ `SampleEntry::Unknown` で組み立てられる
- `pbt/tests/prop_container_boxes.rs:719-726` `subtitle_scheme_matrix` は 3 組すべてを回している（3 経路のデマルチプレクサテスト用）

### `SampleEntry` の網羅 match 箇所（バリアント追加で必ずコンパイル修正が必要）

以下 7 箇所は `_` フォールバック無しの網羅 match のためコンパイルエラーで検出される（0044 で `Wvtt` 追加後の行番号）。

- `src/boxes_sample_entry.rs:93-108` `SampleEntry::inner_box`
- `src/boxes_sample_entry.rs:111-128` `impl Encode for SampleEntry`
- `src/boxes_sample_entry.rs:130-147` `impl Decode for SampleEntry`（`Unknown` フォールバックは残す。`Tx3g` は `Tx3gBox::TYPE` の arm を明示追加）
- `crates/c-api/src/boxes.rs:243-521` `Mp4SampleEntryOwned::to_mp4_sample_entry`
- `crates/c-api/src/boxes.rs:614-651` `Mp4SampleEntry::to_sample_entry`（11 バリアントを `_` フォールバック無しで網羅列挙している。Tx3g 追加で必ずコンパイルエラーになる）
- `crates/wasm/src/boxes.rs:5-56` `fmt_json_mp4_sample_entry`
- `crates/wasm/src/boxes.rs:148-201` `mp4_sample_entry_free`

### `SampleEntry` の非網羅 match 箇所（`_` フォールバックあり。arm 追加は任意だが挙動を確認）

以下は `_` フォールバックが利くためコンパイルは通るが、Subtitle 用の挙動を明示するかを実装時に判定する。

- `src/boxes_sample_entry.rs:36-43` `audio_channel_count`（fallback で `None`。Tx3g も `None` で正しいため arm を追加しない）
- `src/boxes_sample_entry.rs:57-64` `audio_sample_rate`（同上）
- `src/boxes_sample_entry.rs:69-76` `audio_sample_size`（同上）
- `src/boxes_sample_entry.rs:81-91` `video_resolution`（同上）
- `src/mux_fmp4_segment.rs:1006-1015` `extract_video_dimensions`（fallback で `(0, 0)`。Tx3g も同じで正しい）
- `src/mux_mp4_file.rs:724-758` / `src/mux_fmp4_segment.rs:503-540` の `build_final_ftyp_box` / `build_ftyp`（fallback で追加ブランドなし。字幕系ブランドの追加方針は「### `compatible_brands` の方針」節を参照）
- `crates/c-api/src/boxes.rs:131-241` `Mp4SampleEntryOwned::new`（現状 `_ => None` で Unknown を C API に露出しない設計。Tx3g arm を追加して Some を返す）
- `crates/wasm/src/boxes.rs:59-145` `parse_json_mp4_sample_entry`（末尾で不明 kind をエラーとする形。`"tx3g"` の arm を明示追加）
- `pbt/tests/prop_mux_demux.rs:583-586, 886-894` `TrackKind::Subtitle => unreachable!("字幕トラックは本テストの対象外")`（本 issue の範囲では unreachable! のまま維持）

## 設計方針

### `Tx3gBox` のバイナリレイアウト

`TextSampleEntry` は `SampleEntry` を継承し、`SampleEntry` から以下のヘッダーを引き継ぐ:

- 6 bytes reserved（`0u8; 6`）
- `data_reference_index: u16`（`NonZeroU16` 相当）

その直後に本体フィールドが以下の順で並ぶ（3GPP TS 26.245 §5.16 の順序に一致）:

1. `display_flags: u32`（3GPP TS 26.245 §5.16.1.1 の表示挙動ビットマスク。本 issue では値域チェックはせず生の `u32` を保持する）
2. `horizontal_justification: i8`（3GPP TS 26.245 §5.16 の規定で `0 = left`、`1 = centered`、`-1 = right`。0043 / 0044 と同じく本 issue では値域チェックはしない）
3. `vertical_justification: i8`（同 §5.16 の規定で `0 = top`、`1 = centered`、`-1 = bottom`。同上）
4. `background_color_rgba: [u8; 4]`（RGBA 4 バイト。0043 / 0044 と同じく値域チェックなし）
5. `default_text_box: BoxRecord`（8 バイト固定。詳細は「### `BoxRecord` のバイナリレイアウト」節）
6. `default_style: StyleRecord`（12 バイト固定。詳細は「### `StyleRecord` のバイナリレイアウト」節）

その後に子ボックスが並ぶ:

1. `ftab_box: FtabBox`: 必須。「### `FtabBox` のバイナリレイアウト」節参照
2. その他任意子（`dprp` 等）: すべて `unknown_boxes` に集約（「### 子ボックスの扱い」節参照）

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tx3gBox {
    /// データ参照インデックス（`dref` 内のエントリーを 1-based で指す）
    pub data_reference_index: NonZeroU16,
    /// 表示挙動フラグ（3GPP TS 26.245 §5.16.1.1 のビットマスク。値域チェックはしない）
    pub display_flags: u32,
    /// 水平方向のジャスティフィケーション（`0 = left`、`1 = centered`、`-1 = right`）
    pub horizontal_justification: i8,
    /// 垂直方向のジャスティフィケーション（`0 = top`、`1 = centered`、`-1 = bottom`）
    pub vertical_justification: i8,
    /// テキスト背景色（RGBA 4 バイト）
    pub background_color_rgba: [u8; 4],
    /// テキスト表示領域の既定矩形（top / left / bottom / right の i16 4 値）
    pub default_text_box: BoxRecord,
    /// 既定のテキストスタイル
    pub default_style: StyleRecord,
    /// 必須の FontTableBox
    pub ftab_box: FtabBox,
    /// 型付き実装を持たない任意の子ボックス（`dprp` 等）
    pub unknown_boxes: Vec<UnknownBox>,
}

impl Tx3gBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"tx3g");
    /// [`Tx3gBox::data_reference_index`] のデフォルト値
    pub const DEFAULT_DATA_REFERENCE_INDEX: NonZeroU16 = NonZeroU16::MIN;
}
```

- `#[derive(...)]` は既存 `WvttBox`（`src/boxes_sample_entry.rs:2016`）に揃える。特に `PartialEq` / `Eq` / `Hash` は `resolve_segment_tracks` 内の `known_entry == sample_entry` 比較（`src/mux_fmp4_segment.rs:808`）で必要。本体フィールドはすべて `Hash` 実装済み型のため問題ない（`f32` / `f64` 等は含まれない）
- decode 実装では `with_box_type(Self::TYPE, || { ... })` の定型（既存 `WvttBox::decode:2049` 参照）で全体を囲む。ヘッダー 8 バイト（reserved + data_reference_index）を先読みしたのち、本体固定サイズ 30 バイト（4 + 1 + 1 + 4 + 8 + 12）を順に読む。以降 while ループで残バイトを BoxHeader 単位に読み進めて `FtabBox::TYPE` を検出したら `ftab_box` に代入、それ以外は `unknown_boxes` に落とす
- 必須子は `check_mandatory_box(ftab_box, "ftab", "tx3g")?` で担保する（既存 `WvttBox::decode:2077` `check_mandatory_box(vttc_box, "vttC", "wvtt")` パターン）

### `BoxRecord` のバイナリレイアウト

`BoxRecord` は 3GPP TS 26.245 §5.17.1.1 の 8 バイト固定レコードで、テキスト表示領域の矩形を表す。

- `top: i16`
- `left: i16`
- `bottom: i16`
- `right: i16`

`Encode` / `Decode` は実装するが `BaseBox` は実装しない（Box ではなく単なる Record）。`SampleEntry::Tx3g` の内部でのみ使うため `pub` として `boxes_sample_entry` から公開する。

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoxRecord {
    pub top: i16,
    pub left: i16,
    pub bottom: i16,
    pub right: i16,
}
```

- `Default` を derive してデフォルト値（全 0）を提供する（`Tx3gBox` のテスト用最小構成で使う）
- `Copy` を derive（12 バイト以下の POD で問題ない）
- 値の妥当性チェックはしない（負値・逆転を許容。3GPP TS 26.245 は許容範囲を明示していない）

### `StyleRecord` のバイナリレイアウト

`StyleRecord` は 3GPP TS 26.245 §5.17.1.2 の 12 バイト固定レコードで、既定のテキストスタイルを表す。

- `start_char: u16`（style を適用する文字範囲の開始インデックス）
- `end_char: u16`（同終了インデックス）
- `font_id: u16`（`FtabBox::entries` の `font_id` を参照）
- `face_style_flags: u8`（`0x01 = Bold`、`0x02 = Italic`、`0x04 = Underline` のビットマスク）
- `font_size: u8`（ピクセル単位）
- `text_color_rgba: [u8; 4]`

`Encode` / `Decode` は実装するが `BaseBox` は実装しない（Box ではなく Record）。

```rust
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StyleRecord {
    pub start_char: u16,
    pub end_char: u16,
    pub font_id: u16,
    pub face_style_flags: u8,
    pub font_size: u8,
    pub text_color_rgba: [u8; 4],
}
```

- 値の妥当性チェックはしない（`start_char > end_char` や `font_id = 0` を許容。3GPP TS 26.245 は最小値を明示していない）

### `FtabBox` のバイナリレイアウト

`FontTableBox` は 3GPP TS 26.245 §5.16 の必須子ボックス。エントリー数と font エントリー配列を持つ。

- BoxHeader（`ftab`）
- `entry_count: u16`
- `entries: FontRecord[entry_count]`

各 `FontRecord` は可変長:

- `font_id: u16`（`StyleRecord::font_id` から参照される。仕様上 `1..` の使用が慣習だが、本 issue では値域チェックはしない）
- `font_name_length: u8`（0-255 バイト）
- `font_name: [u8; font_name_length]`（Pascal string。null 終端なし。3GPP TS 26.245 は文字エンコーディングを明示していないため raw bytes として保持し、UTF-8 として検証しない。QuickTime レガシー由来の Mac OS Roman 等の可能性もあるため consumer 側で判定する）

```rust
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct FtabBox {
    /// フォントエントリー（`entry_count` は `entries.len()` から一意に決まる）
    pub entries: Vec<FontRecord>,
}

impl FtabBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"ftab");
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontRecord {
    /// フォント識別子（`StyleRecord::font_id` からの参照先）
    pub font_id: u16,
    /// フォント名の生バイト列（Pascal string、null 終端なし、最大 255 バイト）
    ///
    /// 3GPP TS 26.245 は文字エンコーディングを明示していないため、
    /// 本ライブラリは UTF-8 バリデーションを行わない
    pub font_name: Vec<u8>,
}
```

- `FtabBox` は Box のため `Encode` / `Decode` / `BaseBox` を実装する。`entries.len()` は encode 時に `u16::try_from(entries.len())` で検証し、`u16::MAX` を超えたら `Error::invalid_input("ftab.entry_count exceeds u16::MAX")` を返す
- `FontRecord` は Record（Box ではない）のため `Encode` / `Decode` の 2 trait のみ独立して実装する（`BaseBox` は実装しない。`AudioSampleEntryFields` 等のヘルパー構造体パターンと揃える）
- `FontRecord::Encode` は `font_id: u16` を書き、`u8::try_from(font_name.len())` で長さ検証してから `font_name_length: u8` と `font_name` バイト列を書き出す。`u8::MAX (=255)` を超えたら `Error::invalid_input("FontRecord.font_name_length exceeds u8::MAX")` を返す
- `FontRecord::Decode` は `font_id: u16` → `font_name_length: u8` → `font_name: Vec<u8>` の順に読む。悪意ある入力（`FtabBox` の `entry_count = 65535` かつ payload 短で `font_name_length` が残バイトを超える等）に対する境界チェックとして、`font_name` 読み込みの前に `Error::check_buffer_size(font_name_length as usize, &buf[offset..])?` を必ず入れる（既存 `Utf8String::decode` は null 探索で境界を暗黙に守るが、length-prefix 型の `FontRecord` は明示チェックが必要。既存の `Error::check_buffer_size` の使用パターンは `src/basic_types.rs` 内の複数箇所を参照）。境界チェックが通ったら `buf[offset..offset + font_name_length as usize].to_vec()` で取り出す
- `FtabBox::Decode` は `entry_count: u16` を先読みし、`entry_count` 回 `FontRecord::decode_at(payload, &mut offset)` を呼ぶループで `entries: Vec<FontRecord>` を埋める（`decode_at` は既存の Decode trait のヘルパで、`payload` と `&mut offset` を受け取り部分読みを進める既存パターン）

### 公開範囲

新規追加する 5 型（`BoxRecord` / `StyleRecord` / `FontRecord` / `FtabBox` / `Tx3gBox`）はすべて `pub` として `boxes_sample_entry` モジュールに配置し、`src/boxes.rs:16-20` の `pub use crate::boxes_sample_entry::{...}` に追記する（既存の `StppBox` / `VttCBox` / `WvttBox` と揃える）。`Mp4SampleEntryTx3g::to_sample_entry` は `shiguredo_mp4::boxes::{Tx3gBox, FtabBox, FontRecord, BoxRecord, StyleRecord}` を参照するため、この 5 型が `boxes` モジュールから公開されていないと `crates/c-api` 側で参照できずコンパイル失敗する。

### 子ボックスの扱い

- 3GPP TS 26.245 §5.16 の `TextSampleEntry` は Release 10 以降で任意子 `DisparityBox` (`dprp`) を持ち得る
- 本 issue では **どの任意子も型付き実装しない**。既存の全 SampleEntry（`Avc1Box` / `StppBox` / `WvttBox` 等）と同じく `unknown_boxes: Vec<UnknownBox>` に落として保持する（0043 / 0044 の子ボックス方針と揃える）
- 型付き対応が必要になった場合は別 issue とする
- 子ボックスの presence 判定は、BoxHeader の残バイトを while ループで読み進める既存パターン（`WvttBox::decode` の 2062-2072 行）に倣う

### `BaseBox::children` の実装

`Tx3gBox` は必須子 `ftab_box` を持つため、既存の `WvttBox::children`（`src/boxes_sample_entry.rs:2091-2097`）と同じ「必須子 + `unknown_boxes`」パターンに揃える:

```rust
fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
    Box::new(
        core::iter::empty()
            .chain(core::iter::once(&self.ftab_box).map(as_box_object))
            .chain(self.unknown_boxes.iter().map(as_box_object)),
    )
}
```

`FtabBox` は子 Box を持たない（`FontRecord` は Box ではない Record）ため、既存 `SmhdBox::children` と同じ空 iterator を返す。`BoxRecord` / `StyleRecord` は Box ではないため `BaseBox` を実装しない。

### `SampleEntry::Tx3g` バリアントの追加

既存バリアント命名規則（box_type 4 バイト ASCII → PascalCase 化。`avc1` → `Avc1` / `stpp` → `Stpp` / `wvtt` → `Wvtt` 等）に従い `tx3g` → `Tx3g(Tx3gBox)` を採用する。「### `SampleEntry` の網羅 match 箇所」で列挙した箇所すべてに arm を追加する。

### `derive_trak_attributes` の分岐追加

`src/mux_fmp4_segment.rs:953-1000` の `derive_trak_attributes` は現状「`SampleEntry::Wvtt(_) => TEXT` / それ以外は暫定的に `SUBT`」の 2 分岐で、Media Header は match 外で `MediaHeader::Sthd(SthdBox)` に固定されている。0042 の対応表と 0044 の TODO コメント（`src/mux_fmp4_segment.rs:955-958, 981-985`）に従い、本 issue で **暫定 fallback を除去** し、`Stpp` / `Wvtt` / `Tx3g` の 3 バリアントすべてを明示 arm 化する。

対応表（0042 の 「## 依存関係」 節、および `issues/closed/0042-add-subtitle-track-common.md:98, 113`）:

- `stpp` → `handler_type = subt`、`media_header = sthd`
- `wvtt` → `handler_type = text`、`media_header = sthd`
- `tx3g` → `handler_type = text`、`media_header = nmhd`

tx3g は **`sthd` から `nmhd` に切り替わる最初のバリアント** のため、Media Header を match 外に固定できない。`(handler_type, media_header)` のタプルを match 内で決定する構造に書き換える。

分岐追加の実装案（`src/mux_fmp4_segment.rs:981-999` を書き換え）:

```rust
// tx3g のみ Media Header が nmhd で、stpp / wvtt は sthd。
// SampleEntry::Unknown を含む「stpp / wvtt / tx3g 以外」の Subtitle 経路は
// SampleEntry::decode の Unknown フォールバック（src/boxes_sample_entry.rs:145）
// を通ってきた未知バリアントで、対応表を持てないため subt + sthd に丸める
TrackKind::Subtitle => {
    let (handler_type, media_header) = match sample_entry {
        SampleEntry::Stpp(_) => (HdlrBox::HANDLER_TYPE_SUBT, MediaHeader::Sthd(SthdBox)),
        SampleEntry::Wvtt(_) => (HdlrBox::HANDLER_TYPE_TEXT, MediaHeader::Sthd(SthdBox)),
        SampleEntry::Tx3g(_) => (HdlrBox::HANDLER_TYPE_TEXT, MediaHeader::Nmhd(NmhdBox)),
        _ => (HdlrBox::HANDLER_TYPE_SUBT, MediaHeader::Sthd(SthdBox)),
    };
    Ok(TrakDerivation {
        volume: TkhdBox::DEFAULT_VIDEO_VOLUME,
        width: FixedPointNumber::default(),
        height: FixedPointNumber::default(),
        handler_type,
        media_header,
    })
}
```

- `_ => (SUBT, Sthd)` の fallback は `SampleEntry::Unknown` 経路の防御として残す。decode 時に `SampleEntry::decode` が知らない Subtitle 系サンプルエントリー（またはコーデックとして意味が破れた入力）を `Unknown` に落とすため、この経路は本 issue 完了後も存在し続ける。demux → mux の invariant として `TrackKind::Subtitle` に映像・音声系 SampleEntry (`Avc1` / `Opus` 等) が渡る運用ケースは実質存在しないため、この fallback は事実上 `SampleEntry::Unknown` に対する防御である
- 現状 `src/mux_fmp4_segment.rs:953-958` の doc コメントを次の文面に書き換える: 「[`TrackKind::Subtitle`] 分岐は方式ごとの対応表に従い `SampleEntry::Stpp` / `SampleEntry::Wvtt` / `SampleEntry::Tx3g` を個別 arm で扱う（stpp / wvtt は `sthd`、tx3g は `nmhd` の Media Header を返す）。未知の Subtitle 系サンプルエントリー（`SampleEntry::Unknown` にフォールバックしたもの）は防御的に `subt` + `sthd` を返す」
- 現状 `src/mux_fmp4_segment.rs:981-990` のインラインコメントを対応表に沿った説明に置き換える（「暫定」表現を除去する）

### C API 露出

以下を追加する。既存 11 バリアント（`Avc1` / `Hev1` / `Hvc1` / `Vp08` / `Vp09` / `Av01` / `Opus` / `Mp4a` / `Flac` / `Stpp` / `Wvtt`）と同じパターンに揃える。

- `crates/c-api/src/boxes.rs:11-44` `Mp4SampleEntryKind` に `MP4_SAMPLE_ENTRY_KIND_TX3G` を末尾に追加（`#[repr(C)]` の順序位置固定のため末尾追加）
- `crates/c-api/src/boxes.rs:46-129` `Mp4SampleEntryOwned` に `Tx3g { inner: Tx3gBox, ftab_font_ids: Vec<u16>, ftab_font_name_ptrs: Vec<*const u8>, ftab_font_name_sizes: Vec<u32> }` バリアントを追加。既存 `Avc1` バリアント（`crates/c-api/src/boxes.rs:47-60`）と同様に「`inner` およびバッキング 3 本が途中で更新されると C 側で保持されているポインタが不正になる可能性がある」旨の `[NOTE]` コメントをフィールド定義の前に追加する（既存 `Hev1` / `Hvc1` は `[NOTE] Avc1 のコメントを参照` に短縮しており、Tx3g もそれに揃える）。可変長 `ftab` エントリの C ABI 露出のため、`Avc1` の `sps_data` / `sps_sizes` パターンと同じ並行配列パターンで 3 本の backing storage を持つ。`FontRecord` は `font_id: u16 + font_name: Vec<u8>` の非連続レイアウトのため、`ftab_font_ids` は `inner.ftab_box.entries.iter().map(|e| e.font_id).collect::<Vec<_>>()` で `u16` の連続バッファを新規に確保する必要がある（`inner.ftab_box.entries.as_ptr() as *const u16` は使えない）。`ftab_font_name_ptrs` / `ftab_font_name_sizes` はそれぞれ `inner.ftab_box.entries[i].font_name.as_ptr()` / `.len() as u32` から組み立てる（heap 位置は `inner` が drop / 再代入されるまで安定。drop 順序は既存 `Avc1` と同じで、`inner` が最初に drop されて `Vec<u8>` が解放されたあと `Vec<*const u8>` の drop はポインタを参照外ししないため Rust 側で undefined behavior は起きない）
- `crates/c-api/src/boxes.rs:131-241` `Mp4SampleEntryOwned::new` の match に `Tx3g` arm 追加（`_ => None` は残す。上記 3 本の backing storage を `inner.ftab_box.entries.iter().map(...)` で組み立てる）
- `crates/c-api/src/boxes.rs:243-521` `Mp4SampleEntryOwned::to_mp4_sample_entry` の match に `Tx3g` arm 追加
- `crates/c-api/src/boxes.rs:614-651` `Mp4SampleEntry::to_sample_entry` の match に `MP4_SAMPLE_ENTRY_KIND_TX3G => unsafe { self.data.tx3g.to_sample_entry() }` arm 追加（この match は `_` フォールバック無しで網羅列挙のため必ずコンパイルエラー）
- `crates/c-api/src/boxes.rs:528-562` `Mp4SampleEntryData` union に `tx3g: Mp4SampleEntryTx3g` フィールド追加。既存最大サイズは `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` の 80 バイト、新 `Mp4SampleEntryTx3g` は概算 64 バイトのため union サイズは増えず、既存 field の offset は変わらない（`#[repr(C)]` union は全 variant が offset 0。実サイズは実装時に cbindgen 出力および `std::mem::size_of` で最終確認する）
- 新規 `Mp4SampleEntryTx3g` 構造体を追加（`#[repr(C)]`）。フィールドは以下。既存 `Mp4SampleEntryStpp` / `Mp4SampleEntryWvtt` の命名パターン（`<field>_data` / `<field>_size`、ポインタ配列は `<field>_ptrs` / `<field>_sizes` / `<field>_count`）に揃える:

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Mp4SampleEntryTx3g {
    /// 表示挙動フラグ（3GPP TS 26.245 §5.16.1.1 のビットマスク。値域チェックはしない）
    pub display_flags: u32,
    /// 水平方向のジャスティフィケーション（`0 = left` / `1 = centered` / `-1 = right`）
    pub horizontal_justification: i8,
    /// 垂直方向のジャスティフィケーション（`0 = top` / `1 = centered` / `-1 = bottom`）
    pub vertical_justification: i8,
    /// テキスト背景色（RGBA）
    pub background_color_rgba: [u8; 4],
    /// テキスト表示領域の既定矩形（top / left / bottom / right）
    pub default_text_box: [i16; 4],
    /// 既定スタイル: style を適用する文字範囲の開始
    pub default_style_start_char: u16,
    /// 既定スタイル: style を適用する文字範囲の終了
    pub default_style_end_char: u16,
    /// 既定スタイル: font-ID
    pub default_style_font_id: u16,
    /// 既定スタイル: face-style-flags（Bold / Italic / Underline のビットマスク）
    pub default_style_face_style_flags: u8,
    /// 既定スタイル: font-size（ピクセル）
    pub default_style_font_size: u8,
    /// 既定スタイル: text-color-rgba
    pub default_style_text_color_rgba: [u8; 4],
    /// ftab の font-ID 配列（長さは `ftab_count`）
    pub ftab_font_ids: *const u16,
    /// ftab の font-name ポインタ配列（各要素は `ftab_font_name_sizes[i]` バイト、null 終端なし）
    ///
    /// 3GPP TS 26.245 は文字エンコーディングを明示していないため、
    /// バイト列は UTF-8 として保証されない
    pub ftab_font_name_ptrs: *const *const u8,
    /// ftab の font-name 長さ配列
    pub ftab_font_name_sizes: *const u32,
    /// ftab のエントリー数
    pub ftab_count: u32,
}
```

- `data_reference_index` は `Mp4SampleEntryTx3g` に含めない（既存 `Mp4SampleEntryStpp` / `Mp4SampleEntryWvtt` と同じく、C API 側では `Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX` 相当のデフォルト値で復元する運用に揃える）。**この設計により C API 経由の decode → encode で元の `data_reference_index` は失われる**（常に 1 に丸められる。既存 Stpp / Wvtt / Mp4a と同じ制約）
- `impl Mp4SampleEntryTx3g { fn to_sample_entry(self) -> Result<SampleEntry, Mp4Error> { ... } }` を追加。既存 `Mp4SampleEntryWvtt::to_sample_entry`（`crates/c-api/src/boxes.rs:1592-1621`）に揃える骨格:

```rust
fn to_sample_entry(self) -> Result<shiguredo_mp4::boxes::SampleEntry, Mp4Error> {
    // ftab エントリを組み立てる
    let mut entries = Vec::with_capacity(self.ftab_count as usize);
    if self.ftab_count > 0 {
        if self.ftab_font_ids.is_null()
            || self.ftab_font_name_ptrs.is_null()
            || self.ftab_font_name_sizes.is_null()
        {
            return Err(Mp4Error::MP4_ERROR_NULL_POINTER);
        }
        let ids = unsafe {
            std::slice::from_raw_parts(self.ftab_font_ids, self.ftab_count as usize)
        };
        let ptrs = unsafe {
            std::slice::from_raw_parts(self.ftab_font_name_ptrs, self.ftab_count as usize)
        };
        let sizes = unsafe {
            std::slice::from_raw_parts(self.ftab_font_name_sizes, self.ftab_count as usize)
        };
        for i in 0..self.ftab_count as usize {
            let size = sizes[i] as usize;
            if size > u8::MAX as usize {
                return Err(Mp4Error::MP4_ERROR_INVALID_INPUT);
            }
            let name = if size == 0 {
                Vec::new()
            } else {
                if ptrs[i].is_null() {
                    return Err(Mp4Error::MP4_ERROR_NULL_POINTER);
                }
                unsafe { std::slice::from_raw_parts(ptrs[i], size) }.to_vec()
            };
            entries.push(shiguredo_mp4::boxes::FontRecord {
                font_id: ids[i],
                font_name: name,
            });
        }
    }
    // Tx3gBox を組み立てて SampleEntry::Tx3g として返す
    Ok(shiguredo_mp4::boxes::SampleEntry::Tx3g(
        shiguredo_mp4::boxes::Tx3gBox {
            data_reference_index: shiguredo_mp4::boxes::Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX,
            display_flags: self.display_flags,
            horizontal_justification: self.horizontal_justification,
            vertical_justification: self.vertical_justification,
            background_color_rgba: self.background_color_rgba,
            default_text_box: shiguredo_mp4::boxes::BoxRecord {
                top: self.default_text_box[0],
                left: self.default_text_box[1],
                bottom: self.default_text_box[2],
                right: self.default_text_box[3],
            },
            default_style: shiguredo_mp4::boxes::StyleRecord {
                start_char: self.default_style_start_char,
                end_char: self.default_style_end_char,
                font_id: self.default_style_font_id,
                face_style_flags: self.default_style_face_style_flags,
                font_size: self.default_style_font_size,
                text_color_rgba: self.default_style_text_color_rgba,
            },
            ftab_box: shiguredo_mp4::boxes::FtabBox { entries },
            unknown_boxes: Vec::new(),
        },
    ))
}
```

- `crates/c-api/build.rs` の cbindgen 経由で `crates/c-api/include/mp4.h` が更新される。`cargo build` 後に該当ヘッダーの diff を確認する（`Mp4SampleEntryTx3g` 構造体・`MP4_SAMPLE_ENTRY_KIND_TX3G` の生成、および `Mp4SampleEntryData` union に `tx3g` メンバが末尾追加されるか）
- `crates/c-api/examples/demux.c:46-73` および `crates/c-api/examples/remux.c:32-57` の `get_sample_entry_kind_name` に `"tx3g (3GPP Timed Text)"` の case を追加（両方の examples に同名関数がある）。`print_sample_entry_info` の switch（`demux.c:75-138`）は tx3g 用の詳細情報表示 case は追加しない（stpp / wvtt と同じく default に流す）

### WASM 露出

- `crates/wasm/src/boxes.rs:5-54` `fmt_json_mp4_sample_entry` の match に `MP4_SAMPLE_ENTRY_KIND_TX3G` arm 追加
- `crates/wasm/src/boxes.rs:59-145` `parse_json_mp4_sample_entry` の match に `"tx3g"` arm 追加
- `crates/wasm/src/boxes.rs:148-201` `mp4_sample_entry_free` の match に `Tx3g` arm 追加
- `crates/wasm/src/boxes_tx3g.rs` を新規作成。雛形は `crates/wasm/src/boxes_avc1.rs` の可変長配列露出パターン（`allocate_and_copy_array_list` / `free_array_list` の利用）を参考にする。fixed-size 部分は `crates/wasm/src/boxes_wvtt.rs` パターンでフィールドを 1 個ずつ扱う
- JSON スキーマ:

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
    "ftab": [
        { "font_id": 1, "font_name": [83, 101, 114, 105, 102] }
    ]
}
```

- `font_name` は数値配列（バイト列）として露出する（UTF-8 を保証しないため JSON 文字列にはしない。0044 の `VttCBox::config` が `String` = UTF-8 保証のため文字列露出だったのと対照的）
- `data_reference_index` は C API 露出方針に揃えて JSON にも含めない
- `parse_json_mp4_sample_entry_tx3g` は既存 stpp arm（`crates/wasm/src/boxes.rs:129-135`）と同じ形で `Mp4SampleEntry { kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_TX3G, data: Mp4SampleEntryData { tx3g } }` を組み立てる
- 数値配列（`background_color_rgba` / `default_text_box` / `text_color_rgba` / 各 `font_name`）は既存 `nojson` の配列 API（`value.to_member("...")?.required()?.to_array_iter()?.map(|v| v.parse::<u8>())` パターン等）で扱う
- 解放処理はデータ形状ごとに 2 種類のヘルパを使い分ける:
  - `ftab_font_name_ptrs: *const *const u8` と `ftab_font_name_sizes: *const u32` は `allocate_and_copy_array_list(&font_names: &[Vec<u8>])`（`crates/wasm/src/boxes.rs:239`）で確保し、`free_array_list`（`crates/wasm/src/boxes.rs:273`）で解放する（既存 `boxes_avc1.rs:56-64` / `121-129` の SPS / PPS 配列と同じパターン）
  - `ftab_font_ids: *const u16` は「u16 の 1 本の連続バッファ」で `allocate_and_copy_array_list` の対象（`&[Vec<u8>]`）ではないため、本 issue の範囲内で `crates/wasm/src/boxes.rs` に以下 2 個のヘルパを新規追加する（`bytemuck` 依存追加は行わない。既存の他バリアントで u16 配列を露出している例は無いため新設が必要）:
    - `pub fn allocate_and_copy_u16_array(data: &[u16]) -> (*const u16, u32)`: 既存 `allocate_and_copy_bytes`（`crates/wasm/src/boxes.rs:219-234`）と同じく `mp4_alloc(data.len() * 2)` で領域を確保し、`std::ptr::copy_nonoverlapping(data.as_ptr() as *const u8, allocated, data.len() * 2)` でバイト単位コピーする。返り値のポインタは `allocated as *const u16`、`u32` は要素数（バイト数ではない）。**`Vec::leak` は使わない**（`Vec` 由来のアロケータと `mp4_alloc` のアロケータが異なるため `mp4_free` で解放するとアライメント不整合になる。既存 `allocate_and_copy_bytes` と同じ mp4_alloc パスに揃える）
    - `pub unsafe fn free_u16_array(ptr: *mut u16, count: u32)`: 内部で `crate::mp4_free(ptr as *mut u8, count * 2)` を呼ぶ。`mp4_free` のシグネチャは `pub unsafe fn mp4_free(ptr: *mut u8, size: u32)`（`crates/wasm/src/lib.rs:57`、size はバイト単位）
  - fixed-size フィールドは解放不要
- テスト（`test_tx3g_to_json` / `test_json_to_tx3g_and_free` / `test_tx3g_json_roundtrip_with_empty_ftab` / `test_tx3g_json_roundtrip_with_multiple_fonts`）を `crates/wasm/src/boxes_avc1.rs` パターンで追加する

### `compatible_brands` の方針

`src/mux_fmp4_segment.rs:503-540` `build_ftyp` / `src/mux_mp4_file.rs:724-758` `build_final_ftyp_box` は SampleEntry 種別に応じて compatible_brands を追加する。0045 では字幕系ブランド（`msubs` 等）や 3GPP 系ブランド（`3gp*`）は **追加しない**（0042 / 0043 / 0044 と同じ方針を継続。0046 の「### `build_ftyp` / `compatible_brands`」節でも「本 issue の範囲では追加しない方向を第一候補」とされており、そちらとも整合）。

実プレイヤーでの字幕再生（QuickTime Player / VLC 等での認識）に必要な brand 対応は、必要になれば別 issue で扱う（tx3g ではなく 3GPP 側の推奨表に依存する話でもある）。

### サンプルデータの扱い方針

- 本 issue では **サンプルデータ全体は生バイト列** として扱い、内部構造の parse / build は consumer 側に委ねる
- サンプルデータは 3GPP TS 26.245 §5.17 に従い `text_length: u16` (BE) + テキスト本体 + 任意 modifier boxes（`styl` / `hlit` / `hclr` / `krok` / `dlay` / `href` / `tbox` / `blnk` / `twrp`）で構成される
- 理由: 既存の映像・音声サンプルの扱いと一貫させ、実装スコープを抑えるため。modifier boxes の型付きパースを本ライブラリで持つとテキスト装飾専用のパーサ依存が発生する
- サンプルサイズは consumer 側で `2 + text.len() + modifier_bytes.len()` として整合的に組み立てる責務を持つ（`Sample::data_size` はこの合計値になる）。0044 の VttCue と同じく `Sample` レベルでは生バイト列のみを扱う
- サンプル単位の推奨値は既存の `Sample` 構造体（`src/mux_mp4_file.rs:179-215`、0042 で subtitle 全般に対応済み）で文書化済み（`keyframe = true`、`composition_time_offset = None`）を踏襲する
- 追加で内部構造の型付き対応が必要になった場合は別 issue とする

### 後方互換性への影響

- `SampleEntry` へのバリアント追加は網羅 match を破壊するため SemVer 上の破壊的変更（`CHANGES.md` では `[CHANGE]` を使う。詳細は「## CHANGES.md」節）
- `Unknown` フォールバック（`src/boxes_sample_entry.rs:145`）が残るため、decode 側の未知バリアント互換は維持される
- ただし、これまで `SampleEntry::Unknown { box_type: BoxType::Normal(*b"tx3g"), .. }` として観測されていた tx3g サンプルエントリーが本 issue 完了後は `SampleEntry::Tx3g(_)` として観測される。既存 consumer で `match sample_entry { Unknown(b) if b.box_type == ... => }` のような判定を書いていた場合は影響する
- C API の `Mp4SampleEntryKind` enum に新規バリアントが末尾追加されるため、C 側の switch 網羅性を破壊しうる（追加後の bindgen 出力を利用者側でも取り込む必要がある）
- WASM JSON API に `"tx3g"` kind が追加される（既存の `"avc1"` / `"stpp"` / `"wvtt"` 等と同列）
- `derive_trak_attributes` の Subtitle 分岐の暫定 fallback が除去され、SampleEntry 種別ごとの明示分岐に切り替わる。`SampleEntry::Tx3g` を含む Subtitle トラックを mux した際の handler_type が 0044 完了時点の暫定 `subt` から `text` へ、Media Header も `sthd` から `nmhd` へ変わる
- `SampleEntry::Unknown` を持つ Subtitle トラックを mux した場合の handler_type / Media Header は 0044 完了時点と変わらず `subt` + `sthd`（fallback として維持）
- `Tx3gBox` / `FtabBox` / `FontRecord` / `BoxRecord` / `StyleRecord` は新規公開 API のため既存 consumer には影響しない

## 依存関係

- 0042（`issues/closed/0042-add-subtitle-track-common.md`）は完了済み。以下を利用する:
  - `TrackKind::Subtitle`
  - `HdlrBox::HANDLER_TYPE_TEXT` (`text`) — tx3g 用の handler_type
  - `MediaHeader::Nmhd(NmhdBox)` — tx3g 用の Media Header（対応表通り。本 issue で初めて mux 経路で使う）
  - `Fmp4SegmentMuxer::derive_trak_attributes`（`src/mux_fmp4_segment.rs:953-1000`）— 本 issue で SampleEntry 種別分岐を完全化する（暫定 fallback を除去する）
  - `MuxError::UnsupportedTrackKind` — `Mp4FileMuxer` の拒否経路は本 issue でも維持
- 0043（`issues/closed/0043-add-subtitle-stpp.md`）は完了済み。以下を利用する:
  - `SampleEntry::Stpp(StppBox)` バリアント（`src/boxes_sample_entry.rs:27, 1917-2009`）を参考実装として使う（バイナリレイアウト、`BaseBox::children`、C API / WASM 露出のパターン）
- 0044（`issues/closed/0044-add-subtitle-wvtt.md`）は完了済み。以下を利用する:
  - `SampleEntry::Wvtt(WvttBox)` バリアント（`src/boxes_sample_entry.rs:28, 2011-2098`）と `VttCBox`（`src/boxes_sample_entry.rs:2100-2148`）を参考実装として使う（必須子ボックスを持つ SampleEntry のパターン。`Tx3gBox` の必須子 `FtabBox` は同じ構造）
  - 0044 が `derive_trak_attributes` に残した「wvtt のみ明示、他 fallback」の状態および doc / インラインコメントの TODO を本 issue で消化する（詳細は「### `derive_trak_attributes` の分岐追加」節）
- 0046（`issues/0046-add-mp4-file-muxer-subtitle.md`、open）は「`Mp4FileMuxer` / `Mp4FileDemuxer` 経由の MP4 ラウンドトリップ」検証で前提となる。0046 未完了時は `Fmp4SegmentMuxer` / `Fmp4SegmentDemuxer` 経由の fMP4 ラウンドトリップのみで完了と判断する
- 本 issue は他の open issue の依存元ではない（各方式は独立に追加可能）

## 完了条件

### 実装完了

- `BoxRecord` の `Encode` / `Decode` を実装する（8 バイト固定、`i16` × 4）
- `StyleRecord` の `Encode` / `Decode` を実装する（12 バイト固定）
- `FtabBox` の `Encode` / `Decode` / `BaseBox` を実装する（`entries: Vec<FontRecord>` フィールドを持ち、`entry_count: u16` は encode 時に `u16::try_from(entries.len())` で検証、`font_name_length: u8` は `u8::try_from(font_name.len())` で検証）
- `FontRecord` の `Encode` / `Decode` を実装する（可変長。`Encode` のみ独立、`Decode` は `FtabBox::decode` 内で `entry_count` ぶん pop）
- `Tx3gBox` の `Encode` / `Decode` / `BaseBox` を実装する（本体固定 30 バイト + 必須子 `ftab_box` + `unknown_boxes`。必須子は `check_mandatory_box` で担保する）
- `SampleEntry::Tx3g(Tx3gBox)` バリアントを追加し、「### `SampleEntry` の網羅 match 箇所」で列挙した 7 箇所すべてに arm を追加する。「### `SampleEntry` の非網羅 match 箇所」で列挙した箇所のうち、`Mp4SampleEntryOwned::new` と `parse_json_mp4_sample_entry` の 2 箇所も arm 追加が必要（コンパイルは通るが `Unknown` フォールバックに落ちて C API / WASM に露出されないため）
- `derive_trak_attributes` の Subtitle 分岐を SampleEntry 種別 match に切り替え、`Stpp` / `Wvtt` / `Tx3g` の 3 arm を明示化する。`Tx3g` arm では `HANDLER_TYPE_TEXT` + `MediaHeader::Nmhd(NmhdBox)` を返す。既存 `SampleEntry::Unknown` 経路は防御的 fallback `(SUBT, Sthd)` として残す。doc / インラインコメントを実態に合わせて更新する
- C API 露出（`Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_TX3G`、`Mp4SampleEntryOwned::Tx3g`、`Mp4SampleEntryData::tx3g`、`Mp4SampleEntryTx3g` 構造体、`Mp4SampleEntryOwned::new` / `to_mp4_sample_entry` / `Mp4SampleEntry::to_sample_entry` の各 arm、`Mp4SampleEntryTx3g::to_sample_entry` 実装）を追加する
- WASM 露出（`crates/wasm/src/boxes.rs` の 3 関数の arm、`crates/wasm/src/boxes_tx3g.rs` 新規作成）を追加する
- `crates/c-api/examples/demux.c` および `remux.c` の `get_sample_entry_kind_name` に `"tx3g (3GPP Timed Text)"` の case を追加する

### PBT 追加

以下のテストを `pbt/tests/prop_additional_boxes.rs` に追加する（既存の `stpp_box_roundtrip` / `wvtt_box_roundtrip` と同じファイルに揃える）:

- `box_record_roundtrip`: `BoxRecord` の decode / encode ラウンドトリップ（4 値 `i16` の網羅）
- `style_record_roundtrip`: `StyleRecord` の decode / encode ラウンドトリップ（全フィールドの網羅）
- `ftab_box_roundtrip`: `FtabBox` の decode / encode ラウンドトリップ（proptest ブロック内で以下パターンを網羅）:
  - `entries` 空（境界値、エッジケース: `entry_count = 0`）
  - `entries` 1 個（最小構成）
  - `entries` 複数（`arb_ftab_box` で 0-8 個の範囲）
  - `font_name` 空 / ASCII / 非 UTF-8 バイト列を含むケース
  - `font_name` 長さ 255（`u8::MAX` 境界値）
- `tx3g_box_roundtrip`: `Tx3gBox` の decode / encode ラウンドトリップ（proptest ブロック内で以下パターンを網羅）:
  - `ftab` のみ（最小構成）
  - `ftab` + `unknown_boxes`（0 〜 3 個の任意子）
  - justification の全値域（-1 / 0 / 1）
  - `background_color_rgba` / `default_text_box` / `default_style` の全フィールド網羅
- strategy の実装:
  - `arb_box_record() -> impl Strategy<Value = BoxRecord>`: `any::<i16>()` × 4
  - `arb_style_record() -> impl Strategy<Value = StyleRecord>`: 各フィールドを `any::<u16>()` / `any::<u8>()` 等で生成
  - `arb_font_name() -> impl Strategy<Value = Vec<u8>>`: `prop::collection::vec(any::<u8>(), 0..=255)` で最大 255 バイト
  - `arb_font_record() -> impl Strategy<Value = FontRecord>`
  - `arb_ftab_box() -> impl Strategy<Value = FtabBox>`: `prop::collection::vec(arb_font_record(), 0..=8)` で個数を抑制（combinatorial 爆発回避）
  - `arb_tx3g_box() -> impl Strategy<Value = Tx3gBox>`（既存 `arb_wvtt_box` に揃え、`arb_unknown_box` を使って `unknown_boxes` を混ぜる）
- 独立関数として `sample_entry_tx3g_methods` / `sample_entry_tx3g_encode_decode_roundtrip` を新設する（既存 wvtt パターン `pbt/tests/prop_additional_boxes.rs:1386-1419` に揃え、`SampleEntry::Tx3g` の固定インスタンスで検証）

### 単体テスト追加

以下の単体テストを `pbt/tests/prop_additional_boxes.rs` に追加する（既存の wvtt 用単体テスト群 1421-1532 行と同じファイルに揃える）:

- `build_valid_tx3g_bytes(ftab_entries: &[(u16, &[u8])]) -> Vec<u8>` ヘルパを追加（`build_valid_wvtt_bytes`（1425-1446 行）と同様）。BoxHeader 8 バイト + 6 bytes reserved + `data_reference_index = 1u16` 固定 + 30 バイト本体 + `ftab` 子ボックスの入れ子で組み立てる
- `tx3g_box_decode_valid_bytes`: 有効なバイト列で組み立てて decode できることを確認する
- `tx3g_box_missing_ftab`: `tx3g` payload に `ftab` 子ボックスが無い場合に `Err` が返る（必須子欠落エラー、`check_mandatory_box` 経由、メッセージに `"ftab"` を含む）
- `tx3g_box_decode_wrong_box_type`: `Tx3gBox::decode` に `tx3g` 以外の box_type を持つバイト列を渡すとエラー
- `ftab_box_decode_wrong_box_type`: `FtabBox::decode` に `ftab` 以外の box_type を持つバイト列を渡すとエラー
- `ftab_box_decode_entry_count_zero_roundtrip`: BoxHeader (`size = 10, type = b"ftab"`、8 バイト) + `entry_count = 0` (u16 BE、2 バイト) の計 10 バイト固定バイト列を decode → encode してラウンドトリップが成立することを確認する（`minf_box_subtitle_nmhd_roundtrip` の tx3g typed 化後の invariant `ftab_box: FtabBox { entries: vec![] }` を deterministic に担保する）
- `ftab_box_encode_too_many_entries`: `FtabBox::encode` で `entries.len()` が `u16::MAX + 1` 以上の場合にエラー（`u16::try_from` 経由。実際には `u16::MAX` 個 + 1 個のエントリーを組み立てて encode を試みるコストが高いため、`u16::MAX + 1` の閾値を関数レベルの検証で代替する形で構わない。実装コスト次第で省略可）
- `font_record_encode_too_long_name`: `FontRecord::encode` で `font_name.len() > 255` の場合にエラー
- `sample_entry_decode_tx3g_dispatches_to_tx3g_variant`: `SampleEntry::decode` で `tx3g` box_type を持つ入力が `SampleEntry::Tx3g(_)` として取り出される（0045 完了以前は `Unknown` にフォールバックしていた挙動の回帰確認）

`pbt/tests/prop_error_paths.rs:378-` の `sample_entry_inner_box_tests` モジュールに `sample_entry_tx3g_inner_box` を追加する（既存 `sample_entry_stpp_inner_box`（632 行）/ `sample_entry_wvtt_inner_box`（652 行）の隣）。既存 `sample_entry_wvtt_inner_box` パターン（`box_type()` と `is_unknown_box()` の検証 + 必須子を持つ `children().count() == 1` の検証）に揃える。テスト用インスタンスは `SampleEntry::Tx3g(Tx3gBox { data_reference_index: Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX, display_flags: 0, horizontal_justification: 0, vertical_justification: 0, background_color_rgba: [0; 4], default_text_box: BoxRecord::default(), default_style: StyleRecord::default(), ftab_box: FtabBox { entries: vec![] }, unknown_boxes: vec![] })` で作成する。

### 既存テストの更新

- `pbt/tests/prop_container_boxes.rs:205-238` `minimal_stsd_box_subtitle` の tx3g 分岐は typed `SampleEntry::Tx3g` に切り替える（Unknown tx3g payload では `Tx3gBox::decode` が必須子 `ftab` 欠落で失敗し既存 `subtitle_track_via_*_demuxer` テストが round-trip で壊れるため。0044 で wvtt を typed に切り替えたのと同じ判断）
- 上記に伴い、Subtitle 系の `SampleEntry::Unknown` 経路を通す自動テストは残らなくなる。`SampleEntry::Unknown` の accessor / children 検証は `pbt/tests/prop_additional_boxes.rs:1137-1151` `sample_entry_unknown_methods` および `pbt/tests/prop_error_paths.rs:667-677` `sample_entry_unknown_inner_box` で担保されるが、これらは `Unknown` を直接構築して検証する形で、`SampleEntry::decode` の Unknown フォールバック分岐（`src/boxes_sample_entry.rs:145`）と `derive_trak_attributes` の `SampleEntry::Unknown => (SUBT, Sthd)` 防御的 fallback は自動テストの経路上通らなくなる。ただし前者は 1 行の `UnknownBox::decode` 委譲、後者も 1 行の値を返すだけであり、回帰リスクは低い。fallback 経路の自動テスト担保が必要になれば別 issue で `subtitle_scheme_matrix` に架空 box_type（例: `*b"dumy"`）を持つエントリーを追加する（本 issue のスコープには含めない）
- `pbt/tests/prop_container_boxes.rs:719-726` `subtitle_scheme_matrix` は 3 組すべて typed 化された状態でも行番号は変わらない（対応表は方式ごとの handler_type 対応の宣言であり typed / Unknown の別ではない）
- `pbt/tests/prop_container_boxes.rs` の `subtitle_track_via_*_demuxer` 3 経路テスト（既存の 776-864 行付近、0044 で新規追加された Fmp4 経路 2 本の隣）は wvtt / stpp / tx3g がすべて typed 化された状態で pass することを確認する
- `pbt/tests/prop_container_boxes.rs:688-706` `minf_box_subtitle_nmhd_roundtrip` は `sample_entry_box_type = *b"tx3g"` を渡している（tx3g だけ Unknown フォールバックだった 0044 完了時点の状態）。tx3g typed 化後もこのテストは pass する（stsd 内の SampleEntry が Unknown → Tx3g に切り替わるが、`MinfBox` の Media Header ラウンドトリップに影響しない。tx3g 用の`FtabBox` 必須子を含む typed 実装が最小構成でも成立する invariant を担保する必要があるため、テスト内で `SampleEntry::Tx3g` を組み立てる際は `ftab_box` を空 `entries: vec![]` で初期化する）
- `pbt/tests/prop_container_boxes.rs:886-953` `subtitle_track_mux_tkhd_via_fmp4_segment_muxer`（`SampleEntry::Stpp` を使用）は変更しない

### `derive_trak_attributes` の Tx3g 分岐検証テスト追加

`pbt/tests/prop_container_boxes.rs` の `subtitle_track_mux_tkhd_via_fmp4_segment_muxer_wvtt` と同構造の新規テストを追加する:

- `subtitle_track_mux_tkhd_via_fmp4_segment_muxer_tx3g`: 最小の `SampleEntry::Tx3g` を持つ Sample を `Fmp4SegmentMuxer` に渡し、生成された moov 内 trak について以下を検証する:
  - `handler_type == HdlrBox::HANDLER_TYPE_TEXT`（tx3g 用の text）
  - `media_header == Some(MediaHeader::Nmhd(NmhdBox))`（tx3g 用の nmhd。**stpp / wvtt との決定的な差**）
  - tkhd 属性（`volume = TkhdBox::DEFAULT_VIDEO_VOLUME`、`width = 0`、`height = 0`）は stpp / wvtt 版と同じ

### Fmp4 経路 2 本の Tx3g 検証と合成ラウンドトリップ

`TrackInfo`（`src/demux_mp4_file.rs:53-70`）は `sample_entries` フィールドを持たない。`SampleEntry` を取り出すには `Sample.sample_entry: Option<&SampleEntry>`（`src/demux_mp4_file.rs:84`）から取得する必要があるため、init segment / moov だけでなく media segment / mdat + サンプルデータを含む合成データを組み立てる必要がある。

以下 2 経路の Tx3g 正常経路担保テストを `pbt/tests/prop_container_boxes.rs` に追加する（既存 `wvtt_sample_entry_via_*_demuxer` の隣）:

- `tx3g_sample_entry_via_fmp4_file_demuxer`: `SampleEntry::Tx3g(_)` を持つ init + moof + mdat の合成バイト列を組み立て、`Fmp4FileDemuxer` から取り出した `sample.sample_entry` が `Some(SampleEntry::Tx3g(_))` にマッチすることを検証
- `tx3g_sample_entry_via_fmp4_segment_demuxer`: init segment + media segment の合成バイト列を組み立て、`Fmp4SegmentDemuxer::handle_media_segment` の戻り値の最初の Sample.sample_entry を検証（既存 wvtt テストと揃える）

`Mp4FileDemuxer` 経路（`tx3g_sample_entry_via_mp4_file_demuxer`）は本 issue のスコープに含めない（`Mp4FileMuxer` が Subtitle を拒否する現状では合成データを Muxer 経由で吐かせられないため。0046 完了後に別途追加。`issues/0046-add-mp4-file-muxer-subtitle.md:102-107` の「本 issue 完了後の追随タスク」節を参照）。

Fmp4 経路 2 本の合成は `Fmp4SegmentMuxer`（0042 で Subtitle 受け入れ済み）を経由して組み立てる。既存 `pbt/tests/prop_container_boxes.rs:1146` の `build_wvtt_fmp4_segments`（`boundary_tests` モジュール直下の通常 fn）は同モジュール内なら再利用可能だが、`WvttBox` を組み立てるロジックが埋め込まれているため tx3g 用に別 SampleEntry 初期化の並列ラッパーとして `build_tx3g_fmp4_segments` を独立追加する（3 方式共通ヘルパへの refactor は本 issue のスコープ外。0046 完了後に 3 方式共通で見直す余地がある）。

sample payload は任意のバイト列（例: `b"\x00\x05HELLO"` = `text_length = 5` を u16 BE で表現 + テキスト `"HELLO"`）で十分。3GPP TS 26.245 §5.17.1 に従い先頭 2 バイトは `text_length: u16` を **BE** で書き出す（stpp / wvtt のサンプル payload には長さプレフィックスが無いため、tx3g 特有の invariant として明示する）。本 issue のスコープは `sample_entry` が `SampleEntry::Tx3g(_)` として取り出せることの検証であり、`Fmp4SegmentMuxer` は payload 内部構造を検証しないため（既存 wvtt / stpp と同じ扱い）、modifier boxes を組み立てる必要はない。

### 検証

- `cargo clippy --all-targets --all-features` が warning なしで通る
- `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が warning なしで通る（新規 doc コメントの intra-doc link 検証。CI ではこのコマンドが実行される）
- `cargo test --workspace` がすべて pass する
- cbindgen 出力（`crates/c-api/include/mp4.h`）の diff を確認する（`Mp4SampleEntryTx3g` 構造体・`MP4_SAMPLE_ENTRY_KIND_TX3G` の生成、および `Mp4SampleEntryData` union に `tx3g` メンバが末尾追加されているか）
- 既存の他バリアント（`Avc1` / `Hev1` / `Hvc1` / `Vp08` / `Vp09` / `Av01` / `Opus` / `Mp4a` / `Flac` / `Stpp` / `Wvtt` / `Unknown`）の decode / encode 動作が変わらない（既存 PBT / 単体テストが pass）
- 既存 `subtitle_track_mux_tkhd_via_fmp4_segment_muxer`（stpp を検証）と `_wvtt`（wvtt を検証）が pass（`derive_trak_attributes` 分岐完全化後も stpp / wvtt の挙動は変わらない）

## 解決方法

以下の順で実装する。相互依存で「単独では cargo build が通らない」手順は同一コミット単位でまとめる。途中コミットも `cargo build` が通ることを目安とし、`cargo clippy` / `cargo test` は最終コミット時点で通ることを確認する。

1. `BoxRecord` / `StyleRecord` の 2 record 型を `boxes_sample_entry` に追加（`Encode` / `Decode` を実装、`BaseBox` は実装しない。`Default` を derive）。doc コメントは `` /// [3GPP TS 26.245] BoxRecord (親: [`Tx3gBox`]) `` / `` /// [3GPP TS 26.245] StyleRecord (親: [`Tx3gBox`]) `` 形式に揃える
2. `FontRecord` / `FtabBox` を実装（`Encode` / `Decode` / `BaseBox`。`FontRecord` は record のため `BaseBox` 不要、`FtabBox` は Box のため実装。`FtabBox::TYPE = BoxType::Normal(*b"ftab")` の関連定数も定義）。doc コメントは `` /// [3GPP TS 26.245] FontTableBox (親: [`Tx3gBox`]) `` / `` /// [3GPP TS 26.245] FontRecord (親: [`FtabBox`]) `` 形式に揃える
3. `Tx3gBox` を実装（`Encode` / `Decode` / `BaseBox`。`Tx3gBox::TYPE = BoxType::Normal(*b"tx3g")` と `Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX = NonZeroU16::MIN` の関連定数も定義する）。doc コメントは既存 SampleEntry の形式（半角括弧で終わる）に揃え、`` /// [3GPP TS 26.245] TextSampleEntry class (親: [`StsdBox`][crate::boxes::StsdBox]) `` とする
4. **同一コミット単位で実施**: `SampleEntry::Tx3g(Tx3gBox)` バリアントを追加し、「### `SampleEntry` の網羅 match 箇所」で列挙した 7 箇所（`src/boxes_sample_entry.rs` の 3 箇所 / `crates/c-api/src/boxes.rs` の 2 箇所 / `crates/wasm/src/boxes.rs` の 2 箇所）すべてに arm を追加する。バリアント追加と網羅 match arm 追加を分けるとワークスペースの `cargo build` が通らない（非網羅 match 箇所 `Mp4SampleEntryOwned::new` は手順 6 の C API 露出詳細、`parse_json_mp4_sample_entry` は手順 7 の WASM 露出詳細で扱う。これらは別コミットに分けてもビルドは通る）
5. `Fmp4SegmentMuxer::derive_trak_attributes` の Subtitle 分岐を SampleEntry 種別 match に切り替える（`Stpp` / `Wvtt` / `Tx3g` の 3 arm を明示化し、Media Header を match 内に取り込む。`SampleEntry::Unknown` 経路の防御的 fallback は残す）。同時に doc コメント / インラインコメントを「### `derive_trak_attributes` の分岐追加」節に従って書き換える
6. **同一コミット単位で実施**: 網羅 match 以外の C API 露出詳細を追加する（`Mp4SampleEntryOwned::Tx3g` の match arm 内実装、`Mp4SampleEntryData::tx3g`、`Mp4SampleEntryTx3g` 構造体、`Mp4SampleEntryOwned::new` の Tx3g arm、`Mp4SampleEntryTx3g::to_sample_entry` の実装）。同時に `crates/c-api/examples/demux.c` / `remux.c` の switch も更新する。cbindgen によるヘッダー再生成を `cargo build` 後に確認する
7. WASM 露出詳細を追加する（`crates/wasm/src/boxes_tx3g.rs` を新規作成、`boxes.rs` の 3 関数の arm 内実装）
8. PBT を追加する（`pbt/tests/prop_additional_boxes.rs` に `box_record_roundtrip` / `style_record_roundtrip` / `ftab_box_roundtrip` / `tx3g_box_roundtrip`、`sample_entry_tx3g_methods` / `sample_entry_tx3g_encode_decode_roundtrip`、`arb_box_record` / `arb_style_record` / `arb_font_name` / `arb_font_record` / `arb_ftab_box` / `arb_tx3g_box` strategy 追加）
9. 単体テストを追加する（`build_valid_tx3g_bytes` ヘルパ、必須子欠落 / 各 box_type 誤り / エントリー数超過 / `SampleEntry::decode` の tx3g 経路 / `pbt/tests/prop_error_paths.rs` の `sample_entry_tx3g_inner_box`）
10. `minimal_stsd_box_subtitle` の tx3g 分岐を typed 化する（`SampleEntry::Tx3g` に切り替え。既存 `subtitle_track_via_*_demuxer` テストが typed 経路で pass するようになる）
11. `derive_trak_attributes` の Tx3g 分岐検証テスト（`subtitle_track_mux_tkhd_via_fmp4_segment_muxer_tx3g`）と Fmp4 経路 2 本の Tx3g 検証テスト（`tx3g_sample_entry_via_fmp4_file_demuxer` / `tx3g_sample_entry_via_fmp4_segment_demuxer`）を追加する。サンプルデータを含む合成データを `Fmp4SegmentMuxer` 経由で組み立て、`Sample.sample_entry` から Tx3g を取り出せることを検証する。`Mp4FileDemuxer` 経路のテストは 0046 完了後に別途追加するため本 issue に含めない
12. `cargo clippy --all-targets --all-features` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` / `cargo test --workspace` / cbindgen 出力の diff で検証する

## CHANGES.md

機能単位に以下 2 エントリで記載する（担当者行 `- @ユーザー名` は実装時に補う）。0044 のスタイル（C API / WASM 露出は上位エントリの子項目として書く）に倣う。

- `[CHANGE]` `SampleEntry` に `Tx3g` バリアントを追加する
  - `tx3g` サンプルエントリー（3GPP TS 26.245 `TextSampleEntry`）を型付きで扱えるようにする
  - C API `Mp4SampleEntryKind` に `MP4_SAMPLE_ENTRY_KIND_TX3G` を追加し、`Mp4SampleEntryTx3g` 構造体を新設する
  - WASM の JSON API で `{ "kind": "tx3g", ... }` の入出力に対応する
  - `Fmp4SegmentMuxer::derive_trak_attributes` の Subtitle 分岐を対応表を持つ 3 方式（`Stpp` / `Wvtt` / `Tx3g`）で明示 arm 化し、0042 以来の暫定固定選択を廃止する。tx3g は handler_type = `text`、Media Header = `nmhd` を返す（0042 の対応表通り）。未知の Subtitle 系サンプルエントリー（`SampleEntry::Unknown` 経由）向けの防御的 fallback (`subt` + `sthd`) は維持する
- `[ADD]` 3GPP TS 26.245 の `Tx3gBox` (`tx3g`) と `FtabBox` (`ftab`) を追加する
  - `Tx3gBox` は必須子 `FtabBox` と本体固定 30 バイト（displayFlags / justification / RGBA / BoxRecord / StyleRecord）を持つ
  - `FtabBox` はフォントテーブル（`FontRecord` の可変長配列、各エントリーは font-ID と Pascal-string font-name）を保持する
  - 補助型 `BoxRecord`（i16 × 4）と `StyleRecord`（12 バイト固定）を追加する
  - サンプルデータは 3GPP TS 26.245 §5.17 の `text_length` + テキスト + 任意 modifier boxes を生バイト列として扱う
