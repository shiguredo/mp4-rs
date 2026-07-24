# WASM の parse_json_mp4_sample_entry 系関数を部分失敗リーク回避のため allocate 順序を deferred に統一する

- Priority: Medium
- Created: 2026-07-23
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/refactor-wasm-parse-json-defer-allocate
- Polished: YYYY-MM-DD

## 目的

`crates/wasm/src/` 配下の `parse_json_mp4_sample_entry_*` 系関数で、JSON パース中に `mp4_alloc` 経由の C 側 raw 領域を **早めに** 確保している。その後の JSON フィールド `try_into()?` が失敗すると、確保済み領域を回収する owner が Rust 側に存在せず、`mp4_free` が呼ばれずにリークする。

全 JSON フィールドを Rust 型に落とし切ってから **最後にまとめて** `allocate_and_copy_bytes` / `allocate_and_copy_array_list` / `allocate_and_copy_u16_array` を呼ぶ順序に統一する refactor を行う。

## 優先度根拠

Medium。実運用では JSON 入力の妥当性は上流で保証される想定のため実質的にリーク発火経路は狭い。ただし WASM の consumer（TypeScript 等）が壊れた JSON を送りつけたときに気付かれないうちに徐々にリークするリスクが残り、防御は行うべき。バグではなく設計改善（refactor カテゴリ）として扱う。

## 現状

対象関数（`parse_json_mp4_sample_entry_*`）は以下 9 個:

- `crates/wasm/src/boxes_avc1.rs:46-113` `parse_json_mp4_sample_entry_avc1`
- `crates/wasm/src/boxes_hev1.rs` `parse_json_mp4_sample_entry_hev1`
- `crates/wasm/src/boxes_hvc1.rs` `parse_json_mp4_sample_entry_hvc1`
- `crates/wasm/src/boxes_av01.rs` `parse_json_mp4_sample_entry_av01`
- `crates/wasm/src/boxes_mp4a.rs` `parse_json_mp4_sample_entry_mp4a`
- `crates/wasm/src/boxes_flac.rs` `parse_json_mp4_sample_entry_flac`
- `crates/wasm/src/boxes_stpp.rs` `parse_json_mp4_sample_entry_stpp`
- `crates/wasm/src/boxes_wvtt.rs` `parse_json_mp4_sample_entry_wvtt`
- `crates/wasm/src/boxes_tx3g.rs` `parse_json_mp4_sample_entry_tx3g`

典型パターン（`boxes_avc1.rs:46-113`）:

1. L50-54 で `sps` フィールドを `Vec<Vec<u8>>` として集約
2. L56 で `allocate_and_copy_array_list(&sps_vec)` を呼び C 側領域を確保
3. L59-63 で `pps` フィールドを集約、L65 で同じく allocate
4. L67 以降で `width` / `height` / `avcProfileIndication` 等の残りフィールドを `try_into()?` で読む
5. L67-113 のいずれかで `try_into()?` が失敗すると、既に確保済みの SPS / PPS の raw 領域が `mp4_free` されずにリーク

同種のパターンが 9 関数すべてに存在する。`boxes_tx3g.rs:49-118` の `parse_json_mp4_sample_entry_tx3g` では、`allocate_and_copy_u16_array(&font_ids_vec)` と `allocate_and_copy_array_list(&font_names_vec)` の直後に `display_flags` 以降を parse するため、同じリスクを持つ。

対応する `mp4_sample_entry_*_free` 系関数はいずれも `Mp4SampleEntry` を先頭で受け取って `mp4_free` を呼ぶ設計で、`parse_json_*` が途中で `Err` を返したケースはカバーしていない。

## 設計方針

全 JSON フィールドを **Rust 型（`Vec<Vec<u8>>` / `Vec<u16>` / スカラー）** に落とし切ってから、関数末尾で `allocate_and_copy_*` を呼ぶ順序に統一する。

パターン:

```rust
pub fn parse_json_mp4_sample_entry_XXX(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryXXX, nojson::JsonParseError> {
    // フェーズ 1: すべての JSON フィールドを Rust 型に落とす（allocate 前）
    let sps_vec: Vec<Vec<u8>> = ...;
    let pps_vec: Vec<Vec<u8>> = ...;
    let width: u16 = value.to_member("width")?.required()?.try_into()?;
    let height: u16 = ...;
    // ... 他フィールド ...

    // フェーズ 2: すべての parse が成功したときだけ allocate する
    let (sps_data, sps_sizes, sps_count) = crate::boxes::allocate_and_copy_array_list(&sps_vec);
    let (pps_data, pps_sizes, pps_count) = crate::boxes::allocate_and_copy_array_list(&pps_vec);

    Ok(Mp4SampleEntryXXX { width, height, sps_data, sps_sizes, sps_count, ... })
}
```

これにより「allocate 済みで parse 途中失敗」の経路が構造的に消える。allocate の後に発生し得る失敗は無い（`allocate_and_copy_*` 自体は失敗しない設計）。

## 完了条件

- `parse_json_mp4_sample_entry_*` 系 9 関数すべてを「フェーズ 1: Rust 型に落とす → フェーズ 2: 末尾 allocate」の順序に統一する
- `cargo test --workspace` が全 pass する
- `cargo clippy --all-targets --all-features -- -D warnings` が warning なしで通る
- `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が warning なしで通る
- 既存のテスト（`crates/wasm/src/boxes_*.rs` の `#[cfg(test)]` ブロック内）が引き続き pass する

## 解決方法

以下の順で対応する:

1. `boxes_avc1.rs` を書き換える（SPS/PPS を末尾 allocate に移動）
2. `boxes_hev1.rs` / `boxes_hvc1.rs`（NALU リスト同型パターン）
3. `boxes_av01.rs` / `boxes_mp4a.rs` / `boxes_flac.rs`（単一 array or bytes パターン）
4. `boxes_stpp.rs` / `boxes_wvtt.rs`（allocate_and_copy_bytes 単発。実質リスクは低いが順序統一のため対応）
5. `boxes_tx3g.rs`（3 並列 `allocate_and_copy_u16_array` + `allocate_and_copy_array_list`）
6. 各関数に対応する `#[cfg(test)]` ラウンドトリップテストが引き続き pass することを確認する
7. `cargo test --workspace` / `cargo clippy` / `cargo doc` で最終検証する

各関数の refactor は独立して機能するため、機能単位のコミットに分けても良い。

## CHANGES.md

`[UPDATE]` として記載する。挙動変化はない（正常経路の結果は同一）ため `[FIX]` ではなく `[UPDATE]` が妥当。
