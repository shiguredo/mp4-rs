.PHONY: test cover pbt pbt-with-cover fuzzing fuzzing-list miri check clippy fmt clean

# 全テストを実行する
test:
	cargo test --workspace

# 全テストカバレッジ付きで実行する
cover:
	cargo llvm-cov --tests --workspace

# PBT を実行する
pbt:
	cargo test -p pbt

# PBT をカバレッジ付きで実行する
pbt-with-cover:
	cargo llvm-cov -p pbt --tests

# Fuzzing を全ターゲットで 30 秒ずつ実行する
fuzzing:
	@for target in $$(cargo fuzz list); do \
		echo "=== Fuzzing $$target ==="; \
		cargo +nightly fuzz run $$target -- -max_total_time=30 || exit 1; \
	done

# Fuzzing ターゲット一覧を表示する
fuzzing-list:
	cargo fuzz list

# wasm クレートを miri で実行する
#
# wasm クレート自体は unsafe を使わないが、依存する nojson / shiguredo_mp4 を含む
# コードパスを Miri で実行し、未定義動作（align 契約違反 / uninit read / UAF 等）の
# 混入を検出する。
# nightly と miri component が必要（`rustup toolchain install nightly --component miri`）。
# `cargo +nightly` は `rust-toolchain.toml` の MSRV 1.93 を rustup override で上書きするため、
# リポジトリ規定の 1.93 と併存できる。
#
# miri 特有の失敗が出た場合の対処方針:
# - 実 UB（不正なポインタ操作・契約違反等）: コードを直す（skip しない）
# - miri 未サポート（リンク先ライブラリ経由の外部 FFI や未実装 intrinsic 等、実 UB でないもの）:
#   当該テストを `#[cfg(not(miri))]` で条件コンパイル外に落とし、理由をコメントに残す
# - 判断に迷う場合は skip せず、まず再現最小ケースを切り出して調査する
#
# 必要に応じて `MIRIFLAGS='-Zmiri-symbolic-alignment-check'` 等の追加検査を有効化する余地がある。
miri:
	cargo +nightly miri test -p wasm

# cargo check を実行する
check:
	cargo check --workspace

# cargo clippy を実行する
clippy:
	cargo clippy --workspace -- -D warnings

# cargo fmt を実行する
fmt:
	cargo fmt --all

# ビルド成果物を削除する
clean:
	cargo clean
