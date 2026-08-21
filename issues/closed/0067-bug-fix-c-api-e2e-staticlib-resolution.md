# C API E2E テストが現在の staticlib をリンクするようにする

- Created: 2026-08-18
- Completed: 2026-08-20
- Branch: feature/fix-c-api-e2e-staticlib-resolution
- Polished: 2026-08-20

## 目的

`cargo test` で実行される C API E2E テストが、同じビルドで生成された staticlib を必ずリンクするようにする。

現在は過去の `cargo build` が残した staticlib を誤ってリンクできるため、Rust 実装と C ヘッダーの ABI が一致しない状態でテストが実行される。テスト結果がローカルのビルド履歴に依存しないようにする。

## 現状

`crates/c-api/tests/e2e.rs` の `test_c_examples_compile` と `test_simple_mux_demux` は、リンク対象をプロジェクトルート基準の `target/debug/libmp4.a` に固定している。

一方、`cargo test -p c-api --test e2e` が当該テスト用に更新する staticlib は integration test 実行ファイルと同じ `deps` ディレクトリ側に生成される。top-level の `target/debug/libmp4.a` は `cargo test` だけでは更新されず、存在しない場合はテストが事前の `cargo build` を要求し、存在する場合は古い成果物をリンクできる。

実際に、C API の `mp4_estimate_maximum_moov_box_size` を配列とトラック数を受け取るシグネチャへ変更する前の staticlib が top-level に残っている状態で、次のコマンドを実行すると `test_simple_mux_demux` が失敗した。

```console
cargo test -p c-api --test e2e test_simple_mux_demux -- --exact --nocapture
```

現行ヘッダーに従う C コードから旧 ABI の関数が呼ばれ、ポインター値に由来する巨大な `moov` 予約サイズが生成された結果、`Buffer overflow` で終了した。表示される要求サイズは実行ごとに変化した。

その後に `cargo build -p c-api` を実行して top-level の staticlib を更新すると、同じ E2E テストは成功した。この結果から、MP4 処理自体ではなくリンク対象の鮮度が失敗条件であることを確認済みである。

`.github/workflows/ci.yml` の `ci` ジョブは `cargo test` の前に `cargo build --workspace` を実行するため、CI では top-level の staticlib が更新され、この問題が隠れる。

また、`target/debug` の固定パスは `CARGO_TARGET_DIR`、ビルドプロファイル、target triple を反映しない。staticlib 名も GNU 系の `libmp4.a` に固定されており、MSVC の `mp4.lib` を扱えない。

## 設計方針

`crates/c-api/tests/e2e.rs` で、現在実行中の integration test と同じビルド成果物ディレクトリを `std::env::current_exe()` から解決するヘルパーを追加する。

- `current_exe()` の親ディレクトリにある、同じ `cargo test` で生成された staticlib をリンクする
- staticlib 名は target に合わせ、MSVC では `mp4.lib`、それ以外では `libmp4.a` とする
- C コンパイラーが生成するテスト実行ファイルも同じ成果物ディレクトリへ出力し、プロジェクトルート基準の `target/debug` に依存しないようにする
- `test_c_examples_compile` と `test_simple_mux_demux` の両方で同じ解決処理を使う
- staticlib または成果物ディレクトリを解決できない場合は、確認したパスを含む日本語のテスト失敗メッセージを返す

テスト内から `cargo build` を入れ子で起動する方法は採用しない。ビルドロック、実行時間、依存取得の有無がテスト内部の挙動へ持ち込まれるためである。Cargo が当該 integration test と同時に生成した staticlib を直接使う。

外部依存は追加せず、モックやスタブも使用しない。

### テスト方針

- 事前に `cargo build` していない空の target directory を `CARGO_TARGET_DIR` に指定し、`cargo test -p c-api --test e2e` だけで 2 件の E2E テストが成功することを確認する
- 通常の `cargo test --workspace` が成功することを確認する
- 現在の CI と同じ build → test の順序でも成功することを確認する
- target ごとの staticlib 名と実行ファイル名を既存の `cfg` で処理し、少なくとも現在 CI 対象の GNU 系 Windows、Linux、macOS で既存テストを維持する

## 完了条件

- `crates/c-api/tests/e2e.rs` がプロジェクトルート基準の `target/debug/libmp4.a` を参照していないこと
- `test_c_examples_compile` と `test_simple_mux_demux` が、現在の `cargo test` で生成された staticlib をリンクすること
- 事前の `cargo build` がなくても C API E2E テストが成功すること
- 古い top-level staticlib の有無や内容がテスト結果に影響しないこと
- `CARGO_TARGET_DIR`、ビルドプロファイル、target triple が異なっても現在の成果物ディレクトリを解決できること
- MSVC とそれ以外の staticlib 名の違いを処理できること
- 新しい外部依存、モック、スタブを追加していないこと
- `cargo fmt --all -- --check`、`cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz -- -D warnings`、`cargo test --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz --no-deps` が通ること

## 解決方法

`crates/c-api/tests/e2e.rs` に、現在実行中のテスト実行ファイルが置かれている成果物ディレクトリを返す `get_artifact_dir`、そのディレクトリにある staticlib のパスを返す `get_staticlib_path`、C コンパイラーが生成する実行ファイルの出力パスを返す `get_exe_output_path` を追加した。

`test_c_examples_compile` と `test_simple_mux_demux` は、プロジェクトルート基準の `target/debug/libmp4.a` 固定をやめ、`get_staticlib_path()` が返す現在の `cargo test` で生成された staticlib をリンクする。staticlib 名は MSVC では `mp4.lib`、それ以外では `libmp4.a` とし、C 実行ファイルの出力先も Windows では `.exe` を付けた名前で同じ成果物ディレクトリへ置く。

次の検証で完了条件をすべて満たすことを確認した。

- 空の `CARGO_TARGET_DIR` を指定した状態で `cargo test -p c-api --test e2e` だけで 2 件の E2E テストが成功する
- 通常の `cargo test --workspace` が成功する
- build → test の順序でも成功する
- 古い top-level staticlib の有無や内容がテスト結果に影響しない
- `cargo fmt --all -- --check`、`cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz -- -D warnings`、`cargo test --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz --no-deps` が通る
