# wvtt (WVTTSampleEntry) サンプルエントリー対応を追加する

- Priority: Low
- Created: 2026-07-21
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-subtitle-wvtt
- Polished: 2026-07-23

## 目的

WebVTT を ISO BMFF に格納する `wvtt` サンプルエントリー（`WVTTSampleEntry`、ISO/IEC 14496-30）の decode / encode 対応を追加する。HLS の fMP4 プロファイルと DASH で現役の標準であり、Web 系プレイヤーとの親和性が最も高い。

本 issue は「WebVTT 設定テキストと cue ボックス列を格納するコンテナ」の対応であって、cue 内部（timestamp / settings / payload text）の型付きパースは行わない。サンプルデータは不透明バイト列として扱う（詳細は「### サンプルデータの扱い方針」節を参照）。

## 優先度根拠

Low。バグ由来ではなく緊急要求も無い。0043 / 0045 と並ぶ字幕方式追加の 1 つで、依存元 0042（closed）と依存先 0046（open）もいずれも Low のため、格上げする根拠が無い。

## 現状

字幕トラックの共通基盤は 0042 で整備済み（`CHANGES.md` の `## develop` セクション参照）。stpp サンプルエントリーは 0043 で追加済み。以下が既に利用可能:

- `src/basic_types.rs:684-690` `TrackKind::Subtitle`
- `src/boxes_moov_tree.rs:926-929` `HdlrBox::HANDLER_TYPE_SUBT` (`subt`) / `HdlrBox::HANDLER_TYPE_TEXT` (`text`)
- `src/boxes_moov_tree.rs:1086-1150` `MediaHeader` enum（`Smhd` / `Vmhd` / `Sthd` / `Nmhd` の 4 バリアント）
- `src/boxes_moov_tree.rs:1293-1347` `SthdBox`
- `src/boxes_moov_tree.rs:1349-1404` `NmhdBox`
- `src/boxes_moov_tree.rs:987-999` `MinfBox::media_header: Option<MediaHeader>`
- `src/boxes_sample_entry.rs:17-29` `SampleEntry` enum に `Stpp(StppBox)` バリアントが追加済み（0043）
- `src/boxes_sample_entry.rs:1913-2005` `StppBox` 実装（本 issue の参考実装）
- `src/demux_mp4_file.rs:511-517` / `src/demux_fmp4_file.rs:320-326` / `src/demux_fmp4_segment.rs:145-151` で `subt` / `text` を `TrackKind::Subtitle` にマップする分岐
- `src/mux_fmp4_segment.rs:953-996` `derive_trak_attributes` で `TrackKind::Subtitle` の暫定固定選択（`subt` + `sthd`）。stpp の対応表と一致するため 0043 では未分岐のまま（`src/mux_fmp4_segment.rs:955-958, 981-987` の doc / インラインコメントが陳腐化するため本 issue で更新対象）
- `src/mux_mp4_file.rs:557-627` `Mp4FileMuxer::append_sample` は `MuxError::UnsupportedTrackKind` で Subtitle を拒否（本 issue の範囲では変更しない。受け入れは 0046 で対応）

一方、方式固有のサンプルエントリーとしては `wvtt` は未実装で、`wvtt` box_type のサンプルエントリーは `SampleEntry::Unknown` にフォールバックしている。

- `src/boxes_sample_entry.rs:127-144` `SampleEntry::decode` で `wvtt` は `Unknown` へフォールバック
- `pbt/tests/prop_container_boxes.rs:211-230` `minimal_stsd_box_subtitle` は 3 方式（stpp / wvtt / tx3g）のうち `stpp` のみ型付き `SampleEntry::Stpp`、`wvtt` / `tx3g` は `SampleEntry::Unknown` で組み立てられる（0043 完了時の判断）
- `pbt/tests/prop_container_boxes.rs:712-718` `subtitle_scheme_matrix` は 3 組すべてを回している（3 経路のデマルチプレクサテスト用）

### `SampleEntry` の網羅 match 箇所（バリアント追加で必ずコンパイル修正が必要）

以下は網羅 match のためコンパイルエラーで検出される（0043 で `Stpp` 追加後の行番号）。

- `src/boxes_sample_entry.rs:92-106` `SampleEntry::inner_box`
- `src/boxes_sample_entry.rs:109-125` `impl Encode for SampleEntry`
- `src/boxes_sample_entry.rs:127-144` `impl Decode for SampleEntry`（`Unknown` フォールバックは残す。`Wvtt` は `WvttBox::TYPE` の arm を明示追加）
- `crates/c-api/src/boxes.rs:230-495` `Mp4SampleEntryOwned::to_mp4_sample_entry`
- `crates/c-api/src/boxes.rs:585-620` `Mp4SampleEntry::to_sample_entry`（10 バリアントを `_` フォールバック無しで網羅列挙している。Wvtt 追加で必ずコンパイルエラーになる）
- `crates/wasm/src/boxes.rs:9-51` `fmt_json_mp4_sample_entry`
- `crates/wasm/src/boxes.rs:144-182` `mp4_sample_entry_free`

### `SampleEntry` の非網羅 match 箇所（`_` フォールバックあり。arm 追加は任意だが挙動を確認）

以下は `_` フォールバックが利くためコンパイルは通るが、Subtitle 用の挙動を明示するかを実装時に判定する。

- `src/boxes_sample_entry.rs:35-42` `audio_channel_count`（fallback で `None`。Wvtt も `None` で正しいため arm を追加しない）
- `src/boxes_sample_entry.rs:56-63` `audio_sample_rate`（同上）
- `src/boxes_sample_entry.rs:68-75` `audio_sample_size`（同上）
- `src/boxes_sample_entry.rs:80-90` `video_resolution`（同上）
- `src/mux_fmp4_segment.rs:1002-1011` `extract_video_dimensions`（fallback で `(0, 0)`。Wvtt も同じで正しい）
- `src/mux_mp4_file.rs:724-758` / `src/mux_fmp4_segment.rs:503-540` の `build_final_ftyp_box` / `build_ftyp`（fallback で追加ブランドなし。字幕系ブランドの追加方針は「### `compatible_brands` の方針」節を参照）
- `crates/c-api/src/boxes.rs:119-228` `Mp4SampleEntryOwned::new`（現状 `_ => None` で Unknown を C API に露出しない設計。Wvtt arm を追加して Some を返す）
- `crates/wasm/src/boxes.rs:55-134` `parse_json_mp4_sample_entry`（末尾で不明 kind をエラーとする形。`"wvtt"` の arm を明示追加）
- `pbt/tests/prop_mux_demux.rs:583-586, 886-894` `TrackKind::Subtitle => unreachable!("字幕トラックは本テストの対象外")`（本 issue の範囲では unreachable! のまま維持）

## 設計方針

ISO/IEC 14496-30 §7.5 に従い、`WVTTSampleEntry` (`wvtt`) と必須子 `WebVTTConfigurationBox` (`vttC`) を追加する。参照する版は ISO/IEC 14496-30:2014（第 1 版）を基準とする（0043 と揃える）。本 issue で追加する `vttC` は第 1 版時点で定義済み。任意子（`vlab` / `btrt`）はいずれも本 issue で型付き実装しない（0043 の子ボックス方針と揃える）。

### `WvttBox` のバイナリレイアウト

`WVTTSampleEntry` は `PlainTextSampleEntry` を継承し、その先の `SampleEntry` から以下のヘッダーを引き継ぐ:

- 6 bytes reserved（`0u8; 6`）
- `data_reference_index: u16`（`NonZeroU16` 相当）

その直後に子ボックスが並ぶ:

1. `vttC` (WebVTTConfigurationBox): 必須。「### `VttCBox` のバイナリレイアウト」節参照
2. その他任意子（`vlab` / `btrt` 等）: すべて `unknown_boxes` に集約（「### 子ボックスの扱い」節参照）

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WvttBox {
    /// データ参照インデックス（`dref` 内のエントリーを 1-based で指す）
    pub data_reference_index: NonZeroU16,
    /// 必須の WebVTT 設定ボックス
    pub vttc_box: VttCBox,
    /// 型付き実装を持たない任意の子ボックス（`vlab` / `btrt` 等）
    pub unknown_boxes: Vec<UnknownBox>,
}

impl WvttBox {
    /// ボックス種別（`wvtt` は全て小文字。子の `vttC` の末尾大文字と混同しないこと）
    pub const TYPE: BoxType = BoxType::Normal(*b"wvtt");
    /// [`WvttBox::data_reference_index`] のデフォルト値
    pub const DEFAULT_DATA_REFERENCE_INDEX: NonZeroU16 = NonZeroU16::MIN;
}
```

- `#[derive(...)]` は既存 `StppBox`（`src/boxes_sample_entry.rs:1917`）に揃える。特に `PartialEq` / `Eq` / `Hash` は `resolve_segment_tracks` 内の `known_entry == sample_entry` 比較（`src/mux_fmp4_segment.rs:808`）で必要
- decode 実装では `with_box_type(Self::TYPE, || { ... })` の定型（既存 `StppBox::decode:1961` 参照）で全体を囲む。fn 本体（`Avc1Box::decode` の 281-305 行相当）は、ヘッダー 8 バイト（reserved + data_reference_index）を先読みしたのち、while ループ（`Avc1Box::decode` の 284-294 行相当）で残バイトを BoxHeader 単位に読み進めて `VttCBox::TYPE` を検出したら `vttc_box` に代入、それ以外は `unknown_boxes` に落とす
- 必須子は `check_mandatory_box(vttc_box, "vttC", "wvtt")?` で担保する（既存 `Avc1Box::decode:299` は `"avcc"` の小文字表記だが、`vpcC` を扱う既存箇所（`vpcC` パターン）に倣い実際の box_type 表記 `vttC` をそのまま渡す）

### `VttCBox` のバイナリレイアウト

`WebVTTConfigurationBox` は追加ペイロードとして `unsigned int(8) config[]` を持つ（BoxHeader の残バイトを 1 個の UTF-8 テキストとして扱う）。null 終端 **ではない**（サイズは box_size から一意に決まる）。既存 `Utf8String`（`src/basic_types.rs:518-582`）は null 終端前提のため流用できない。代わりに以下を採用する:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VttCBox {
    /// WebVTT 設定テキスト（"WEBVTT" 行で始まる UTF-8 文字列。null 終端なし、box payload 全体）
    pub config: String,
}

impl VttCBox {
    /// ボックス種別（末尾大文字 C に注意。`vpcC` と同じ Configuration Box 慣習）
    pub const TYPE: BoxType = BoxType::Normal(*b"vttC");
}
```

- 内部型は `String` を採用（UTF-8 有効性を型システムで担保）。`Vec<u8>` にはしない（consumer 側が毎回 `str::from_utf8` する運用は本ライブラリの他 API と一貫しない）
- `#[derive(...)]` は `WvttBox` と同じセットに揃える
- FullBox ではない（バージョン / フラグを持たない）
- `Encode::encode` は BoxHeader を書いたのち `self.config.as_bytes()` を書き出すのみ（null 終端は書き出さない）
- `Decode::decode` も `WvttBox` 同様 `with_box_type(Self::TYPE, || { ... })` の定型で全体を囲む。BoxHeader を先読みし、残バイトを `String::from_utf8(payload.to_vec())` で復元する。UTF-8 として不正な場合は `Err(Error::invalid_input(format!("vttC.config: {e}")))` を返す（`StppBox::decode:1972` の `format!("stpp.namespace: {e}")` と同じ Display 指定子・接頭辞パターンに揃える）。なお `{e}` 部分の詳細は Stpp（`Utf8String::decode` 由来）と wvtt（`FromUtf8Error` 由来）で異なる文字列になるため、テストでは接頭辞 `"vttC.config"` のみを `contains` で照合する（既存 `stpp_box_invalid_utf8_in_namespace` パターンと同じ）
- `"WEBVTT"` プレフィクス検証は **本 issue のスコープに含めない**（consumer 側の責務。将来必要になれば別 issue で追加）

### 子ボックスの扱い

- ISO/IEC 14496-30 §7.5 の `WVTTSampleEntry` は任意の子ボックスとして `WebVTTSourceLabelBox` (`vlab`) と `BitRateBox` (`btrt`) を持ち得る
- ただし本 issue では **どちらも型付き実装しない**。既存の全 SampleEntry（`Avc1Box` / `StppBox` 等）と同じく `unknown_boxes: Vec<UnknownBox>` に落として保持する（0043 の子ボックス方針と揃える）
- 型付き対応が必要になった場合は別 issue とする（`BtrtBox` の独立実装は複数 SampleEntry の共通対応になるため、`VlabBox` と併せて起票する運用が良い）
- 子ボックスの presence 判定は、BoxHeader の残バイトを while ループで読み進める既存パターン（`Avc1Box::decode` の 284-294 行）に倣う

### `BaseBox::children` の実装

`WvttBox` は必須子 `vttc_box` を持つため、既存の `Avc1Box::children`（`src/boxes_sample_entry.rs:313-319`）と同じ「必須子 + `unknown_boxes`」パターンに揃える:

```rust
fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
    Box::new(
        core::iter::empty()
            .chain(core::iter::once(&self.vttc_box).map(as_box_object))
            .chain(self.unknown_boxes.iter().map(as_box_object)),
    )
}
```

`VttCBox` は子を持たないため、既存 `SmhdBox::children` と同じ空 iterator を返す。

### `SampleEntry::Wvtt` バリアントの追加

既存バリアント命名規則（box_type 4 バイト ASCII → PascalCase 化。`avc1` → `Avc1` / `hev1` → `Hev1` / `vp08` → `Vp08` / `stpp` → `Stpp` 等）に従い `wvtt` → `Wvtt(WvttBox)` を採用する。「### `SampleEntry` の網羅 match 箇所」で列挙した箇所すべてに arm を追加する。

### `derive_trak_attributes` の分岐追加

`src/mux_fmp4_segment.rs:953-996` の `derive_trak_attributes` は現状 `TrackKind::Subtitle` 全体で `subt` + `sthd` を暫定固定選択している（0042 の設計判断）。0043 の doc コメントは「wvtt / tx3g の SampleEntry バリアントが実装された時点で、この分岐を `sample_entry` の種別で細分化し暫定固定選択を除去する」と保留している（`src/mux_fmp4_segment.rs:955-958, 981-987`）。

wvtt の対応表は `wvtt → text + sthd`（0042 の対応表: `issues/closed/0042-add-subtitle-track-common.md:98, 113`）。**stpp（`subt` + `sthd`）と handler_type が異なる**（`text` vs `subt`）ため、本 issue で SampleEntry 種別ごとの分岐を開始する。

分岐追加の実装案（`src/mux_fmp4_segment.rs:988-994` を書き換え）:

```rust
TrackKind::Subtitle => {
    // wvtt は text + sthd、Stpp / Unknown フォールバックは subt + sthd（暫定固定選択。0045 で fallback 除去予定）。
    // tuple 形は 0045 で tx3g が加わる際に MediaHeader::Nmhd(NmhdBox) を持たせるための拡張余地として先取り
    let (handler_type, media_header) = match sample_entry {
        SampleEntry::Wvtt(_) => (HdlrBox::HANDLER_TYPE_TEXT, MediaHeader::Sthd(SthdBox)),
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

- `Stpp(_)` arm は明示せず fallback に含める（対応表が fallback と一致するため。0045 で fallback 除去時に `Stpp(_)` / `Tx3g(_)` の arm を同時に明示化する運用）
- Media Header は wvtt でも `sthd`（0042 の対応表通り）
- 本 issue のコミットで以下 2 箇所を実態に合わせて更新する:
  - doc コメント（`src/mux_fmp4_segment.rs:953-958`）: 現状「wvtt / tx3g の SampleEntry バリアントが実装された時点で、この分岐を `sample_entry` の種別で細分化し暫定固定選択を除去する」を、例えば「tx3g の SampleEntry バリアントが実装された時点で、fallback (subt + sthd) を除去して SampleEntry 種別ごとの分岐に完全置換する（wvtt は本 issue で追加された）」に書き換える
  - インラインコメント（`src/mux_fmp4_segment.rs:981-987`）: 現状 7 行（空 `//` 1 行を含む）のうち末尾 2 行「`// wvtt / tx3g の SampleEntry バリアントが実装された時点で` / `// SampleEntry 種別ごとの分岐に完全置換する`」を「`// tx3g の SampleEntry バリアントが実装された時点で fallback を除去する`」に書き換える。tkhd volume 慣習・width / height の説明・空 `//`・「fallback: stpp と非 wvtt Unknown を含む（tx3g は 0045 で明示化予定）」の 5 行を残す（元の「stpp の対応表はこれと一致する」行は本 issue の fallback 網羅設計に合わせて書き換える）

### C API 露出

以下を追加する。既存 10 バリアント（`Avc1` / `Hev1` / `Hvc1` / `Vp08` / `Vp09` / `Av01` / `Opus` / `Mp4a` / `Flac` / `Stpp`）と同じパターンに揃える。

- `crates/c-api/src/boxes.rs:11-41` `Mp4SampleEntryKind` に `MP4_SAMPLE_ENTRY_KIND_WVTT` を末尾に追加（`#[repr(C)]` の順序位置固定のため末尾追加）
- `crates/c-api/src/boxes.rs:43-117` `Mp4SampleEntryOwned` に `Wvtt { inner: WvttBox }` バリアント追加（`inner` のみ保持し backing storage は持たない。既存 `Stpp` バリアント（108-116 行）の設計と揃える。C 側に渡すポインタは `to_mp4_sample_entry` の中で `inner.vttc_box.config.as_bytes().as_ptr()` / `inner.vttc_box.config.as_bytes().len() as u32` で生成する（既存 Stpp と同じく `&[u8]` 経由で扱う。既存例: `crates/c-api/src/boxes.rs:478` `inner.namespace.get().as_bytes()`、479-480 行に同パターン）。`String` の裏の heap バッファは `inner` が drop / 再代入されるまで有効）
- `crates/c-api/src/boxes.rs:119-228` `Mp4SampleEntryOwned::new` の match に `Wvtt` arm 追加（`_ => None` は残す。Tx3g は 0045 で追加されるまで None を返し続ける）
- `crates/c-api/src/boxes.rs:230-495` `Mp4SampleEntryOwned::to_mp4_sample_entry` の match に `Wvtt` arm 追加
- `crates/c-api/src/boxes.rs:585-620` `Mp4SampleEntry::to_sample_entry` の match に `MP4_SAMPLE_ENTRY_KIND_WVTT => unsafe { self.data.wvtt.to_sample_entry() }` arm 追加（この match は `_` フォールバック無しで網羅列挙のため必ずコンパイルエラー）
- `crates/c-api/src/boxes.rs:502-533` `Mp4SampleEntryData` union に `wvtt: Mp4SampleEntryWvtt` フィールド追加
- 新規 `Mp4SampleEntryWvtt` 構造体を追加（`#[repr(C)]`）。フィールドは以下の 2 個。既存 `Mp4SampleEntryStpp` の `<field>_data` / `<field>_size` 命名パターンに揃える:
  - `config_data: *const u8`
  - `config_size: u32`
- バイト列は null 終端を **含まない**（`inner.vttc_box.config.as_bytes()` の内容そのまま。`.len()` は null 終端バイトを含まない）
- `data_reference_index` は `Mp4SampleEntryWvtt` に含めない（既存 `Mp4SampleEntryStpp` 等と同じく、C API 側では `WvttBox::DEFAULT_DATA_REFERENCE_INDEX` 相当のデフォルト値で復元する運用に揃える）。**この設計により C API 経由の decode → encode で元の `data_reference_index` は失われる**（常に 1 に丸められる。既存 Stpp / Mp4a と同じ制約）。`Mp4SampleEntryWvtt` の doc コメントに明記する（既存 `Mp4SampleEntryStpp` の doc にも同種の情報損失は書かれていないが、本 issue のスコープでは追記しない。必要なら別 issue で一括対応する）
- `Mp4SampleEntryWvtt::config_data` は `String::as_bytes()` の生バイト列で、既存 `Mp4SampleEntryStpp` の `Utf8String` invariant（null 除外）と異なり **interior null を含み得る**。C consumer 側で `strlen` などバイト列内 null をターミネータとみなす API を使うと途中で切れる恐れがあるため、必ず `.config_size` を長さとして利用する旨も doc コメントに明記する
- `impl Mp4SampleEntryWvtt { fn to_sample_entry(self) -> Result<SampleEntry, Mp4Error> { ... } }` を追加。既存 `Mp4SampleEntryStpp::to_sample_entry`（`crates/c-api/src/boxes.rs:1488-1528`）に揃える骨格:

```rust
// size == 0 は空 config として許容する（vttC の "WEBVTT" 必須検証は本 issue のスコープ外）
let config = if self.config_size == 0 {
    String::new()
} else {
    if self.config_data.is_null() {
        return Err(Mp4Error::MP4_ERROR_NULL_POINTER);
    }
    let bytes = unsafe { std::slice::from_raw_parts(self.config_data, self.config_size as usize) };
    std::str::from_utf8(bytes)
        .map(String::from)
        .map_err(|_| Mp4Error::MP4_ERROR_INVALID_INPUT)?
};
// SampleEntry::Wvtt(WvttBox { data_reference_index: WvttBox::DEFAULT_DATA_REFERENCE_INDEX, vttc_box: VttCBox { config }, unknown_boxes: Vec::new() }) を返す
```

- 既存の `to_sample_entry`（例: Hev1 `crates/c-api/src/boxes.rs:808`、Mp4a `1357`、Stpp `1489` 等）はいずれも `std::slice::from_raw_parts` / `std::str::from_utf8` を使っており、`core::*` ではなく `std::*` に揃える（`crates/c-api` は `no_std` ではなく `std` 前提）
- `crates/c-api/build.rs` の cbindgen 経由で `crates/c-api/include/mp4.h` が更新される。`cargo build` 後に該当ヘッダーの diff を確認する（`Mp4SampleEntryWvtt` 構造体・`MP4_SAMPLE_ENTRY_KIND_WVTT` の生成、および `Mp4SampleEntryData` union に `wvtt` メンバが末尾追加されるか。`Mp4SampleEntryWvtt` は 2 フィールドと小さいため union 最大サイズは変わらない見込み）
- `crates/c-api/examples/demux.c:46-71` および `crates/c-api/examples/remux.c:31-55` の `get_sample_entry_kind_name` に `"wvtt (WebVTT)"` の case を追加（両方の examples に同名関数がある）。`print_sample_entry_info` の switch（`demux.c:73-138`）は stpp と同じく default に流すため wvtt 用の case 追加は不要

### WASM 露出

- `crates/wasm/src/boxes.rs:9-51` `fmt_json_mp4_sample_entry` の match に `MP4_SAMPLE_ENTRY_KIND_WVTT` arm 追加
- `crates/wasm/src/boxes.rs:55-134` `parse_json_mp4_sample_entry` の match に `"wvtt"` arm 追加
- `crates/wasm/src/boxes.rs:144-182` `mp4_sample_entry_free` の match に `Wvtt` arm 追加
- `crates/wasm/src/boxes_wvtt.rs` を新規作成。雛形は `crates/wasm/src/boxes_stpp.rs` をベースにする（`fmt_json_mp4_sample_entry_wvtt` / `parse_json_mp4_sample_entry_wvtt` / `mp4_sample_entry_wvtt_free` / `raw_bytes_as_str` の 4 関数）
- JSON スキーマ: `{ "kind": "wvtt", "config": "WEBVTT\n..." }`（`data_reference_index` は C API 露出方針に揃えて JSON にも含めない）
- `parse_json_mp4_sample_entry` の match arm 内実装は既存 stpp arm（`crates/wasm/src/boxes.rs:125-131`）と同じ形で `Mp4SampleEntry { kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_WVTT, data: Mp4SampleEntryData { wvtt } }` を組み立てる
- 文字列は `value.to_member("config")?.required()?.to_unquoted_string_str()?` パターン（既存例: `crates/wasm/src/boxes_stpp.rs:44-47`）で読み取り、書き出しは nojson の `JsonFormatter::member(name, value)` に `&str` を渡す
- 解放処理は `crates/wasm/src/boxes_stpp.rs` の `mp4_sample_entry_stpp_free` パターン（ポインタフィールドの解放処理を持つため必要）。バイト列を WASM メモリに確保する際は `crate::boxes::allocate_and_copy_bytes(bytes)`（`crates/wasm/src/boxes.rs:189-204`）を使う
- `raw_bytes_as_str` は既存 stpp 版（`crates/wasm/src/boxes_stpp.rs:114-120`）と同じシグネチャで実装する。ただし `VttCBox::config: String` は Stpp の `Utf8String` と違い **interior null を許容する** invariant のため、JSON 出力パスでは `nojson::JsonFormatter` の escape に委ねる（`raw_bytes_as_str` の返り値は interior null を含み得る）。nojson が `\0` を RFC 8259 §7 に従い ` ` としてエスケープするかは実装時に確認し、interior null を含む config で `fmt_json` → JSON パーサ復元のラウンドトリップをテストで検証する。エスケープが不十分ならば JSON 出力前の手動エスケープ等の別対応が必要
- `parse_json_mp4_sample_entry_wvtt` は stpp 版と異なり `config` の 1 本のみ扱うため、`allocate_and_copy_bytes` 呼び出しも 1 回で、部分失敗リーク対策の順序制約（stpp 版で採用の「先に全 `&str` を取り出す」パターン）は原理的に不要
- テスト（`test_wvtt_to_json` / `test_json_to_wvtt_and_free`）を `crates/wasm/src/boxes_stpp.rs:122-183` パターンで追加

### `compatible_brands` の方針

`src/mux_fmp4_segment.rs:503-540` `build_ftyp` / `src/mux_mp4_file.rs:724-758` `build_final_ftyp_box` は SampleEntry 種別に応じて compatible_brands を追加する。0044 では字幕系ブランド（`wvtt` / `msubs` 等）は **追加しない**（0042 / 0043 と同じ方針を継続。0046 の「### `build_ftyp` / `compatible_brands`」節でも「本 issue の範囲では追加しない方向を第一候補」とされており、そちらとも整合）。

なお `Brand` は `src/boxes.rs` の `pub struct Brand([u8; 4]);`（tuple struct）で、`ISOM` / `AVC1` 等は associated const（`impl Brand { pub const ISOM: Self = ...; }` 相当）として定義されている。現状 `WVTT` associated const は定義されていない。将来的に追加する場合は 0044 とは独立の別 issue で `Brand::WVTT` 定数の追加と `compatible_brands` のロジック改修を扱う。

### サンプルデータの扱い方針

- 本 issue では **サンプルデータ全体は不透明なバイト列** として扱い、内部構造の parse / build は consumer 側に委ねる
- サンプルデータは WebVTT の cue box 列（ISO/IEC 14496-30 §7.6 の `vttc` / `vtte` / `vtta` 等）で構成される
- 理由: 既存の映像・音声サンプルの扱いと一貫させ、実装スコープを抑えるため。WebVTT の cue 内部（timestamp / settings / payload text）の型付きパースを本ライブラリで持つと WebVTT パーサ依存が発生して no_std / wasm 前提と競合するのも避けたい
- サンプル単位の推奨値は既存の `Sample` 構造体（`src/mux_mp4_file.rs:179-215`、0042 で subtitle 全般に対応済み）で文書化済み（`keyframe = true`、`composition_time_offset = None`）を踏襲する
- 追加で内部構造の型付き対応が必要になった場合は別 issue とする

### 後方互換性への影響

- `SampleEntry` へのバリアント追加は網羅 match を破壊するため SemVer 上の破壊的変更（`CHANGES.md` では `[CHANGE]` を使う。詳細は「## CHANGES.md」節）
- `Unknown` フォールバック（`src/boxes_sample_entry.rs:141`）が残るため、decode 側の未知バリアント互換は維持される
- ただし、これまで `SampleEntry::Unknown { box_type: BoxType::Normal(*b"wvtt"), .. }` として観測されていた wvtt サンプルエントリーが本 issue 完了後は `SampleEntry::Wvtt(_)` として観測される。既存 consumer で `match sample_entry { Unknown(b) if b.box_type == ... => }` のような判定を書いていた場合は影響する
- C API の `Mp4SampleEntryKind` enum に新規バリアントが末尾追加されるため、C 側の switch 網羅性を破壊しうる（追加後の bindgen 出力を利用者側でも取り込む必要がある）
- WASM JSON API に `"wvtt"` kind が追加される（既存の `"avc1"` / `"stpp"` 等と同列）
- `derive_trak_attributes` の Subtitle 分岐に SampleEntry 種別 match が導入されるため、`SampleEntry::Wvtt` を含む Subtitle トラックを mux した際の handler_type が 0043 完了時点の暫定固定選択（`subt`）から `text` へ変わる

## 依存関係

- 0042（`issues/closed/0042-add-subtitle-track-common.md`）は完了済み。以下を利用する:
  - `TrackKind::Subtitle`
  - `HdlrBox::HANDLER_TYPE_TEXT` (`text`) — wvtt 用の handler_type
  - `MediaHeader::Sthd(SthdBox)` — wvtt 用の Media Header（対応表通り）
  - `Fmp4SegmentMuxer::derive_trak_attributes`（`src/mux_fmp4_segment.rs:953-996`）— 本 issue で SampleEntry 種別分岐を追加する起点となる
  - `MuxError::UnsupportedTrackKind` — `Mp4FileMuxer` の拒否経路は本 issue でも維持
- 0043（`issues/closed/0043-add-subtitle-stpp.md`）は完了済み。以下を利用する:
  - `SampleEntry::Stpp(StppBox)` バリアント（`src/boxes_sample_entry.rs:27, 1913-2005`）を参考実装として使う（バイナリレイアウト、`BaseBox::children`、C API / WASM 露出のパターン）
  - 本 issue の変更は 0043 で更新済みの `src/mux_fmp4_segment.rs:953-958, 981-987` の doc / インラインコメントを書き換える（詳細は「### `derive_trak_attributes` の分岐追加」節）
- 0046（`issues/0046-add-mp4-file-muxer-subtitle.md`、open）は「`Mp4FileMuxer` / `Mp4FileDemuxer` 経由の MP4 ラウンドトリップ」検証で前提となる。0046 未完了時は `Fmp4SegmentMuxer` / `Fmp4SegmentDemuxer` 経由の fMP4 ラウンドトリップのみで完了と判断する
- 本 issue は 0045 の依存元ではない（各方式は独立に追加可能）。0045 側の issue 記述の追随更新は本 issue のスコープに含めない（各 issue の refresh は独立に行う）

## 完了条件

### 実装完了

- `VttCBox` の `Encode` / `Decode` / `BaseBox` を実装する（`config: String` フィールドを持ち、BoxHeader 残バイトを UTF-8 として一括読み書きする）
- `WvttBox` の `Encode` / `Decode` / `BaseBox` を実装する（`data_reference_index` / `vttc_box` / `unknown_boxes` フィールドを持ち、必須子は `check_mandatory_box` で担保する）
- `SampleEntry::Wvtt(WvttBox)` バリアントを追加し、「### `SampleEntry` の網羅 match 箇所」で列挙した 7 箇所すべてに arm を追加する
- `derive_trak_attributes` の Subtitle 分岐を SampleEntry 種別 match に切り替え、wvtt arm を追加する（対応表: `text` + `sthd`）。doc / インラインコメントを実態に合わせて更新する
- C API 露出（`Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_WVTT`、`Mp4SampleEntryOwned::Wvtt`、`Mp4SampleEntryData::wvtt`、`Mp4SampleEntryWvtt` 構造体、`Mp4SampleEntryOwned::new` / `to_mp4_sample_entry` / `Mp4SampleEntry::to_sample_entry` の各 arm、`Mp4SampleEntryWvtt::to_sample_entry` 実装）を追加する
- WASM 露出（`crates/wasm/src/boxes.rs` の 3 関数の arm、`crates/wasm/src/boxes_wvtt.rs` 新規作成）を追加する
- `crates/c-api/examples/demux.c` および `remux.c` の `get_sample_entry_kind_name` に `"wvtt (WebVTT)"` の case を追加する

### PBT 追加

以下のテストを `pbt/tests/prop_additional_boxes.rs` に追加する（既存の `stpp_box_roundtrip`（518-533 行）と同じファイルに揃える）:

- `vttc_box_roundtrip`: `VttCBox` の decode / encode ラウンドトリップ（proptest ブロック内で以下パターンを網羅）:
  - 最小 config（`"WEBVTT"`）
  - 複数行 config（`"WEBVTT\n\nSTYLE\n..."` 等の設定を含む）
  - UTF-8 マルチバイト文字を含む config
  - 空 config（パーサ堅牢性のため）
- `wvtt_box_roundtrip`: `WvttBox` の decode / encode ラウンドトリップ（proptest ブロック内で以下パターンを網羅）:
  - `vttC` のみ（最小構成）
  - `vttC` + `unknown_boxes`（0 〜 3 個の任意子）
- strategy の実装:
  - `arb_wvtt_config() -> impl Strategy<Value = String>`: `VttCBox::config` は interior null と改行を許容するため、null / 改行の両方をカバーする regex を採用する。proptest 内部の `regex_syntax` は既定で `.` が `\n` を除外するため、`.{0,100}` のような単純な dot 表記だと改行経路が生成されず「### PBT 追加」節冒頭の「複数行 config」パターンをカバーできない。改行 + null を両方生成するには dotall フラグ付き `"(?s).{0,100}"` または任意の非制御文字を明示する `"[\\s\\S]{0,100}"` を採用する
  - `arb_vttc_box() -> impl Strategy<Value = VttCBox>`
  - `arb_wvtt_box() -> impl Strategy<Value = WvttBox>`（既存 `arb_stpp_box`（267-282 行）のパターンに揃え、`arb_unknown_box` を使って `unknown_boxes` を混ぜる）
- 独立関数として `sample_entry_wvtt_methods` / `sample_entry_wvtt_encode_decode_roundtrip` を新設する（既存 stpp パターン `pbt/tests/prop_additional_boxes.rs:1123-1158` に揃え、`SampleEntry::Wvtt` の固定インスタンスで検証）

### 単体テスト追加

以下の単体テストを `pbt/tests/prop_additional_boxes.rs` に追加する（既存の stpp 用単体テスト群 1188-1312 行と同じファイルに揃える）:

- `build_valid_wvtt_bytes(config: &[u8]) -> Vec<u8>` ヘルパを追加（`build_valid_stpp_bytes`（1164-1186 行）と同様）。BoxHeader 8 バイト + 6 bytes reserved + `data_reference_index = 1u16` 固定 + `vttC` 子ボックス（BoxHeader 8 バイト + `config` バイト列）の入れ子で組み立てる
- `vttc_box_decode_valid_bytes`: 有効なバイト列で組み立てて decode できることを確認する（`config = "WEBVTT"`）
- `vttc_box_invalid_utf8_config`: `vttC` の payload に UTF-8 として不正なバイト列（例: `[0xff, 0xfe]`）を渡すと `Err` が返る（`ErrorKind::InvalidInput`、メッセージに `"vttC.config"` を含む）
- `vttc_box_decode_wrong_box_type`: `VttCBox::decode` に `vttC` 以外の box_type を持つバイト列を渡すとエラー
- `wvtt_box_missing_vttc`: `wvtt` payload に `vttC` 子ボックスが無い場合に `Err` が返る（必須子欠落エラー、`check_mandatory_box` 経由）
- `wvtt_box_decode_wrong_box_type`: `WvttBox::decode` に `wvtt` 以外の box_type を持つバイト列を渡すとエラー
- `sample_entry_decode_wvtt_dispatches_to_wvtt_variant`: `SampleEntry::decode` で `wvtt` box_type を持つ入力が `SampleEntry::Wvtt(_)` として取り出される（0044 完了以前は `Unknown` にフォールバックしていた挙動の回帰確認）

`pbt/tests/prop_error_paths.rs:378-658` の `sample_entry_inner_box_tests` モジュールに `sample_entry_wvtt_inner_box` を追加する（既存 `sample_entry_stpp_inner_box` は 632-644 行）。既存 `sample_entry_stpp_inner_box` パターン（`box_type()` と `is_unknown_box()` の検証）に加えて、Wvtt は必須の型付き子ボックス `vttc_box` を持つため `assert_eq!(entry.children().count(), 1)` を検証する。テスト用インスタンスは `SampleEntry::Wvtt(WvttBox { data_reference_index: WvttBox::DEFAULT_DATA_REFERENCE_INDEX, vttc_box: VttCBox { config: String::from("WEBVTT") }, unknown_boxes: vec![] })` で作成する。

### 既存テストの更新

- `pbt/tests/prop_container_boxes.rs:211-230` `minimal_stsd_box_subtitle` の wvtt 分岐は **`SampleEntry::Unknown` のまま維持する**（0045 完了までの Unknown フォールバック経路の互換性担保のため。3 方式が揃った時点で Stpp / Wvtt / Tx3g それぞれに置換するかを 0045 側で最終判断する）。合わせて同ヘルパ 205-210 行の doc コメント記述「未実装の wvtt / tx3g は Unknown フォールバックのままにする」を「型付き実装のある wvtt も Unknown フォールバック経路担保のため意図的に Unknown で作成する。未実装の tx3g は Unknown のまま」に書き換える
- `pbt/tests/prop_container_boxes.rs:787-876` `subtitle_track_via_*_demuxer`（3 経路のマトリクス）も Unknown 経路のまま維持する（fallback 経路の担保として）
- `pbt/tests/prop_container_boxes.rs:886-945` `subtitle_track_mux_tkhd_via_fmp4_segment_muxer`（`SampleEntry::Stpp` を使用）は変更しない

### `derive_trak_attributes` の Wvtt 分岐検証テスト追加

`pbt/tests/prop_container_boxes.rs` の `subtitle_track_mux_tkhd_via_fmp4_segment_muxer`（886-945 行）と同構造の新規テストを追加する:

- `subtitle_track_mux_tkhd_via_fmp4_segment_muxer_wvtt`: `SampleEntry::Wvtt(WvttBox { ..., vttc_box: VttCBox { config: "WEBVTT".to_owned() }, ... })` を持つ Sample を `Fmp4SegmentMuxer` に渡し、生成された moov 内 trak について以下を検証する:
  - `handler_type == HdlrBox::HANDLER_TYPE_TEXT`（wvtt 用の text）
  - `media_header == Some(MediaHeader::Sthd(SthdBox))`（wvtt 用の sthd）
  - tkhd 属性（`volume = TkhdBox::DEFAULT_VIDEO_VOLUME`、`width = 0`、`height = 0`）は stpp 版と同じ

### Fmp4 経路 2 本の Wvtt 検証と合成ラウンドトリップ

`TrackInfo`（`src/demux_mp4_file.rs:53-70`）は `sample_entries` フィールドを持たない。`SampleEntry` を取り出すには `Sample.sample_entry: Option<&SampleEntry>`（`src/demux_mp4_file.rs:84`）から取得する必要があるため、init segment / moov だけでなく media segment / mdat + サンプルデータを含む合成データを組み立てる必要がある。

以下 2 経路の Wvtt 正常経路担保テストを `pbt/tests/prop_container_boxes.rs` に追加する（既存 `stpp_sample_entry_via_*_demuxer`（992-1060 行）の隣）:

- `wvtt_sample_entry_via_fmp4_file_demuxer`: `SampleEntry::Wvtt(_)` を持つ init + moof + mdat の合成バイト列を組み立て、`Fmp4FileDemuxer` から取り出した `sample.sample_entry` が `Some(SampleEntry::Wvtt(_))` にマッチすることを検証
- `wvtt_sample_entry_via_fmp4_segment_demuxer`: init segment + media segment の合成バイト列を組み立て、`Fmp4SegmentDemuxer::handle_media_segment` の戻り値の最初の Sample.sample_entry を検証（既存 stpp テスト `1053-1059` と揃える）

`Mp4FileDemuxer` 経路（`wvtt_sample_entry_via_mp4_file_demuxer`）は本 issue のスコープに含めない（`Mp4FileMuxer` が Subtitle を拒否する現状では合成データを Muxer 経由で吐かせられないため。0046 完了後に別途追加。`issues/0046-add-mp4-file-muxer-subtitle.md:102-107` の「本 issue 完了後の追随タスク」節を参照）。

Fmp4 経路 2 本の合成は `Fmp4SegmentMuxer`（0042 で Subtitle 受け入れ済み）を経由して組み立てる。既存 `pbt/tests/prop_container_boxes.rs:955-990` `build_stpp_fmp4_segments` は `proptest!` ブロック内の fn として閉じ込められているためモジュール外から参照できず、本 issue でも `build_wvtt_fmp4_segments` を独立コピーして wvtt 用に調整する。

sample payload は任意のバイト列（例: `b"WEBVTT-cue-payload-placeholder"`）で十分。本 issue のスコープは `sample_entry` が `SampleEntry::Wvtt(_)` として取り出せることの検証であり、`Fmp4SegmentMuxer` は payload 内部構造を検証しないため（既存 `build_stpp_fmp4_segments` も TTML 断片を任意バイト列として扱う）、`vttc` / `payl` の BoxHeader を組み立てる必要はない。

### 検証

- `cargo clippy --all-targets --all-features` が warning なしで通る
- `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が warning なしで通る（新規 doc コメントの intra-doc link 検証。CI ではこのコマンドが実行される）
- `cargo test --workspace` がすべて pass する
- cbindgen 出力（`crates/c-api/include/mp4.h`）の diff を確認する（`Mp4SampleEntryWvtt` 構造体・`MP4_SAMPLE_ENTRY_KIND_WVTT` の生成、および `Mp4SampleEntryData` union に `wvtt` メンバが末尾追加されているか）
- 既存の他バリアント（`Avc1` / `Hev1` / `Hvc1` / `Vp08` / `Vp09` / `Av01` / `Opus` / `Mp4a` / `Flac` / `Stpp` / `Unknown`）の decode / encode 動作が変わらない（既存 PBT / 単体テストが pass）
- 既存 `subtitle_track_mux_tkhd_via_fmp4_segment_muxer`（stpp を検証）が pass（`derive_trak_attributes` 分岐追加後も stpp の挙動は変わらない）

## 解決方法

以下の順で実装する見込み。相互依存で「単独では cargo build が通らない」手順は同一コミット単位でまとめる。途中コミットも `cargo build` が通ることを目安とし、`cargo clippy` / `cargo test` は最終コミット時点で通れば良い。

1. `VttCBox` を実装（`Encode` / `Decode` / `BaseBox`。`config: String` フィールド、`String::from_utf8` での decode、`as_bytes()` での encode）。doc コメントは既存の SampleEntry の子ボックス形式（半角括弧で終わる、`AvccBox` / `OpusSpecificBox` を参考）に揃え、`` /// [ISO/IEC 14496-30] WebVTTConfigurationBox class (親: [`WvttBox`]) `` とする
2. `WvttBox` を実装（`Encode` / `Decode` / `BaseBox`。`WvttBox::TYPE = BoxType::Normal(*b"wvtt")` と `WvttBox::DEFAULT_DATA_REFERENCE_INDEX = NonZeroU16::MIN` の関連定数も定義する）。doc コメントは既存 SampleEntry の形式（半角括弧で終わる）に揃え、`` /// [ISO/IEC 14496-30] WVTTSampleEntry class (親: [`StsdBox`][crate::boxes::StsdBox]) `` とする（既存 `Avc1Box` / `StppBox` を参考にする）
3. **同一コミット単位で実施**: `SampleEntry::Wvtt(WvttBox)` バリアントを追加し、「### `SampleEntry` の網羅 match 箇所」で列挙した 7 箇所すべて（`src/boxes_sample_entry.rs` の 3 箇所 / `crates/c-api/src/boxes.rs` の 2 箇所 / `crates/wasm/src/boxes.rs` の 2 箇所）に arm を追加する。バリアント追加と網羅 match arm 追加を分けるとワークスペースの `cargo build` が通らない
4. `Fmp4SegmentMuxer::derive_trak_attributes` の Subtitle 分岐を SampleEntry 種別 match に切り替える（wvtt arm 追加、fallback は残す）。同時に doc コメント / インラインコメントを「### `derive_trak_attributes` の分岐追加」節に従って書き換える
5. **同一コミット単位で実施**: 網羅 match 以外の C API 露出詳細を追加する（`Mp4SampleEntryOwned::Wvtt` の match arm 内実装、`Mp4SampleEntryData::wvtt`、`Mp4SampleEntryWvtt` 構造体、`Mp4SampleEntryOwned::new` の Wvtt arm、`Mp4SampleEntryWvtt::to_sample_entry` の実装）。同時に `crates/c-api/examples/demux.c` / `remux.c` の switch も更新する。cbindgen によるヘッダ再生成を `cargo build` 後に確認する
6. WASM 露出詳細を追加する（`crates/wasm/src/boxes_wvtt.rs` を新規作成、`boxes.rs` の 3 関数の arm 内実装。`boxes_stpp.rs` を雛形にする）
7. PBT を追加する（`pbt/tests/prop_additional_boxes.rs` に `vttc_box_roundtrip` / `wvtt_box_roundtrip`、`sample_entry_wvtt_methods` / `sample_entry_wvtt_encode_decode_roundtrip`、`arb_wvtt_config` / `arb_vttc_box` / `arb_wvtt_box` strategy 追加）
8. 単体テストを追加する（`build_valid_wvtt_bytes` ヘルパ、UTF-8 不正 / vttC 以外の box_type / 必須子欠落 / `SampleEntry::decode` の wvtt 経路 / `pbt/tests/prop_error_paths.rs` の `sample_entry_wvtt_inner_box`）
9. `derive_trak_attributes` の Wvtt 分岐検証テスト（`subtitle_track_mux_tkhd_via_fmp4_segment_muxer_wvtt`）と Fmp4 経路 2 本の Wvtt 検証テスト（`wvtt_sample_entry_via_fmp4_file_demuxer` / `wvtt_sample_entry_via_fmp4_segment_demuxer`）を追加する。サンプルデータを含む合成データを `Fmp4SegmentMuxer` 経由で組み立て、`Sample.sample_entry` から Wvtt を取り出せることを検証する。`Mp4FileDemuxer` 経路のテストは 0046 完了後に別途追加するため本 issue に含めない
10. `cargo clippy --all-targets --all-features` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` / `cargo test --workspace` / cbindgen 出力の diff で検証する

## CHANGES.md

機能単位に以下 2 エントリで記載する（担当者行 `- @ユーザー名` は実装時に補う）。0043 のスタイル（C API / WASM 露出は上位エントリの子項目として書く）に倣う。

- `[CHANGE]` `SampleEntry` に `Wvtt` バリアントを追加する
  - `wvtt` サンプルエントリー（ISO/IEC 14496-30 `WVTTSampleEntry`）を型付きで扱えるようにする
  - C API `Mp4SampleEntryKind` に `MP4_SAMPLE_ENTRY_KIND_WVTT` を追加し、`Mp4SampleEntryWvtt` 構造体を新設する
  - WASM の JSON API で `{ "kind": "wvtt", ... }` の入出力に対応する
- `[ADD]` ISO/IEC 14496-30 の `WvttBox` (`wvtt`) と `VttCBox` (`vttC`) を追加する
  - `WvttBox` は必須子 `VttCBox` を持つ
  - `VttCBox` は WebVTT 設定テキスト（`"WEBVTT"` で始まる UTF-8 文字列。null 終端なし、box payload 全体）を保持する
  - サンプルデータは WebVTT の cue box 列（`vttc` / `vtte` / `vtta` 等）を不透明バイト列として扱う
  - `Fmp4SegmentMuxer::derive_trak_attributes` の Subtitle 分岐に wvtt arm（`text` + `sthd`）を追加する（0042 の暫定固定選択 `subt` + `sthd` からの細分化を開始）
