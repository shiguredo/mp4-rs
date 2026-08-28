# AV1 ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: 2026-08-25
- Branch: feature/add-av1-bitstream-utilities
- Polished: 2026-08-21

## 目的

AV1 の Low Overhead Bitstream Format に含まれる OBU 列、Sequence Header OBU、フレームヘッダー先頭部を解析し、実際のストリーム情報から `av01` / `av1C` を構築できる汎用ユーティリティを追加する。

MP4 の `configOBUs` とサンプルでは OBU のサイズフィールドに異なる制約があり、`Av1cBox` の各フィールドは Sequence Header OBU と一致させる必要がある。これらを `shiguredo_mp4` で一貫して扱えるようにする。

## 現状

- `src/boxes_sample_entry.rs` には `Av01Box` と `Av1cBox` があるが、`config_obus` や MP4 サンプルを OBU として解析する API はない。`Av1cBox::config_obus` は残バイトの生 `Vec<u8>` である
- `src/bitstream.rs` は既に公開されており、`vp8` / `vp9` だけを公開している。`av1` サブモジュールは無い
- `shiguredo/hisui` の `src/video/av1.rs` の `av1_sample_entry` は、`Av1cBox` のレコード欄 (`seq_profile` / `seq_level_idx_0` / `seq_tier_0` / `high_bitdepth` / `twelve_bit` / `monochrome` / chroma 欄) を固定値で埋めている。`config_obus` バイト列は引数をそのまま格納し、中身とレコード欄の一致は見ていない。幅・高さは呼び出し側引数である
- `shiguredo/sora-rust-sdk` の `src` に AV1 OBU / Sequence Header パーサーは無い。`src/video_codecs/mp4.rs` の `SampleEntry::Av01` 分岐は `visual.width` / `height` だけを読み、`config_obus` は保存しない
- 同 SDK の open issue `0097-bug-preserve-mp4-av1-config-obus.md` が同種の解析を要求している。その設計には RTP / libwebrtc 由来の Tile List 拒否、単一 operating point 制限、OBU 順の制約がある。汎用部分を crate 側へ置く価値がある

参照仕様は [AV1 Bitstream & Decoding Process Specification](https://aomediacodec.github.io/av1-spec/)（以降 AV1 spec）と [AV1 Codec ISO Media File Format Binding v1.3.0](https://aomediacodec.github.io/av1-isobmff/v1.3.0.html)（以降 Binding）とする。

## 設計方針

### モジュール構成

`src/lib.rs` から公開済みの `bitstream` モジュール配下に AV1 用サブモジュールを追加する。`mod.rs` は使わない。

```text
src/bitstream.rs
src/bitstream/av1.rs
```

`src/bitstream.rs` に `pub mod av1;` を追記する。本体は `src/bitstream/av1.rs` に置く。他コーデックの issue (0062 / 0063 / 0069) と実装が並列する場合、`src/bitstream.rs` の追加コミットが競合し得る点に注意する。AV1 の解析は NAL 層に依存せず、`bitstream::av1` 単独で完結する。ビット読み取りヘルパは `bitstream::vp9` の `BitReader` と同様、モジュール内の非公開型に留める。

### 解析コンテキスト

Binding の違いを API で明示するため、入力コンテキストを enum などで区別する。真偽値は使わない。

- `ConfigObus`: `av1C` の `configOBUs`。Binding §2.3.4 により、すべての OBU で `obu_has_size_field = 1` が必須。空入力は許容する (zero or more OBUs)
- `Sample`: MP4 サンプル。Binding §2.4 により、最後以外の OBU は `obu_has_size_field = 1` が必須。最後の OBU だけ MAY で省略でき、省略時はサンプル末尾までを payload とする。空入力は Temporal Unit にならないため拒否する。`parse_obus` はこのコンテキストでも Sequence Header の個数・配置を検証しない。Binding §2.4 の sync sample 条件 (最初の Frame Header OBU または Frame OBU より前に Sequence Header があること。その前の Metadata OBU は NOTE で許容) は、RAP 判定が必要な呼び出し側が先頭部解析と組み合わせて使う

### OBU 列と LEB128

公開 API は OBU のヘッダー、種別、extension header、payload の範囲を借用ベースで列挙する。

AV1 spec §4.10.5 の `leb128()` を安全にデコードする API も公開する。`obu_size` がこの構文であり、OBU 列挙とは独立に呼び手が使えるようにするためである。テストのためだけに公開しない。上限は同節のとおり、最大 8 バイト、8 バイト目の continuation bit は 0、値は `(1 << 32) - 1` 以下。非最短表現は同節が許容するため受理する。未終端、8 バイト超過、`u32` 超過は `crate::Error` とする。

OBU ヘッダーは AV1 spec §5.3.2 / §6.2.2 に従い、次を検証する。

- `obu_forbidden_bit` は 0
- `obu_reserved_1bit` は 0
- extension header があるとき `extension_header_reserved_3bits` は 0
- 宣言サイズが入力境界を超えないこと

予約済み `obu_type` (0 および 9..=14) は、AV1 spec §5.4 の NOTE により `obu_size` を使って読み飛ばす。`obu_type` は 4 ビットなので、この予約値以外の未定義種別は無い。`OBU_TILE_LIST` (8) は予約ではなく、後述のとおり Error とする。

`OBU_SEQUENCE_HEADER` は layer-specific ではないため、`obu_extension_flag` は 0 でなければならない (AV1 spec §6.2.2 の表)。

Binding はこの版で Tile List をサポートしない (§1 NOTE)。サンプルでは `OBU_TILE_LIST` は SHALL NOT (§2.4)。両コンテキストで `OBU_TILE_LIST` を `crate::Error` とする。根拠は Binding であり、libwebrtc の RTP 除外理由ではない。

`OBU_TEMPORAL_DELIMITER` / `OBU_PADDING` / `OBU_REDUNDANT_FRAME_HEADER` はサンプルで SHOULD NOT (§2.4) であり、汎用パーサーは構文として受理する。拒否しない。

### Sequence Header とフレームヘッダー先頭部

Sequence Header OBU の payload を解析し、少なくとも次を返す。公開型のフィールド名は実装時に既存 API と整合させてよい。

- `Av1cBox` へ写す値: `seq_profile`、`seq_level_idx[0]`、`seq_tier[0]`、`high_bitdepth`、`twelve_bit`、`mono_chrome`、`subsampling_x` / `subsampling_y`、`chroma_sample_position`
- `Av01Box` の幅・高さへ写す値: `max_frame_width_minus_1 + 1`、`max_frame_height_minus_1 + 1`
- フレームヘッダー先頭部の解析に必要な状態 (`reduced_still_picture_header` など)。公開結果は Av1cBox / 寸法 / 先頭部に必要なものに限定する

`seq_profile` は 0..=2 のみ受理する (AV1 spec §6.4.1。3..=7 は予約)。

`twelve_bit` が構文に現れないときは 0 とする (AV1 spec §5.5.2。`seq_profile == 2 && high_bitdepth` のときだけ符号化。Binding §2.3.4 も不在時 SHALL 0)。`chroma_sample_position` が構文に現れないときも Binding §2.3.4 に従い 0 とする。

複数 operating point がある正当な入力を、Sora Rust SDK の都合で拒否しない。`Av1cBox` に必要な index 0 の値は必ず取得し、残りの operating point は後続構文を正しく走査できるだけ解析する。公開結果に全 operating point を保持する必要はない。

`OBU_FRAME_HEADER` と `OBU_FRAME` の payload 先頭は、いずれも uncompressed header である (AV1 spec §5.9 / §5.10。`frame_obu` は `frame_header_obu` のあと tile group が続く)。先頭部から Binding §2.4 の RAP 判定に必要な最小限、すなわち `show_existing_frame`、`frame_type`、`show_frame` を取得する。`reduced_still_picture_header == 1` のときは AV1 spec §5.9.2 が代入する値 (`show_existing_frame = 0`、`frame_type = KEY_FRAME`、`show_frame = 1`) を同じ条件として扱う。`show_existing_frame == 1` のときは同構文が早期 return し、このヘッダーに `show_frame` は現れない。その場合は `show_existing_frame == 1` だけで Binding の RAP 条件は満たさないと判定してよい。tile data や算術符号化された残りは解析しない。

### サンプルエントリー構築 API

解析済み Sequence Header と `configOBUs` バイト列から `Av01Box` を構築する。戻り値は `Av01Box` であり、`SampleEntry` に包まない。Hisui の固定 profile / 4:2:0 / 8-bit は移植しない。

#### 固定値 (関数側で埋める)

- `VisualSampleEntryFields` の `horizresolution` / `vertresolution` / `frame_count` / `compressorname` / `depth`: 同構造体のデフォルト (`NULL_COMPRESSORNAME` を含む)。Binding §2.2.4 の compressorname `"\012AOM Coding"` は RECOMMENDED であり SHALL ではない。姉妹の VP8 / VP9 構築に合わせ DEFAULT を使う。特殊な値が必要な場合は戻り値の `Av01Box::visual` を書き換える
- `VisualSampleEntryFields::data_reference_index` = `VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX`
- `Av01Box::unknown_boxes` = 空 `Vec`
- `Av1cBox` の marker / version は既存の `Av1cBox::encode` が書く値に任せる

#### ストリーム導出値 (Sequence Header から写す)

- `Av1cBox` の `seq_profile` / `seq_level_idx_0` / `seq_tier_0` / `high_bitdepth` / `twelve_bit` / `monochrome` / `chroma_subsampling_x` / `chroma_subsampling_y` / `chroma_sample_position` (Binding §2.3.4 の SHALL 一致)
- `VisualSampleEntryFields::width` / `height` = `max_frame_width_minus_1 + 1` / `max_frame_height_minus_1 + 1` (Binding §2.2.4 の SHALL)。VP8 / VP9 が呼び出し側にトラック上限を集約させたのとは仕様が異なる
- AV1 spec §5.5.1 の `frame_width_bits_minus_1` は `f(4)` なので、`max_frame_width_minus_1 + 1` は最大 65536 になり得る。`VisualSampleEntryFields::width` / `height` は `u16` (最大 65535) なので、65536 は `crate::Error` で拒否する。飽和しない

#### 呼び出し側指定値

- `initial_presentation_delay_minus_one`: Sequence Header だけでは一意に決まらない (Binding §2.3.4)。`Option` で保持し、`None` は `initial_presentation_delay_present = 0` として書き込む
- `config_obus`: 呼び出し側が渡すバイト列を `Av1cBox::config_obus` に格納する。構築前に `ConfigObus` コンテキストで解析し、次を検証する
  - Sequence Header OBU は高々 1 個。あるなら先頭 (Binding §2.3.4)。この個数・先頭制約は `configOBUs` 専用であり、サンプルには適用しない
  - Sequence Header がある場合、レコード欄は引数の解析済み Sequence Header と一致すること
  - 空の `configOBUs` は許容する。その場合もレコード欄と幅・高さは引数の Sequence Header から埋める

`colr` / `pasp` / `clli` / `mdcv` は本 issue の対象外とする。Binding は configOBUs に Sequence Header が無いとき nclx `colr` を SHALL とし、render 寸法が max frame と違うとき `pasp` を SHALL とするが、現行 `Av01Box` にこれらの型付き子ボックスは無い。本 issue の構築結果は `visual` + `av1c_box` までとする。

公開 API は `no_std` を維持し、crate 本体 (`shiguredo_mp4`) に新しい外部依存は追加しない。エラーは既存の `crate::Error` に統合する。

以下は骨格例である。型名・関数名は実装時に既存 API と整合させて調整してよい。

```rust
pub enum Av1ObuParseContext {
    ConfigObus,
    Sample,
}

pub fn decode_leb128(input: &[u8]) -> Result<(u32, usize)>;

pub fn parse_obus(input: &[u8], ctx: Av1ObuParseContext) -> Result<...>; // 借用ベースの列挙

pub fn parse_sequence_header(payload: &[u8]) -> Result<Av1SequenceHeader>;

pub fn parse_frame_header_prefix(
    payload: &[u8],
    seq: &Av1SequenceHeader,
) -> Result<Av1FrameHeaderPrefix>;

pub struct Av1SampleEntryConfig {
    pub initial_presentation_delay_minus_one: Option<u8>, // 0..=15。None は不在
}

pub fn build_av01_box(
    seq: &Av1SequenceHeader,
    config_obus: &[u8],
    config: &Av1SampleEntryConfig,
) -> Result<Av01Box>;
```

### テスト

- 単体テスト (`tests/test_bitstream_av1.rs`): LEB128 (1 バイト / 複数バイト / 非最短 / 未終端 / 8 バイト超過 / `u32` 超過)。`ConfigObus` では size 必須・最後の省略拒否・空入力の受理。`Sample` では最後だけ省略可・空入力の拒否。forbidden / reserved bit、短い extension header、宣言サイズ超過、`OBU_TILE_LIST`、configOBUs の Sequence Header 個数・先頭違反。profile 0 / 1 / 2、8 / 10 / 12 bit (`high_bitdepth` / `twelve_bit`)、monochrome、chroma subsampling、複数 operating point、reduced still picture header。フレーム先頭部の RAP 条件
- PBT (`pbt/tests/prop_bitstream_av1.rs`): `noprop` で OBU 境界が入力を重複なく覆うこと、LEB128、構築した正当なヘッダーの不変条件。`pbt/Cargo.toml` の noprop は issue 0068 で追加済み
- 実データ fixture: `tests/testdata/` に小さな AV1 OBU 列を置く。既存の `black-av1-video.mp4` から抽出してもよい。ネットワークや外部コマンドなしでテストが完結すること
- Fuzzing: `fuzz/fuzz_targets/fuzz_bitstream_av1.rs` に公開パーサーを対象とするターゲットを追加し、`fuzz/Cargo.toml` に `[[bin]]` エントリを追加する

### 対象外

- AV1 の完全なデコーダー、tile data、算術符号化された構文の解析
- `colr` / `pasp` / `clli` / `mdcv` の型付きボックス追加
- RTP payload、SDP、libwebrtc 固有の OBU フィルタリングや並べ替え
- Sora Rust SDK の issue 0097 自体の実装、利用側の依存バージョン更新
- C API / WASM バインディング

## 完了条件

- `bitstream::av1` が公開され、LEB128、OBU 列、Sequence Header、フレームヘッダー先頭部の解析が利用できること
- `ConfigObus` と `Sample` のサイズフィールド規則を API 上で区別して検証できること
- Sequence Header から Hisui の固定レコード欄を使わず `Av1cBox` / `Av01Box` を構築でき、`configOBUs` に Sequence Header があるときはレコード欄との一致が保証されること
- `VisualSampleEntryFields::width` / `height` が Sequence Header の max frame 寸法から埋まること
- 複数 operating point を利用側固有の理由で拒否しないこと
- RTP / libwebrtc 固有のポリシーが含まれていないこと。`OBU_TILE_LIST` の拒否根拠が Binding であること
- `OBU_TEMPORAL_DELIMITER` / `OBU_PADDING` / `OBU_REDUNDANT_FRAME_HEADER` を SHOULD NOT 理由だけで拒否していないこと
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と crate 本体の依存ライブラリ 0 を維持すること (pbt 側の `noprop` は crate 本体の依存ではない)
- 決定的テスト (`tests/test_bitstream_av1.rs`)、`noprop` PBT (`pbt/tests/prop_bitstream_av1.rs`)、実データ fixture (`tests/testdata/` 配下)、fuzz target (`fuzz/fuzz_targets/fuzz_bitstream_av1.rs`) が追加され、`fuzz/Cargo.toml` に `[[bin]]` エントリが登録されていること
- 公開 API の rustdoc に解析コンテキスト、サイズフィールド規則、固定値 / ストリーム導出値 / 呼び出し側指定値の分類、保持するバイト範囲、エラー条件が記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること

## 解決方法

`src/bitstream.rs` に `pub mod av1` を追加し、`src/bitstream/av1.rs` に以下の公開 API を追加した。

- `Av1ObuParseContext` (`ConfigObus` / `Sample`): Binding の違いを API で区別する入力コンテキスト
- `Av1ObuType` / `Av1Obu`: 借用ベースの OBU 列公開型
- `decode_leb128`: AV1 spec §4.10.5 の LEB128 デコード
- `parse_obus`: `obu_type` / reserved bit / extension header / `obu_size` を検証し、OBU 列を返す
- `Av1SequenceHeader` / `parse_sequence_header`: Sequence Header の解析。`av1C` / 寸法 / フレーム先頭部解析に必要な値だけを公開する
- `Av1FrameHeaderPrefix` / `parse_frame_header_prefix`: uncompressed header 先頭部の RAP 判定用解析
- `Av1SampleEntryConfig` / `build_av01_box`: 解析済み Sequence Header と `configOBUs` から `Av01Box` を構築する

### 設計方針からの変更点

コードレビューを経て以下を issue から変更した。

- `Av1FrameHeaderPrefix` は struct ではなく enum (`ShowExistingFrame` / `NewFrame { frame_type, show_frame }`) にした。`show_existing_frame == 1` のとき `frame_type` / `show_frame` が現れない排他関係を型で表現し、RAP 判定用に `is_rap` メソッドを追加した
- 非 layer-specific OBU (Sequence Header / Temporal Delimiter) の `obu_extension_flag = 1` を拒否する (AV1 spec §6.2.2)
- `reduced_still_picture_header == 1` のとき `still_picture = 1` を要求する (AV1 spec §5.5.2)
- `max_frame_width / max_frame_height` は `VisualSampleEntryFields` の `u16` に収まる 1..=65535 だけ受理する (issue のとおり 65536 は拒否)

### テストと fuzz

- 決定的テスト `tests/test_bitstream_av1.rs`、noprop PBT `pbt/tests/prop_bitstream_av1.rs`、実データ fixture (`tests/testdata/black-av1-video.mp4` / `black-av1-config-obus.bin`)、fuzz target `fuzz/fuzz_targets/fuzz_bitstream_av1.rs` を追加した
