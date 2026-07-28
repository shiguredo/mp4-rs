# 公開ボックス型の `#[expect(missing_docs)]` を撤廃してフィールドに doc コメントを付ける

- Priority: Medium
- Created: 2026-07-28
- Completed: YYYY-MM-DD
- Model: Opus 5
- Branch: feature/update-remove-expect-missing-docs
- Polished: YYYY-MM-DD

## 目的

`src/lib.rs:3` で `#![warn(missing_docs)]` を有効にしているにもかかわらず、`src/` 配下の 67 箇所の `#[expect(missing_docs)]` がそれを打ち消しており、公開ボックス型のフィールドに説明が無い。抑制を撤廃してフィールドに doc コメントを付ける。

特に `duration` と `timescale` は、値がどのタイムスケール単位なのかが型からも doc からも判別できない。

## 優先度根拠

Medium。

`issues/closed/0008-bug-tkhd-duration-movie-timescale.md` は、`tkhd` の `duration` を `mvhd` の `timescale` 単位ではなくそのトラックの `timescale` 単位のまま書いていた不具合で、`tkhd` の `duration` を参照するプレイヤーでサンプルが打ち切られる実害が出ていた。

`TkhdBox::duration`（`src/boxes_moov_tree.rs:344`）にも `MvhdBox::timescale` / `MvhdBox::duration`（同 114-115）にも単位の記述が無い。`boxes` モジュールは公開されており利用者がこれらの型を直接組み立てられるため、同種の取り違えは今後も起こりうる。

一方で現時点で壊れている出力があるわけではなく、作業自体は機械的である。よって High ではなく Medium とする。

## 現状

`src/lib.rs:3` に `#![warn(missing_docs)]` があるが、次のとおり型単位で抑制されている。

| ファイル | `#[expect(missing_docs)]` | doc が必要な pub フィールド（概算） |
| --- | ---: | ---: |
| `src/boxes_moov_tree.rs` | 30 | 89 |
| `src/boxes_sample_entry.rs` | 18 | 86 |
| `src/boxes_fmp4.rs` | 11 | 43 |
| `src/descriptors.rs` | 3 | 15 |
| `src/boxes.rs` | 3 | 4 |
| `src/basic_types.rs` | 2 | 0 |
| 合計 | 67 | 約 237 |

抑制は型に付いており、フィールドには何も書かれていない。

```rust
// src/boxes_moov_tree.rs:333-344
/// [ISO/IEC 14496-12] TrackHeaderBox class (親: [`TrakBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct TkhdBox {
    // ...
    pub duration: u64,
```

型自体には規格名と親ボックスの doc があるが、フィールドの単位や意味は分からない。

フィールドに doc が付いている例外は `MdhdBox::language`（`src/boxes_moov_tree.rs:776` の `/// ISO-639-2/T language code`）の 1 つだけで、これが唯一の先例になる。

## 設計方針

型単位の `#[expect(missing_docs)]` を削除し、その型の全 pub フィールドに doc コメントを付ける。

- doc は日本語で書く（`AGENTS.md` の「コメントは全て日本語にすること」）
- **単位・基準が値の解釈に影響するフィールドには、それを必ず書く**。特に次は 0008 の再発防止として重点的に扱う
  - `MvhdBox::timescale` / `MvhdBox::duration`
  - `TkhdBox::duration`（`mvhd` の `timescale` 単位である旨）
  - `MdhdBox::timescale` / `MdhdBox::duration`（そのトラック固有の `timescale` 単位である旨）
- 仕様由来の値には根拠資料名を添える（`shiguredo-rust` の「仕様由来の機能を実装する場合は、根拠資料名・節番号・将来変更される可能性があることをコードコメントで明記すること」）。節番号は原典で確認できた場合のみ添える（本リポジトリに `refs/` は無いため、既存の引用はすべてクラス名のみ）
- **フィールド名の言い換えにしかならない doc は書かない**。`missing_docs` を満たすためだけに `/// track_id` のような内容の無い 1 行を並べると、抑制を消した意味が失われる。書くことが無いフィールドは、その型の doc に補足を足すか、隣接フィールドとまとめて説明する

### 実装順

0008 の原因になった `boxes_moov_tree.rs` から着手し、以降は抑制の多い順（`boxes_sample_entry.rs` → `boxes_fmp4.rs` → `descriptors.rs` → `boxes.rs` → `basic_types.rs`）とする。

`basic_types.rs` の 2 件は pub フィールドを持たない型に付いているため、対象が他と異なる。実装時に個別に確認する。

## 完了条件

- `src/` 配下から `#[expect(missing_docs)]` が 0 件になること
- `MvhdBox` / `TkhdBox` / `MdhdBox` の `timescale` と `duration` に、どのタイムスケール単位かが明記されていること
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm` が通ること
- `cargo fmt --all --check` / `cargo test --workspace --exclude c-api` / `cargo test -p c-api --lib` / `cargo clippy --workspace --all-targets -- -D warnings` が通ること
- `CHANGES.md` の `### misc` にエントリを追加すること

## 解決方法

1. 対象ファイルごとに `#[expect(missing_docs)]` を削除し、`cargo doc` が出す `missing documentation` 警告を手がかりに、フィールドへ doc コメントを追加する
2. 規格の記述と実装の対応が曖昧なフィールドは、`decode` / `encode` の実装を読んで意味を確認してから書く。推測で書かない
3. 全ファイルを終えたら `RUSTDOCFLAGS="-D warnings" cargo doc` で残りが無いことを確認する
