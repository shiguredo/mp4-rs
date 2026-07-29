# boxes_sample_entry.rs の Hev1Box と Hvc1Box が完全に同一の構造・ロジックで重複している

- Priority: Medium
- Created: 2026-07-20
- Completed: 2026-07-29
- Model: qwen3.8-max-preview
- Branch: feature/refactor-codec-sample-entry-dedup
- Polished: 2026-07-29

## 目的

`Hev1Box` と `Hvc1Box` は、フィールド構成・`Encode`・`Decode`・`BaseBox` の実装がボックス種別定数（`b"hev1"` vs `b"hvc1"`）と `check_mandatory_box` に渡すエラーメッセージ用の文字列を除いて完全に同一である。HEVC 対で約 150 行のコピペ重複があり、将来的に HEVC 関連のボックスに修正が必要になった場合、両方を同時に修正する必要があり修正漏れのリスクがある。

`hev1` と `hvc1` は ISO/IEC 14496-15 上、パラメータセット（VPS / SPS / PPS）が in-band に現れるか out-of-band (`hvcC`) に現れるかの違いだけで、内部構造は仕様上完全に同一である。この「仕様レベルで semantically same」という事実がコードに反映されていない。

C API 層（`crates/c-api/src/boxes.rs`）にも `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` の重複が伝播している。

### 対象外: Vp08Box / Vp09Box

`Vp08Box` と `Vp09Box` も MP4 コンテナ内の box 形状は同一だが、VP8 と VP9 は独立したコーデック仕様であり、共通の box 形状は「両方とも `vpcC` を使う」という偶然の一致に過ぎない。共通化すると、将来 VP9 に新フィールドが追加されたり VP8 側の挙動だけ変えたくなった場合に**偽の結合**として邪魔になる。よって Vp08/Vp09 および C API 側の `Mp4SampleEntryVp08` / `Mp4SampleEntryVp09` は本 issue の対象外とし、独立実装のまま維持する。

## 優先度根拠

直ちにバグを引き起こすわけではないが、修正漏れリスクと可読性の観点から解消すべき技術的負債。`hev1` / `hvc1` は仕様上 semantically same であるため、共通化に仕様的な根拠がある（対して VP8/VP9 は共通化が正当化できないため対象外とした）。

## 現状

- `src/boxes_sample_entry.rs:583-665`（Hev1Box）と `667-749`（Hvc1Box）: `visual: VisualSampleEntryFields` + `hvcc_box: HvccBox` + `unknown_boxes: Vec<UnknownBox>` で同一。差分は `TYPE` 定数と `check_mandatory_box` に渡す親ボックス名の文字列（`"hev1"` / `"hvc1"`）のみ
- `crates/c-api/src/boxes.rs:918-1033`（Mp4SampleEntryHev1）と `1081-1190`（Mp4SampleEntryHvc1）: struct フィールドは完全一致、`to_sample_entry()` は最後に返す enum variant (`SampleEntry::Hev1` / `SampleEntry::Hvc1`) 以外は同一、`nalu_data_index()` は完全一致

## 設計方針

`Hev1Box` / `Hvc1Box` の struct 定義・`TYPE` 定数・trait impl の外枠は不変のまま、Encode / Decode の重複ロジックを **共通ヘルパー関数** に抽出する。マクロもトレイトも新設しない（AGENTS.md / shiguredo-rust スキルの「マクロを作らないこと」「トレイトを作らないこと」規約に従う）。

「共通の内部構造体を抽出して薄いラッパーにする」という代替案は、公開 struct の `pub` フィールドが変わり後方互換性を壊すため採用しない。「トレイト（例: `trait HevcSampleEntry`）でフィールドアクセスを抽象化する」代替案も上記規約に反するため採用しない。

### コアライブラリ側の抽出候補

- `encode_hevc_sample_entry(buf, box_type, visual, hvcc_box, unknown_boxes) -> Result<usize>`
  - box ヘッダ書き込みから `finalize_box_size` までを含む「box 全体のエンコード」を担う
- `decode_hevc_sample_entry(buf, expected_type, parent_name) -> Result<(VisualSampleEntryFields, HvccBox, Vec<UnknownBox>, usize)>`
  - `with_box_type` / `BoxHeader::decode_header_and_payload` / `header.box_type.expect` を含む「box 全体のデコード」を担う
  - `check_mandatory_box` に渡す親ボックス名（`"hev1"` / `"hvc1"`）は呼び出し側から `parent_name: &str` として明示的に受け取る。`BoxType` は `as_bytes() -> &[u8]` しか提供しておらず、`&str` への導出には `core::str::from_utf8` の失敗パス処理や `unsafe` の追加、あるいは `BoxType` 側 API 追加が必要になり、いずれも本 issue のスコープを超えるため、引数で受ける方針とする
- `BaseBox::children()` の実装は 5 行程度で共通化の効果が薄いためインラインのまま残す

`Hev1Box` / `Hvc1Box` の `Encode::encode` / `Decode::decode` はそれぞれヘルパー関数への薄い委譲になる。

### C API 層の抽出候補

`Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` は `#[repr(C)]` の ABI であり、struct 定義自体は不変。両者は 23 個の `pub` フィールドがすべて完全同一のため、次の 2 段構えでヘルパーへ渡す。

- **中間構造体**: c-api 内部（非 `#[repr(C)]`・非 `pub`）に `HevcSampleEntryRaw` 相当の中間 struct を導入する。フィールド構成は `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` と同型（数値フィールド 18 個 + `nalu_array_count: u32` + 生ポインタ 4 本 = 23 フィールド）を **生の型のまま** 保持する。生ポインタは C API 呼び出し側のメモリを指しており、有効期間は Rust の借用ではなく C 側の契約（`to_sample_entry` の呼び出しスコープ中に生存）で決まるため、ライフタイムパラメータは付けない（付けると `Copy` スカラーと生ポインタだけの構成では `unused lifetime parameter` になり、回避のための `PhantomData` は装飾的でむしろ誤解を招く）。`Uint<u8, 2, 6>` 等のラップは中間構造体では行わず、後述の `build_hvcc_box` 側で `Uint::new(...)` にラップして責務を 1 箇所に集約する。`Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` の各 `impl` に `fn to_raw(&self) -> HevcSampleEntryRaw` を持たせて 23 フィールドの写し替えを 1 箇所ずつ書く（この写し替えは完全同一だが、`&self` の型が異なるため実装は 2 箇所残る。フィールドが多いためこれ以上の共通化は行わない）
- **NALU 配列構築ヘルパー**: `fn build_hvcc_nalu_arrays(raw: &HevcSampleEntryRaw) -> Result<Vec<HvccNalUintArray>, Mp4Error>` として抽出する。関数自体は safe とし、生ポインタを触る箇所は関数内部の `unsafe` ブロックに閉じ、`SAFETY:` コメントで null 契約（`crates/c-api/src/boxes.rs` の `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` docstring の `# ポインタフィールドの null 契約` を参照）を文書化する。現状の `nalu_data_index` 相当のロジックはこのヘルパー内部にインライン展開し、別関数として残さない（両 `impl` から完全に消える）
- **HvccBox 構築ヘルパー**: `fn build_hvcc_box(raw: &HevcSampleEntryRaw, nalu_arrays: Vec<HvccNalUintArray>) -> HvccBox`
- **`to_sample_entry()`**: 既存の `self` 値渡しシグネチャは維持し、内部でまず `let raw = self.to_raw();` として `&raw` を各ヘルパーへ渡す。最後は `SampleEntry::Hev1` / `SampleEntry::Hvc1` variant を生成するだけの薄い委譲になる

### 後方互換性への影響

公開 API（`Hev1Box` / `Hvc1Box` の struct フィールド、`Encode` / `Decode` / `BaseBox` の trait impl）は不変。C API 側の `#[repr(C)]` struct とその公開関数シグネチャも不変。ヘルパー関数の抽出は private な実装詳細であり、外部からの型参照・フィールドアクセスに影響しない。

## 完了条件

- `Hev1Box` / `Hvc1Box` の Encode / Decode の重複コードが解消されること
- C API 側の `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` の `to_sample_entry()` および `nalu_data_index()` の重複コードが解消されること
- Vp08Box / Vp09Box および C API 側の `Mp4SampleEntryVp08` / `Mp4SampleEntryVp09` は変更しないこと
- 公開 API（Rust struct フィールド、trait impl、C ABI）の後方互換性が保たれること
- 既存のテストが通ること
- 既存の PBT（ラウンドトリップテスト）が通ること
- `cargo clippy` が通ること

## 解決方法

`feature/refactor-codec-sample-entry-dedup` ブランチで対応した。

### 実施内容

- `src/boxes_sample_entry.rs` に `encode_hevc_sample_entry` / `decode_hevc_sample_entry` を追加し、`Hev1Box` / `Hvc1Box` の Encode / Decode を薄い委譲にした。`BaseBox::children()` はインラインのまま残した
- `crates/c-api/src/boxes.rs` に非公開の中間構造体 `HevcSampleEntryRaw` と `build_hvcc_nalu_arrays` / `build_hvcc_box` を追加し、`Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` の `to_sample_entry` を委譲にした。旧 `nalu_data_index` はヘルパー内にインライン展開して削除した
- `CHANGES.md` の `### misc` に `[UPDATE]` を追記した
- Vp08Box / Vp09Box および C API 側の `Mp4SampleEntryVp08` / `Mp4SampleEntryVp09` は変更していない
- 公開 API（Rust の構造体フィールド・trait impl・C ABI）は変更していない

### 計画から外れた点

- issue 文面では `to_raw(&self)` としていたが、`Copy` 型への `to_*` は clippy の `wrong_self_convention` に抵触するため `to_raw(self)` にした

### 検証

- `cargo fmt` / `cargo clippy -D warnings` / workspace test / c-api test / PBT が通ることを確認した
- `/review-diff-code` で致命的・重要が 0 件であることを確認した

## CHANGES.md

`[UPDATE]` で記載する（内部実装のリファクタリングであり、公開 API の変更はないため）。
