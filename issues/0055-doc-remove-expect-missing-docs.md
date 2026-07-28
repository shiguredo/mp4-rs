# 公開型の `#[expect(missing_docs)]` を撤廃して各 pub 項目に doc コメントを付ける

- Priority: Medium
- Created: 2026-07-28
- Completed: 2026-07-28
- Model: Opus 5
- Branch: feature/update-remove-expect-missing-docs
- Polished: 2026-07-28

## 目的

`src/lib.rs:3` で `#![warn(missing_docs)]` を有効にしているにもかかわらず、`src/` 配下の 67 箇所の `#[expect(missing_docs)]` がそれを打ち消しており、公開型（ボックス型・ディスクリプター型・基本型）の pub フィールド・pub enum variant・struct-like variant 内の named field に説明が無い。抑制を撤廃してこれらの pub 項目に doc コメントを付ける。

特に `duration` は、値がどの `timescale`（movie 全体か特定トラックか）で表された尺なのかが型からも doc からも判別できない。`timescale` 自身についても、それが何のタイムスケール（movie 全体か特定トラックか）を定義しているのかが doc に無い。

## 優先度根拠

Medium。

`issues/closed/0008-bug-tkhd-duration-movie-timescale.md` は、`tkhd` の `duration` を `mvhd` の `timescale` 単位ではなくそのトラックの `timescale` 単位のまま書いていた不具合で、`tkhd` の `duration` を参照するプレイヤーでサンプルが打ち切られる実害が出ていた。

`TkhdBox::duration`（`src/boxes_moov_tree.rs:344`）にも `MvhdBox::timescale` / `MvhdBox::duration`（同 114-115）にも単位の記述が無い。`boxes` モジュールは公開されており利用者がこれらの型を直接組み立てられるため、同種の取り違えは今後も起こりうる。

一方で現時点で壊れている出力があるわけではないため、High ではなく Medium とする。

## 現状

`src/lib.rs:3` に `#![warn(missing_docs)]` があるが、次のとおり型単位で抑制されている。抑制対象には struct だけでなく enum も含まれるため、doc 付与の対象は「pub フィールド」と「pub enum variant（および struct-like variant の内側の named field）」の合算になる。

| ファイル | `#[expect(missing_docs)]` | doc が必要な pub 項目（フィールド + variant） |
| --- | ---: | ---: |
| `src/boxes_moov_tree.rs` | 30 | 92（フィールド 90（`StszBox::Fixed` / `Variable` 内の named field 3 を含む）+ `StszBox` の variant 2） |
| `src/boxes_sample_entry.rs` | 18 | 106（フィールド 93 + `SampleEntry` の variant 13） |
| `src/boxes_fmp4.rs` | 11 | 38 |
| `src/descriptors.rs` | 3 | 15 |
| `src/boxes.rs` | 3 | 11（フィールド 4 + `RootBox` の variant 7） |
| `src/basic_types.rs` | 2 | 4（`BoxSize` の variant 2 + `Either` の variant 2。フィールドは無い） |
| 合計 | 67 | 266 |

件数は 6 ファイル全体の `#[expect(missing_docs)]` 行をコメントアウトして `cargo build --lib` を通し、`missing_docs` 警告の実測値（フィールド 240（うち struct-like variant 内の named field 3 件を含む）+ variant 26 = 266）から確定した数値。

抑制対象の型のうち、以下は enum で、variant 側に doc が必要になる。

- `src/basic_types.rs`: `BoxSize`（`U32(u32)` / `U64(u64)`）、`Either<A, B>`（`A(A)` / `B(B)`）
- `src/boxes.rs`: `RootBox`（`Free` / `Mdat` / `Moov` / `Moof` / `Mfra` / `Sidx` / `Unknown`）
- `src/boxes_sample_entry.rs`: `SampleEntry`（`Avc1` / `Hev1` / `Hvc1` / `Vp08` / `Vp09` / `Av01` / `Opus` / `Mp4a` / `Flac` / `Stpp` / `Wvtt` / `Tx3g` / `Unknown`）
- `src/boxes_moov_tree.rs`: `StszBox`（`Fixed { sample_size, sample_count }` / `Variable { entry_sizes }`。struct-like variant の named field も個別に doc が必要）

内部の pub フィールド・variant の大半には doc コメントが無い。例:

```rust
// src/boxes_moov_tree.rs:332-344
/// [ISO/IEC 14496-12] TrackHeaderBox class (親: [`TrakBox`])
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct TkhdBox {
    // ...
    pub duration: u64,
```

型自体には規格名と親ボックスの doc があるが、フィールドの単位や意味は分からない。

ただし抑制ブロック内のフィールドの一部には既に doc が付いている（一覧は網羅ではなく代表例。行番号は起票時点のもの）。

- `src/boxes_moov_tree.rs:776` `MdhdBox::language`（`/// ISO-639-2/T language code`。ただし `language code` の部分は日本語化対象。「規格用語の写し以外は日本語」規則により、今回の機会に `/// ISO-639-2/T 言語コード` 等へ揃える）
- `src/boxes_moov_tree.rs:905` `HdlrBox::name`（他実装の仕様外れに対処するため型を仕様と変えた事情の注記）
- `src/boxes_moov_tree.rs:991` `MinfBox::media_header`（`Option` でラップする理由と、`None` を許容する理由）
- `src/boxes_fmp4.rs:442` `TfdtBox::version`、`src/boxes_fmp4.rs:1077` `TfraBox::version`（ラウンドトリップ用に元のバージョンを保持する旨の注記）
- `src/boxes_fmp4.rs:1084` `TfraBox::length_size_of_traf_num` ほか（`length_size_of_*` の意味と値域の注記）

## 設計方針

型単位の `#[expect(missing_docs)]` を削除し、その型の全 pub フィールド・全 pub enum variant（struct-like variant の内側の named field も含む）に doc コメントを付ける。`missing_docs` は各 pub 項目に個別の `///` を要求するため、複数のフィールド・variant で 1 つの doc ブロックを共有することはできない。

- doc は日本語で書く（`AGENTS.md` の「コメントは全て日本語にすること」）。ISO/IEC 規格の class 名・フィールド名や `ISO-639-2/T` のような正式名称の写しは英語のまま許容する（原典と一致させる必要があるため）が、それ以外の説明文は日本語で書く
- 既存の doc は原則書き換えないが、次の 2 つは例外として書き換える。範囲は本 issue で扱う **6 ファイル全体（`src/basic_types.rs` / `src/boxes.rs` / `src/boxes_fmp4.rs` / `src/boxes_moov_tree.rs` / `src/boxes_sample_entry.rs` / `src/descriptors.rs`）の pub 項目全体** とし、`#[expect(missing_docs)]` を外す対象型に限定しない
  - **例外 A（英語規則違反の日本語化）**: 上記の「英語規格用語の写し以外は日本語」の規則に反する既存 doc は、今回の機会に日本語へ揃える（例: `CttsBox::version` の `/// full box version`、`CslgBox::version` / `composition_to_dts_shift` / `composition_start_time` / `composition_end_time` の英語一行 doc 群）
  - **例外 B（単位・基準の追記）**: 単位・基準（timescale 依存など）が値の解釈に影響するのに既存 doc に単位表記が無い場合は、単位表記を含めた形に doc を直す（既存 doc が日本語で単位だけが足りない場合は追記、既存 doc が例外 A の日本語化対象でもある場合は日本語化と同時に単位も含めて書き直す）。例: `SidxReference::subsegment_duration` の `/// サブセグメントの継続時間` は日本語のまま `SidxBox::timescale` 単位である旨を追記、`CslgBox::composition_start_time` / `composition_end_time` は例外 A の日本語化と同時に media timescale 単位を含める
- 上記 2 例外の派生として、新規 variant doc の意味が既存の型 doc だけでは理解できない場合は、その variant を含む型の doc にも共通の背景（例: `BoxSize` の型 doc に `size==1` 分岐と `largesize` の存在への言及を追加）を必ず追記する
- 既存 doc のスタイル（規格用語の直訳、実装上の注記、値域や単位の注記）は新規 doc の参考にする
- **単位・基準が値の解釈に影響するフィールドには、それを必ず書く**。特に次は 0008 の再発防止として重点的に扱う（同種のトラップは他フィールドにもあるが、以下は特に混同が発生しやすい代表例）
  - `MvhdBox::timescale`（movie 全体のタイムスケール定義。1 秒あたりの時間単位数）
  - `MvhdBox::duration`（`MvhdBox::timescale` 単位で表した movie 全体の尺）
  - `TkhdBox::duration`（`MvhdBox::timescale` 単位で表したトラックの尺。トラック固有の `MdhdBox::timescale` ではないことを明記する）
  - `MdhdBox::timescale`（そのトラック固有のタイムスケール定義。1 秒あたりの時間単位数）
  - `MdhdBox::duration`（`MdhdBox::timescale` 単位で表したトラックの尺）
  - `ElstEntry::edit_duration` と `ElstEntry::media_time`（`src/boxes_moov_tree.rs:580-582`。同一 struct 内で `edit_duration` が movie timescale 単位・`media_time` が media timescale 単位となる二重 timescale の代表例。取り違えると 0008 と同種の被害が出る）
  - `SidxBox::timescale`（`src/boxes_fmp4.rs:801`。その sidx が定める独立したタイムスケール定義。movie/media とは別系統。1 秒あたりの時間単位数）
- 上記以外の pub フィールドについても、`decode` / `encode` を読んで timescale やその他の単位・基準が値の解釈に影響する場合は同様に単位を明記する（例: `MehdBox::fragment_duration` は movie timescale、`SttsEntry::sample_delta` や `TrexBox::default_sample_duration` は media timescale、`SidxBox::earliest_presentation_time` や `SidxReference::subsegment_duration` は `SidxBox::timescale` 単位、など）
- 型で表現済みの不変条件（例: `NonZeroU32` によるゼロ不可）は doc で重複させず、フィールド固有の意味の記述に注力する
- 仕様由来の値には根拠資料名を添える（`shiguredo-rust` の「仕様由来の機能を実装する場合は、根拠資料名・節番号・将来変更される可能性があることをコードコメントで明記すること」）。節番号は原典で確認できた場合のみ添える（本リポジトリに `refs/` は無いため、既存の引用はすべてクラス名のみ）
- enum variant の doc は variant の性格に応じて次のように書く
  - **規格由来の variant**: その variant が表す規格上の意味を書く（例: `SampleEntry::Avc1` → 「H.264 用のサンプルエントリー」）
  - **`Unknown` などの catch-all variant**: 既知の分類に該当しない値を保持する旨と、どのような場合に到達するかを書く（例: `RootBox::Unknown` → 「既知のトップレベルボックス型（`free` / `mdat` / `moov` / `moof` / `mfra` / `sidx`）に該当しない box を demux 時に保持する場合や、mux 時に任意の未知 box を組み込む場合に使う」）
  - **規格上の符号化分岐を variant で表す場合**（例: `BoxSize::U32` / `U64`、`StszBox::Fixed` / `Variable`）: 規格上は 1 つの型で表現される符号化の分岐（`Box` 定義の `if (size==1) { largesize }`、`SampleSizeBox` の `sample_size==0` 分岐）を Rust の variant で区別している。分岐条件が規格で強制される場合と、規格が許す複数の符号化表現から実装が選ぶ場合の両方を含む。variant doc にはその variant が対応する符号化上の条件を書く。例: `BoxSize::U32` → 「32-bit の `size` フィールドで表される場合」、`BoxSize::U64` → 「`size==1` として 64-bit の `largesize` フィールドが後続する場合」、`StszBox::Fixed` → 「規格の wire-format 上の `sample_size` フィールドが非零で、全サンプルが同一サイズの場合」、`StszBox::Variable` → 「規格の wire-format 上の `sample_size` フィールドが 0 で per-sample の `entry_size` 配列を持つ場合」
- **フィールド名・variant 名の言い換えにしかならない doc は書かない**。`missing_docs` を満たすためだけに `/// track_id` のような内容の無い 1 行を並べると、抑制を消した意味が失われる。各項目には固有の意味（単位・値域・規格上の役割・実装上の注記のいずれか）を必ず書く。ただし `Either<A, B>` の `A(A)` / `B(B)` のように、variant が保持する値以外に意味を追加していない汎用ラッパーの variant に限り、型の doc に共通の背景を十分に書いた上で variant 側は「型引数 `A` に対応する variant」等の最小限の記述に留めてよい（`BoxSize::U32` / `U64` は規格上の分岐条件という追加意味を持つため該当しない）

### 実装順

0008 の原因になった `boxes_moov_tree.rs` から着手し、以降は抑制の多い順（`boxes_sample_entry.rs` → `boxes_fmp4.rs` → `descriptors.rs` → `boxes.rs` → `basic_types.rs`）とする。

## 完了条件

- `src/` 配下から `#[expect(missing_docs)]` が 0 件になること
- `MvhdBox::timescale` の doc に「movie 全体のタイムスケール定義（1 秒あたりの時間単位数）」である旨が、`MdhdBox::timescale` の doc に「そのトラック固有のタイムスケール定義（1 秒あたりの時間単位数）」である旨が明記されていること
- `MvhdBox::duration` / `MdhdBox::duration` の doc に、それぞれが自身の型の `timescale` 単位で表した尺である旨が明記されていること
- **`TkhdBox::duration` の doc に、`MvhdBox::timescale` 単位で表したトラックの尺である旨と、トラック固有の `MdhdBox::timescale` 単位ではない旨が明記されていること**（0008 の直接の再発防止条件）
- `ElstEntry::edit_duration` の doc に movie timescale 単位である旨、`ElstEntry::media_time` の doc に media timescale 単位である旨、および同一 struct 内で timescale が異なる旨が明記されていること
- `SidxBox::timescale` の doc に「その sidx が定める独立したタイムスケール定義（1 秒あたりの時間単位数）」である旨と、movie / media とは別系統である旨が明記されていること
- 例外 A（英語規則違反の日本語化）の対象既存 doc が本 issue で扱う 6 ファイル内に残っていないこと（少なくとも `CttsBox::version`、`CslgBox::version` / `composition_to_dts_shift` / `composition_start_time` / `composition_end_time` の英語一行 doc 群が日本語へ揃っていること。解決方法 step 3 の grep コマンドの残件から規格用語の写しを除いた結果が 0 件であることで verify する）
- 例外 B（既存 doc への単位追記）の対象既存 doc に単位が明記されていること（少なくとも `SidxReference::subsegment_duration` の doc に `SidxBox::timescale` 単位である旨、`CslgBox::composition_start_time` / `composition_end_time` の doc に media timescale 単位である旨が書かれていること）
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm` が通ること
- `cargo fmt --all --check` / `cargo test --workspace --exclude c-api` / `cargo test -p c-api --lib` / `cargo clippy --workspace --all-targets -- -D warnings` が通ること
- `CHANGES.md` の `## develop` 直下の `### misc` にエントリを追加すること

## 解決方法

1. 対象ファイルごとに `#[expect(missing_docs)]` を削除し、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm` が出す `missing documentation` エラーを手がかりに、pub フィールドおよび pub enum variant（および struct-like variant の内側の named field）へ doc コメントを追加する
2. 規格の記述と実装の対応が曖昧な項目は、`decode` / `encode` の実装を読んで意味を確認してから書く。推測で書かない
3. 例外 A（既存英語 doc の日本語化）と例外 B（既存 doc への単位追記）の対象を網羅的に洗い出す。`grep -nE '^\s*///\s+[A-Za-z][A-Za-z]' src/{basic_types,boxes,boxes_fmp4,boxes_moov_tree,boxes_sample_entry,descriptors}.rs` で英字で始まる既存 doc 行を機械的に列挙し（大文字始まりも小文字始まりも拾う。設計方針で名指しした `full box version` / `composition ...` 系はいずれも小文字始まりのため `[A-Z]` だけでは取り逃す）、規格用語の写しを除いた行を日本語へ揃える。単位追記対象（`SidxReference::subsegment_duration` は既存日本語 doc に単位を追記、`CslgBox::composition_start_time` / `CslgBox::composition_end_time` は例外 A の日本語化と同時に media timescale 単位を含めて書き直す）は個別に読んで対応する
4. 全ファイルを終えたら再度 `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm` を通し、残りが無いことを確認する
5. `cargo fmt --all` を実行して doc 追加時のフォーマット崩れを直したうえで、`cargo fmt --all --check` を含む完了条件のコマンド一式が通ることを確認する
