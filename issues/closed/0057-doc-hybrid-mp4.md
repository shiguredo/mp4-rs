# Hybrid MP4 の取り扱いについての補足ドキュメントを追加する

- Priority: Medium
- Created: 2026-07-28
- Completed: 2026-07-31
- Model: Opus 4.7
- Branch: feature/add-hybrid-mp4-doc
- Polished: 2026-07-31
- Reporter: @tohta

## 目的

`Mp4FileMuxer::advance_position()` を用いた Hybrid MP4 の書き出しについて、以下 2 つの役割を果たす補足ドキュメントを `docs/` 配下に追加する。

1. **役割分担の明確化**: 本 crate が担当する範囲（内部書き込み位置の管理、`advance_position()` 直後の強制チャンク切り替え、`data_offset` の整合検査など）と、利用側が担当する範囲（moof の構築、`trun.data_offset` の計算、mdat ヘッダの書き出し、書き込み位置の同期）を明示する。骨格コードよりもこの分界の説明が中心になる。
2. **Hybrid MP4 の用語参照先の提供**: Hybrid MP4 という形式の簡潔な定義（何を解こうとしている形式か、標準 MP4 / fMP4 との違い）を crate 側で用意しておくことで、hisui など上位実装のドキュメントや API リファレンスから「Hybrid MP4 とは」の参照先として使えるようにする。現状は用語の一次説明が本 crate と hisui のどちらにも無く、外部（OBS ブログ）に頼るしかない。

現状、当該 API は Rust および C API 双方の doc コメントで「OBS の Hybrid MP4 のように、サンプルデータの間に moof / mdat ヘッダなどの非サンプルデータが挿入される場合に使用する」と説明されているが、以下が API リファレンス側からは伝わらない。

- Hybrid MP4 とは何か（何を解こうとしている形式か）
- 標準 MP4 / fMP4 との違い
- 本 crate が担当する範囲と、利用側が担当する範囲
- 実際に書き出す際の呼び出し順序

追加ドキュメントは `src/docs.rs` から `include_str!` で取り込み、doctest として検証する。粒度は必ずしも `docs/subtitle.md` に合わせない（後述）。

## 優先度根拠

Medium。既存の API (`Mp4FileMuxer::advance_position()`) は 2026.3.0 で追加済みで、機能そのものは動作するため High ではない。ただし、この API 単体では組み立て手順の全体像が伝わらず、利用者が OBS のブログ記事や hisui の実装を都度読みに行く必要がある。本 crate 単体で自己完結させるためには追加が必要であり、後回しにする理由も無いため Low ではない。

## 現状

- `src/mux_mp4_file.rs` の `Mp4FileMuxer::advance_position()` に、Hybrid MP4 を意図した用途である旨の doc コメントがある
- 同等の API が C API にも公開されている
  - `crates/c-api/src/mux.rs` の `mp4_file_muxer_advance_position()`
  - `crates/c-api/include/mp4.h` の `mp4_file_muxer_advance_position()` 宣言
- `CHANGES.md` の 2026.3.0 に `[ADD] Mp4FileMuxer::advance_position() メソッドを追加する` のエントリがあり、Hybrid MP4 対応が動機であることが記載されている
- 一方、`docs/` 配下には Hybrid MP4 の説明が無く、既存の補足ドキュメントは `docs/subtitle.md` のみ
- `src/docs.rs` は `docs/*.md` を `include_str!` で取り込み `pub mod` として公開する index になっており、Rust コード例は doctest で検証される仕組み

## 設計方針

- `docs/hybrid_mp4.md` を新規追加する
- `src/docs.rs` に `pub mod hybrid_mp4 {}` を追加し、`#[doc = include_str!("../docs/hybrid_mp4.md")]` で取り込む
  - 取り込み方法は `subtitle.md` と同じ
- **Rust コード例の書き方は `docs/subtitle.md` に揃える**。すなわち、コード例本体は関数定義だけを書き、その関数を呼び出さない形にする（`docs/subtitle.md` の `fn mux_subtitle() -> Result<Vec<u8>, MuxError>` の流儀）。doctest はデフォルトの `fn main() {}` に包まれ、コンパイルは検証されるが実行はされない。これにより「moof の実バイトを組み立てないまま骨格を書ける」という要件と「`cargo test --doc` が通る」という要件を同時に満たせる。`no_run` / `ignore` 属性は使わない
- **粒度は `docs/subtitle.md` に揃えない**。subtitle.md は本 crate 内で完結する 3 形式（stpp / wvtt / tx3g）の詳細説明で情報密度が高いが、Hybrid MP4 に関して本 crate が実際に担当する処理は「位置管理・チャンク切り替え」に留まるため、書ける内容は subtitle.md より本質的に少ない。無理に埋めて情報を薄めない
- 骨格コードは呼び出し順序と責任分界を示す最小の関数定義に留める。moof の構築、`trun.data_offset` の計算、mdat ヘッダの書き出しといった詳細は本 crate の範囲外なので、これらは hisui 実装への外部リンクに委譲する
- C API の呼び出し例は Rust の doctest では扱えないので、文中で C API 側の対応関数名（`mp4_file_muxer_advance_position()`）を参照するだけに留める
- OBS のブログ記事および hisui の実装への参考リンクを本文末尾に置く

想定する節構成（叩き台。実装時に必要に応じて調整）:

- Hybrid MP4 とは（形式の定義。上位実装からの参照先になる部分。1〜2 段落で簡潔に）
- 標準 MP4 / fMP4 との違い（moof/mdat 構造を moov の後段に併存させる、といった構造レベルの差を短く）
- 本 crate の対応方針と責任分界（**中心となる節**。crate 側 = 位置管理・チャンク切り替え・`data_offset` 整合検査 / 利用側 = moof 構築・`trun.data_offset` 計算・mdat ヘッダ書き出し・書き込み位置の同期、を明示）
- 書き出しの呼び出し順序（`Mp4FileMuxer::new()` → `initial_boxes_bytes()` → 利用側の書き込み → `append_sample()` / `advance_position()` → `finalize()` → `FinalizedBoxes::offset_and_bytes_pairs()` による後書き、の骨格を doctest で示す。`finalize()` 後の後書きは crate の使用フロー（`src/mux_mp4_file.rs` のクレートレベル doc 例）に揃える。moof 内部の詳細は省略）
- 注意事項（`advance_position()` を呼んだ直後の次サンプルは強制的に新規チャンク開始になる、`data_offset` は crate 内部の書き込み位置と一致させる必要がある、など既存 API doc の制約を利用者視点で整理）
- 参考リンク

## 完了条件

- `docs/hybrid_mp4.md` が新規作成され、以下をすべて含んでいること
  - Hybrid MP4 の定義と背景（何を解く形式か、標準 MP4 / fMP4 との違い）。上位実装から用語参照先として使える簡潔な説明になっていること
  - 本 crate での対応方針（`Mp4FileMuxer::advance_position()` の役割、moof / mdat ヘッダの組み立ては利用側の責務であること）。**crate 側と利用側の責任分界が読者に明確に伝わること**
  - Rust API での呼び出し順序の骨格コード例（doctest として `cargo test --doc` が通る。moof 内部の詳細は範囲外として省略してよい）
  - 利用側で意識すべき制約（`advance_position()` を呼んだ後の挙動、`data_offset` と内部書き込み位置の整合など）
  - 参考リンク（OBS ブログ・hisui 実装）
- `src/docs.rs` に `hybrid_mp4` モジュールが追加され、`cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` および `cargo test --doc` が通ること

## 解決方法

- `docs/hybrid_mp4.md` を新規作成した
  - 節構成: 「Hybrid MP4 とは」「標準 MP4 / fMP4 との違い」「本 crate の対応方針と責任分界」「書き出しの呼び出し順序」「注意事項」「参考リンク」
  - Rust コード例は関数定義のみを書き、`no_run` / `ignore` を使わずデフォルトの `fn main() {}` でコンパイルを検証する形（`docs/subtitle.md` に揃えた）
- `src/docs.rs` に `pub mod hybrid_mp4 {}` を追加し、`#[doc = include_str!("../docs/hybrid_mp4.md")]` で取り込んだ
- `CHANGES.md` に `[ADD] Hybrid MP4 の取り扱いについての補足ドキュメントを追加する` を追加した
- レビュー指摘に基づく修正
  - `finalize()` 後の書き戻しループで、`output` の長さを超える位置に配置される moov に対して `copy_from_slice` が範囲外書き込みを起こす問題を、事前 `output.resize()` で解消した
  - 「フラグメントを繰り返す場合」のコメントを、1 フラグメント内に複数サンプルが入る前提の「非サンプル書き出し 1 回 → advance_position 1 回 →（サンプル書き出し + append_sample） × N」表現に書き直した
  - 比較表の「録画中の構造」列と標準 MP4 行の時制齟齬、および「完成後の見え方」列の情報量ゼロを解消（列名を「完成後の構造」に変え、finalize 後のレイアウトを明示）。fMP4 の「不完全な `moov`」を「サンプルテーブル無しの `moov`」に修正
  - C API 参照の 1 段落を削除し、ドキュメントを Rust API に閉じさせた
- 検証
  - `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` 警告なし
  - `cargo test --doc` pass

## 参考

- OBS Studio Hybrid MP4 の解説: <https://obsproject.com/blog/obs-studio-hybrid-mp4>
- hisui の Hybrid MP4 ライター実装: <https://github.com/shiguredo/hisui/blob/516339747ad8083b6ddb61e88546ce128cafe586/src/mp4/hybrid_writer.rs#L84>
- 既存 API の doc コメント: `Mp4FileMuxer::advance_position()`（`src/mux_mp4_file.rs`）および `mp4_file_muxer_advance_position()`（`crates/c-api/src/mux.rs` / `crates/c-api/include/mp4.h`）
- 既存の補足ドキュメント参考例: `docs/subtitle.md`
