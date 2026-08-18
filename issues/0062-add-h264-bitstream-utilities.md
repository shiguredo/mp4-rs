# H.264 ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/add-h264-bitstream-utilities
- Polished: {YYYY-MM-DD}

## 目的

H.264 の Annex B / length-prefixed NAL ユニット列の解析、SPS の解析、パラメータセットの抽出、`avc1` / `avcC` の構築を `shiguredo_mp4` の汎用ユーティリティとして提供する。

これらは MP4 ボックス自体の処理ではないが、H.264 ストリームから `Avc1Box` を構築する場合と、MP4 サンプルをデコーダーへ渡せる形式に変換する場合の双方で必要になる。利用側ごとの重複実装をなくし、MP4 のサンプルエントリーと整合する解析結果を 1 箇所で提供する。

## 現状

- `src/boxes_sample_entry.rs` には `Avc1Box` と `AvccBox` があるが、NAL ユニット列や SPS を解析してこれらを構築する API はない
- `shiguredo/hisui` の `src/video/h264.rs` には Annex B の走査、SPS の解析、Annex B からのサンプルエントリー構築、Annex B から length-prefixed 形式への変換が実装されている
- `shiguredo/sora-rust-sdk` の `src/video_codecs/mp4.rs` には length-prefixed 形式から Annex B への変換が独立に実装されている。切り詰められた NAL ユニットを黙って無視する挙動は汎用パーサーの契約として適切ではない
- H.264 と H.265 は Annex B の開始コード探索と length-prefixed 形式の境界検証を共有できるが、NAL ヘッダーの長さ、種別のビット配置、妥当性条件は異なる

参照仕様は [ITU-T H.264](https://www.itu.int/rec/T-REC-H.264-202606-I/en) および ISO/IEC 14496-15 とする。

## 設計方針

### モジュール構成

`src/lib.rs` から公開する `bitstream` モジュールを追加し、H.264 の公開 API は `bitstream::h264` に配置する。`mod.rs` は使わない。

```text
src/bitstream.rs
src/bitstream/h264.rs
src/bitstream/nal.rs
```

`src/bitstream/nal.rs` は crate 内部だけで使う非公開モジュールとする。公開の `bitstream::nal`、コーデック共通の公開 NAL 型、共通化のためのトレイトは追加しない。

非公開 NAL 層の責務は次に限定する。

- 3 バイトおよび 4 バイトの開始コードを認識し、Annex B の NAL ユニット境界を走査する
- length-prefixed NAL ユニット列を指定された長さフィールド幅で走査し、境界超過、切り詰め、長さのオーバーフローを検出する
- Annex B と length-prefixed 形式の間で、コーデックに依存しないフレーミング変換を行う
- NAL ユニット本体をバイト列として呼び出し側へ渡し、H.264 / H.265 のヘッダー解釈は行わない

H.264 側は 1 バイトの NAL ヘッダーを検証し、`forbidden_zero_bit` と `nal_unit_type` を H.264 固有の型・API として扱う。開始コードがない入力、空の NAL ユニット、不正なヘッダー、切り詰められた length-prefixed 入力は `crate::Error` を返し、黙って読み飛ばさない。

### 公開 API の責務

`bitstream::h264` では、少なくとも次の操作を公開する。型名と関数名は実装時に Rust の既存 API と整合させるが、責務の境界は変えない。

- Annex B NAL ユニット列を借用ベースで列挙し、各 NAL ユニットの種別と raw バイト列を取得する
- Annex B と length-prefixed 形式を相互変換する。長さフィールド幅は呼び出し側が明示し、`AvccBox::length_size_minus_one` から得た値も検証して扱えるようにする
- length-prefixed サンプルから SPS / PPS など指定種別の NAL ユニットを抽出する
- SPS の RBSP と Exp-Golomb 符号を解析し、`AvccBox` と `VisualSampleEntryFields` の構築に必要な profile、compatibility、level、chroma format、bit depth、クロップ適用後の幅・高さを得る
- SPS / PPS のリスト、または Annex B 入力から、具体的な `Avc1Box` を構築する

サンプルエントリー構築 API は `SampleEntry` に包まず `Avc1Box` を返す。NAL 長フィールド幅などストリームから一意に導出できない値は、呼び出し側が明示する設定値として受け取る。Hisui の利用条件に由来するプロファイル制限や固定値は持ち込まず、H.264 と ISO/IEC 14496-15 で正当な入力を扱う。

公開 API は `no_std` を維持し、新しい外部依存は追加しない。入力サイズをそのまま信頼した事前確保は行わない。エラーは新しい公開エラー体系を増やさず、既存の `crate::Error` / `ErrorKind` に統合する。

### テスト

- 3 バイト / 4 バイト / 混在開始コード、先頭・末尾のゼロ、複数 NAL ユニット、空入力を決定的テストで確認する
- 開始コード欠落、空 NAL、切り詰め、長さ超過、不正ヘッダー、壊れた SPS が必ずエラーになることを確認する
- Baseline / Main / High 系を含む SPS とクロップ後の解像度、`AvccBox` へのフィールド反映を確認する
- x264 が生成した実データを小さな fixture としてリポジトリに含め、外部コマンドやネットワークなしで解析する
- `noprop` で Annex B と length-prefixed 形式のラウンドトリップ、および構築した正当な入力に対する不変条件を検証する
- 公開パーサーを対象とする `cargo-fuzz` の fuzz target を追加する

### 対象外

- H.265 / AV1 / VP8 / VP9 のコーデック固有処理
- Hisui / Sora Rust SDK 側の呼び出し置換と依存バージョン更新
- RTP / SDP、デコーダーやエンコーダー固有のポリシー、コーデック文字列生成
- C API / WASM バインディング。利用要件が明確になった時点で別 issue とする
- PBT 専用 SPS ビルダーの公開 API 化

## 完了条件

- `bitstream::h264` が公開され、Annex B 走査、length-prefixed 形式との相互変換、SPS 解析、SPS / PPS 抽出、`Avc1Box` 構築が利用できること
- `src/bitstream/nal.rs` が非公開であり、コーデック共通の公開 NAL 型やトレイトが追加されていないこと
- 正当なプロファイルを Hisui 固有の許可リストで制限していないこと
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と依存ライブラリ 0 を維持すること
- 決定的テスト、`noprop`、実データ fixture、fuzz target が追加されていること
- 公開 API の rustdoc に入力形式、返す NAL バイト列へヘッダーを含むか、エラー条件が記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
