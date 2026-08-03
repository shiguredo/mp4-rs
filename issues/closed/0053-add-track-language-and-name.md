# 両 muxer でトラックの言語（`mdhd.language`）とトラック名（`hdlr.name`）を指定できるようにする

- Priority: Low
- Created: 2026-07-27
- Completed: 2026-07-31
- Model: Opus 5
- Branch: feature/add-track-language-and-name
- Polished: 2026-07-31

## 目的

`Mp4FileMuxer` / `Fmp4SegmentMuxer` が生成するトラックの言語（`mdhd.language`）とトラック名（`hdlr.name`）を、利用側から指定できるようにする。現状はどちらもハードコードされており、公開 API から設定する手段が存在しない。

## 優先度根拠

Low。現時点で具体的な要求は無く、バグ由来でもない。音声・映像トラックでは `und` でも実害が小さい一方、字幕トラックでは指定できないことが機能上の制約として意味を持つ（プレイヤーがトラック一覧を提示して利用者に選択させる際の主要メタデータのため）。

## 現状

両 muxer とも `mdhd.language` は `MdhdBox::LANGUAGE_UNDEFINED`（`*b"und"`）、`hdlr.name` は `Utf8String::EMPTY.into_null_terminated_bytes()`（末尾 null 1 バイトのみ）で固定されている。

- `Mp4FileMuxer::build_mdia_box`（`src/mux_mp4_file.rs`）内で `MdhdBox { language: ..., ... }` と `HdlrBox { name: ..., ... }` を組み立てている
- `Fmp4SegmentMuxer::build_init_trak`（`src/mux_fmp4_segment.rs`）内でも同様に組み立てている

設定用のフィールドはいずれの公開型にも無い。

- `Mp4FileMuxerOptions` — `reserved_moov_box_size` と `creation_timestamp` のみ
- `SegmentMuxerOptions` — `creation_timestamp` のみ
- `mux::Sample` — トラック単位のメタデータを持つフィールドは無い

## 設計方針

3 論点を以下で確定する。

同一 `TrackKind` の複数トラック対応（0049）で必要になる指定インタフェース（トラックインデックス指定など）は本 issue の範囲外とし、当面「1 kind = 1 メタデータ」を前提に設計する。0049 が着手されるタイミングで、必要に応じて破壊的な API 変更を行うことを許容する。

### 1. 設定を持たせる場所: `Options` に `TrackKind` 別フィールドを追加する

`Sample` 側に持たせるのは不適。トラック単位の属性を毎サンプルに運ばせるのは冗長で、「どのサンプルの値が採用されるか」も曖昧になる。muxer は最初のサンプルから暗黙にトラックを起こす設計のため、値の取り違えが起きやすい。

現行 `Options` はトラック種別非依存だが、`Mp4FileMuxerOptions` / `SegmentMuxerOptions` は既に「muxer 全体の設定」の入れ場所として確立している。ここに `TrackKind` ごとのフィールドを追加するのが最小侵襲。`HashMap<TrackKind, _>` を採らずに静的な kind 別フィールドを採るのは、全 kind のデフォルト値が型定義から明示的に読めること、値の欠落や実行時アクセス失敗を型システムで排除できること、`Default` 派生で現行挙動との一致を機械的に保証できることを重視するため。

具体形:

```rust
/// トラック単位の任意メタデータ（`mdhd.language` / `hdlr.name`）
#[derive(Debug, Clone, Default)]
pub struct TrackMetadata {
    /// 未指定時は `LanguageCode::UNDEFINED`（`*b"und"`）
    pub language: LanguageCode,
    /// 未指定時は空文字列
    pub name: Utf8String,
}
```

`Mp4FileMuxerOptions` / `SegmentMuxerOptions` の両方に以下 3 フィールドを追加する:

```rust
pub audio_track: TrackMetadata,
pub video_track: TrackMetadata,
pub subtitle_track: TrackMetadata,
```

`Default` は全て `TrackMetadata::default()`（`LANGUAGE_UNDEFINED` + 空文字列）となり、生成される MP4 のバイト列は現行と完全一致する。

一方、既存のフィールド指定リテラル（例: `Mp4FileMuxerOptions { reserved_moov_box_size: X, creation_timestamp: Y }`）はコンパイルできなくなる。両 `Options` には `#[non_exhaustive]` は付いていない（`shiguredo-rust` の「`#[non_exhaustive]` を使わない」規約と整合）ため、今回もこれは付けない。利用側は `..Default::default()` を付けるか、追加された 3 フィールドを明示的に埋める形に書き換える必要がある。

リポジトリ内でフィールドをすべて明示指定していて `..Default::default()` を持たない struct literal が 2 箇所ある。フィールド追加後にコンパイルが通らなくなるため、これらは `..Default::default()` を付ける追随修正が必要になる:

- `crates/c-api/src/fmp4_segment_mux.rs` の `fmp4_segment_muxer_new_with_options` 内の `SegmentMuxerOptions { creation_timestamp: ... }`
- `src/mux_mp4_file.rs` の `#[cfg(test)]` 内の `Mp4FileMuxerOptions { reserved_moov_box_size: 4096, creation_timestamp: Duration::from_secs(0) }`（`Mp4FileMuxerOptions::with_options` の基本テスト）

上記以外の struct literal（`src/mux_mp4_file.rs` の `#[cfg(test)]` の他 3 箇所、`pbt/tests/prop_mux_demux.rs` の 2 箇所、`pbt/tests/prop_mp4_file_kind_detector.rs` の 1 箇所、`fuzz/fuzz_targets/fuzz_mp4_file_mux.rs` の 1 箇所）はすでに `..Default::default()` を含むため、`TrackMetadata: Default` を経由してフィールド追加後もそのままコンパイルが通る。追随修正は不要。

C API / WASM から新フィールド（`language` / `name`）を指定できるようにするバインディング拡張は本 issue のスコープ外とし、上記追随箇所ではデフォルト値のまま保つ。バインディング拡張が必要になった時点で別 issue で扱う。

`TrackMetadata` は両 muxer で共有するため、既存の `Sample` / `MuxError` と同様に `mux_mp4_file` に定義し、`mux_fmp4_segment` から use する。公開 API として利用側から明示的に構築できるよう、`Sample` / `MuxError` と同様に `src/mux.rs` から `pub use` で re-export する。

muxer 内での引き当てはヘルパ関数 `fn track_metadata(options: &TrackMetadataProvider, kind: TrackKind) -> &TrackMetadata`（あるいはこれと等価な関数）を 1 箇所に置き、`Mp4FileMuxer::build_mdia_box`（`src/mux_mp4_file.rs`）と `Fmp4SegmentMuxer::build_init_trak`（`src/mux_fmp4_segment.rs`）の両方から呼ぶ。`Fmp4SegmentMuxer::build_init_trak` は現状 `self.options` を参照していないため、`build_init_moov` から `&self.options` を渡すシグネチャに変更する（現行の `creation_time` を渡している経路と同じ形にする）。`Mp4FileMuxer::build_mdia_box` は既に `self.options.creation_timestamp` を参照している（`Mp4FileTime::from_unix_time(self.options.creation_timestamp)` として使用）ので、同じ `self.options` から `TrackMetadata` も参照する形で拡張する。

### 2. `hdlr.name` の型: `Utf8String`

`HdlrBox::name` の実型は `Vec<u8>` である。これは decode 側の耐性のためで、doc コメントには「ISO の仕様書上はここは `Utf8String` であるべきだが、中身が UTF-8 ではなかったり、null 終端文字列ではなく先頭にサイズバイトを格納する形式で MP4 ファイルを作成する実装が普通に存在するため、ここでは単なるバイト列として扱っている」と明記されている（`CHANGES.md` 2024.2.0 の `[FIX]` エントリで意図的に `Utf8String` から `Vec<u8>` に緩めた履歴あり）。この decode 側の緩さは維持する。

muxer 入力層は encode 側のみを対象とするため、生成される MP4 の `hdlr.name` を仕様どおり「null 終端の UTF-8」に揃える方向で狭める。`Utf8String::new(&str) -> Option<Self>` が null 禁止を API 境界で強制するため、`Utf8String` を受け取れば `HdlrBox::name` へ流し込む段階では検証済み。`Utf8String` はリポジトリで既に `boxes_sample_entry.rs` の `namespace` などで採用されており、公開 API の慣例にも合う。

内部変換は現行と同じく `Utf8String::into_null_terminated_bytes()` を通す。この結果、demuxer 側の `HdlrBox::name`（型は `Vec<u8>` のまま）には末尾 null 1 バイトを含んだバイト列が入る。ラウンドトリップの意味論は「入力 `Utf8String` を muxer に流し、demux 側の `HdlrBox::name`（`Vec<u8>`）が `Utf8String::new(s).unwrap().into_null_terminated_bytes()` と等しいこと」で定義する（完了条件で明示する）。

`Utf8String` に `Default` 実装を追加し、空文字列を返すようにする（`TrackMetadata` の `#[derive(Default)]` を通すため）。`Utf8String` は `pub struct Utf8String(String)` で `String::default()` は空文字列 `String::new()` と一致するため、実装は `#[derive(Default)]` を struct に追加するだけで足りる（既存の `Utf8String::EMPTY` の内部と同じ値になる）。既存の `pub const EMPTY` は const 値としてそのまま残し、明示指定と `Default::default()` の両方が同じ空文字列を返す。

### 3. `mdhd.language` の型: `LanguageCode` newtype を新設

生 `[u8; 3]` を受けると、0029 の encode 時 5 ビット検証で「finalize 段階でのエラー」という遅い失敗になる。Options 構築時点で弾けたほうが UX が良いため、newtype で入力側の検証を行う。

検証範囲は 0029 と揃え、各バイトを `0x60..=0x7F` に限る。ISO-639-2/T の文字集合（`a-z`）まで絞る厳格化は 0029 と同じ理由で本 issue の範囲外とする（`0x7B..=0x7F` の `{|}~<DEL>` などは 5 ビット的には有効なので受理する。プレイヤーが表示できるかは呼び出し側の責任）。

```rust
/// `MdhdBox::language` 用の 3 文字言語コード
///
/// 各バイトは `0x60..=0x7F` の範囲に収まる必要がある
/// （ISO/IEC 14496-12 の `unsigned int(5)[3]` パック規約に由来）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LanguageCode([u8; 3]);

impl LanguageCode {
    /// 未定義言語（`*b"und"`）
    pub const UNDEFINED: Self = Self(*b"und");

    /// 3 バイト配列から作る。各バイトが `0x60..=0x7F` の範囲外なら `None`
    pub fn new(code: [u8; 3]) -> Option<Self> { ... }

    /// 3 文字 ASCII 文字列から作る（例: `"eng"`, `"jpn"`）。バイト長が 3 でない、
    /// または各バイトが `0x60..=0x7F` の範囲外なら `None`
    pub fn from_ascii(s: &str) -> Option<Self> { ... }

    pub fn as_bytes(self) -> [u8; 3] { self.0 }
}

impl Default for LanguageCode {
    fn default() -> Self { Self::UNDEFINED }
}
```

文字列からの構築は `core::str::FromStr` を実装せず、inherent method `from_ascii` として提供する。`FromStr::from_str` は戻り値が `Result` に固定される一方、本型は `Utf8String::new(&str) -> Option<Self>` と同じく `Option` で不正入力を扱うのが自然なため、trait 名と衝突する `from_str` は避ける。命名は「ASCII 全域を受理する」印象を与える恐れがあるので、doc コメントで受理範囲が `0x60..=0x7F` に限られる旨（大文字 `"ENG"` などは拒否される）を明記する。

`MdhdBox::language` の型（`pub [u8; 3]`）は変更しない。muxer 経由の利用者には newtype 経由の型安全なインタフェースを、直接 `MdhdBox` を組み立てる利用者には生バイト列を、それぞれ提供する棲み分けとなる。

配置は `basic_types.rs`（既存の `Utf8String` / `TrackKind` / `Mp4FileTime` 等と並ぶ位置）で、`lib.rs` から re-export する。`shiguredo-rust` は「re-export は基本的にやらない」を規約とするが、`basic_types.rs` は private モジュールなので公開経路として `lib.rs` からの re-export が必要になる。これは既存の `Utf8String` / `TrackKind` 等に付いている既往の例外を踏襲する扱い。

## 完了条件

- `LanguageCode` 型が `basic_types.rs` に新設され、`lib.rs` から re-export されること
- `Utf8String` に `Default` 実装が追加され、`Utf8String::default()` が `Utf8String::EMPTY` と等しい値（空文字列）を返すこと。回帰防止のため `assert_eq!(Utf8String::default(), Utf8String::EMPTY)` 相当の単体テストがあること
- `TrackMetadata` 型が `mux_mp4_file` に新設され、`Mp4FileMuxerOptions` / `SegmentMuxerOptions` の両方に `audio_track` / `video_track` / `subtitle_track` の 3 フィールドとして追加されていること
- `Mp4FileMuxer` / `Fmp4SegmentMuxer` の両方で、Options 経由で指定した `mdhd.language` と `hdlr.name` が生成トラックに反映されること
- Options のフィールドを指定しなかった場合、生成される MP4 のバイト列が現行と完全一致すること（`und` と null 終端の空 UTF-8）
- 指定した値が mux → demux のラウンドトリップで復元されることを検証する PBT が `Mp4FileMuxer` / `Fmp4SegmentMuxer` の両 muxer について存在すること。ラウンドトリップの意味論は次の 2 点:
    - `language`: 入力 `LanguageCode` と、demux 側の `MdhdBox::language`（`[u8; 3]`）を `LanguageCode::new` で再構築した値が一致する
    - `name`: 入力 `Utf8String`（`into_null_terminated_bytes()` は `self` を消費するため、比較時は `clone()` してから通す）を `into_null_terminated_bytes()` に通したバイト列と、demux 側の `HdlrBox::name`（`Vec<u8>`）が一致する
- `LanguageCode::from_ascii` / `LanguageCode::new` の境界値単体テスト（バイト長 3 以外、`0x60..=0x7F` の境界とその内外、`LanguageCode::UNDEFINED` の値、`from_ascii("ENG")` が `None` を返すことなど）があること
- 新フィールド追加後に workspace 全体のビルドが通ること。`..Default::default()` を持たない `crates/c-api/src/fmp4_segment_mux.rs` の `fmp4_segment_muxer_new_with_options` 内の `SegmentMuxerOptions { ... }` と、`src/mux_mp4_file.rs` の `#[cfg(test)]` 内の `Mp4FileMuxerOptions { reserved_moov_box_size: 4096, creation_timestamp: ... }` は `..Default::default()` を付ける追随修正が必要（既に `..Default::default()` を含む他の struct literal は追加修正不要）
- `CHANGES.md` の `## develop` に以下のエントリが追加されていること:
    - `[CHANGE]` `Mp4FileMuxerOptions` / `SegmentMuxerOptions` に `audio_track` / `video_track` / `subtitle_track` フィールドを追加する（既存の struct literal 呼び出しは破壊的）
    - `[ADD]` `LanguageCode` 型を新設する
    - `[ADD]` `TrackMetadata` 型を新設し `mux::` から公開する
    - `[ADD]` `Utf8String` に `Default` を実装する
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が warning なしで通ること

## 解決方法

### 型と Options の追加

- `basic_types.rs` に `LanguageCode` 型を新設した
    - 内部は `[u8; 3]`。`new` と `from_ascii` の入り口で `0x60..=0x7F` を検証し、`UNDEFINED` 定数（`*b"und"`）を提供
    - `Default` は `UNDEFINED`、`Debug` / `Display` は `BoxType` に倣って 3 文字表示にした
    - `lib.rs` から `pub use` で再エクスポート
- `basic_types.rs` の `Utf8String` に `#[derive(Default)]` を追加した（空文字列を返し、`Utf8String::EMPTY` と同値）
- `mux_mp4_file` に `TrackMetadata { language: LanguageCode, name: Utf8String }` 型を新設し、`mux::` から再エクスポートした
- `Mp4FileMuxerOptions` / `SegmentMuxerOptions` の両方に `audio_track` / `video_track` / `subtitle_track` の 3 フィールドを追加した
    - 両 Options とも `#[derive(Debug, Clone, Default)]` に統一。手書き `Default` は削除
    - 引き当ては両 Options に `pub(crate) fn track_metadata(&self, kind: TrackKind) -> &TrackMetadata` として inherent method 化（当初検討した 4 引数自由関数は廃止）
- `Mp4FileMuxer::build_mdia_box` と `Fmp4SegmentMuxer::build_init_trak` の両方で、`self.options.track_metadata(entry.track_kind)` から `mdhd.language` と `hdlr.name` を反映するようにした

### 追随修正

- `..Default::default()` を含まなかった 2 箇所（`crates/c-api/src/fmp4_segment_mux.rs` の `fmp4_segment_muxer_new_with_options`、`src/mux_mp4_file.rs` の `#[cfg(test)]` 内 `Mp4FileMuxerOptions` リテラル）に `..Default::default()` を追加

### バインディング拡張のスコープ

- C API / WASM 経由の利用者は本フィールドを指定する手段を持たず、デフォルト値（`und` + 空文字列）に固定される
- 実際に指定したいという要求が発生した時点で改めて検討する（本 issue のスコープからは外す）
- CHANGES.md の `[CHANGE]` エントリに上記の制約を明示した

### テスト

- `LanguageCode` の境界値単体テスト（`tests/test_basic_types.rs`）
    - `new` の位置別網羅（1 / 2 / 3 バイト目それぞれ範囲外のケース）と `UNDEFINED` の値固定
    - `from_ascii` の受理・拒否（大文字・空文字列・非 ASCII マルチバイト 3 バイト）
- `Utf8String::default()` が `Utf8String::EMPTY` と等しいことの単体テスト
- `TrackMetadata::default()` が生成する `mdhd.language` / `hdlr.name` のバイト列が現行と一致することを固定する単体テスト（`src/mux_mp4_file.rs` の `mod tests`）
- 両 muxer について「Options で指定した language / name が mux → demux で復元されること」を検証する PBT を追加した
    - `Mp4FileMuxer` / `Fmp4SegmentMuxer` の両方で映像・音声・字幕の 3 kind すべてを流し、`MoovBox::decode` の返却サイズも `bytes.len()` と一致することを固定
- PBT の共通ヘルパー（`arb_language_code` / `arb_track_name` / `arb_track_metadata` / `assert_track_metadata`）を `pbt/tests/common/mod.rs` に集約した

### CHANGES.md

- `[CHANGE]` `Mp4FileMuxerOptions` / `SegmentMuxerOptions` に 3 フィールド追加（既存リテラルは破壊的、C API / WASM 経由の利用者はデフォルト固定になる旨を含む）
- `[ADD]` `LanguageCode` / `TrackMetadata` / `Utf8String` の `Default`
