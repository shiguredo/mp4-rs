# `MdhdBox::language` の型を `LanguageCode` に置き換える

- Priority: Low
- Created: 2026-08-03
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/refactor-mdhd-language-as-type
- Polished: 2026-08-03

## 目的

`MdhdBox::language` の型を `[u8; 3]` から `LanguageCode` に置き換え、値の妥当性検証を型境界で強制する。

0053 で `LanguageCode` 型を新設した際、muxer API 経由で使う場合のみ typed だったが、`MdhdBox` を直接扱う経路（decode / 直接構築）では生 `[u8; 3]` のままで、`LanguageCode` があるにもかかわらず使っていない不整合が残っていた。これを解消する。

## 優先度根拠

Low。現状も `MdhdBox::encode` が `0x60..=0x7F` の範囲外を `Error::invalid_input` として返すため機能上の欠陥は無い。ただし encode 時までバリデーションが遅延する、`MdhdBox::LANGUAGE_UNDEFINED` と `LanguageCode::UNDEFINED` が独立した 2 定数として並立している、既存の型パターン（`Utf8String` / `Uint` / `FixedPointNumber` / `Mp4FileTime`）と整合しない、といった設計清書の余地がある。

## 現状

### 生バイト列で保持

`src/boxes_moov_tree.rs` の `MdhdBox`:

```rust
pub struct MdhdBox {
    ...
    pub language: [u8; 3],
}

impl MdhdBox {
    pub const LANGUAGE_UNDEFINED: [u8; 3] = *b"und";
}
```

`MdhdBox::encode` は各バイトが `0x60..=0x7F` の範囲外だと `Error::invalid_input` を返す遅延バリデーション。構築時のチェックは無い。

### 独立した 2 定数

- `MdhdBox::LANGUAGE_UNDEFINED: [u8; 3] = *b"und"`（`src/boxes_moov_tree.rs`）
- `LanguageCode::UNDEFINED: LanguageCode = Self(*b"und")`（`src/basic_types.rs`）

同値だが独立に定義されており、値の一致は 0053 で追加した `test_default_track_metadata_bytes`（`src/mux_mp4_file.rs` の `mod tests`）で runtime に固定しているだけ。片方だけ書き換わる余地がある。

`LanguageCode::UNDEFINED` の doc も `[`crate::boxes::MdhdBox::LANGUAGE_UNDEFINED`]` を参照している。

### 影響範囲

`MdhdBox::LANGUAGE_UNDEFINED` の参照（コード・doc コメント含む）:

- `src/boxes_moov_tree.rs` の `MdhdBox::decode` 内の初期値
- `src/basic_types.rs` の `LanguageCode::UNDEFINED` doc（intra-doc link）
- `src/mux_mp4_file.rs` の `test_default_track_metadata_bytes`（assert と doc）
- `examples/transcode_wasm/src/mp4.rs` の `build_mdia_box`
- `pbt/tests/prop_boxes_moov_tree.rs` の 3 箇所（有効値の構築）
- `pbt/tests/prop_container_boxes.rs` の 1 箇所
- `pbt/tests/prop_boxes.rs` の 1 箇所

`MdhdBox::language` を `[u8; 3]` として扱っている箇所（型置換で必ず追従が必要）:

- 上記の各参照箇所
- `src/mux_mp4_file.rs` の `Mp4FileMuxer::build_mdia_box`（`metadata.language.as_bytes()` で代入）
- `src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::build_init_trak`（同上）
- `pbt/tests/common.rs` の `assert_track_metadata`（`LanguageCode::new(mdhd.language)` で再構築）
- `pbt/tests/prop_boxes.rs` の `mdhd_box_v0_roundtrip` / `mdhd_box_v1_roundtrip` / `mdhd_box_language_boundary`（`[u8; 3]` Strategy・リテラル・比較）
- `pbt/tests/prop_container_boxes.rs` の `mdia_box_roundtrip`（同上）
- `pbt/tests/prop_boxes_moov_tree.rs` の `mod moov_tree_error_tests`（不正 `[u8; 3]` で encode エラーを検証する 7 テスト。型置換後は構築不能のため削除対象。方針は設計方針 7）

## 設計方針

### 1. 型置換

```rust
pub struct MdhdBox {
    ...
    pub language: LanguageCode,
}
```

`LanguageCode` の内部は `[u8; 3]` なので値としての runtime 表現は変わらない。構築時（`LanguageCode::new` / `LanguageCode::from_ascii`）で `0x60..=0x7F` を validate 済みなので、不正な `MdhdBox` を作る経路は型で塞がれる（タプルフィールドは private で、公開の構築経路は `new` / `from_ascii` / `UNDEFINED` / `Default` のみ）。

あわせて `MdhdBox::language` の doc を更新する。現状は「3 バイト配列で保持する」「encode 時に範囲外ならエラーを返す」と書いてあるが、型置換と設計方針 4 の後は事実でなくなる。検証は `LanguageCode` の構築時に移った旨へ書き換える（encode 実装内の「各バイトの値域は `MdhdBox::language` の doc を参照」コメントも整合させる）。

### 2. `LanguageCode::as_bytes` を `const fn` 化

`Mp4FileTime::as_secs` / `FullBoxFlags::get` など `basic_types.rs` の他の小さなアクセサが既に `const fn` であるのと揃える。

```rust
pub const fn as_bytes(self) -> [u8; 3] {
    self.0
}
```

`self` は `Copy` なので変更コストは小さい。`MdhdBox::encode` 自体は `const fn` ではないため、encode から呼ぶこと自体は `const` 化の必須理由にはならない。

### 3. `MdhdBox::LANGUAGE_UNDEFINED` の扱い

削除して `LanguageCode::UNDEFINED` に集約する。理由:

- 型 refactor で「同値の 2 定数が並立している」問題を解消するのがこの issue の主目的の 1 つなので、残す意義が薄い
- 型が `LanguageCode` になるため、既存の `[u8; 3]` 型の const を残すと型ミスマッチで直接使えなくなる
- コード上の参照箇所は機械的に `LanguageCode::UNDEFINED` へ置き換えれば済む

ただし doc コメント内の参照は機械置換では済まない:

- `src/basic_types.rs` の `LanguageCode::UNDEFINED` doc にある `[`crate::boxes::MdhdBox::LANGUAGE_UNDEFINED`]` への言及は削除する（自己言及になるため `LanguageCode::UNDEFINED` へ置き換えない。`*b"und"` である旨の記述は残す）
- 削除しないと `RUSTDOCFLAGS="-D warnings" cargo doc` が `broken_intra_doc_links` で落ちる

### 4. Encode の遅延バリデーション削除

`MdhdBox::encode` の 5 ビット範囲チェック（`checked_sub(0x60)` と `> 31` 判定）は、`LanguageCode` 構築時に保証されているため冗長。削除して encode 実装を単純化する。

削除後の encode（イメージ）:

```rust
let mut language: u16 = 0;
for l in self.language.as_bytes() {
    let code = l - 0x60;
    language = (language << 5) | code as u16;
}
```

### 5. Decode 側

`MdhdBox::decode` は 5 ビットマスク + `0x60` オフセットで各バイトを `0x60..=0x7F` に必ず収める処理。新しい実装:

```rust
let language_bytes = [
    ((language >> 10) & 0b11111) as u8 + 0x60,
    ((language >> 5) & 0b11111) as u8 + 0x60,
    (language & 0b11111) as u8 + 0x60,
];
this.language = LanguageCode::new(language_bytes)
    .expect("5-bit masked language bytes are always in 0x60..=0x7F");
```

`expect` は数学的に unreachable だが、将来 decode のマスク処理を誰かが外したら気付く防御になる。メッセージは `src/` の既存慣行に合わせ英語とする。

### 6. 参照箇所の一括書き換え

`MdhdBox::LANGUAGE_UNDEFINED` を参照している全箇所と、`MdhdBox::language` を `[u8; 3]` として扱っていた全箇所を書き換える。

- `examples/transcode_wasm/src/mp4.rs`: `language: LanguageCode::UNDEFINED`
- `pbt/tests/prop_boxes_moov_tree.rs` / `prop_container_boxes.rs` / `prop_boxes.rs` の `LANGUAGE_UNDEFINED` 参照: 同上
- `src/mux_mp4_file.rs` / `src/mux_fmp4_segment.rs`: `metadata.language.as_bytes()` で `[u8; 3]` に変換して代入している箇所を、`metadata.language` をそのまま代入する形に変更
- `src/mux_mp4_file.rs` の `test_default_track_metadata_bytes`: 2 定数の同値担保は定数集約により不要になるため削除する。代わりに `assert_eq!(metadata.language.as_bytes(), *b"und")` でデフォルト言語のバイト列を固定し、`hdlr.name` のバイト列固定は現状どおり残す。doc からも `MdhdBox::LANGUAGE_UNDEFINED` への言及を外す
- `pbt/tests/common.rs` の `assert_track_metadata`: `mdhd.language` が既に `LanguageCode` になるため、`LanguageCode::new(raw)` による再構築と「範囲外なら失敗」の防御 `prop_assert!` は不要。`prop_assert_eq!(expected.language, trak_box.mdia_box.mdhd_box.language)` に簡素化する
- `pbt/tests/prop_boxes.rs` / `prop_container_boxes.rs` の roundtrip / boundary: `[u8; 3]` Strategy・リテラル・`assert_eq!(decoded.language, ...)` を `LanguageCode` に合わせる。Strategy は各ファイル内で `prop::array::uniform3(0x61u8..=0x7Au8).prop_map(|b| LanguageCode::new(b).expect(...))` 相当を挟む（`pbt/tests/common.rs` の `arb_language_code` は流用しない。これらのファイルが `mod common;` を追加して `arb_language_code` だけを使うと、同モジュール内の未使用 `pub fn` が `dead_code` になり、0053 で `#![allow(dead_code)]` を外した方針と衝突する）

### 7. 0029 由来の encode エラーパステストの削除

`pbt/tests/prop_boxes_moov_tree.rs` の `mod moov_tree_error_tests`（`mdhd_box_invalid_language_code_*` 6 本と `mdhd_box_language_code_5bit_boundaries` 1 本）は、不正な `[u8; 3]` を `MdhdBox::language` に直接入れて encode がエラーになることを検証している。0029 で追加された回帰テストだが、型置換後は不正値の `LanguageCode` 自体が構築不能なため **書き換えでは救済できず、モジュールごと削除する**。

同等の境界・範囲外拒否は既に `tests/test_basic_types.rs` の `LanguageCode::new` / `from_ascii` テスト（`new_accepts_boundary_bytes` / `new_rejects_out_of_range_bytes` 等）が担っている。バリデーションの責務が encode から型構築へ移ることに対応した削除であり、回帰カバレッジの喪失ではない。

## 完了条件

- `MdhdBox::language` の型が `LanguageCode` になっていること
- `MdhdBox::LANGUAGE_UNDEFINED` が削除され、コード・doc を含む全参照が解消されていること（コード参照は `LanguageCode::UNDEFINED` へ、`LanguageCode::UNDEFINED` doc の旧リンクは削除）
- `MdhdBox::language` の doc が型置換後の事実（検証は `LanguageCode` 構築時）と一致していること
- `MdhdBox::encode` から 5 ビット範囲チェックが削除され、encode 実装が単純化されていること
- `MdhdBox::decode` が `LanguageCode::new(...).expect(...)`（英語メッセージ）で構築するようになっていること
- `LanguageCode::as_bytes` が `const fn` になっていること
- `pbt/tests/prop_boxes_moov_tree.rs` の `mod moov_tree_error_tests` が削除されていること
- 参照している全箇所（`examples/transcode_wasm/`、`pbt/tests/`（`common.rs` / `prop_boxes.rs` / `prop_container_boxes.rs` / `prop_boxes_moov_tree.rs` を含む）、`src/mux_mp4_file.rs`、`src/mux_fmp4_segment.rs`、`src/basic_types.rs`、`src/boxes_moov_tree.rs`）が新型に追従していること
- `CHANGES.md` に破壊的変更として `[CHANGE]` エントリが記載されていること（公開フィールドの型変更と `LANGUAGE_UNDEFINED` 削除。主目的は設計清書のため Branch prefix は `feature/refactor-` のままとする。0053 が機能追加に伴う破壊的 API 変更を `feature/add-` + `[CHANGE]` で扱った先例に倣う）
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が warning なしで通ること
