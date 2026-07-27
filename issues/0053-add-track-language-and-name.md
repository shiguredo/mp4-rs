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

以下を決める必要がある。

1. **設定を持たせる場所**: トラック単位の属性なので `Sample` ではなく Options 側が素直だが、`Options` は現状トラック種別に依存しない構成になっている。`TrackKind` ごとの設定を持つ形にするか、別の入り口を用意するかを決める
2. **`hdlr.name` の扱い**: `HdlrBox::name` は null 終端バイト列（`Utf8String::into_null_terminated_bytes()`）である。利用側には `&str` ないし `Utf8String` を受け取り、内部で変換する形が望ましい
3. **言語コードの検証**: `mdhd.language` は ISO-639-2/T の 3 文字を 5 ビットずつパックする。5 ビットに収まらない入力の扱いは 0029 と重なるため、方針を揃える

### 0049（同一 `TrackKind` の複数トラック許容）との関係

pending の 0049 でも「5. 言語情報の扱い」として `mdhd.language` を指定可能にするかが検討事項に挙がっている。ただし 0049 は同一 kind の複数トラック対応が主題で、`alternate_group` の扱いなど設計判断が固まるまで pending になっている。

本 issue が扱うのは「**トラックが 1 本しかなくても言語とトラック名を宣言できない**」という範囲であり、複数トラック対応を待たずに解決できる。多言語トラックの出し分けそのものは 0049 の範囲であり、本 issue には含めない。

## 完了条件

- `Mp4FileMuxer` / `Fmp4SegmentMuxer` の両方で、生成されるトラックの `mdhd.language` と `hdlr.name` を利用側から指定できること
- 指定しなかった場合は現状どおり `und` と空文字列になること（後方互換の維持）
- 指定した値が mux → demux のラウンドトリップで復元されることを検証する PBT があること
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が通ること
