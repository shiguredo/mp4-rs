# stpp (XMLSubtitleSampleEntry) サンプルエントリー対応を追加する

- Priority: Low
- Created: 2026-07-21
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-subtitle-stpp
- Polished: 2026-07-22

## 目的

XML 形式の字幕を格納する `stpp` サンプルエントリー（`XMLSubtitleSampleEntry`、ISO/IEC 14496-30）の decode / encode 対応を追加する。DASH の IMSC / TTML 系ワークフローで実装標準となっており、broadcast / OTT 系のファイルで実際に使われている。

本 issue は「XML ドキュメントを格納するコンテナ」の対応であって、XML そのものの方言（TTML / IMSC / EBU-TT / SMPTE-TT 等）は問わない。サンプルデータは不透明バイト列として扱う（詳細は「### サンプルデータの扱い方針」節を参照）。

## 優先度根拠

Low。バグ由来ではなく緊急要求も無い。0044 / 0045 と並ぶ字幕方式追加の 1 つで、依存元 0042（closed）と依存先 0046（open）もいずれも Low のため、格上げする根拠が無い。

## 現状

字幕トラックの共通基盤は 0042 で整備済み（`CHANGES.md` の `## develop` セクション参照）。以下が既に利用可能:

- `src/basic_types.rs:677-691` `TrackKind::Subtitle`
- `src/boxes_moov_tree.rs:926-929` `HdlrBox::HANDLER_TYPE_SUBT` (`subt`) / `HdlrBox::HANDLER_TYPE_TEXT` (`text`)
- `src/boxes_moov_tree.rs:1088-1150` `MediaHeader` enum（`Smhd` / `Vmhd` / `Sthd` / `Nmhd` の 4 バリアント）
- `src/boxes_moov_tree.rs:1297-1347` `SthdBox`
- `src/boxes_moov_tree.rs:1349-` `NmhdBox`
- `src/boxes_moov_tree.rs:987-999` `MinfBox::media_header: Option<MediaHeader>`
- `src/demux_mp4_file.rs:511-515` / `src/demux_fmp4_file.rs:320-324` / `src/demux_fmp4_segment.rs:145-149` で `subt` / `text` を `TrackKind::Subtitle` にマップする分岐
- `src/mux_fmp4_segment.rs:953-993` `derive_trak_attributes` で `TrackKind::Subtitle` の暫定固定選択（`subt` + `sthd`）
- `src/mux_mp4_file.rs:557-572, 620-627` `Mp4FileMuxer::append_sample` は `MuxError::UnsupportedTrackKind` で Subtitle を拒否（本 issue の範囲では変更しない。受け入れは 0046 で対応）

一方、方式固有のサンプルエントリーは未実装で、`stpp` box_type のサンプルエントリーは `SampleEntry::Unknown` にフォールバックしている。

- `src/boxes_sample_entry.rs:17-28` `SampleEntry` enum に `Stpp` バリアントは存在しない
- `src/boxes_sample_entry.rs:124-140` `SampleEntry::decode` で `stpp` は `Unknown` へフォールバック
- `pbt/tests/prop_container_boxes.rs:210-218` `minimal_stsd_box_subtitle` は 3 方式（stpp / wvtt / tx3g）とも `SampleEntry::Unknown` で組み立てられ、0042 のラウンドトリップは Unknown 経路で検証されている

### `SampleEntry` の網羅 match 箇所（バリアント追加で必ずコンパイル修正が必要）

以下は網羅 match のためコンパイルエラーで検出される。

- `src/boxes_sample_entry.rs:91-104` `SampleEntry::inner_box`
- `src/boxes_sample_entry.rs:107-121` `impl Encode for SampleEntry`
- `src/boxes_sample_entry.rs:124-140` `impl Decode for SampleEntry`（`Unknown` フォールバックは残す。`Stpp` は `StppBox::TYPE` の arm を明示追加）
- `crates/c-api/src/boxes.rs:217-` `Mp4SampleEntryOwned::to_mp4_sample_entry`
- `crates/c-api/src/boxes.rs:550-582` `Mp4SampleEntry::to_sample_entry`（`Mp4SampleEntryKind` の 9 バリアントを `_` フォールバック無しで網羅列挙している。Stpp 追加で必ずコンパイルエラーになる）
- `crates/wasm/src/boxes.rs:9-47` `fmt_json_mp4_sample_entry`
- `crates/wasm/src/boxes.rs:132-167` `mp4_sample_entry_free`

### `SampleEntry` の非網羅 match 箇所（`_` フォールバックあり。arm 追加は任意だが挙動を確認）

以下は `_` フォールバックが利くためコンパイルは通るが、Subtitle 用の挙動を明示するかを実装時に判定する。

- `src/boxes_sample_entry.rs:34-41` `audio_channel_count`（fallback で `None`。Stpp も `None` で正しいため arm を追加しない）
- `src/boxes_sample_entry.rs:55-62` `audio_sample_rate`（同上）
- `src/boxes_sample_entry.rs:67-74` `audio_sample_size`（同上）
- `src/boxes_sample_entry.rs:79-89` `video_resolution`（同上）
- `src/mux_fmp4_segment.rs:999-1008` `extract_video_dimensions`（fallback で `(0, 0)`。Stpp も同じで正しい）
- `src/mux_mp4_file.rs:724-758` / `src/mux_fmp4_segment.rs:503-540` の `build_final_ftyp_box` / `build_ftyp`（fallback で追加ブランドなし。字幕系ブランドの追加方針は「### `compatible_brands` の方針」節を参照）
- `crates/c-api/src/boxes.rs:108-215` `Mp4SampleEntryOwned::new`（現状 `_ => None` で Unknown を C API に露出しない設計。Stpp arm を追加して Some を返す）
- `crates/wasm/src/boxes.rs:51-122` `parse_json_mp4_sample_entry`（末尾で不明 kind をエラーとする形。`"stpp"` の arm を明示追加）
- `pbt/tests/prop_mux_demux.rs:583-586, 886-894` `TrackKind::Subtitle => unreachable!("字幕トラックは本テストの対象外")`（本 issue の範囲では unreachable! のまま維持）

## 設計方針

ISO/IEC 14496-30 §7.5 に従い、`XMLSubtitleSampleEntry` (`stpp`) を追加する。参照する版は ISO/IEC 14496-30:2014（第 1 版）を基準とする。本 issue で追加する 3 本体フィールド（`namespace` / `schema_location` / `auxiliary_mime_types`）はすべて必須（第 1 版時点で定義済み）。任意なのは子ボックス（`btrt` / `m4ds` 等）で、これらは「### 子ボックスの扱い」節に集約する。

### `StppBox` のバイナリレイアウト

`XMLSubtitleSampleEntry` は `PlainTextSampleEntry` を継承し、その先の `SampleEntry` から以下のヘッダーを引き継ぐ:

- 6 bytes reserved（`0u8; 6`）
- `data_reference_index: u16`（`NonZeroU16` 相当）

その直後に本体フィールドが以下の順で並ぶ:

1. `namespace`: null 終端 UTF-8 文字列。XML 名前空間 URI をスペース区切りで連結した文字列を単一 `Utf8String` として保持する（consumer 側で split する運用）。仕様上は非空前提だが、パーサの堅牢性のため空文字列（`\0` 1 バイト）も受け入れる
2. `schema_location`: null 終端 UTF-8 文字列（常に書き出す。空なら `\0` 1 バイト）
3. `auxiliary_mime_types`: null 終端 UTF-8 文字列（常に書き出す。空なら `\0` 1 バイト）

その後に任意の子ボックスが続く（下記「### 子ボックスの扱い」節）。

Rust 側の型は既存の null 終端文字列パターン（`src/basic_types.rs:518-582` `Utf8String`）に揃える。3 フィールドとも仕様上は必須のため `Option` にはしない（空文字列を `\0` 1 バイトで表現する）。

```rust
pub struct StppBox {
    pub data_reference_index: NonZeroU16,
    pub namespace: Utf8String,
    pub schema_location: Utf8String,
    pub auxiliary_mime_types: Utf8String,
    pub unknown_boxes: Vec<UnknownBox>,
}
```

- `data_reference_index` のデフォルト値は既存 `AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX` / `VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX` に倣って `NonZeroU16::MIN` を用意する。`SubtitleSampleEntryFields` のような共通構造体を切るかどうかは本 issue のスコープに含めない（他 SampleEntry と同じく `StppBox` に独立フィールドとして持つ）
- decode 実装では既存 `Utf8String::decode`（`src/basic_types.rs:558-582`）を利用する。既存実装は null 終端が見つからない場合 `Err(Error::invalid_input("Null-terminated string not found"))`、UTF-8 として不正な場合 `Err(Error::invalid_input(format!("Invalid UTF-8 string: {:?}", ...)))` を返す（いずれも `ErrorKind::InvalidInput`）。`StppBox::decode` では 3 フィールドすべてについて `Utf8String::decode_at(payload, &mut offset).map_err(|e| Error::invalid_input(format!("stpp.{field_name}: {e}")))?` の形で呼び出し、フィールド名を接頭辞として付けつつ元エラーメッセージを保持する（null 終端欠落と UTF-8 不正の 2 種類をメッセージから判別できるように残す）

### 子ボックスの扱い

- ISO/IEC 14496-30 §7.5 の `XMLSubtitleSampleEntry` は任意の子ボックスとして `BitRateBox` (`btrt`) と `MPEG4ExtensionDescriptorsBox` (`m4ds`) を持ち得る
- ただし本 issue では **どちらも型付き実装しない**。既存の全 SampleEntry（`Avc1Box` 等）と同じく `unknown_boxes: Vec<UnknownBox>` に落として保持する（0044 / 0045 と揃える）
- 型付き対応が必要になった場合は別 issue とする（`BtrtBox` の独立実装は複数 SampleEntry の共通対応になるため）
- 子ボックスの presence 判定は、BoxHeader の残バイトを while ループで読み進める既存パターン（`Avc1Box::decode` の 280-289 行）に倣う

### `BaseBox::children` の実装

必須の型付き子ボックスを持たないため、`unknown_boxes` のみを返す。既存の `Avc1Box::children`（`src/boxes_sample_entry.rs:309-315`）は必須子 `avcc_box` を持つ点が異なるが、iterator の組み立て方は同じパターンに揃える:

```rust
fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
    Box::new(core::iter::empty().chain(self.unknown_boxes.iter().map(as_box_object)))
}
```

### `SampleEntry::Stpp` バリアントの追加

既存 `SampleEntry` バリアント命名規則（`Avc1` / `Hev1` / `Opus` 等）に従い `Stpp(StppBox)` を採用する。「### `SampleEntry` の網羅 match 箇所」で列挙した 3 箇所（`inner_box` / `Encode::encode` / `Decode::decode`）と C API / WASM の網羅 match 箇所すべてに arm を追加する。

### `derive_trak_attributes` の doc コメント更新

`src/mux_fmp4_segment.rs:953-993` の `derive_trak_attributes` は現状 `TrackKind::Subtitle` 全体で `subt` + `sthd` を暫定固定選択している（0042 の設計判断）。stpp の対応表も `subt` + `sthd` のため、本 issue では **暫定固定選択そのものは変更しない**。実装として `Stpp(_)` arm を追加しても値は fallback と同一で `clippy::match_same_arms` の発火抑制で複雑度が上がるだけのため、arm 追加は 0044 実装時（wvtt が加わって初めて分岐に意味が生じる）に見送る。

本 issue のコミットで以下 2 箇所の既存コメントのみ更新し、実装意図の陳腐化を防ぐ:

- doc コメント（`src/mux_fmp4_segment.rs:953-956`）: 現状「stpp / wvtt / tx3g が実装され次第、SampleEntry 種別ごとの分岐に完全置換する」を、例えば「wvtt / tx3g が実装され次第、SampleEntry 種別ごとの分岐に完全置換する（stpp は本 issue で追加されたが、対応表が暫定固定選択と同じ `subt` + `sthd` のため置換は 0044 実装時に開始する）」に書き換える
- inline コメント（`src/mux_fmp4_segment.rs:979-984`）: 同じ趣旨で更新する

補足:

- 0042 の対応表の該当行は `stpp → subt + sthd`（`issues/closed/0042-add-subtitle-track-common.md:98, 112-115`）
- 0042 の「暫定 fallback は残さない」要求は 3 方式すべてが揃った最後の issue（想定: 0045）で満たされる（本 issue では満たしきらない）
- `derive_trak_attributes` の実質的な分岐追加は 0044 で開始し、0045 で fallback を除去する運用（0044 / 0045 側の issue 記述の追随更新は本 issue のスコープに含めない。各 issue の refresh は独立に行う）

### C API 露出

以下を追加する。既存 9 バリアント（`Avc1` / `Hev1` / `Hvc1` / `Vp08` / `Vp09` / `Av01` / `Opus` / `Mp4a` / `Flac`）と同じパターンに揃える。

- `crates/c-api/src/boxes.rs:11-38` `Mp4SampleEntryKind` に `MP4_SAMPLE_ENTRY_KIND_STPP` を末尾に追加（`#[repr(C)]` の順序位置固定のため末尾追加）
- `crates/c-api/src/boxes.rs:40-105` `Mp4SampleEntryOwned` に `Stpp { inner: StppBox }` バリアントを追加（`inner` のみ保持し backing storage は持たない。既存 `Vp08` / `Vp09` / `Opus` バリアントの設計と揃える。C 側に渡すポインタは `to_mp4_sample_entry` の中で `inner.namespace.get().as_ptr()` / `inner.namespace.get().len() as u32` で生成する。`Utf8String::get()` は `&str` を返す `src/basic_types.rs:537-539` ため、`inner` が生きている限りポインタは有効）
- `crates/c-api/src/boxes.rs:108-215` `Mp4SampleEntryOwned::new` の match に `Stpp` arm 追加（`_ => None` は残す。Wvtt / Tx3g は 0044 / 0045 で追加されるまで None を返し続ける）
- `crates/c-api/src/boxes.rs:217-` `Mp4SampleEntryOwned::to_mp4_sample_entry` の match に `Stpp` arm 追加
- `crates/c-api/src/boxes.rs:550-582` `Mp4SampleEntry::to_sample_entry` の match に `MP4_SAMPLE_ENTRY_KIND_STPP => unsafe { self.data.stpp.to_sample_entry() }` arm 追加（この match は `_` フォールバック無しで網羅列挙のため必ずコンパイルエラー）
- `crates/c-api/src/boxes.rs:471-498` `Mp4SampleEntryData` union に `stpp: Mp4SampleEntryStpp` フィールド追加
- 新規 `Mp4SampleEntryStpp` 構造体を追加（`#[repr(C)]`）。フィールドは以下の 6 個（`*const u8 + u32` ペア × 3 フィールド分）。既存 `Mp4SampleEntryMp4a.dec_specific_info: *const u8, dec_specific_info_size: u32`（`crates/c-api/src/boxes.rs:438-439` 相当）や `Mp4SampleEntryFlac.streaminfo_data: *const u8, streaminfo_size: u32` と同じ命名パターン `<field>_data` / `<field>_size` に揃える:
  - `namespace_data: *const u8`
  - `namespace_size: u32`
  - `schema_location_data: *const u8`
  - `schema_location_size: u32`
  - `auxiliary_mime_types_data: *const u8`
  - `auxiliary_mime_types_size: u32`
- バイト列は null 終端を **含まない**（`inner.namespace.get().as_bytes()` の内容そのまま。`.len()` は null 終端バイトを含まない）。C 側は `_size` で長さを取得する運用で、`sps_data + sps_sizes` パターンと揃う
- `data_reference_index` は `Mp4SampleEntryStpp` に含めない（既存 `Mp4SampleEntryMp4a` 等と同じく、C API 側では `AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX` 相当のデフォルト値で復元する運用に揃える）
- `impl Mp4SampleEntryStpp { fn to_sample_entry(self) -> Result<SampleEntry, Mp4Error> { ... } }` を追加。C 側から受け取ったポインタから `Utf8String` を復元し、`SampleEntry::Stpp(StppBox { ... })` を組み立てる完全な骨格:

```rust
// unsafe ブロック内でポインタからスライスを取り出す（既存 to_sample_entry は std::slice::from_raw_parts / std::str::from_utf8 で統一されているため、それに合わせる）
let namespace_bytes = unsafe { std::slice::from_raw_parts(self.namespace_data, self.namespace_size as usize) };
let namespace_str = std::str::from_utf8(namespace_bytes)
    .map_err(|_| Mp4Error::from(shiguredo_mp4::Error::invalid_input("invalid UTF-8 in stpp.namespace")))?;
let namespace = Utf8String::new(namespace_str)
    .ok_or_else(|| Mp4Error::from(shiguredo_mp4::Error::invalid_input("stpp.namespace contains NUL byte")))?;

// schema_location / auxiliary_mime_types も同じパターンで復元
let schema_location = /* ... 同様 ... */;
let auxiliary_mime_types = /* ... 同様 ... */;

Ok(SampleEntry::Stpp(StppBox {
    data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
    namespace,
    schema_location,
    auxiliary_mime_types,
    unknown_boxes: Vec::new(),
}))
```

- `Utf8String::new` の実際のシグネチャは `pub fn new(s: &str) -> Option<Self>` （`src/basic_types.rs:529`）。`&str` を受け取り `Option` を返すため、`?` で unwrap するには `.ok_or_else(...)` を挟む
- `data_reference_index` は `StppBox::DEFAULT_DATA_REFERENCE_INDEX = NonZeroU16::MIN` の関連定数を `StppBox` 側で定義し、C API 側は常にこの定数値で復元する（`Mp4SampleEntryMp4a` 等が `AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX` を参照するのと同じパターン。C API から異なる値を渡す経路は本 issue のスコープに含めない）
- `Mp4Error` へのマッピングは既存 `impl From<shiguredo_mp4::Error> for Mp4Error`（`crates/c-api/src/error.rs`）で `MP4_ERROR_INVALID_INPUT` に落ちる想定
- 既存の `to_sample_entry`（`crates/c-api/src/boxes.rs:788, 1325, 1392` 等）はいずれも `std::slice::from_raw_parts` / `std::str::from_utf8` を使っており、`core::*` ではなく `std::*` に揃える（`crates/c-api` は `no_std` ではなく `std` 前提）
- `Mp4SampleEntryOwned::Stpp { inner }` が backing storage を持たない設計は既存 `Vp08` / `Vp09` / `Opus` バリアント（スカラー値のみ露出、ポインタ露出なし）とも、既存の `Avc1` / `Hev1` 等（intermediate `Vec` を保持してポインタ露出）とも異なる第 3 のパターン。`Utf8String` の内部 `String` が既に heap buffer を保持しており `inner.namespace.get().as_ptr()` で二重コピーを回避できるための最適化として選択する（本 issue で明示的にこのパターンを採用する）

- `crates/c-api/build.rs` cbindgen 経由で `crates/c-api/include/mp4.h` が更新される。`cargo build` 後に該当ヘッダーの diff を確認する
- `crates/c-api/examples/demux.c` および `crates/c-api/examples/remux.c` の `get_sample_entry_kind_name` に `"stpp"` の case を追加（両方の examples に同名関数がある）

### WASM 露出

- `crates/wasm/src/boxes.rs:9-47` `fmt_json_mp4_sample_entry` の match に `MP4_SAMPLE_ENTRY_KIND_STPP` arm 追加
- `crates/wasm/src/boxes.rs:51-122` `parse_json_mp4_sample_entry` の match に `"stpp"` arm 追加
- `crates/wasm/src/boxes.rs:132-167` `mp4_sample_entry_free` の match に `Stpp` arm 追加
- `crates/wasm/src/boxes_stpp.rs` を新規作成。雛形は以下の 2 つを組み合わせて使う:
  - JSON マッピング（`fmt_*` / `parse_*`）: `crates/wasm/src/boxes_opus.rs` パターン。ただし Opus はフィールドがすべて数値のため文字列フィールドの前例は WASM 側に無い。文字列は `value.to_member("namespace")?.required()?.to_unquoted_string_str()?` パターン（既存例: `crates/wasm/src/boxes.rs:55`）で読み取り、書き出しは nojson の `JsonFormatter::member(name, value)` に `&str` を渡す
  - 解放処理（`_free`）: `crates/wasm/src/boxes_mp4a.rs` の `mp4_sample_entry_mp4a_free` または `crates/wasm/src/boxes_flac.rs` の `mp4_sample_entry_flac_free` パターン（ポインタフィールドの解放処理を持つため、Opus と違って `_free` 関数が必要）。バイト列を WASM メモリに確保する際は `crate::boxes::allocate_and_copy_bytes(bytes)`（`crates/wasm/src/boxes.rs:174`）を使う
- JSON スキーマ: `{ "kind": "stpp", "namespace": "...", "schema_location": "...", "auxiliary_mime_types": "..." }`（`data_reference_index` は C API 露出方針に揃えて JSON にも含めない）

### `compatible_brands` の方針

`src/mux_fmp4_segment.rs:503-540` `build_ftyp` / `src/mux_mp4_file.rs:724-758` `build_final_ftyp_box` は SampleEntry 種別に応じて compatible_brands を追加する。0043 では字幕系ブランド（`stpp` / `msubs` 等）は **追加しない**（0042 と同じ方針を継続。0046 の「### `build_ftyp` / `compatible_brands`」節でも「本 issue の範囲では追加しない方向を第一候補」とされており、そちらとも整合）。

実プレイヤーでの字幕再生（DASH.js / Safari / VLC 等での認識）に必要な brand 対応は、必要になれば別 issue で扱う（stpp ではなく DASH-IF 側の推奨表に依存する話でもある）。

### サンプルデータの扱い方針

- 本 issue では **サンプルデータ全体は不透明なバイト列** として扱い、内部構造の parse / build は consumer 側に委ねる
- 理由: 既存の映像・音声サンプルの扱いと一貫させ、実装スコープを抑えるため。TTML / IMSC の型付きパースを本ライブラリで持つと XML パーサ依存が発生して no_std / wasm 前提と競合するのも避けたい
- サンプル単位の推奨値は既存の `Sample` 構造体（`src/mux_mp4_file.rs:179-182`）で文書化済み（`keyframe = true`、`composition_time_offset = None`）を踏襲する
- 追加で内部構造の型付き対応が必要になった場合は別 issue とする

### `Mp4FileMuxer` の Subtitle 拒否経路

本 issue の範囲では `Mp4FileMuxer::append_sample` の Subtitle 拒否経路（`src/mux_mp4_file.rs:557-572, 620-627`）は変更しない。`Mp4FileMuxer` 経由での字幕 mux 対応は 0046 で行う。したがって 0046 未完了時点では、0043 で追加した `StppBox` を含む Sample を `Mp4FileMuxer::append_sample` に渡すと `MuxError::UnsupportedTrackKind` が返る（0042 で追加済みの単体テストで担保）。

`Fmp4SegmentMuxer` 側は 0042 で既に Subtitle 受け入れ済み（`pbt/tests/prop_container_boxes.rs:874-908` `subtitle_track_mux_tkhd_via_fmp4_segment_muxer` テストで `SampleEntry::Unknown(stpp)` を渡す形で確認済み）。本 issue の合成テストはこの経路を利用する。

### 後方互換性への影響

- `SampleEntry` へのバリアント追加は網羅 match を破壊するため SemVer 上の破壊的変更（`CHANGES.md` では `[CHANGE]` を使う。詳細は「## CHANGES.md」節）
- `Unknown` フォールバック（`src/boxes_sample_entry.rs:137`）が残るため、decode 側の未知バリアント互換は維持される
- ただし、これまで `SampleEntry::Unknown { box_type: BoxType::Normal(*b"stpp"), .. }` として観測されていた stpp サンプルエントリーが本 issue 完了後は `SampleEntry::Stpp(_)` として観測される。既存 consumer で `match sample_entry { Unknown(b) if b.box_type == ... => }` のような判定を書いていた場合は影響する
- C API の `Mp4SampleEntryKind` enum に新規バリアントが末尾追加されるため、C 側の switch 網羅性を破壊しうる（追加後の bindgen 出力を利用者側でも取り込む必要がある）
- WASM JSON API に `"stpp"` kind が追加される（既存の `"avc1"` 等と同列）

## 依存関係

- 0042（`issues/closed/0042-add-subtitle-track-common.md`）は完了済み。以下を利用する
  - `TrackKind::Subtitle`
  - `HdlrBox::HANDLER_TYPE_SUBT` (`subt`)
  - `MediaHeader::Sthd(SthdBox)`
  - `Fmp4SegmentMuxer::derive_trak_attributes`（`src/mux_fmp4_segment.rs:953-993`）の暫定 Subtitle 分岐（本 issue では暫定固定選択のまま利用し、arm 追加はしない。doc コメントのみ実態に合わせて更新する）
  - `MuxError::UnsupportedTrackKind`（`Mp4FileMuxer` の拒否経路は本 issue でも維持）
- 0046（`issues/0046-add-mp4-file-muxer-subtitle.md`、open）は「`Mp4FileMuxer` / `Mp4FileDemuxer` 経由の MP4 ラウンドトリップ」検証で前提となる。0046 未完了時は `Fmp4SegmentMuxer` / `Fmp4SegmentDemuxer` 経由の fMP4 ラウンドトリップのみで完了と判断する
- 本 issue は 0044 / 0045 の依存元ではない（各方式は独立に追加可能）。0044 実装時に `derive_trak_attributes` の Subtitle 分岐を SampleEntry 種別 match に切り替え、0045 で暫定 fallback を除去する運用（本 issue のコミットで更新した doc コメント / インライン TODO を辿って追随できる）。0044 / 0045 側の issue 記述の追随更新は本 issue のスコープに含めない（各 issue の refresh は独立に行う）

## 完了条件

### 実装完了

- `StppBox` の `Encode` / `Decode` / `BaseBox` を実装する
- `SampleEntry::Stpp(StppBox)` バリアントを追加し、「### `SampleEntry` の網羅 match 箇所」で列挙した箇所すべてに arm を追加する
- `derive_trak_attributes` の doc コメントおよびインラインコメントを実態に合わせて更新する（match arm 自体は変更しない）
- C API 露出（`Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_STPP`、`Mp4SampleEntryOwned::Stpp`、`Mp4SampleEntryData::stpp`、`Mp4SampleEntryStpp` 構造体、`Mp4SampleEntryOwned::new` / `to_mp4_sample_entry` / `Mp4SampleEntry::to_sample_entry` の各 arm、`Mp4SampleEntryStpp::to_sample_entry` 実装）を追加する
- WASM 露出（`crates/wasm/src/boxes.rs` の 3 関数の arm、`crates/wasm/src/boxes_stpp.rs` 新規作成）を追加する
- `crates/c-api/examples/demux.c` および `remux.c` の `get_sample_entry_kind_name` に `"stpp"` の case を追加する

### PBT 追加

以下のテストを `pbt/tests/prop_additional_boxes.rs` に追加する（既存の `opus_box_roundtrip`（288 行目）/ `avc1_box_roundtrip`（358 行目）と同じファイルに揃える）:

- `stpp_box_roundtrip`: `StppBox` の decode / encode ラウンドトリップ（proptest ブロック内で以下パターンを網羅）:
  - `namespace` 非空 / `schema_location` 非空 / `auxiliary_mime_types` 非空（一般ケース）
  - `namespace` 非空 / `schema_location` 空 / `auxiliary_mime_types` 非空
  - `namespace` 非空 / `schema_location` 非空 / `auxiliary_mime_types` 空
  - `namespace` 非空 / `schema_location` 空 / `auxiliary_mime_types` 空
  - `namespace` 空（パーサ堅牢性のため）
  - `namespace` が単一 URI / スペース区切り複数 URI
- strategy の実装場所は追加先ファイル内に `fn arb_utf8_string_no_null() -> impl Strategy<Value = String> { "[^\x00]{0,100}" }` を用意する（既存 `pbt/tests/prop_basic_types.rs:41` の `arb_utf8_string` と同じ regex リテラル表記。integration test の性質上、他ファイルから直接再利用できないためコピー）
- `pbt/tests/prop_additional_boxes.rs:1007` の既存 `sample_entry_encode_decode_roundtrip`（通常の `#[test]` で proptest ブロックではない。`SampleEntry::Opus` のみを固定インスタンスで検証している）はそのまま残し、独立関数として `sample_entry_stpp_encode_decode_roundtrip` を新設する。既存 Opus パターンに揃えて `SampleEntry::Stpp` の固定インスタンス（3 フィールドとも非空）で encode/decode ラウンドトリップを検証する

### 単体テスト追加

以下の単体テストを追加する。追加先は既存の `tests/decode_encode_test.rs` に統合するのを第一候補とする（既存の統合テストは同ファイルのみ）:

- `stpp_box_missing_namespace_null_terminator`: `namespace` に null 終端が無いバイト列を渡すと `Err` が返る（`ErrorKind::InvalidInput`、メッセージに `"stpp.namespace"` を含む）
- `stpp_box_missing_schema_location_null_terminator`: 同上（schema_location）
- `stpp_box_missing_auxiliary_mime_types_null_terminator`: 同上（auxiliary_mime_types）
- `stpp_box_invalid_utf8_in_namespace`: `namespace` に UTF-8 として不正なバイト列（例: `[0xff, 0x00]`）を渡すと `Err` が返る（`ErrorKind::InvalidInput`、メッセージに `"stpp.namespace"` を含む）
- `stpp_box_decode_wrong_box_type`: `StppBox::decode` に `stpp` 以外の box_type を持つバイト列を渡すとエラー
- `sample_entry_decode_stpp_dispatches_to_stpp_variant`: `SampleEntry::decode` で `stpp` box_type を持つ入力が `SampleEntry::Stpp(_)` として取り出される（0043 完了以前は `Unknown` にフォールバックしていた挙動の回帰確認）

`pbt/tests/prop_error_paths.rs:378-` の `sample_entry_inner_box_tests` モジュールに `sample_entry_stpp_inner_box` を追加。既存 `sample_entry_*_inner_box` パターン（`box_type()` と `is_unknown_box()` の検証）に加えて、Stpp は必須の型付き子ボックスを持たないため `assert_eq!(entry.children().count(), 0)` を検証（`unknown_boxes: vec![]` で作成する）。

### 既存テストの更新

- `pbt/tests/prop_container_boxes.rs:210-218` `minimal_stsd_box_subtitle` は `sample_entry_box_type` 引数を受け取って現状 `SampleEntry::Unknown` を返す。本 issue では **同ヘルパを Stpp に置換せず、Unknown のまま維持する**（0044 / 0045 完了までの Unknown フォールバック経路の互換性担保のため。3 方式が揃った時点で Stpp / Wvtt / Tx3g それぞれに置換するかを 0045 側で最終判断する）
- `pbt/tests/prop_container_boxes.rs:874-908` `subtitle_track_mux_tkhd_via_fmp4_segment_muxer` テストも同じく Unknown 経路のまま維持する（fallback 経路の担保として）
- 上記 2 つは維持した上で、Stpp 正常経路担保は別途新規テスト（次項「### 3 経路デマルチプレクサ検証と合成ラウンドトリップ」）で追加する

### 3 経路デマルチプレクサ検証と合成ラウンドトリップ

`TrackInfo`（`src/demux_mp4_file.rs:53-70`）は `sample_entries` フィールドを持たない。SampleEntry を取り出すには `Sample.sample_entry: Option<&SampleEntry>`（`src/demux_mp4_file.rs:84`）から取得する必要があるため、init segment / moov だけでなく media segment / mdat + サンプルデータを含む合成データを組み立てる必要がある。

以下 2 経路の Stpp 正常経路担保テストを `pbt/tests/prop_container_boxes.rs` に追加する（既存 `subtitle_track_via_*_demuxer` の隣、777-864 行付近）:

- `stpp_sample_entry_via_fmp4_file_demuxer`: `SampleEntry::Stpp(_)` を持つ init + moof + mdat の合成バイト列を組み立て、`Fmp4FileDemuxer` から取り出した `sample.sample_entry` が `Some(SampleEntry::Stpp(_))` にマッチすることを検証
- `stpp_sample_entry_via_fmp4_segment_demuxer`: init segment + media segment の合成バイト列を組み立て、`Fmp4SegmentDemuxer::handle_media_segment` の戻り値の各 `Sample.sample_entry` を検証

`Mp4FileDemuxer` 経路（`stpp_sample_entry_via_mp4_file_demuxer`）は本 issue のスコープに含めない。`Mp4FileMuxer` が Subtitle を拒否する現状（`src/mux_mp4_file.rs:557-572`、0046 で解消予定）では合成データを Muxer 経由で吐かせられず、`stsz` / `stsc` / `stco` / `stts` / `mdat` を実装者が手動で整合的に組み立てる必要が生じる。既存の `subtitle_track_via_mp4_file_demuxer`（`pbt/tests/prop_container_boxes.rs:777-801`、0042 で追加）も mdat を省略して tracks() レベルまでしか検証していないため、`Mp4FileDemuxer` 経由の Stpp 正常経路担保は 0046 完了後に別途追加する。

サンプルデータは最小 XML 相当の合成バイト列（例: `b"<tt xmlns=\"http://www.w3.org/ns/ttml\"/>"`）を使い、実 XML ファイルは要求しない。

Fmp4 経路 2 本の合成は `Fmp4SegmentMuxer`（0042 で Subtitle 受け入れ済み）を経由して組み立てる。既存 `pbt/tests/prop_fmp4_segment_mux_demux.rs:141` `build_complete_media_segment` / `pbt/tests/prop_fmp4_segment_mux_demux.rs:203` `feed_fmp4_file_demuxer` と同等の手順で組み立てる。integration test の性質上、他ファイルのヘルパは直接再利用できないため、`arb_utf8_string_no_null` と同じ運用でコピーする。

### 検証

- `cargo clippy --all-targets --all-features` が warning なしで通る
- `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が warning なしで通る（新規 doc コメントの intra-doc link 検証。CI ではこのコマンドが実行される）
- `cargo test --workspace` がすべて pass する
- cbindgen 出力（`crates/c-api/include/mp4.h`）の diff を確認する（`Mp4SampleEntryStpp` 構造体・`MP4_SAMPLE_ENTRY_KIND_STPP` の生成、および union レイアウトへの影響）
- 既存の他バリアント（`Avc1` / `Hev1` / `Hvc1` / `Vp08` / `Vp09` / `Av01` / `Opus` / `Mp4a` / `Flac` / `Unknown`）の decode / encode 動作が変わらない（既存 PBT / 単体テストが pass）
- `sidx` 等のセグメント index との相互作用は 0042 で担保済みのため本 issue では追加検証しない
- 0046 完了後、`Mp4FileMuxer` / `Mp4FileDemuxer` 経由の MP4 ラウンドトリップも検証する（本 issue の完了条件からは除外）

## 解決方法

以下の順で実装する見込み。相互依存で「単独では cargo build が通らない」手順は同一コミット単位でまとめる。途中コミットも `cargo build` が通ることを目安とし、`cargo clippy` / `cargo test` は最終コミット時点で通れば良い。

1. `StppBox` を実装（`Encode` / `Decode` / `BaseBox`。`StppBox::DEFAULT_DATA_REFERENCE_INDEX = NonZeroU16::MIN` の関連定数も定義する）。doc コメントは既存 SampleEntry の形式（半角括弧で終わる）に揃え、`` /// [ISO/IEC 14496-30] XMLSubtitleSampleEntry class (親: [`StsdBox`][crate::boxes::StsdBox]) `` とする（既存 `Avc1Box` を参考にする）
2. **同一コミット単位で実施**: `SampleEntry::Stpp(StppBox)` バリアントを追加し、「### `SampleEntry` の網羅 match 箇所」で列挙した 3 箇所の網羅 match（`inner_box` / `Encode` / `Decode`）に arm を追加する。バリアント追加と match arm 追加を分けると `cargo build` が通らない
3. `Fmp4SegmentMuxer::derive_trak_attributes` の doc コメントおよびインラインコメントを「### `derive_trak_attributes` の doc コメント更新」節に従って書き換える（match arm 自体は変更しない）
4. **同一コミット単位で実施**: C API 露出を追加する（`Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_STPP`、`Mp4SampleEntryOwned::Stpp`、`Mp4SampleEntryData::stpp`、`Mp4SampleEntryStpp` 構造体、`Mp4SampleEntryOwned::new` / `to_mp4_sample_entry` / `Mp4SampleEntry::to_sample_entry` の 3 箇所の match arm、`Mp4SampleEntryStpp::to_sample_entry`）。同時に `crates/c-api/examples/demux.c` / `remux.c` の switch も更新する。cbindgen によるヘッダ再生成を `cargo build` 後に確認する
5. WASM 露出を追加する（`crates/wasm/src/boxes.rs` の 3 関数 + `crates/wasm/src/boxes_stpp.rs` を新規作成。JSON マッピングは `boxes_opus.rs`、解放処理は `boxes_mp4a.rs` / `boxes_flac.rs` を雛形に組み合わせる）
6. PBT を追加する（`pbt/tests/prop_additional_boxes.rs` に `stpp_box_roundtrip`、`sample_entry_encode_decode_roundtrip` に Stpp ケース追加、`arb_utf8_string_no_null` 追加）
7. 単体テストを追加する（null 終端欠落エラー / stpp 以外の box_type エラー / SampleEntry::decode の stpp 経路 / `pbt/tests/prop_error_paths.rs` の `sample_entry_stpp_inner_box`）
8. Fmp4 経路 2 本の Stpp 検証テスト（`stpp_sample_entry_via_fmp4_file_demuxer` / `stpp_sample_entry_via_fmp4_segment_demuxer`）を追加する。サンプルデータを含む合成データを `Fmp4SegmentMuxer` 経由で組み立て、`Sample.sample_entry` から Stpp を取り出せることを検証する。`Mp4FileDemuxer` 経路のテストは 0046 完了後に別途追加するため本 issue に含めない
9. `cargo clippy --all-targets --all-features` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` / `cargo test --workspace` / cbindgen 出力の diff で検証する

## CHANGES.md

機能単位に以下 2 エントリで記載する（担当者行 `- @ユーザー名` は実装時に補う）。0042 のスタイル（C API / WASM 露出は上位エントリの子項目として書く）に倣う。

- `[CHANGE]` `SampleEntry` に `Stpp` バリアントを追加する
  - `stpp` サンプルエントリー（ISO/IEC 14496-30 `XMLSubtitleSampleEntry`）を型付きで扱えるようにする
  - 網羅 match への影響がある（利用者側でコンパイルエラーになりうる）
  - C API `Mp4SampleEntryKind` に `MP4_SAMPLE_ENTRY_KIND_STPP` を追加し、`Mp4SampleEntryStpp` 構造体を新設する
  - WASM の JSON API で `{ "kind": "stpp", ... }` の入出力に対応する
- `[ADD]` ISO/IEC 14496-30 の `StppBox` (`stpp`) を追加する
  - `namespace` / `schema_location` / `auxiliary_mime_types` の 3 フィールド（`Utf8String`）と任意子ボックスを持つ
  - サンプルデータは XML ドキュメント（TTML / IMSC 等）を不透明バイト列として扱う
