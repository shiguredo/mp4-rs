# H.265 ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/add-h265-bitstream-utilities
- Polished: {YYYY-MM-DD}

## 目的

H.265 の Annex B / length-prefixed NAL ユニット列の解析、SPS の解析、VPS / SPS / PPS の抽出、`hev1` / `hvc1` / `hvcC` の構築を `shiguredo_mp4` の汎用ユーティリティとして提供する。

H.264 と共有できるのは NAL ユニットを区切る外側のフレーミング処理だけに限定し、H.265 固有のヘッダーとパラメータセットの意味を独立した公開 API で表現する。

## 現状

- `src/boxes_sample_entry.rs` には `Hev1Box`、`Hvc1Box`、`HvccBox`、`HvccNalUintArray` があるが、H.265 ストリームから構築する API はない
- `shiguredo/hisui` の `src/video/h265.rs` には Annex B の走査、SPS の解析、VPS / SPS / PPS からのサンプルエントリー構築がある
- Hisui の実装は H.264 側の開始コード探索を再利用しており、外側のフレーミング処理には実際の共通性がある。一方で、2 バイトの H.265 NAL ヘッダー、NAL 種別、SPS 構文は H.264 と異なる
- Hisui 固有のプロファイル許可リスト、固定の 4 バイト長、固定フレームレート方針、常に `hvc1` を選ぶ方針は汎用 crate の契約にはできない

参照仕様は [ITU-T H.265](https://www.itu.int/rec/T-REC-H.265-202601-I/en) および ISO/IEC 14496-15 とする。

## 設計方針

### モジュールと共通化の境界

公開 API は `bitstream::h265` に配置する。0062 で追加する非公開の `src/bitstream/nal.rs` を、Annex B の境界走査と length-prefixed 形式の境界検証・変換だけに再利用する。

H.265 側は 2 バイトの NAL ヘッダーを独自に解析し、`forbidden_zero_bit`、`nal_unit_type`、layer ID、`nuh_temporal_id_plus1` の妥当性を検証する。H.264 の公開型、公開関数、ヘッダー解釈を流用しない。共通トレイトや公開 `bitstream::nal` モジュールも追加しない。

### 公開 API の責務

`bitstream::h265` では、少なくとも次の操作を公開する。型名と関数名は実装時に Rust の既存 API と整合させる。

- Annex B NAL ユニット列を借用ベースで列挙し、H.265 固有のヘッダー情報と raw バイト列を取得する
- Annex B と length-prefixed 形式を相互変換し、長さフィールド幅と入力境界を検証する
- Annex B または length-prefixed サンプルから VPS / SPS / PPS を抽出し、同一種別の複数 NAL ユニットを入力順で保持する
- SPS の RBSP、profile tier level、chroma format、bit depth、sub-layer 情報、conformance window を解析し、クロップ適用後の幅・高さを得る
- VPS / SPS / PPS のリスト、または Annex B 入力から、具体的な `Hev1Box` または `Hvc1Box` を構築する

`hev1` と `hvc1` の選択は呼び出し側が明示する。専用関数または H.265 モジュール内の enum を使い、真偽値では表現しない。戻り値を `SampleEntry` に包まない。

`HvccBox` のうち SPS などから一意に導出できる値は解析結果から設定する。`avg_frame_rate`、`constant_frame_rate`、`parallelism_type` など一意に導出できない値は設定引数として明示するか、ISO/IEC 14496-15 が定める「不明」の値を使う。Hisui の `FrameRate`、固定 CFR、固定 4 バイト長、プロファイル許可リストは移植しない。

公開 API は `no_std` を維持し、新しい外部依存は追加しない。不正な開始コード列、短い NAL ヘッダー、不正な temporal ID、切り詰められた length-prefixed 入力、壊れた SPS は `crate::Error` を返す。

### テスト

- 3 バイト / 4 バイト / 混在開始コードと、H.265 固有の 2 バイト NAL ヘッダーを決定的テストで確認する
- 不正な `forbidden_zero_bit`、ゼロの `nuh_temporal_id_plus1`、空 NAL、切り詰め、長さ超過を拒否することを確認する
- Main / Main 10 を含む SPS、conformance window、profile tier level、複数 VPS / SPS / PPS の保持を確認する
- x265 が生成した実データを小さな fixture としてリポジトリに含める
- `noprop` でフレーミングのラウンドトリップと H.265 ヘッダーの不変条件を検証する
- 公開パーサーを対象とする `cargo-fuzz` の fuzz target を追加する

### 対象外

- 非公開 NAL 層を公開 API に昇格すること
- H.264 と H.265 の NAL ヘッダー型、SPS 型、サンプルエントリー構築 API の共通化
- Hisui / Sora Rust SDK 側の呼び出し置換と依存バージョン更新
- RTP / SDP、フレームレート推定、デコーダー固有のプロファイル制限
- C API / WASM バインディング

## 完了条件

- `bitstream::h265` が公開され、Annex B 走査、length-prefixed 形式との相互変換、SPS 解析、VPS / SPS / PPS 抽出、`Hev1Box` / `Hvc1Box` 構築が利用できること
- 共有処理が `src/bitstream/nal.rs` のフレーミング層に限定され、H.264 / H.265 の公開 API が独立していること
- `hev1` / `hvc1` を呼び出し側が明示的に選択できること
- Hisui 固有のプロファイル、フレームレート、NAL 長の制約を持ち込んでいないこと
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と依存ライブラリ 0 を維持すること
- 決定的テスト、`noprop`、実データ fixture、fuzz target が追加されていること
- 公開 API の rustdoc に入力形式、ヘッダー検証、`hev1` / `hvc1` の選択契約、エラー条件が記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
