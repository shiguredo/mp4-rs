.PHONY: test cover pbt pbt-cover fuzz fuzzing fuzzing-list check clippy fmt clean

# 全テストを実行する
test:
	cargo test --workspace --exclude c-api

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
