# AV1 ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/add-av1-bitstream-utilities
- Polished: {YYYY-MM-DD}

## 目的

AV1 の Low Overhead Bitstream Format に含まれる OBU 列、Sequence Header OBU、フレームヘッダー先頭部を解析し、実際のストリーム情報から `av01` / `av1C` を構築できる汎用ユーティリティを追加する。

MP4 の `configOBUs` とサンプルでは OBU のサイズフィールドに異なる制約があり、`Av1cBox` の各フィールドは Sequence Header OBU と一致させる必要がある。これらを `shiguredo_mp4` で一貫して扱えるようにする。

## 現状

- `src/boxes_sample_entry.rs` には `Av01Box` と `Av1cBox` があるが、`config_obus` や MP4 サンプルを解析する API はない
- `shiguredo/hisui` の `src/video/av1.rs` は profile、level、bit depth、chroma subsampling、色設定を固定値として `Av01Box` を構築しており、入力ストリームの Sequence Header OBU を反映していない
- `shiguredo/sora-rust-sdk` の `src/video_codecs/av1.rs` には LEB128、OBU、Sequence Header、フレームヘッダー先頭部のパーサーがあるが、libwebrtc / RTP の利用条件に由来する Tile List 拒否、単一 operating point 制限、OBU 配置方針も同じモジュールに含まれている
- Sora Rust SDK の open issue `0097-bug-preserve-mp4-av1-config-obus.md` でも同種の解析が必要になっており、汎用部分を crate 側へ置く価値がある

参照仕様は [AV1 Bitstream & Decoding Process Specification](https://aomediacodec.github.io/av1-spec/av1-spec.pdf) と [AV1 Codec ISO Media File Format Binding](https://aomediacodec.github.io/av1-isobmff/) とする。

## 設計方針

### モジュールと解析モード

公開 API は `bitstream::av1` に配置する。OBU のヘッダー、種別、extension header、payload の範囲を借用ベースで返すパーサーを提供する。

AV1 ISOBMFF Binding の違いを API で明示するため、少なくとも次の 2 つの入力コンテキストを enum などで区別する。真偽値は使わない。

- `av1C` の `configOBUs`: すべての OBU で `obu_has_size_field = 1` が必須
- MP4 サンプル: 原則としてサイズフィールドが必要だが、最後の OBU だけは省略でき、その場合はサンプル末尾までを payload とする

OBU パーサーは `forbidden_bit`、reserved bit、extension header の reserved bits、LEB128 の終了・桁数・オーバーフロー、宣言サイズの境界を検証する。仕様で未知または予約済みの OBU 種別を構文上読み飛ばせる場合と、MP4 サンプルとして禁止される OBU を区別する。

### 公開 API の責務

`bitstream::av1` では、少なくとも次の操作を公開する。型名と関数名は実装時に Rust の既存 API と整合させる。

- AV1 の LEB128 を安全にデコードする
- OBU 列を解析し、OBU 種別、temporal ID、spatial ID、ヘッダー・payload・OBU 全体の範囲を取得する
- Sequence Header OBU を解析し、`Av1cBox` に必要な `seq_profile`、先頭 operating point の level / tier、bit depth、monochrome、chroma subsampling、chroma sample position と、フレームヘッダー先頭部の解析に必要な状態を取得する
- `OBU_FRAME_HEADER` または `OBU_FRAME` の uncompressed header 先頭部から、`show_existing_frame`、frame type、`show_frame` などランダムアクセス判定に必要な最小限の情報を取得する
- `configOBUs` と解析済み Sequence Header から、具体的な `Av1cBox` および `Av01Box` を構築する

Sequence Header に複数 operating point がある正当な入力を Sora Rust SDK の都合で拒否しない。`Av1cBox` に必要な index 0 の値は必ず取得し、残りの operating point は後続構文を正しく走査できるだけの情報を解析する。公開結果へどこまで保持するかは実装時に最小限で決める。

`configOBUs` 内に Sequence Header OBU がある場合は、構築する `Av1cBox` の各フィールドとの一致を保証する。Sequence Header OBU の個数と配置は AV1 ISOBMFF Binding に従って検証する。`initial_presentation_delay_minus_one` は Sequence Header だけでは一意に決まらないため、呼び出し側の明示値として扱う。

Hisui の固定 profile / 色設定や、Sora Rust SDK の RTP packetization、libwebrtc の受理条件、単一 operating point 方針は持ち込まない。公開 API は `no_std` を維持し、新しい外部依存は追加せず、エラーを既存の `crate::Error` に統合する。

### テスト

- 1 バイト / 複数バイト / 非最短表現の LEB128 と、未終端・8 バイト超過・`u32` 超過を確認する
- config / sample の両コンテキストで、サイズフィールドあり・最後だけ省略・複数 OBU・空入力の契約を確認する
- forbidden / reserved bit、短い extension header、宣言サイズ超過、禁止 OBU、Sequence Header の個数・配置違反を確認する
- profile 0 / 1 / 2、8 / 10 / 12 bit、monochrome、chroma subsampling、複数 operating point を含む Sequence Header を確認する
- reduced still picture header と通常ヘッダーのフレーム種別先頭部を確認する
- aomenc などが生成した実データを小さな fixture としてリポジトリに含める
- `noprop` で OBU 境界が入力を重複なく覆うこと、LEB128、構築した正当なヘッダーの不変条件を検証する
- 公開パーサーを対象とする `cargo-fuzz` の fuzz target を追加する

### 対象外

- AV1 の完全なデコーダー、tile data、算術符号化された構文の解析
- RTP payload、SDP、libwebrtc 固有の OBU フィルタリングや並べ替え
- Sora Rust SDK の issue 0097 自体の実装、利用側の依存バージョン更新
- C API / WASM バインディング

## 完了条件

- `bitstream::av1` が公開され、LEB128、OBU 列、Sequence Header、フレームヘッダー先頭部の解析が利用できること
- `configOBUs` と MP4 サンプルのサイズフィールド規則を API 上で区別して検証できること
- Sequence Header から固定値を使わず `Av1cBox` / `Av01Box` を構築でき、`configOBUs` との整合性が保証されること
- 複数 operating point を利用側固有の理由で拒否しないこと
- RTP / libwebrtc 固有のポリシーが含まれていないこと
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と依存ライブラリ 0 を維持すること
- 決定的テスト、`noprop`、実データ fixture、fuzz target が追加されていること
- 公開 API の rustdoc に解析コンテキスト、サイズフィールド規則、保持するバイト範囲、エラー条件が記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
