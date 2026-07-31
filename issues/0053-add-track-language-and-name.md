# 両 muxer でトラックの言語（`mdhd.language`）とトラック名（`hdlr.name`）を指定できるようにする

- Priority: Low
- Created: 2026-07-27
- Completed: YYYY-MM-DD
- Model: Opus 5
- Branch: feature/add-track-language-and-name
- Polished: YYYY-MM-DD

## 目的

`Mp4FileMuxer` / `Fmp4SegmentMuxer` が生成するトラックの言語（`mdhd.language`）とトラック名（`hdlr.name`）を、利用側から指定できるようにする。

現状はどちらもハードコードされており、公開 API から設定する手段が存在しない。字幕トラックを出力できるようになった結果、プレイヤー上で識別できない無名・言語不明のトラックしか作れない制約が顕在化している。

## 優先度根拠

Low。現時点で具体的な要求は無く、バグ由来でもない。音声・映像トラックでは `und`（言語未定義）でも実害が小さい。

ただし字幕トラックにおいては、言語はプレイヤーがトラック一覧を提示して利用者に選択させるための主要メタデータであり、指定できないことは機能上の制約として意味を持つ。

## 現状

両 muxer とも `mdhd.language` は `MdhdBox::LANGUAGE_UNDEFINED`（`*b"und"`）、`hdlr.name` は空文字列で固定されている。

- `src/mux_mp4_file.rs:981` / `src/mux_mp4_file.rs:986`
- `src/mux_fmp4_segment.rs:657` / `src/mux_fmp4_segment.rs:626`

設定用のフィールドはいずれの公開型にも無い。

- `src/mux_mp4_file.rs:106` `Mp4FileMuxerOptions` — `reserved_moov_box_size` と `creation_timestamp` のみ
- `src/mux_fmp4_segment.rs:73` `SegmentMuxerOptions` — `creation_timestamp` のみ
- `mux::Sample` — トラック単位のメタデータを持つフィールドは無い

`MdhdBox::language` と `HdlrBox::name` はどちらも `pub` フィールドなので、muxer を経由せずボックスを直接組み立てれば設定できるが、その場合 muxer の利用を諦めることになる。

## 設計方針

3 論点を以下で確定する。

同一 `TrackKind` の複数トラック対応（0049）で必要になる指定インタフェース（トラックインデックス指定など）は本 issue の範囲外とし、当面「1 kind = 1 メタデータ」を前提に設計する。0049 が着手されるタイミングで、必要に応じて破壊的な API 変更を行うことを許容する。

### 1. 設定を持たせる場所: `Options` に `TrackKind` 別フィールドを追加する

`Sample` 側に持たせるのは不適。トラック単位の属性を毎サンプルに運ばせるのは冗長で、「どのサンプルの値が採用されるか」も曖昧になる。muxer は最初のサンプルから暗黙にトラックを起こす設計のため、値の取り違えが起きやすい。

現行 `Options` はトラック種別非依存だが、`Mp4FileMuxerOptions` / `SegmentMuxerOptions` は既に「muxer 全体の設定」の入れ場所として確立している。ここに `TrackKind` ごとのフィールドを追加するのが最小侵襲。`HashMap<TrackKind, _>` ではなく静的な kind 別フィールドにすることで、typo を型で防ぐ。

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

`Default` は全て `TrackMetadata::default()`（`LANGUAGE_UNDEFINED` + 空文字列）となり、現行挙動と完全一致する。

`TrackMetadata` は両 muxer で共有するため、既存の `Sample` / `MuxError` と同様に `mux_mp4_file` に定義し、`mux_fmp4_segment` から use する。

### 2. `hdlr.name` の型: `Utf8String`

`HdlrBox::name` は null 終端バイト列だが、リポジトリでは既に `Utf8String` が「null 文字を含まない UTF-8」の正規表現として使われている（`boxes_sample_entry.rs` の `namespace` など）。`Utf8String::new(&str) -> Option<Self>` が null 禁止を API 境界で強制するため、`Utf8String` を受け取れば `HdlrBox::name` へ流し込む段階では検証済み。

`Utf8String` に `Default` 実装を追加し、`Utf8String::EMPTY` を返すようにする。デフォルト時は現行どおり `Utf8String::EMPTY.into_null_terminated_bytes()` を書き込む。

### 3. `mdhd.language` の型: `LanguageCode` newtype を新設

生 `[u8; 3]` を受けると、0029 の encode 時 5 ビット検証で「finalize 段階でのエラー」という遅い失敗になる。Options 構築時点で弾けたほうが UX が良いため、newtype で入力側の検証を行う。

検証範囲は 0029 と揃え、各バイトを `0x60..=0x7F` に限る。ISO-639-2/T の文字集合（`a-z`）まで絞る厳格化は 0029 と同じ理由で本 issue の範囲外とする。

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

    /// 3 文字 ASCII 文字列から作る（例: `"eng"`, `"jpn"`）
    pub fn from_str(s: &str) -> Option<Self> { ... }

    pub fn as_bytes(&self) -> [u8; 3] { self.0 }
}

impl Default for LanguageCode {
    fn default() -> Self { Self::UNDEFINED }
}
```

`MdhdBox::language` の型（`pub [u8; 3]`）は変更しない。muxer 経由の利用者には newtype 経由の型安全なインタフェースを、直接 `MdhdBox` を組み立てる利用者には生バイト列を、それぞれ提供する棲み分けとなる。

`LanguageCode` は `Utf8String` と同様の位置付けなので `basic_types.rs` に配置し、`lib.rs` から re-export する。

## 完了条件

- `Mp4FileMuxer` / `Fmp4SegmentMuxer` の両方で、生成されるトラックの `mdhd.language` と `hdlr.name` を利用側から指定できること
- 指定しなかった場合は現状どおり `und` と空文字列になること（後方互換の維持）
- 指定した値が mux → demux のラウンドトリップで復元されることを検証する PBT があること
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が通ること
