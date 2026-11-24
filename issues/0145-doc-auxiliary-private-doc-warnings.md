# `--document-private-items` 付きの rustdoc で `src/auxiliary.rs` に出る警告を解消する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-auxiliary-private-doc-warnings
- Polished: {YYYY-MM-DD}

## 目的

`cargo doc --document-private-items` で出る rustdoc の警告をなくし、private な項目を含むドキュメントも警告なしで生成できるようにする。

## 現状

リポジトリの複製で、`rust-toolchain.toml` の指定（1.93）のツールチェーンで確認した。1.95.0 でも、`cargo doc -p shiguredo_mp4 --no-deps --document-private-items` の警告は同じ 3 件だった。

- `cargo doc -p shiguredo_mp4 --no-deps --document-private-items` は、`rustdoc::redundant_explicit_links` の警告を 3 件出す
- `RUSTDOCFLAGS="-D warnings"` を付けると、同じ 3 件がエラーになり、ドキュメントの生成に失敗する
- 3 件はいずれも、`src/auxiliary.rs` の `SampleTableAccessorError::InconsistentSampleCount` の doc にある `` [`SttsBox`][crate::boxes::SttsBox] `` である
  - variant `InconsistentSampleCount` 自体の doc
  - フィールド `stts_sample_count` の doc
  - フィールド `other_box_type` の doc
- `src/auxiliary.rs` は先頭で `crate::boxes::SttsBox` を `use` している。そのため、ラベルの `` `SttsBox` `` だけで同じリンク先に解決でき、明示したリンク先が冗長と判定される
- `--document-private-items` を付けない場合は、`-D warnings` を付けても警告は出ない。CI（`.github/workflows/ci.yml` の `ci` ジョブ）の `cargo doc` は `--document-private-items` を付けていないため、この警告を検出しない
- `--document-private-items` を付けて `cargo doc --workspace --no-deps` を実行しても、警告はこの 3 件だけで、ほかのファイルやクレートには出ない
- 同じ enum の `LastChunkIndexIsTooLarge::max_chunk_index` と `SampleDataOffsetOverflow` の doc にある `` [`StcoBox`][crate::boxes::StcoBox] `` / `` [`Co64Box`][crate::boxes::Co64Box] `` は、`src/auxiliary.rs` で `use` していないため冗長ではなく、警告も出ない

## 設計方針

- 3 か所の `` [`SttsBox`][crate::boxes::SttsBox] `` を `` [`SttsBox`] `` にする
  - 複製で修正して確認したところ、警告はなくなった
  - 公開ドキュメントの `aux::SampleTableAccessorError` のページは修正前とバイト単位で同じで、`--document-private-items` 付きの `auxiliary::SampleTableAccessorError` のページでも、3 か所のリンク先は修正前と同じ `boxes::SttsBox` のページだった
- `StcoBox` / `Co64Box` の明示リンクは、明示しないと解決できないため変えない
- CI の `cargo doc` に `--document-private-items` を加えて再発を検出することは、本 issue の対象外とする
- 公開ドキュメントの出力が変わらないため、`CHANGES.md` には記載しない

## 完了条件

- `RUSTDOCFLAGS="-D warnings" cargo doc -p shiguredo_mp4 --no-deps --document-private-items` が成功する
- CI と同じ `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` が成功する
- 公開ドキュメントの `aux::SampleTableAccessorError` のページで、3 か所の `SttsBox` のリンク先が `boxes::SttsBox` のページのままである
