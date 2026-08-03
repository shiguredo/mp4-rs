# examples と doc コメント内サンプルコードの日本語出力文字列を英語に置換する

- Priority: Low
- Created: 2026-05-20
- Completed: 2026-07-24
- Model: opencode mimo-v2.5-pro
- Branch: feature/update-examples-samples-english
- Polished: 2026-07-24

## 目的

`examples/*.rs` と `src/*.rs` の doc コメント内サンプルコード (いずれも `no_run` フェンス内) のうち、`println!` / `.expect()` / `assert_eq!` / `todo!()` の引数として渡す文字列に日本語が残っている箇所を英語に置換する。既に英語で書かれている文字列のスタイル統一 (Sentence case / lowercase / `key=value` / `Label: value` などの揃え直し) は本 issue のスコープ外とする。

- `examples/demux.rs` の `eprintln!` が既に英語であり、日本語の `println!` などを英語化すると同一プロジェクト内の日英混在が解消される。
- `println!` / `.expect()` / `assert_eq!` は実行時に stdout / stderr / パニックメッセージとして出力されるため、CLAUDE.md「ログメッセージは全て英語にすること」を適用する。
- `todo!()` の引数は `no_run` サンプルでは実際にはパニックとして実行されないが、実行時にはパニックメッセージとなるうえサンプルコード内で読者に提示される文字列でもあるため、同じ規約を適用する。
- 上記 4 マクロに準ずる実行時出力・パニック系マクロ (`panic!` / `unreachable!` / `unimplemented!` / `assert!` / `assert_ne!` / `write!` / `writeln!` など) にも同じ規約を適用する。ただし対象 10 ファイル (下記) の doc コメント内サンプルには該当箇所が 0 件のため、本 issue で実際に書き換えるのは上記 4 マクロのみ。完了条件の grep パターンは将来の追加・退化検出のために全マクロを含める。
- `.expect()` / `assert!` 系メッセージの英訳は Rust 標準ライブラリの慣習に沿い lowercase 始まりの平文とする (原文が失敗を述べる場合は `"failed to ..."`、事前条件を述べる場合は `"XXX must ..."` など)。既存英語 (`"NAL unit size exceeds u32::MAX"` / `"muxer creation failed"` など) との `"failed to ..."` / `"... failed"` のスタイル分裂は本 issue では許容し、統一が必要なら別 issue で対応する。

Branch prefix は shiguredo-git の 6 種 (`fix-` / `add-` / `change-` / `update-` / `refactor-` / `debug-`) に `fmt-` が含まれないため `feature/update-` を採用した。

## 優先度根拠

機能への影響はない表記統一のため Low。ただし放置すると examples 系サンプルコードのスタイル分裂が続き、新しいサンプルを追加するたびに「どちらに合わせるか」の判断コストがかかる。

## 対象範囲

各行番号はマクロ呼び出しの開始行 (単一行の場合はその行、複数行呼び出しの場合は先頭 `println!(` / `.expect(` / `assert_eq!(` / `todo!(` を含む行)。

### 英語化対象

**`examples/fmp4.rs`:**

- `println!` の日本語文字列 10 箇所 (行 152, 162, 191, 198, 207, 210, 225, 238, 252, 254)
  - 行 252 `"mfro.size = {mfro_size} (mfra 全体サイズと一致)"` は英日混在のため日本語部分のみ英語化する。
- `.expect()` の日本語文字列 2 箇所 (行 149, 245)
- `assert_eq!` のメッセージ引数 1 箇所 (行 247 から始まる `assert_eq!` の第 3 引数、日本語文字列自体は行 250)

**`src/*.rs` の doc コメント内サンプル (全て `no_run` フェンス):**

- `src/demux.rs`
  - `.expect()` の日本語文字列 (行 12, 23)
  - `println!` の日本語文字列 (行 24, 26)
- `src/demux_mp4_file.rs`
  - `.expect()` の日本語文字列 (行 14, 25, 33)
  - `println!` の日本語文字列 (行 26, 28, 34)
- `src/demux_fmp4_file.rs`
  - `todo!()` の日本語文字列 (行 20)
  - `println!` の日本語文字列 (行 33)
- `src/demux_fmp4_segment.rs`
  - `todo!()` の日本語文字列 (行 22, 29)
  - `println!` の日本語文字列 (行 26)
- `src/demux_mp4_file_kind_detector.rs`
  - `todo!()` の日本語文字列 (行 15)
- `src/mux_mp4_file.rs`
  - `todo!()` の日本語文字列 (行 32)
- `src/mux_fmp4_segment.rs`
  - `todo!()` の日本語文字列 (行 36)

### そのまま維持 (既に英語)

以下は本 issue のスコープ (日本語→英語の置換) から外れる。スタイル (Sentence case / lowercase / `key=value` 形式 / `Label: value` 形式など) の統一は行わない。

- `examples/demux.rs` の `println!` 11 箇所 (行 50, 54, 55, 56, 65, 72, 73, 74, 75, 76, 80) と `eprintln!` 全 4 箇所 (行 13, 19, 45, 86)
- `examples/fmp4.rs` の英語 `println!` 3 箇所 (行 200 / 212 / 227、いずれも `"track_id=..."` 形式)
- `examples/fmp4.rs` の英語 `.expect()` 3 箇所 (行 76, 106, 107、それぞれ `"NAL unit size exceeds u32::MAX"` / `"non-zero"` / `"non-zero"`)
- `src/demux_fmp4_file.rs:36` の doc コメント内 `println!` (`"track_id={}, timestamp={}, size={}"`)
- `src/demux_mp4_file_kind_detector.rs:32-33` の doc コメント内 `println!` (`"regular MP4"` / `"fragmented MP4"`)
- `src/mux.rs:12` の doc コメント内 `.expect()` (`"muxer creation failed"`)
- `src/mux_fmp4_segment.rs:42` の doc コメント内 `.expect()` (`"non-zero"`)

### 対象外 (別カテゴリの規約に従うので触らない)

- コメント本文 (`//` / `///` / `//!` の説明文、および行末インラインコメント): CLAUDE.md「コメントは全て日本語にすること」に従い日本語のまま維持する。
  - 既知の日本語インラインコメント: `examples/demux.rs:8` (`// 1MB のバッファサイズ`)、`examples/fmp4.rs:117` (`// 48000 Hz で 20ms = 960`)、`examples/fmp4.rs:248` (`// u32 -> usize: 常に安全`)、`src/mux_mp4_file.rs:1269` (`// 誤ったオフセット`)。
- `examples/demux.rs:60, 77` の空引数 `println!();`: 文字列を持たない改行呼び出しなので置換対象がない。
- 対象 10 ファイル内の `#[cfg(test)]` unit test コード: CLAUDE.md「テストのログメッセージは全て日本語にすること」に従い日本語のまま維持する。既知の日本語 assert メッセージ: `src/mux_mp4_file.rs:1319` (`"Display 出力に \"Subtitle\" が含まれていない: {display}"`)。目的セクションの「準ずるマクロ」条項はこの `#[cfg(test)]` 配下のテストコードには適用しない。
- `pbt/tests/` と `crates/c-api/tests/e2e.rs` の `.expect()` / `assert!` / `prop_assert!`: 同じく日本語のまま維持する。これらは issue 0037 で対応する。`tests/decode_encode_test.rs` は既に日本語化されており規約準拠のため触らない。
- `examples/dump_wasm/` / `examples/transcode_wasm/` 配下: 独立クレートで `cargo run --example` の対象ではなく、対象マクロは全て既に英語であるため本 issue のスコープ外。

## 完了条件

以下のコマンドは **bash または zsh** で実行する (fish では `set -e` や `LC_ALL=C VAR ...` の文法が異なるため、`bash -c '...'` でラップするか、fish 向けに書き換える必要がある)。

- 上記「英語化対象」に列挙した全ての箇所が英語になっていること。
- 上記「そのまま維持」および「対象外」に列挙した箇所は変更されていないこと (`git diff` で確認)。
- **元の呼び出しの行構造 (改行、インデント、複数行フォーマット)、フォーマット文字列内 `{}` プレースホルダの数と順序、それに続く実引数の数と順序、行末インラインコメントの内容は変更しない**。対象マクロの文字列引数の中身のみを英訳する。特に `examples/fmp4.rs:247-251` の `assert_eq!` を 1 行圧縮して行 248 のインラインコメント `// u32 -> usize: 常に安全` を消失・変更させない。
- `cargo run --example demux tests/testdata/beep-aac-audio.mp4` と `cargo run --example fmp4` が正常終了 (終了ステータス 0) し、標準出力・標準エラー出力に日本語文字が含まれないこと。ANSI カラーエスケープが混入して機械検出を汚染しないよう `--color=never` を付ける。

    ```bash
    set -e
    cargo run --color=never --example demux tests/testdata/beep-aac-audio.mp4 > /tmp/demux_out.txt 2>&1
    LC_ALL=C grep -nE '[^[:print:][:space:]]' /tmp/demux_out.txt && echo NG || echo OK
    cargo run --color=never --example fmp4 > /tmp/fmp4_out.txt 2>&1
    LC_ALL=C grep -nE '[^[:print:][:space:]]' /tmp/fmp4_out.txt && echo NG || echo OK
    ```

    `set -e` により cargo の非零終了は即座に停止する。grep が日本語文字を検出した場合は `NG` を、検出されなかった場合は `OK` を出力する。両コマンドとも `OK` になれば良い。

- `cargo test --doc` が通ること (対象 doc コメントは全て `no_run` フェンスのため、コンパイル確認のみ)。
- `cargo doc --no-deps` の警告数が 0 件を維持していること (現状 develop も 0 件)。
- 機械的な残存チェックとして、対象マクロを直接パターン検索し、日本語を含む対象箇所が残っていないことを確認する。

    ```bash
    LC_ALL=C grep -En '(println!|eprintln!|\.expect\(|assert_eq!|assert!|assert_ne!|todo!\(|panic!\(|unreachable!\(|unimplemented!\(|write!\(|writeln!\().*[^[:print:][:space:]]' \
      examples/demux.rs examples/fmp4.rs \
      src/demux.rs src/demux_mp4_file.rs src/demux_fmp4_file.rs \
      src/demux_fmp4_segment.rs src/demux_mp4_file_kind_detector.rs \
      src/mux.rs src/mux_mp4_file.rs src/mux_fmp4_segment.rs
    ```

    このパターンは対象マクロ呼び出し行に日本語が同一行内に含まれるかを検出する。ただし複数行呼び出しで文字列引数が次行以降にある場合は漏れる。既知の漏れ箇所は `examples/fmp4.rs:152` (`println!(` 開始行、文字列は行 153) と `examples/fmp4.rs:247` (`assert_eq!(` 開始行、文字列は行 250) の 2 箇所であり、これらは目視で個別確認する。将来の複数行呼び出し追加に備えて以下の広めパターンでも補完し目視確認する。

    ```bash
    LC_ALL=C grep -En '[^[:print:][:space:]]' \
      examples/demux.rs examples/fmp4.rs \
      src/demux.rs src/demux_mp4_file.rs src/demux_fmp4_file.rs \
      src/demux_fmp4_segment.rs src/demux_mp4_file_kind_detector.rs \
      src/mux.rs src/mux_mp4_file.rs src/mux_fmp4_segment.rs
    ```

    このコマンドは対象 10 ファイルに含まれる全ての非 ASCII 文字を検出する。出力される行は次のいずれかであることを目視で確認する。それ以外の日本語が残っていれば個別に修正する。
    - **散文コメント** (`//!` / `///` / `//` で始まる説明文の日本語): 無制限に許容する。
    - **行末インラインコメント**: 上記「対象外」節に列挙した既知の 4 箇所 (`examples/demux.rs:8`, `examples/fmp4.rs:117`, `examples/fmp4.rs:248`, `src/mux_mp4_file.rs:1269`) のみに一致すること。
    - **`#[cfg(test)]` unit test コード内のマクロ引数・インラインコメント**: 上記「対象外」節に列挙した通り本 issue の対象外 (既知例: `src/mux_mp4_file.rs:1319` の `assert!` メッセージ)。

## 解決方法

以下の順で実装した。作業ブランチは `feature/update-examples-samples-english`。

1. `examples/fmp4.rs` の日本語 `println!` 10 箇所・`.expect()` 2 箇所・`assert_eq!` メッセージ 1 箇所 (計 13 箇所) を英訳した。行 247-251 の `assert_eq!` は行 248 のインラインコメント `// u32 -> usize: 常に安全` を維持したまま複数行構造を保った。行 152 の複数行 `println!` は英訳後の文字列長が短くなったため cargo fmt (rustfmt) の判定で 1 行に自動整形された (行構造維持ルールより rustfmt を優先)。
2. `src/*.rs` の doc コメント内サンプル 12 箇所を英訳した (`src/demux.rs` / `src/demux_mp4_file.rs` の `.expect()` と `println!`、および `src/demux_fmp4_file.rs` / `src/demux_fmp4_segment.rs` / `src/demux_mp4_file_kind_detector.rs` / `src/mux_mp4_file.rs` / `src/mux_fmp4_segment.rs` の `todo!()` と `println!`)。元の行構造・インデントを維持した。
3. `examples/demux.rs` と `src/mux.rs` は対象マクロ引数の文字列が既に全て英語のため差分ゼロ (「そのまま維持」対象)。`src/demux_fmp4_file.rs:36` の `println!("track_id={}, ...")`、`src/mux_fmp4_segment.rs:42` の `.expect("non-zero")` 等の既存英語箇所もそのまま維持した。
4. `CHANGES.md` の `## develop` セクションの既存 `### misc` サブセクション末尾に `[UPDATE]` エントリを追記した。
5. `cargo test --doc` (8 tests pass) / `cargo doc --no-deps` (警告 0 件) / `cargo run --color=never --example demux tests/testdata/beep-aac-audio.mp4` / `cargo run --color=never --example fmp4` (いずれも正常終了、出力に日本語なし) を pass させた。対象マクロ直接 grep で日本語残存 0 件。全体 grep 結果 (Python で代替検証、下記残懸念参照) は「散文コメント」と既知の 4 箇所インラインコメント (`examples/demux.rs:8` / `examples/fmp4.rs:117` / `examples/fmp4.rs:248` / `src/mux_mp4_file.rs:1269`) のみで、それ以外の日本語残存なし。
6. 本コミットで `Completed:` を作業完了日に更新し、`## 解決方法` を実施結果に書き換えた。
7. 続くコミットで `git mv issues/0004-fmt-examples-samples-english.md issues/closed/` により closed に移動した。

### 完了条件検証の残懸念

issue の「完了条件」に記載した `LC_ALL=C grep -E '[^[:print:][:space:]]'` パターンは macOS BSD grep で日本語文字を検出できない (`printf 'あ\n' | LC_ALL=C grep -E '[^[:print:][:space:]]'` で 0 ヒット) ため、Python (`re.search(r'[^\x00-\x7f]', line)`) で代替検証した。issue の grep パターン修正は別途対応する。

## CHANGES.md

```markdown
- [UPDATE] examples と doc コメント内サンプルコードの日本語出力文字列を英語に置換する
  - @sile
```
