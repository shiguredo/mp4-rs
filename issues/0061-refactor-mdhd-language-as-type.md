# `MdhdBox::language` の型を `LanguageCode` に置き換える

- Priority: Low
- Created: 2026-08-03
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/refactor-mdhd-language-as-type
- Polished: YYYY-MM-DD

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

### 影響範囲

`MdhdBox::LANGUAGE_UNDEFINED` の参照:

- `src/boxes_moov_tree.rs` の `MdhdBox::decode` 内の初期値
- `examples/transcode_wasm/src/mp4.rs` の `build_mdia_box`
- `pbt/tests/prop_boxes_moov_tree.rs` の 3 箇所
- `pbt/tests/prop_container_boxes.rs` の 1 箇所
- `pbt/tests/prop_boxes.rs` の 1 箇所

`MdhdBox::language` を直接構築しているフィールド指定リテラル:

- 上記の各テストと example
- `src/mux_mp4_file.rs` の `Mp4FileMuxer::build_mdia_box`
- `src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::build_init_trak`
- `src/mux_mp4_file.rs` の `test_default_track_metadata_bytes`

## 設計方針

### 1. 型置換

```rust
pub struct MdhdBox {
    ...
    pub language: LanguageCode,
}
```

`LanguageCode` の内部は `[u8; 3]` なので runtime 表現は変わらない。構築時（`LanguageCode::new` / `LanguageCode::from_ascii`）で `0x60..=0x7F` を validate 済みなので、不正な `MdhdBox` を作る経路は型で塞がれる。

### 2. `LanguageCode::as_bytes` を `const fn` 化

`MdhdBox::encode` で `self.language.as_bytes()` を使うため、および今後 const 文脈で使えるように `const fn` にする。

```rust
pub const fn as_bytes(self) -> [u8; 3] {
    self.0
}
```

`self` は `Copy` なので変更コストは小さい。

### 3. `MdhdBox::LANGUAGE_UNDEFINED` の扱い

削除して `LanguageCode::UNDEFINED` に集約する。理由:

- 型 refactor で「同値の 2 定数が並立している」問題を解消するのがこの issue の主目的の 1 つなので、残す意義が薄い
- 型が `LanguageCode` になるため、既存の `[u8; 3]` 型の const を残すと型ミスマッチで直接使えなくなる
- 参照箇所を機械的に `LanguageCode::UNDEFINED` へ置き換えれば済む

### 4. Encode の遅延バリデーション削除

`MdhdBox::encode` の 5 ビット範囲チェック（`checked_sub(0x60)` と `> 31` 判定）は、`LanguageCode` 構築時に保証されているため冗長。削除して encode 実装を単純化する。

削除後の encode（イメージ）:

```rust
let mut language: u16 = 0;
for l in &self.language.as_bytes() {
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
    .expect("5 ビットマスク後の値は必ず 0x60..=0x7F に収まる");
```

`expect` は数学的に unreachable だが、将来 decode のマスク処理を誰かが外したら気付く防御になる。

### 6. 参照箇所の一括書き換え

`MdhdBox::LANGUAGE_UNDEFINED` を参照している全箇所と、`MdhdBox::language` を生バイト列でフィールド指定していた全箇所を書き換える。

- `examples/transcode_wasm/src/mp4.rs`: `language: LanguageCode::UNDEFINED`
- `pbt/tests/prop_boxes_moov_tree.rs` / `prop_container_boxes.rs` / `prop_boxes.rs`: 同上
- `src/mux_mp4_file.rs` / `src/mux_fmp4_segment.rs`: 既に `metadata.language.as_bytes()` で `[u8; 3]` に変換して代入している箇所を、`metadata.language` をそのまま代入する形に変更
- `src/mux_mp4_file.rs` の `test_default_track_metadata_bytes`: `metadata.language.as_bytes() == MdhdBox::LANGUAGE_UNDEFINED` を `metadata.language == LanguageCode::UNDEFINED` に変更（同値担保の主張自体は残す）

## 完了条件

- `MdhdBox::language` の型が `LanguageCode` になっていること
- `MdhdBox::LANGUAGE_UNDEFINED` が削除され、全参照箇所が `LanguageCode::UNDEFINED` に置き換わっていること
- `MdhdBox::encode` から 5 ビット範囲チェックが削除され、encode 実装が単純化されていること
- `MdhdBox::decode` が `LanguageCode::new(...).expect(...)` で構築するようになっていること
- `LanguageCode::as_bytes` が `const fn` になっていること
- 参照している全箇所（`examples/`、`pbt/tests/`、`src/mux_mp4_file.rs`、`src/mux_fmp4_segment.rs`、`tests/test_basic_types.rs`）が新型に追従していること
- `CHANGES.md` に破壊的変更として `[CHANGE]` エントリが記載されていること
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が warning なしで通ること
