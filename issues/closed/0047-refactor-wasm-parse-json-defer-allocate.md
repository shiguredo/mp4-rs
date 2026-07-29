# WASM の parse_json_mp4_sample_entry 系関数を JSON パース途中失敗リーク回避のため allocate 順序を deferred に統一する

- Priority: Medium
- Created: 2026-07-23
- Completed: 2026-07-29
- Model: Opus 4.7
- Branch: feature/refactor-wasm-parse-json-defer-allocate
- Polished: 2026-07-29

## 目的

`crates/wasm/src/` 配下の `parse_json_mp4_sample_entry_*` 系関数で、JSON パース中に `mp4_alloc` 経由の C 側 raw 領域を **早めに** 確保している。その後の JSON フィールド `try_into()?` が失敗すると、確保済み領域を回収する owner が Rust 側に存在せず、`mp4_free` が呼ばれずにリークする。

全 JSON フィールドを Rust 型に落とし切ってから **最後にまとめて** `allocate_and_copy_bytes` / `allocate_and_copy_array_list` / `allocate_and_copy_u16_array` を呼ぶ順序に統一する refactor を行う。

## 優先度根拠

Medium。実運用では JSON 入力の妥当性は上流で保証される想定のため実質的にリーク発火経路は狭い。ただし WASM の consumer（TypeScript 等）が壊れた JSON を送りつけたときに気付かれないうちに徐々にリークするリスクが残り、防御は行うべき。バグではなく設計改善（refactor カテゴリ）として扱う。

## 現状

対象関数（`parse_json_mp4_sample_entry_*`）は以下 7 個:

- `crates/wasm/src/boxes_avc1.rs:46-113` `parse_json_mp4_sample_entry_avc1`
- `crates/wasm/src/boxes_hev1.rs` `parse_json_mp4_sample_entry_hev1`
- `crates/wasm/src/boxes_hvc1.rs` `parse_json_mp4_sample_entry_hvc1`
- `crates/wasm/src/boxes_av01.rs` `parse_json_mp4_sample_entry_av01`
- `crates/wasm/src/boxes_mp4a.rs` `parse_json_mp4_sample_entry_mp4a`
- `crates/wasm/src/boxes_flac.rs` `parse_json_mp4_sample_entry_flac`
- `crates/wasm/src/boxes_tx3g.rs` `parse_json_mp4_sample_entry_tx3g`

なお `parse_json_mp4_sample_entry_stpp` / `_wvtt` は同種の refactor 対象に見えるが、実装確認の結果いずれも本 issue の対象外である:

- `crates/wasm/src/boxes_stpp.rs:42-76` は既にフェーズ 1（3 本の `&str` を先に取り出す）→ フェーズ 2（3 回 `allocate_and_copy_bytes`）の順序で実装済みで、末尾の `Ok(Mp4SampleEntryStpp { .. })` 内に `try_into?` を残さない（`boxes_stpp.rs:45-47` にコメントで意図まで明記されている）
- `crates/wasm/src/boxes_wvtt.rs:26-41` は `config` 1 本のみを扱うため、JSON パース途中失敗リーク経路が構造的に存在しない（`boxes_wvtt.rs:29` にコメントで明記されている）

典型パターン（`boxes_avc1.rs:46-113`）:

1. L50-54 で `sps` フィールドを `Vec<Vec<u8>>` として集約
2. L56 で `allocate_and_copy_array_list(&sps_vec)` を呼び C 側領域を確保
3. L59-63 で `pps` フィールドを集約、L65 で同じく allocate
4. L67 以降で `width` / `height` / `avcProfileIndication` 等の残りフィールドを `try_into()?` で読む
5. L67-113 のいずれかで `try_into()?` が失敗すると、既に確保済みの SPS / PPS の raw 領域が `mp4_free` されずにリーク

同種のパターンが上記 7 関数に存在する。`boxes_tx3g.rs:49-118` の `parse_json_mp4_sample_entry_tx3g` では、`allocate_and_copy_u16_array(&font_ids_vec)` と `allocate_and_copy_array_list(&font_names_vec)` の直後に `display_flags` 以降を parse するため、同じリスクを持つ。

対応する variant 固有の free 関数（`mp4_sample_entry_avc1_free(&mut Mp4SampleEntryAvc1)` など）は、いずれも variant 固有型を第 1 引数で受け取って `mp4_free` / `crate::boxes::free_array_list` / `free_u16_array` を variant に応じて組み合わせて呼び、確保済みバッファを解放する設計だが、`parse_json_*` が途中で `Err` を返して struct 構築に到達しなかったケースはカバーしていない。

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

これにより「allocate 済みで parse 途中失敗」の経路が構造的に消える。

ただし本 refactor が構造的に消せるのは **JSON パース途中失敗** による leak に限られる。allocate 段階の部分失敗による leak は本 refactor の scope 外に残り、次の 2 クラスがある:

1. `allocate_and_copy_bytes`（`crates/wasm/src/boxes.rs:234-249` の early-return は `:242-244`）と `allocate_and_copy_u16_array`（同 `:291-305` の early-return は `:298-300`）は `mp4_alloc` 失敗時に `(null, 0)` を返す。同じ関数内で複数の `allocate_and_copy_*` を順に呼ぶ場合、先の呼び出しが成功して後の呼び出しが失敗すると、先に確保した領域が回収されないまま `Ok(...)` に到達する（`crates/wasm/src/boxes_hev1.rs:240-242` のコメントで別種の非常態として言及されている）
2. `allocate_and_copy_array_list`（同 `:254-285`）は返り値が 3-tuple で、内部の `mp4_alloc` 失敗を検出する early-return を持たない。個別要素の確保（同 `:262-265`）が部分的に失敗すると null が data_ptrs に混ざったまま呼び出し元に伝わる。集約側の `allocate_and_copy_bytes`（同 `:266-272` / `:276-282`）が失敗した場合は個別で確保済みの領域を参照するポインタ配列自体が引き回せなくなり、成功した個別領域がまるごと leak する

これらの allocate 段階の部分失敗経路は本 refactor の対象外とする（別途 issue 化する場合は本 issue とは独立の課題として起票する）。

## 完了条件

- `parse_json_mp4_sample_entry_*` 系 7 関数（avc1 / hev1 / hvc1 / av01 / mp4a / flac / tx3g）を「フェーズ 1: Rust 型に落とす → フェーズ 2: 末尾 allocate」の順序に統一する
- 上記 7 関数のいずれも、関数内で最初の `allocate_and_copy_*` 呼び出しより後段に `?` 演算子を伴う JSON 抽出（`try_into?` / `.required()?` / `.to_member(...)?` 等）が残っていないこと（テストは valid JSON 経路しか叩かないため、順序統一自体はコードレビューで確認する）
- `cargo test --workspace` が全 pass する
- `cargo clippy --all-targets --all-features -- -D warnings` が warning なしで通る
- `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が warning なしで通る
- 既存のテスト（`crates/wasm/src/boxes_*.rs` の `#[cfg(test)]` ブロック内）が引き続き pass する

## 解決方法

`feature/refactor-wasm-parse-json-defer-allocate` ブランチで対応した。

### 実施内容

- 対象 7 関数（avc1 / hev1 / hvc1 / av01 / mp4a / flac / tx3g）を「フェーズ 1: 全 JSON フィールドを Rust 型に落とす → フェーズ 2: 末尾で `allocate_and_copy_*`」の順序に統一した
- いずれの関数も、最初の `allocate_and_copy_*` より後段に `?` を伴う JSON 抽出が残っていないことを確認した
- コメント表記を日本語に揃え、hev1 / hvc1 / tx3g の見出しコメントを補った
- 可変長フィールドは揃えて後段スカラーだけ欠落させた不正 JSON で `Err` になる回帰テストを 7 関数それぞれに追加した
- `CHANGES.md` の `### misc` に `[UPDATE]` を追記した（タイトルはユーザ影響側、実施内容はサブリスト）
- `boxes_stpp.rs` / `boxes_wvtt.rs` は対象外のまま変更していない

### 検証

- `cargo test --workspace` / `cargo clippy --all-targets --all-features -- -D warnings` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が通ることを確認した
- `/review-diff-code` で致命的・重要が 0 件であることを確認した

## CHANGES.md

`[UPDATE]` として記載する。挙動変化はない（正常経路の結果は同一）ため `[FIX]` ではなく `[UPDATE]` が妥当。
