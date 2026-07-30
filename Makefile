.PHONY: test cover pbt pbt-cover fuzz fuzzing fuzzing-list miri check clippy fmt clean

# 全テストを実行する
#
# c-api の integration test は libmp4.a の事前ビルドと C コンパイラが必要なため
# workspace レベルでは除外し、lib test のみを追加で実行する
test:
	cargo test --workspace --exclude c-api
	cargo test -p c-api --lib

# 全テストカバレッジ付きで実行する
cover:
	cargo llvm-cov --tests --workspace --ignore-filename-regex 'crates/c-api/'

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
# UB（align 契約違反 / uninit read / UAF 等）の検出網として使う。
# nightly と miri component が必要（`rustup toolchain install nightly --component miri`）。
#
# miri 特有の失敗が出た場合の対処方針:
# - 実 UB（不正なポインタ操作・契約違反等）: コードを直す（skip しない）
# - miri 未サポート（外部 FFI・未実装 intrinsic 等で実 UB でないもの）:
#   当該テストに `#[cfg_attr(miri, ignore = "...")]` を付け、理由をコメントに残す
# - 判断に迷う場合は skip せず、まず再現最小ケースを切り出して調査する
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
