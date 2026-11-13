# crates/wasm を c-api 依存から切り離して wasm-bindgen ベースにする

- Created: 2026-11-13
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-wasm-bindgen
- Polished: {YYYY-MM-DD}

## 目的

`crates/wasm` を npm パッケージとして公開できる状態にするための前提として、wasm の公開 API を `c-api` クレートへの依存から切り離し、コアライブラリ (`shiguredo_mp4`) を直接使う構成にする。

現状の wasm は「`c-api` の関数群をそのまま再エクスポートし、JSON 補助関数を足したもの」であり、公開している関数 56 個のうち 42 個 (75%) が `c-api` 由来である。`c-api` を削除する判断をした以上、この 42 個は wasm 側で持つ必要がある。この機会に手書きの C ABI を廃し、wasm-bindgen によるメモリ管理へ移行することで、生ポインタと構造体レイアウトに起因するバグの発生源をなくす。

## 現状

- `crates/wasm/Cargo.toml` は `c_api` に依存している
- wasm のソースには `c_api::` への参照が 79 箇所ある。うち `c-api` の関数呼び出しは `fmp4_segment_muxer_new` / `fmp4_segment_muxer_write_media_segment_metadata` 系など 8 個だけで、残りは型の参照である
- `crates/wasm/src/lib.rs` は `mp4_alloc` / `mp4_free` / `mp4_vec_ptr` / `mp4_vec_len` / `mp4_vec_free` を提供している
- `crates/wasm/src/fmp4_segment_mux.rs` の `OwnedMp4SampleEntry` は、C ABI に渡す生ポインタの所有権を `Drop` で管理するためだけに存在する
- wasm のソース 4,702 行のうち 513 行がポインタ橋渡し (`mp4_alloc` / `from_raw_parts` / `is_null` / `Box::into_raw` 等) に費やされている
- JSON の入出力が非対称である。`mp4_demux_sample_to_json` は `"composition_time_offset": null` を出力するが、`mp4_mux_sample_from_json` は `null` を整数として解釈できずに失敗する
- JavaScript 側が `Mp4DemuxTrackInfo` (wasm32 で 24 バイト) や `Mp4DemuxSample` (wasm32 で 56 バイト) のサイズを定数として持つ必要がある
- `crates/wasm/examples/demux.js` は `Mp4DemuxTrackInfo` を 20 バイト刻みで読んでおり、2 トラック以上のファイルで誤った値を出す
- `mp4_alloc` はアラインメント 1 の契約でメモリを確保する

## 設計方針

- wasm-bindgen を導入し、`#[wasm_bindgen]` で JavaScript 向けの型付き API を公開する
  - 生ポインタの受け渡しをなくし、文字列・バイト列・ハンドル型だけを境界に出す
  - `.d.ts` は wasm-bindgen が生成するため、手書きの型定義を持たない
- コアの型 (`shiguredo_mp4::boxes::SampleEntry` / `demux::Sample` / `demux::TrackInfo` / muxer / demuxer) を直接扱う
- JSON スキーマは現行のまま維持する。`crates/wasm/README.md` に記載済みのキー名を変えない
  - サンプルエントリーなどネストしたデータは JSON 文字列で受け渡す
  - ただし `composition_time_offset` は入力側でも `null` を `None` として受理し、出力と往復できるようにする
- ファイル位置・サイズは JavaScript の `number` (f64) で受け渡す
  - 2 の 53 乗まで正確に表現できるため、実用上のファイルサイズでは精度が落ちない
  - `u64` をそのまま公開すると利用者に `BigInt` を強いることになるため避ける
- 公開するクラスは `Mp4FileDemuxer` / `Mp4FileMuxer` / `Fmp4SegmentMuxer` / `Fmp4SegmentDemuxer` / `Mp4FileKindDetector` とし、`version()` 関数でライブラリのバージョンを返す
  - メソッド名は JavaScript の慣習に合わせて camelCase にする
- `release-wasm` プロファイルの `strip = true` を外す
  - wasm-bindgen は `target_features` カスタムセクションを読むため、strip すると機能検出に失敗する
  - バイナリサイズの削減はビルド後に `wasm-opt` を適用して行う
- ビルド手順は cargo → wasm-bindgen → wasm-opt の順にする
- Node.js 向け (`--target nodejs`) とブラウザ向け (`--target web`) の両方を生成する

## 完了条件

- `crates/wasm` の依存から `c_api` が消えている
- `cargo build -p wasm --target wasm32-unknown-unknown --profile release-wasm` が通り、`wasm-bindgen` で `.js` と `.d.ts` を生成できる
- Node.js から `Mp4FileDemuxer` / `Mp4FileMuxer` / `Fmp4SegmentMuxer` / `Fmp4SegmentDemuxer` / `Mp4FileKindDetector` が動作する
- 2 トラック以上の MP4 をデマルチプレックスでき、すべてのトラック情報が正しく取得できる
- デマルチプレックスしたサンプルの JSON を、`composition_time_offset: null` を含めてそのままマルチプレックス側に渡せる
- `cargo test -p wasm` が通る
- `crates/wasm/README.md` の API 一覧と JSON スキーマを新しい構成に更新する

## 解決方法

{実装後に記載する}
