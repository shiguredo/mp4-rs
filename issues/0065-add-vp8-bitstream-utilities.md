# VP8 ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/add-vp8-bitstream-utilities
- Polished: {YYYY-MM-DD}

## 目的

VP8 フレームの uncompressed data chunk を解析し、キーフレーム判定、解像度、`vp08` / `vpcC` の構築に必要なストリーム情報を得る汎用ユーティリティを追加する。

固定値でサンプルエントリーを組み立てるのではなく、入力ストリームから取得できる情報と呼び出し側が指定すべき表示情報を区別し、`Vp08Box` を安全に構築できるようにする。

## 現状

- `src/boxes_sample_entry.rs` には `Vp08Box` と `VpccBox` があるが、VP8 フレームを解析する API はない
- `shiguredo/hisui` の `src/video/vpx.rs` は VP8 用の profile、level、bit depth、chroma subsampling、range、色特性を固定値としてサンプルエントリーを構築している
- VP8 と VP9 は同じ `vpcC` ボックス形式を使うが、独立したコーデック仕様であり、ビットストリームの解析処理や公開型を共通化する根拠はない

参照仕様は [RFC 6386: VP8 Data Format and Decoding Guide](https://www.rfc-editor.org/rfc/rfc6386) と [VP Codec ISO Media File Format Binding](https://www.webmproject.org/vp9/mp4/) とする。

## 設計方針

公開 API は `bitstream::vp8` に配置する。VP8 の uncompressed data chunk を解析し、少なくとも次の情報を返す。

- frame type とキーフレームかどうか
- version、show frame、first partition size
- キーフレームの場合の 3 バイト開始コードの妥当性、幅・高さ、水平・垂直スケール

短すぎるフレーム、未定義の version、不正なキーフレーム開始コード、ゼロの幅・高さ、宣言された first partition が入力境界を超える場合を `crate::Error` として扱う。`color_space` と `clamping_type` は uncompressed data chunk ではなく boolean-coded な第 1 partition のヘッダーにあるため、本 issue のパーサーでは扱わない。圧縮ヘッダーやマクロブロックデータまで解析する完全な VP8 デコーダーにはしない。

### サンプルエントリー構築

解析結果と呼び出し側の設定から、具体的な `Vp08Box` を構築する API を追加する。VP Codec ISO Media File Format Binding に従い、次を明示する。

- VP8 の profile は 0、bit depth は 8 とするなど、VP8 仕様から確定する値は実装側で設定する
- `level` はストリームの 1 フレームだけから一般には確定できないため、呼び出し側の明示値とするか undefined を指定できるようにする
- `colour_primaries`、`transfer_characteristics`、`matrix_coefficients` は VP8 の color space bit だけから一意に導出せず、呼び出し側が明示する
- `video_full_range_flag` は VP8 の clamping type と同義ではないため対応付けず、呼び出し側が明示する
- `codec_initialization_data` は VP8 では空にする
- `VisualSampleEntryFields` の幅・高さには対象サンプルエントリーが参照する全サンプルの上限が必要であるため、単一キーフレームの値を無条件にトラック全体の上限と見なさない。呼び出し側が上限を指定できる形にする

Hisui の BT.709、limited range、4:2:0 などの典型値を暗黙の固定値として移植しない。公開 API は `no_std` を維持し、新しい外部依存は追加せず、エラーを既存の `crate::Error` に統合する。

### VP9 との関係

VP8 と VP9 の公開パーサー、フレームヘッダー型、設定型を共有しない。`Vp08Box` / `Vp09Box` が同じ形であることを理由に共通トレイトや共通 enum を作らない。既存の `VpccBox` を結果として使うことだけを共有点とする。

### テスト

- キーフレームと interframe、show frame、version、first partition size を決定的テストで確認する
- キーフレームの幅・高さ・scale を確認する
- 短い入力、不正な開始コード、予約済み version、ゼロ寸法、partition 境界超過を拒否することを確認する
- libvpx が生成した実データを小さな fixture としてリポジトリに含める
- `noprop` で uncompressed data chunk のビット配置と境界条件を検証する
- 公開パーサーを対象とする `cargo-fuzz` の fuzz target を追加する

### 対象外

- boolean-coded header に含まれる color space / clamping type、partition 本体、マクロブロックの解析やデコード
- VP9 との公開 API 共通化
- Hisui 側の呼び出し置換と依存バージョン更新
- RTP / SDP、コーデック文字列生成
- C API / WASM バインディング

## 完了条件

- `bitstream::vp8` が公開され、VP8 の uncompressed data chunk からフレーム種別、キーフレーム情報、解像度などを取得できること
- 解析結果と明示的な設定から `Vp08Box` を構築できること
- 色特性や level を Hisui の固定値で暗黙に決定していないこと
- VP9 との共通公開型、共通トレイト、共通パーサーが追加されていないこと
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と依存ライブラリ 0 を維持すること
- 決定的テスト、`noprop`、実データ fixture、fuzz target が追加されていること
- 公開 API の rustdoc に解析範囲、導出できる値と呼び出し側が指定する値、エラー条件が記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
