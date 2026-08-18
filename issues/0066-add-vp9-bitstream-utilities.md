# VP9 ビットストリーム処理ユーティリティを追加する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/add-vp9-bitstream-utilities
- Polished: {YYYY-MM-DD}

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

公開 API は `bitstream::vp9` に配置する。VP9 の uncompressed header を解析し、少なくとも次の情報を返す。

- frame marker と profile
- show-existing-frame と参照する frame index
- frame type、show frame、error resilient mode、intra-only
- profile に応じた bit depth、color space、color range、subsampling X / Y
- key frame / intra-only frame の frame size と render size
- inter frame で参照フレームのサイズを利用する構文を、呼び出し側から与える参照状態と合わせて解決できる情報

VP9 のフレームサイズは現在のフレームヘッダーだけでは確定しない場合がある。パーサーを隠れたグローバル状態に依存させず、参照フレーム寸法が必要な経路は呼び出し側が明示的な解析コンテキストを渡すか、未解決であることを型で表す。単純なキーフレーム判定だけを行う利用者に不要な状態管理を強制しない API にする。

frame marker、profile の reserved bit、sync code、profile と bit depth / subsampling の組み合わせ、RGB の制約、切り詰められたヘッダー、ゼロ寸法を検証し、違反は `crate::Error` とする。圧縮ヘッダーや tile data を解析する完全な VP9 デコーダーにはしない。

### サンプルエントリー構築

解析結果と呼び出し側の設定から、具体的な `Vp09Box` を構築する API を追加する。

- profile、bit depth、chroma subsampling、full range はストリームから得た値を `VpccBox` に反映する
- color space と `colour_primaries` / `transfer_characteristics` / `matrix_coefficients` の対応は仕様で導出できる範囲だけ自動設定し、それ以外は呼び出し側が明示する
- level は単一フレームから確定できないため、呼び出し側の明示値とするか undefined を指定できるようにする
- `codec_initialization_data` は VP9 では空にする
- VP9 は動的解像度を持てるため、`VisualSampleEntryFields` の幅・高さにはサンプルエントリーが参照する全サンプルの上限を呼び出し側が指定できる形にする

Hisui の固定 profile 0、8 bit、4:2:0、BT.709、limited range を暗黙値として移植しない。公開 API は `no_std` を維持し、新しい外部依存は追加せず、エラーを既存の `crate::Error` に統合する。

### VP8 との関係

VP8 と VP9 の公開パーサー、フレームヘッダー型、設定型を共有しない。`Vp08Box` / `Vp09Box` の同型性を理由に共通トレイトや共通 enum を作らない。共通点は既存の `VpccBox` を結果として利用することに限定する。

### テスト

- profile 0 〜 3、8 / 10 / 12 bit、各 chroma subsampling、RGB、limited / full range を決定的テストで確認する
- key / inter / intra-only / show-existing-frame、frame size、render size、参照寸法を使う経路を確認する
- 不正な frame marker、reserved bit、sync code、profile と色設定の矛盾、短い入力、ゼロ寸法を拒否することを確認する
- libvpx が生成した実データを小さな fixture としてリポジトリに含める
- `noprop` で uncompressed header のビット配置、profile と色設定の組み合わせ、境界条件を検証する
- 公開パーサーを対象とする `cargo-fuzz` の fuzz target を追加する

### 対象外

- compressed header、tile data、superframe index の解析やデコード。superframe index が必要になった場合は別 issue とする
- VP8 との公開 API 共通化
- Hisui 側の呼び出し置換と依存バージョン更新
- RTP / SDP、コーデック文字列生成
- C API / WASM バインディング

## 完了条件

- `bitstream::vp9` が公開され、VP9 の uncompressed header から profile、フレーム種別、bit depth、色・クロマ情報、解像度を取得できること
- 参照フレーム寸法が必要な構文を明示的なコンテキストまたは未解決値として安全に扱えること
- 解析結果と明示的な設定から `Vp09Box` を構築できること
- profile、bit depth、色特性、level を Hisui の固定値で暗黙に決定していないこと
- VP8 との共通公開型、共通トレイト、共通パーサーが追加されていないこと
- 不正入力を panic や黙った打ち切りではなく `crate::Error` として報告すること
- `no_std` と依存ライブラリ 0 を維持すること
- 決定的テスト、`noprop`、実データ fixture、fuzz target が追加されていること
- 公開 API の rustdoc に解析コンテキスト、未解決値、導出できる値と呼び出し側が指定する値、エラー条件が記載されていること
- `CHANGES.md` の `develop` に `[ADD]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
