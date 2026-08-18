# VP8 ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/add-vp8-bitstream-utilities
- Polished: 2026-08-18

## 目的

VP8 フレームの uncompressed data chunk を解析し、キーフレーム判定、解像度、`vp08` / `vpcC` の構築に必要なストリーム情報を得る汎用ユーティリティを追加する。

固定値でサンプルエントリーを組み立てるのではなく、入力ストリームから取得できる情報と呼び出し側が指定すべき表示情報を区別し、`Vp08Box` を安全に構築できるようにする。

## 現状

- `src/boxes_sample_entry.rs` には `Vp08Box` と `VpccBox` があるが、VP8 フレームを解析する API はない
- `shiguredo/hisui` の `src/video/vpx.rs` は VP8 用の profile、level、bit depth、chroma subsampling、range、色特性を固定値としてサンプルエントリーを構築している
- VP8 と VP9 は同じ `vpcC` ボックス形式を使うが、ビットストリーム構文は独立している

参照仕様は [RFC 6386: VP8 Data Format and Decoding Guide](https://www.rfc-editor.org/rfc/rfc6386) と [VP Codec ISO Media File Format Binding](https://www.webmproject.org/vp9/mp4/) とする。後者は URL パスに `vp9` を含むが同ページで VP8 (`vp08`) のサンプルエントリーも規定しており、VP8 / VP9 共通の binding である。

## 設計方針

### モジュール構成

`src/lib.rs` から公開する `bitstream` モジュール配下に VP8 用のサブモジュールを追加する。`mod.rs` は使わない。

```text
src/bitstream.rs
src/bitstream/vp8.rs
```

`src/bitstream.rs` が既に他 issue (0062〜0064、0066) の実装で追加済みであれば `pub mod vp8;` を追記するだけで足りる。未追加なら本 issue で新設する。他 4 コーデックの issue と実装が並列する場合、`src/bitstream.rs` の追加コミットが競合し得る点に注意する。VP8 の解析処理は NAL 層 (`src/bitstream/nal.rs`) に依存せず、`bitstream::vp8` 単独で完結する。

### フレーム解析 API

`bitstream::vp8` は VP8 フレームの uncompressed data chunk 部分を解析する API を公開する。入力は VP8 フレーム全体を渡す想定で、`first_partition_size` の境界検証がこの入力サイズに対して行われる (uncompressed data chunk だけを渡すと `first_partition_size > 0` が常に境界超過となる)。RFC 6386 Section 9.1 に従い、frame tag 3 バイトは LSB-first で以下の 4 フィールドにパックされる。

- bit 0: `frame_type` (0 = キーフレーム、1 = interframe)
- bits 1..=3: `version` (0..=3 が定義済み、4..=7 は予約)
- bit 4: `show_frame`
- bits 5..=23: `first_partition_size` (19 ビット、後続の第 1 データパーティションのサイズ)

キーフレーム時は frame tag に続けて 7 バイトが uncompressed data chunk として現れる。

- 3 バイトの開始コード `0x9D 0x01 0x2A`
- 16 ビットの `width` フィールド (下位 14 ビット = 幅、上位 2 ビット = 水平スケール)
- 16 ビットの `height` フィールド (下位 14 ビット = 高さ、上位 2 ビット = 垂直スケール)

`color_space` と `clamping_type` は compressed header (第 1 partition の boolean-coded 領域) に格納されており uncompressed data chunk には含まれない。本 issue では扱わない。

以下は公開 API の骨格を示す (型名・関数名・列挙定義は実装時に既存 API と整合させて調整可)。

```rust
/// VP8 のフレーム種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vp8FrameType {
    Key,
    Inter,
}

/// VP8 の uncompressed data chunk から取得できるフレーム情報
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Vp8FrameHeader {
    pub frame_type: Vp8FrameType,
    pub version: u8,               // 0..=3
    pub show_frame: bool,
    pub first_partition_size: u32, // 19 ビット値
    /// キーフレームのときのみ Some。interframe は None。
    pub keyframe: Option<Vp8KeyFrameInfo>,
}

/// キーフレームの uncompressed data chunk から取得できる情報
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Vp8KeyFrameInfo {
    pub width: u16,            // 14 ビット
    pub height: u16,           // 14 ビット
    pub horizontal_scale: u8,  // 2 ビット
    pub vertical_scale: u8,    // 2 ビット
}

pub fn parse_frame_header(input: &[u8]) -> Result<Vp8FrameHeader>;
```

`parse_frame_header` の入力境界検証は以下を `crate::Error` として拒否する。

- interframe の入力が 3 バイト未満
- キーフレームの入力が 10 バイト (frame tag 3 + 開始コード 3 + width 2 + height 2) 未満
- キーフレームの開始コードが `0x9D 0x01 0x2A` と一致しない
- `version` が 4..=7 (RFC 6386 未定義領域を拒否する)
- キーフレームの `width` または `height` が 0
- `first_partition_size` が入力末尾を超える (interframe は入力サイズから frame tag 3 バイトを引いた残りと比較、キーフレームは 10 バイトを引いた残りと比較)

圧縮ヘッダーやマクロブロックデータまで解析する完全な VP8 デコーダーにはしない。

### サンプルエントリー構築 API

解析結果と呼び出し側の設定から具体的な `Vp08Box` を構築する API を追加する。

VP8 仕様および VP Codec ISO Media File Format Binding から確定する値は実装側で固定する。

- `VpccBox::profile` = 0 (VP8 は profile 0 のみ)
- `VpccBox::bit_depth` = 8 (VP8 は 8-bit のみ)
- `VpccBox::chroma_subsampling` = 4:2:0 に対応する値 (VP8 は YUV 4:2:0 固定。VP Codec ISO Media File Format Binding の 3 ビット値では 0 = 4:2:0 vertical、1 = 4:2:0 colocated。VP8 は chroma siting を規定しないため既存テスト (`tests/test_boxes_sample_entry.rs` 内の `VpccBox` 生成箇所) と揃えて 1 を採用する)
- `VpccBox::codec_initialization_data` = 空バイト列

呼び出し側が明示する引数は以下に限定する。VP8 の 1 フレームから一意に導出できない、または VP8 の bit 情報 (color space / clamping type) と `VpccBox` フィールドの意味論が一致しないため。

- `level`: 単一フレームから確定できない (`Option<u8>` などで undefined を選べる形にする)
- `colour_primaries`: VP8 の color space bit から一意に導出しない
- `transfer_characteristics`: 同上
- `matrix_coefficients`: 同上
- `video_full_range_flag`: VP8 の clamping type と同義ではないので対応付けない
- `VisualSampleEntryFields::width` / `height`: 対象サンプルエントリーが参照する全サンプルを収容できる値を呼び出し側が指定する。VP8 では通常フレーム間で解像度は変わらないが、複数キーフレーム間で異なる寸法を持つ MP4 も理屈上は作れるため、単一キーフレームの値を無条件にトラック全体の値にしない
- `VisualSampleEntryFields::data_reference_index`

`VisualSampleEntryFields` の残りのフィールド (`horizresolution` / `vertresolution` / `frame_count` / `compressorname` / `depth`) は既存の `VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION` / `DEFAULT_VERTRESOLUTION` / `DEFAULT_FRAME_COUNT` / `NULL_COMPRESSORNAME` / `DEFAULT_DEPTH` を使って構築 API 側で埋める (呼び出し側が指定する手段は用意しない)。`Vp08Box::unknown_boxes` は構築 API では常に空 `Vec` を設定する。

構築 API の骨格例を以下に示す (型名は実装時に既存 API と整合させて調整可)。

```rust
/// `Vp08Box` の構築に必要な、ストリームから一意に決まらない設定値
#[derive(Debug, Clone)]
pub struct Vp8SampleEntryConfig {
    pub level: Option<u8>,             // None は undefined を意味する
    pub video_full_range_flag: bool,
    pub colour_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub width: u16,                    // トラック全体の幅上限
    pub height: u16,                   // トラック全体の高さ上限
    pub data_reference_index: NonZeroU16,
}

pub fn build_vp08_box(config: &Vp8SampleEntryConfig) -> Result<Vp08Box>;
```

`build_vp08_box` は解析結果 (`Vp8FrameHeader`) を引数に取らず、呼び出し側が集約した上限値を `Vp8SampleEntryConfig::width` / `height` に指定する形にする。単一フレームの解析結果から `Vp08Box` を組むケースでは、呼び出し側が `parse_frame_header` の返り値 (`Vp8FrameHeader::keyframe` の中の `Vp8KeyFrameInfo::width` / `height`) を `Vp8SampleEntryConfig::width` / `height` に写す。

Hisui の BT.709、limited range など個別プロジェクトに閉じた典型値を暗黙の固定値として移植しない。公開 API は `no_std` を維持し、crate 本体 (`shiguredo_mp4`) に新しい外部依存は追加せず、エラーを既存の `crate::Error` に統合する。

### VP9 との関係

VP8 と VP9 の公開パーサー、フレームヘッダー型、設定型を共有しない。`Vp08Box` / `Vp09Box` が同じ形であることを理由に共通トレイトや共通 enum を作らない。既存の `VpccBox` を結果として使うことだけを共有点とする。

### テスト

- 単体テスト (`tests/test_bitstream_vp8.rs`): キーフレーム / interframe / show_frame / version / first_partition_size / キーフレームの width / height / スケールの決定的テスト、および短い入力・不正な開始コード・予約済み version (4..=7)・ゼロ寸法・first_partition_size 境界超過を拒否することを確認する
- PBT (`pbt/tests/prop_bitstream_vp8.rs`): `noprop` サンプラーで生成した frame tag のビット配置を検証する。`pbt/Cargo.toml` の `[dev-dependencies]` に `noprop` を追加する (既存の proptest はそのまま残し、本 issue では移行しない)
- 実データ fixture: libvpx が生成した VP8 キーフレームを `tests/testdata/` に追加する (既存の `black-vp9-video.mp4` などと同じディレクトリ)。ネットワークや外部コマンドなしでテストが完結すること
- Fuzzing: `fuzz/fuzz_targets/fuzz_bitstream_vp8.rs` に `parse_frame_header` を対象とするターゲットを追加し、`fuzz/Cargo.toml` に対応する `[[bin]]` エントリを追加する

### 対象外

- compressed header 本体、partition 本体、マクロブロックの解析やデコード
- VP9 との公開 API 共通化
- Hisui 側の呼び出し置換と依存バージョン更新
- RTP / SDP、コーデック文字列生成 (RFC 6381 の `codecs=` パラメータ生成など)
- C API / WASM バインディング

## 完了条件

- `bitstream::vp8` が公開され、VP8 の uncompressed data chunk からフレーム種別、キーフレーム情報、解像度などを取得できること
- `Vp8FrameHeader` および `Vp8KeyFrameInfo` (または相当する公開型) が RFC 6386 の frame tag 4 フィールドとキーフレーム 7 バイト分の情報を保持すること
- `parse_frame_header` が以下すべてを `crate::Error` として拒否すること: 3 バイト (キーフレームは 10 バイト) 未満の入力、キーフレーム開始コード不一致、`version` が 4..=7、キーフレームのゼロ寸法、`first_partition_size` の境界超過
- `Vp08Box` を構築する API が用意され、profile / bit_depth / chroma_subsampling / codec_initialization_data は VP8 仕様固定値を設定し、level / colour_primaries / transfer_characteristics / matrix_coefficients / video_full_range_flag / `VisualSampleEntryFields::width` / `height` / `data_reference_index` は呼び出し側が明示する引数から設定すること
- 色特性や level を Hisui の固定値で暗黙に決定していないこと
- VP9 との共通公開型、共通トレイト、共通パーサーが追加されていないこと
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と crate 本体の依存ライブラリ 0 を維持すること (pbt 側の `noprop` 追加は crate 本体の依存ではない)
- 決定的テスト、`noprop` PBT、実データ fixture、fuzz target が追加され、`fuzz/Cargo.toml` に `[[bin]]` エントリが登録されていること
- 公開 API の rustdoc に解析範囲、導出できる値と呼び出し側が指定する値、エラー条件が記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
