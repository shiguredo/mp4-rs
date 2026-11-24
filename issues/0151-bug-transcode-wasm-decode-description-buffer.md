# `examples/transcode_wasm` で H.264 / H.265 への変換が `InsufficientBuffer` で失敗しないようにする

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-transcode-wasm-decode-description-buffer
- Polished: {YYYY-MM-DD}

## 目的

`examples/transcode_wasm` で出力コーデックに H.264（`avc1.42e02a`）または H.265（`hev1.1.6.L90.B0`）を選ぶと、エンコーダーの出力を集め終えた後に変換が必ず失敗する。出力のサンプルエントリーを組み立てる `TrackTranscoder::decode_description` が、長さ 0 のバッファにボックスヘッダーを書き込もうとするためである。これを直し、両コーデックへの変換を完了できるようにする。

## 現状

`examples/transcode_wasm/src/transcode.rs`:

- `TrackTranscoder::transcode_chunk` は、チャンクの全サンプルを WebCodecs でデコード・エンコードした後、`TrackTranscoder::build_output_sample_entry` で出力のサンプルエントリーを組み立てる
- `build_output_sample_entry` は、出力コーデックが `avc1.42e02a` なら `decode_description::<AvccBox>`、`hev1.1.6.L90.B0` なら `decode_description::<HvccBox>` を呼び、先頭サンプルの `description` から設定ボックスを復元する。`description` は、エンコーダーの出力のメタデータの `decoderConfig.description`（`transcode.js` の `createVideoEncoder` が渡す）で、`decode_description` はこれをボックスヘッダーを含まない `avcC` / `hvcC` のペイロードとして扱う
- `decode_description` は `let mut data = Vec::new();` の直後に `header.encode(&mut data)` を呼ぶ。`Encode::encode` は `&mut [u8]` に書き込むため、空の `Vec` は長さ 0 のスライスとして渡され、`BoxHeader::encode` は size を書き込む時点で `ErrorKind::InsufficientBuffer` を返す。その後の `data.extend_from_slice(description)` と `T::decode` には到達しない
- 先頭サンプルの `description` が `None` の場合は、その前に `no description in sample` で失敗する。`description` がある場合も上記のとおり `InsufficientBuffer` で失敗するため、どちらのコーデックでも変換は完了しない
- エラーは `TrackTranscoder::run` から `Transcoder::poll_transcode` に届き、wasm の `pollTranscode` が `Err` を返す。`transcode.js` の `pollTranscode` はそれをログに出して変換を止めるため、`buildOutputMp4File` に進まない
- VP8 / VP9 / AV1（`index.html` の既定値は AV1）への変換は `decode_description` を通らないため、影響しない

空の `Vec` に書き込むこの処理は 0132 より前からある。0132 では、`BoxSize::with_payload_size` が `Result` を返すようになったことに合わせてこの関数を更新したが、バッファはそのままだった。

`.github/workflows/ci.yml` は `transcode_wasm` を `cargo check` / `cargo build` / `cargo test` / `cargo clippy` / `cargo doc` から除外しており、`transcode_wasm` にはテストもないため、この不具合はテストで検出されない。一方 `.github/workflows/deploy.yml` は develop への push のたびに `transcode_wasm` を wasm32 向けにビルドし、GitHub Pages に配置している。

再現手順（develop で確認）: `examples/transcode_wasm/src/transcode.rs` に `#[cfg(test)]` のテストを一時的に置き、`cargo test -p transcode_wasm --lib` で次を確かめた。

1. `AvccBox` を `encode_to_vec` で書き出し、先頭 8 バイト（ボックスヘッダー）を除いたバイト列を先頭サンプルの `description` に持つ `Chunk` を組み立てる
2. `TrackTranscoder::decode_description::<AvccBox>(&chunk, AvccBox::TYPE)` は `InsufficientBuffer` のエラーを返す。`decode_description::<HvccBox>(&chunk, HvccBox::TYPE)` も、ペイロードを読む前のヘッダーの書き込みで同じエラーを返す
3. 同じ `BoxHeader` を `encode_to_vec` で書き出して `description` を連結すると、`AvccBox::decode` で元の `AvccBox` に戻る

## 設計方針

- `decode_description` で、ヘッダーを書き込める長さのバッファを使う。たとえば `header.encode_to_vec()` で得たバイト列に `description` を連結する。`BoxSize::with_payload_size` でサイズを求めて `T::decode` で設定ボックスを復元する流れは変えない
- `decode_description` は private のため、回帰を確かめる単体テストを `examples/transcode_wasm/src/transcode.rs` の `#[cfg(test)]` モジュールに置く。WebCodecs を使わず、組み立てた `Chunk` から設定ボックスを復元できることを確かめる
- CI の対象に `transcode_wasm` を加えることは、GitHub Actions の変更になるため本 issue の対象外とする。テストは `cargo test -p transcode_wasm` で実行する

## 完了条件

- `examples/transcode_wasm/src/transcode.rs` の `#[cfg(test)]` のテストで、ボックスヘッダーを除いた `avcC` / `hvcC` のペイロードを先頭サンプルの `description` に持つ `Chunk` から、`decode_description` が元の `AvccBox` / `HvccBox` を復元する
- `cargo test -p transcode_wasm` と `cargo clippy -p transcode_wasm --all-targets -- -D warnings` が成功する
- `deploy.yml` と同じ `cargo build --release --target wasm32-unknown-unknown -p transcode_wasm` が成功する
