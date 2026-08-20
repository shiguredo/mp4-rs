# VP9 ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/add-vp9-bitstream-utilities
- Polished: 2026-08-20

## 目的

VP9 の uncompressed header を解析し、profile、bit depth、色・クロマ情報、フレーム種別、解像度など `vp09` / `vpcC` の構築と MP4 サンプル処理に必要な情報を得る汎用ユーティリティを追加する。

VP8 と表面的に共通化せず、VP9 の可変プロファイル、show-existing-frame、参照フレーム、解像度変更などを仕様どおり表現する。

## 現状

- `src/boxes_sample_entry.rs` には `Vp09Box` と `VpccBox` があるが、VP9 フレームを解析する API はない
- `shiguredo/hisui` の `src/video/vpx.rs` は VP9 用の profile、level、bit depth、chroma subsampling、range、色特性を固定値としてサンプルエントリーを構築している
- VP9 では profile によって bit depth と chroma subsampling の組み合わせが変わり、フレームヘッダーにも frame size、render size、color config が含まれるため、固定値ではストリームと `vpcC` の整合を保証できない
- VP8 と VP9 は同じ `vpcC` ボックス形式を使うが、ビットストリーム構文は独立している

参照仕様は WebM Project が公開する [VP9 Bitstream and Decoding Process Specification](https://www.webmproject.org/vp9/) と [VP Codec ISO Media File Format Binding](https://www.webmproject.org/vp9/mp4/) とする。VP9 仕様が draft であることを認識し、実装時には同ページの authoritative source と libvpx の挙動も突き合わせる。

## 設計方針

### モジュール構成

`src/lib.rs` から公開する `bitstream` モジュール配下に VP9 用のサブモジュールを追加する。`mod.rs` は使わない (shiguredo-rust 規約)。

```text
src/bitstream.rs
src/bitstream/vp9.rs
```

`src/bitstream.rs` は既に姉妹 issue 0065 (VP8) の実装で追加済みなので、`pub mod vp9;` を追記するだけで足りる。他 3 コーデックの issue (0062 / 0063 / 0064) と実装が並列する場合、`src/bitstream.rs` の追加コミットが競合し得る点に注意する。VP9 の解析処理は NAL 層 (`src/bitstream/nal.rs`) に依存せず、`bitstream::vp9` 単独で完結する。

### フレーム解析 API

`bitstream::vp9` は VP9 の uncompressed header を解析する API を公開する。返す情報は少なくとも次のとおり。

- `frame_marker` (常に 2) と `profile` (0..=3)
- `show_existing_frame` と参照する `frame_to_show_map_idx` (0..=7)
- `frame_type` (KEY_FRAME / NON_KEY_FRAME)、`show_frame`、`error_resilient_mode`、`intra_only`
- profile に応じた `bit_depth` (8 / 10 / 12)、`color_space` (0..=7)、`color_range` (0 / 1)、`subsampling_x` / `subsampling_y`
- key frame / intra-only frame の frame size (`width`、`height`) と render size (`render_width`、`render_height`)
- inter frame で参照フレームのサイズを利用する構文 (`frame_size_with_refs` 経路) の解決状態

VP9 のフレームサイズは inter frame の `frame_size_with_refs` 経路では現在のフレームヘッダーだけでは確定しない。パーサーを隠れたグローバル状態に依存させず、参照フレーム寸法が必要な経路は **戻り値の型として未解決状態を保持する** 方針とする。単純なキーフレーム判定だけを行う利用者に不要な状態管理を強制しないため、`parse_frame_header` は解析コンテキスト引数を必須にしない (context の適用は呼び出し側の後処理に任せる)。

以下は公開 API の骨格を示す (型名・列挙定義は実装時に既存 API と整合させて調整可)。

```rust
/// VP9 のフレーム種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vp9FrameType {
    Key,
    NonKey,
}

/// VP9 の uncompressed header から取得できるフレーム情報
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Vp9FrameHeader {
    pub profile: u8,                     // 0..=3
    /// Some(0..=7) なら該当インデックスの復元済みフレームを表示する。
    /// この場合それ以外の header フィールドは未定義扱い
    pub show_existing_frame: Option<u8>,
    pub frame_type: Vp9FrameType,
    pub show_frame: bool,
    pub error_resilient_mode: bool,
    pub intra_only: bool,
    pub bit_depth: u8,                   // 8, 10, 12 のいずれか
    /// 0 = Unknown、1 = BT.601、2 = BT.709、3 = SMPTE 170、
    /// 4 = SMPTE 240、5 = BT.2020、6 = Reserved、7 = sRGB
    pub color_space: u8,
    pub color_range: u8,                 // 0 = studio swing / 1 = full swing
    pub subsampling_x: u8,               // 0 or 1 (color_space = sRGB のときは常に 0)
    pub subsampling_y: u8,               // 0 or 1
    pub frame_size: Vp9FrameSize,
    /// (render_width, render_height)。header に含まれない場合は None
    pub render_size: Option<(u16, u16)>,
}

/// フレームサイズ (参照フレーム寸法が必要な inter 経路では未解決の可能性がある)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Vp9FrameSize {
    /// key / intra-only / inter で自己完結する frame size を持つ
    Resolved { width: u16, height: u16 },
    /// inter で `frame_size_with_refs` により参照フレームのサイズを引き継ぐ
    /// (呼び出し側が参照スロットの寸法から解決する。ref_frame_idx はスロットインデックス 0..=7)
    UsesRefFrames { ref_frame_idx: [u8; 3] },
}

pub fn parse_frame_header(input: &[u8]) -> Result<Vp9FrameHeader>;
```

`parse_frame_header` の入力境界検証は以下を `crate::Error` として拒否する。

- 入力が `frame_marker` / `profile` を読むのに十分な長さでない
- `frame_marker` が 2 と一致しない
- `profile` の予約ビット (profile 3 の reserved zero bit) が 0 でない
- key frame の `sync_code` が `0x49 0x83 0x42` と一致しない
- profile と bit depth / subsampling の組み合わせが仕様外 (例: profile 0 で `bit_depth != 8`、profile 1 で `subsampling_x == 1 && subsampling_y == 1`)
- RGB (`color_space == 7` (sRGB)) で subsampling が 4:4:4 以外
- 切り詰められたヘッダー
- ゼロ寸法 (`frame_width == 0` または `frame_height == 0`)
- `subsampling_x == 0 && subsampling_y == 1` (仕様外組み合わせ)

圧縮ヘッダーや tile data を解析する完全な VP9 デコーダーにはしない。

### サンプルエントリー構築 API

解析結果と呼び出し側の設定から具体的な `Vp09Box` を構築する API を追加する。VP9 仕様および VP Codec ISO Media File Format Binding から確定する値は実装側で固定する。呼び出し側指定値は必要最小限に絞る。

#### 固定値 (関数側で埋める)

- `VpccBox::codec_initialization_data` = 空バイト列 (VP9 では常に空)
- `Vp09Box::unknown_boxes` = 空 `Vec`
- `VisualSampleEntryFields` の `horizresolution` / `vertresolution` / `frame_count` / `compressorname` / `depth` = 同構造体のデフォルト
- `VisualSampleEntryFields::data_reference_index` = `VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX` (自ファイル参照の単一 `dref` エントリー。特殊な dref 構成が必要な場合は戻り値の `Vp09Box::visual` を書き換える。姉妹 issue 0065 の判断を継承)

#### ストリーム導出値 (`Vp9FrameHeader` から `VpccBox` へ写す)

- `VpccBox::profile` = `Vp9FrameHeader::profile`
- `VpccBox::bit_depth` = `Vp9FrameHeader::bit_depth` を `Uint<u8, 4, 4>` に格納
- `VpccBox::chroma_subsampling` = `Vp9FrameHeader::subsampling_x` / `subsampling_y` から VP Codec Binding の 3 ビット値へマッピング (下記 `subsampling → chroma_subsampling` を参照)
- `VpccBox::video_full_range_flag` = `Vp9FrameHeader::color_range` (`Uint<u8, 1>` に格納)

#### 呼び出し側指定値 (`Vp9SampleEntryConfig`)

- `level`: VP Codec Binding で 10..=62 が定義される (10 / 11 / 20 / 21 / 30 / 31 / 40 / 41 / 50 / 51 / 52 / 60 / 61 / 62)。単一フレームからは確定できないため呼び出し側指定。`Option<u8>` で保持し `None` は 0 (Undefined) として書き込む
- `colour_primaries` / `transfer_characteristics` / `matrix_coefficients`: VP9 の `color_space` (0..=7) から ISO/IEC 23001-8 の細分値へ仕様上一意には決まらないため、姉妹 issue 0065 と揃えて呼び出し側が明示する (自動導出はしない)
- `width` / `height`: サンプルエントリーが参照する全サンプルの上限。VP9 は動的解像度を持つため、呼び出し側が集約した最大値を指定する

姉妹 issue 0065 の判断を継承し、色特性の頻出値を `Vp9SampleEntryConfig` の `pub const` として提供する。VP8 版と共通の BT.709 / BT.601 / Unspecified に加え、VP9 で頻出する BT.2020 と sRGB も追加する。

以下は骨格例 (型名・定数名は実装時に既存 API と整合させて調整可)。

```rust
/// `Vp09Box` の構築に必要な、ストリームから一意に決まらない設定値
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Vp9SampleEntryConfig {
    /// VP コーデックのレベル (`None` は 0 = Undefined として書き込む)
    pub level: Option<u8>,
    pub colour_primaries: u8,
    pub transfer_characteristics: u8,
    pub matrix_coefficients: u8,
    pub width: u16,
    pub height: u16,
}

impl Vp9SampleEntryConfig {
    // 姉妹 issue 0065 と揃えた頻出値定数を提供する
    // (BT.709 / BT.601 / Unspecified) に加え VP9 特有の頻出値
    // (BT.2020 / sRGB) も pub const で用意する。
    // 命名は Vp8SampleEntryConfig と同じ形式 (COLOUR_PRIMARIES_BT709 等)
}

pub fn build_vp09_box(
    header: &Vp9FrameHeader,
    config: &Vp9SampleEntryConfig,
) -> Vp09Box;
```

戻り値は `Vp09Box` (実装本体に `Err` を返す経路がない場合。姉妹 issue 0065 の判断を継承)。もし `level` の範囲外値 (10..=62 以外) を拒否したい等の判断で `Err` 経路が必要になれば `Result<Vp09Box>` に切り替える。その場合は根拠を実装時に明記する。

`build_vp09_box` が `Vp9FrameHeader` を引数に取るのは、profile / bit_depth / chroma_subsampling / video_full_range_flag をストリーム由来値としてそのまま `VpccBox` に写す設計のため。`show_existing_frame` フレームなど header 情報が実質的に無いケースへの対応は、そのフレームを sample entry の代表フレームに選ばない (呼び出し側で先に判定する) 前提とする。

#### `subsampling_x` / `subsampling_y` → `VpccBox::chroma_subsampling` マッピング

VP Codec Binding の 3 ビット値は次のとおり。

- 0 = 4:2:0 vertical (chroma_sample_position が vertical)
- 1 = 4:2:0 colocated (chroma_sample_position が colocated)
- 2 = 4:2:2
- 3 = 4:4:4

VP9 uncompressed header の `subsampling_x` / `subsampling_y` から次のようにマッピングする。

- `subsampling_x = 1, subsampling_y = 1` → 4:2:0。VP9 の uncompressed header には chroma siting を示すフィールドが含まれない (chroma_sample_position は AV1 の sequence header に存在するフィールドであり、VP9 では登場しない) ため、姉妹 issue 0065 と揃えて常に 1 (colocated) を採用する
- `subsampling_x = 1, subsampling_y = 0` → 2 (4:2:2)
- `subsampling_x = 0, subsampling_y = 0` → 3 (4:4:4)
- `subsampling_x = 0, subsampling_y = 1` → 仕様外 (`parse_frame_header` 側で拒否済み)

### VP8 との関係

VP8 と VP9 の公開パーサー、フレームヘッダー型、設定型を共有しない。`Vp08Box` / `Vp09Box` の同型性を理由に共通トレイトや共通 enum を作らない。共通点は既存の `VpccBox` を結果として利用することに限定する。

### テスト

- 単体テスト (`tests/test_bitstream_vp9.rs`): profile 0 〜 3、8 / 10 / 12 bit、各 chroma subsampling、RGB、limited / full range を決定的テストで確認する。key / inter / intra-only / show-existing-frame、frame size、render size、参照寸法を使う経路 (`Vp9FrameSize::UsesRefFrames`) を確認する。不正な frame marker、reserved bit、sync code、profile と色設定の矛盾、短い入力、ゼロ寸法、`subsampling_x=0, subsampling_y=1` の仕様外組み合わせを拒否することを確認する
- PBT (`pbt/tests/prop_bitstream_vp9.rs`): `noprop` サンプラーで uncompressed header のビット配置、profile と色設定の組み合わせ、境界条件を検証する。`pbt/Cargo.toml` の `[dev-dependencies]` に `noprop` は既に (issue 0068 で) 追加済み
- 実データ fixture: libvpx が生成した VP9 キーフレーム / inter frame を `tests/testdata/` に追加する (既存の `black-vp8-keyframe.vp8` などと同じディレクトリ)。ネットワークや外部コマンドなしでテストが完結すること
- Fuzzing: `fuzz/fuzz_targets/fuzz_bitstream_vp9.rs` に `parse_frame_header` を対象とするターゲットを追加し、`fuzz/Cargo.toml` に対応する `[[bin]]` エントリを追加する

### 対象外

- compressed header、tile data、superframe index の解析やデコード。superframe index が必要になった場合は別 issue とする
- VP8 との公開 API 共通化
- Hisui 側の呼び出し置換と依存バージョン更新
- RTP / SDP、コーデック文字列生成
- C API / WASM バインディング

## 完了条件

- `bitstream::vp9` が公開され、`Vp9FrameHeader` (または相当する公開型) が VP9 uncompressed header の `frame_marker` / `profile` / `show_existing_frame` / `frame_to_show_map_idx` / `frame_type` / `show_frame` / `error_resilient_mode` / `intra_only` / `bit_depth` / `color_space` / `color_range` / `subsampling_x` / `subsampling_y` / `frame_size` / `render_size` を保持すること
- 参照フレーム寸法が必要な構文が `Vp9FrameSize` の未解決 variant (`UsesRefFrames`) として型で表現され、`parse_frame_header` が context 引数なしで取り出せること
- `Vp09Box` を構築する API (`build_vp09_box`) が用意され、profile / bit_depth / chroma_subsampling / video_full_range_flag はストリーム由来値 (`Vp9FrameHeader` 経由) を反映し、level / colour_primaries / transfer_characteristics / matrix_coefficients / width / height は `Vp9SampleEntryConfig` の呼び出し側指定を反映し、codec_initialization_data / `VisualSampleEntryFields` の各種 `DEFAULT_*` / `data_reference_index` / `unknown_boxes` は関数側で固定すること
- 色特性の頻出値定数 (BT.709 / BT.601 / BT.2020 / sRGB / Unspecified、各 3 種 = 15 個) が `Vp9SampleEntryConfig` の `pub const` として提供されていること
- 色特性や level を Hisui の固定値で暗黙に決定していないこと
- VP8 との共通公開型、共通トレイト、共通パーサーが追加されていないこと
- `parse_frame_header` が以下すべてを `crate::Error` として拒否すること: 短い入力、frame_marker 不一致、profile reserved bit、key frame の sync code 不一致、profile と bit depth / subsampling の矛盾、RGB での subsampling 違反、切り詰められた入力、ゼロ寸法、`subsampling_x=0, subsampling_y=1` の仕様外組み合わせ
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と crate 本体の依存ライブラリ 0 を維持すること (pbt 側の `noprop` は crate 本体の依存ではない)
- 決定的テスト (`tests/test_bitstream_vp9.rs`)、`noprop` PBT (`pbt/tests/prop_bitstream_vp9.rs`)、実データ fixture (`tests/testdata/` 配下)、fuzz target (`fuzz/fuzz_targets/fuzz_bitstream_vp9.rs`) が追加され、`fuzz/Cargo.toml` に対応する `[[bin]]` エントリが登録されていること
- 公開 API の rustdoc に「固定値 / ストリーム導出値 / 呼び出し側指定値」の 3 分類、参照フレーム未解決状態 (`Vp9FrameSize::UsesRefFrames`) の解決方法、エラー条件が記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
