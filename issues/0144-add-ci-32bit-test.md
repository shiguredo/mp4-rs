# CI でライブラリ本体のテストを 32 ビットのターゲットで実行する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/add-ci-32bit-test
- Polished: {YYYY-MM-DD}

## 目的

`usize` が 32 ビットのターゲットで、ライブラリ本体のテスト（`cargo test -p shiguredo_mp4`）を CI で実行する。

本体には `usize` の幅で結果が変わる検査があり、32 ビット用の期待値を持つテストもある。しかし今の CI は、そのテストを 32 ビットで一度も実行していない。

## 現状

`.github/workflows/ci.yml` のジョブは次のとおりで、本体の `cargo test` を 32 ビットのターゲットで実行するものはない。

- `ci`: `ubuntu-24.04`、`ubuntu-24.04-arm`、`windows-2025`、`macos-26` のマトリクスで `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` を実行する。`--target` を付けないため、どれも 64 ビットのホストのターゲットで動く
- `test-wasm`（Test WebAssembly）: `wasm` クレートを `wasm32-unknown-unknown` 向けにビルドする。wasm-bindgen で生成したバインディングに対して、Node.js で `crates/wasm/examples/mux.js` / `demux.js` と `crates/wasm/tests/wasm_api.test.mjs` / `wasm_web.test.mjs` を実行する。本体のコードの一部は JavaScript の API を通じて 32 ビットで動くが、本体の `cargo test` は実行しない
- `build-wasm`（Build WebAssembly）: `wasm` クレートのビルドと、成果物のアップロードだけを行う
- `miri`（Miri (WebAssembly)）: `make miri`（`cargo +nightly miri test -p wasm`）を実行する。`--target` を付けないため 64 ビットのホストのターゲットで動き、対象も `wasm` クレートのテストだけである

`.github/workflows/deploy.yml` も、`dump_wasm` / `transcode_wasm` を `wasm32-unknown-unknown` 向けにビルドするだけである。

本体には、`usize` の幅で結果が変わる検査がある。

- `src/descriptors.rs` の `shift_in_size_bits` は、ディスクリプターの宣言サイズを `usize` で組み立て、`usize` に収まらない場合に `ErrorKind::InvalidData` を返す
- `src/basic_types.rs` の `BoxHeader::decode_header_and_payload`、`src/demux_fmp4_segment.rs`、`src/demux_fmp4_file.rs`、`src/demux_mp4_file.rs`、`src/demux_mp4_file_kind_detector.rs`、`src/mux_fmp4_segment.rs` は、`u64` のボックスサイズやオフセットを `usize::try_from` で変換し、収まらない場合にエラーを返す

次のテストは `cfg!(target_pointer_width = "64")` で期待するエラー理由を分けている。32 ビット側（`box size exceeds usize::MAX`）の分岐は、CI で実行されていない。

- `tests/test_demux_fmp4_segment.rs` の `decode_error_box_offset_overflow_before_moof`、`decode_error_box_offset_overflow_between_moof_and_mdat`、`decode_error_box_offset_overflow_after_mdat`
- `tests/test_demux_fmp4_file.rs` の `decode_error_box_offset_overflow_between_moof_and_mdat`

`#[cfg(target_pointer_width = "64")]` で 64 ビットに限ったテストもある（`src/mux_mp4_file.rs` の `test_append_sample_data_size_exceeds_u32_max` / `test_append_sample_error_keeps_muxer_state`、`tests/test_mux_fmp4_segment.rs` の `mdat_box_size_overflow_returns_overflow` / `mdat_extended_box_size_overflow_returns_overflow`）。テストは 32 ビットでも実行される前提で書かれている。

closed の issue 0127 と 0128（ディスクリプターのサイズの検査）は、`wasm32-wasip1` 向けのビルドで追加したテストが通ることを、手作業で確認していた。

リポジトリの複製で、wasmtime をランナーにして `wasm32-wasip1` 向けにテストを実行した結果は次のとおり。stable の 1.100.0 と wasmtime 16.0.0 で確認した。`rust-toolchain.toml` の 1.93 では確認していない。

- `CARGO_TARGET_WASM32_WASIP1_RUNNER=wasmtime` を付けた `cargo test -p shiguredo_mp4 --target wasm32-wasip1` は、`src/` の単体テスト、`tests/` の全ファイル、doctest のすべてが成功した。上の 4 テストは、32 ビット側の期待値で成功した
- テストデータは `include_bytes!` で埋め込まれており、ファイルシステムへのアクセスは不要だった
- 同じ条件の `cargo test -p pbt --target wasm32-wasip1` は、`pbt/tests/prop_demux.rs` の `prev_sample_roundtrip` で abort した
  - noprop の `TestCaseContext::reject_case` は、`resume_unwind` でケースを棄却する。`wasm32-wasip1` ではパニックが巻き戻されず、abort になる
  - `reject_case` は `pbt/tests/prop_fmp4_segment_mux_demux.rs` でも使っている

## 設計方針

- `ci.yml` に、`wasm32-wasip1` 向けに `cargo test -p shiguredo_mp4` を実行するジョブを追加する。32 ビットの wasm32 は `crates/wasm` が実際に使う構成で、ローカルでも成功を確認できたため、このターゲットを選ぶ
- `pbt` は、`wasm32-wasip1` では `reject_case` で abort するため対象外とする。`i686-unknown-linux-gnu` ならパニックを巻き戻せるが、32 ビットの C ツールチェーンの導入が必要で、ここでは確認していないため採らない
- ツールチェーンは既存のジョブと同じく `rustup show` で `rust-toolchain.toml` のものを導入し、`rustup target add wasm32-wasip1` で target を追加する。rust-cache も既存のジョブと同じ形で使う
- wasmtime は rustup では導入できない。既存の wasm-bindgen / wasm-opt と同じく、リリースのアーカイブを curl で取得し、sha256 を検査してから導入する。外部の action は追加しない（shiguredo-github-actions の規約）
- runner は、`ubuntu-slim` でビルドとテストが timeout 内に終わるかを確認して決める。使えない場合は、その理由をコメントに書き、既存の wasm のジョブと同じ `ubuntu-24.04` を使う
- `slack_notify` の `needs` に新しいジョブを加える
- 本 issue では、テストとライブラリのコードは変更しない。CI で失敗するテストが見つかった場合は、別の issue で扱う
- `CHANGES.md` の `## develop` の `### misc` に `[ADD]` として記載する

## 完了条件

- `ci.yml` に、`wasm32-wasip1` 向けに `cargo test -p shiguredo_mp4` を実行するジョブがあり、成功する
- そのジョブのログで、上の 4 テストが実行されて成功している
- 新しいジョブが `slack_notify` の `needs` に含まれている
- 外部の action を追加していない
