# 複数の Cargo.toml で依存バージョンが patch まで固定されており規約に違反している

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-29
- Model: opencode-go glm-5.2
- Branch: develop
- Polished: YYYY-MM-DD

## 目的

`shiguredo-rust` 規約「バージョン番号はマイナーバージョンまで指定すること」に違反している複数の `Cargo.toml` の依存指定を修正する。

## 優先度根拠

規約違反。patch まで固定すると依存の自動更新が阻害され、セキュリティ修正の取り込みが遅れる。`crates/wasm` の `nojson = "0.3"` は規約どおりであり、他のクレートで不整合がある。

## 現状

```toml
# crates/c-api/Cargo.toml:15
cbindgen = { version = "0.29.2", default-features = false }
```

```toml
# pbt/Cargo.toml:9
proptest = "1.9.0"
```

```toml
# examples/dump_wasm/Cargo.toml:11
nojson = "0.3.10"
```

```toml
# examples/transcode_wasm/Cargo.toml:11-12
futures = "0.3.30"
nojson = "0.3.10"
```

対照的に `crates/wasm/Cargo.toml:13` の `nojson = "0.3"` は規約どおり。

## 設計方針

各依存の patch バージョンを削除し、マイナーバージョンまでの指定に変更する。

- `cbindgen = "0.29.2"` → `cbindgen = "0.29"`
- `proptest = "1.9.0"` → `proptest = "1.9"`
- `nojson = "0.3.10"` → `nojson = "0.3"`（2 箇所）
- `futures = "0.3.30"` → `futures = "0.3"`

## 完了条件

- 全ての外部依存がマイナーバージョンまでの指定になること
- `cargo build` / `cargo test` が通ること
- `cargo clippy` が通ること

## 解決方法

1. `crates/c-api/Cargo.toml` の `cbindgen` は着手時点で既に `"0.29"` になっていたため変更不要だった
2. `pbt/Cargo.toml` / `examples/dump_wasm/Cargo.toml` / `examples/transcode_wasm/Cargo.toml` の依存バージョン指定をマイナーまでに直した
3. `cargo update` で `Cargo.lock` を更新し、`cargo build --workspace` / `cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` が通ることを確認した
4. CHANGES.md は依存指定の規約揃えのみのため更新していない
