# boxes_moov_tree.rs の MdhdBox::encode で言語コードの 5 ビット上限検証が欠落しておりビットフィールドが破壊され得る

- Priority: Medium
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/fix-mdhd-language-code-5bit-validation
- Polished: 2026-07-20

## 目的

`MdhdBox::encode()` で ISO-639-2/T 言語コードの各文字から `0x60` を引いた値が 5 ビット（0〜31）に収まることを検証していない。`0x80` 以上のバイトの場合、`code` が 32 以上になり、`language = (language << 5) | code as u16` で隣接ビットフィールドを破壊し得る。デコード側は `& 0b11111` でマスクするため、ラウンドトリップでデータが変化する。

ISO/IEC 14496-12 の MediaHeaderBox における `language` フィールドは `unsigned int(5)[3]` と定義されており、各文字は `char - 0x60` で 5 ビット値にパックされる。5 ビットに収まらない値は仕様上不正なビットパターンを生成する。

## 優先度根拠

エンコード結果が仕様上不正なビットパターンになり得る。現実の MP4 ファイルで `0x80` 以上の言語コードバイトが使われることはほぼないが、`language` フィールドは `pub [u8; 3]` であり、ライブラリ利用者が直接任意の値を設定できる。内部使用箇所（`mux_mp4_file.rs:904,940`、`mux_fmp4_segment.rs:696`、`examples/transcode_wasm/src/mp4.rs:138`）は全て `MdhdBox::LANGUAGE_UNDEFINED`（`*b"und"` = `[0x75, 0x6E, 0x64]`）で安全だが、外部利用者が不正値を設定するリスクは排除できない。

## 現状

`src/boxes_moov_tree.rs:810-816`:

```rust
let Some(code) = l.checked_sub(0x60) else {
    return Err(Error::invalid_input(format!(
        "Invalid language code: {:?}",
        self.language
    )));
};
language = (language << 5) | code as u16;
```

`checked_sub(0x60)` は `0x60` 未満を拒否するが、`code > 31`（つまり `l > 0x7F`）のチェックがない。

### ビット破壊の具体的な挙動

溢れる文字位置によって挙動が異なる。なお、溢れビット（code=32 の bit 5）は隣接文字フィールドの LSB に OR されるため、隣接文字の code が奇数の場合 OR が冪等になり破壊が観測できない。以下の例は隣接文字の code を偶数にして破壊が観測されるケースを示す:

- **1 文字目が溢れる場合**: 溢れたビットは 2 回の `<< 5` で bit 15 に到達する。decode 側の `& 0b11111` マスク（`boxes_moov_tree.rs:860-862`）で bit 15 は落ちるため、1 文字目だけが変化する（例: `[0x80, 0x61, 0x61]` → decode 後 `[0x60, 0x61, 0x61]`）
- **2 文字目が溢れる場合**: 溢れたビットは 1 回の `<< 5` で bit 10 に到達し、1 文字目のフィールドを破壊し得る（例: `[0x62, 0x80, 0x61]` → decode 後 `[0x63, 0x60, 0x61]`、1 文字目 code 2→3）
- **3 文字目が溢れる場合**: 溢れたビットは bit 5 に留まり、2 文字目のフィールドを破壊し得る（例: `[0x61, 0x62, 0x80]` → decode 後 `[0x61, 0x63, 0x60]`、2 文字目 code 2→3）

### 境界値

| バイト値 | `checked_sub(0x60)` 結果 | 期待動作 |
|---------|------------------------|---------|
| `0x5F` | `None`（アンダーフロー） | エラー（既存） |
| `0x60` | `Some(0)` | 成功（code = 0、5 ビット的に有効） |
| `0x61` (`'a'`) | `Some(1)` | 成功 |
| `0x7A` (`'z'`) | `Some(26)` | 成功 |
| `0x7F` | `Some(31)` | 成功（有効上限） |
| `0x80` | `Some(32)` | **エラー（今回の修正対象）** |
| `0xFF` | `Some(159)` | **エラー（今回の修正対象）** |

本 issue は 5 ビット溢出のみを扱い、ISO 639-2/T の文字集合検証（`a-z` のみ許可等）は行わない。`0x60`（code = 0）や `0x7B`-`0x7F`（code 27-31）は 5 ビット的には有効なため受理し続ける。

## 設計方針

encode 時のみ検証を追加し、decode 側の `& 0b11111` マスク（`boxes_moov_tree.rs:860-862`）は維持する。decode は外部入力を受け入れるため、マスクによる防御的読み取りを維持する方針。encode 側で厳密に検証することで、ライブラリが生成する MP4 ファイルの仕様適合性を担保する。

エラーメッセージは既存の `checked_sub` 失敗時と同じ `"Invalid language code: {:?}"` を使用する。

`pub language: [u8; 3]` の doc comment に「encode 時に各バイトが `0x60..=0x7F` の範囲外の場合エラーを返す」旨を追記する。

## 完了条件

- `code > 31` の場合にエラーが返ること
- 上限エラーパステストが追加されること（下限テスト `mdhd_box_invalid_language_code_low/middle/last` と対称になるよう、1 文字目・2 文字目・3 文字目それぞれの上限超えを `0x80` で検証する）
- 境界値テストが追加されること（全 3 位置で `0x7F` は成功・`0x80` はエラー、`0x60`（code = 0）は成功・`0x5F` はエラー）
- `pub language` の doc comment に有効範囲の制約が追記されること
- 既存のテストが通ること
- `cargo clippy` が通ること

## 解決方法

`checked_sub` の後に `if code > 31 { return Err(...) }` を追加する。

テストは `pbt/tests/prop_error_paths.rs` に追加する（既存の下限エラーパステスト `mdhd_box_invalid_language_code_low/middle/last` と同じファイル・同じパターン）。issue 0003（`prop_error_paths.rs` の分割）が先に実施された場合は、分割後の対応ファイルに追加する。

## 後方互換

有効な ISO-639-2/T 言語コード（小文字 ASCII `a-z`、つまり `0x61-0x7A`）および `LANGUAGE_UNDEFINED`（`*b"und"`）の動作は不変。影響を受けるのは `0x80` 以上のバイトを含む不正な入力のみ（従来は silently 壊れたビット列を出力していた）。不正入力の拒否はバグ修正であり、破壊的変更ではない。

## CHANGES.md

`[FIX]` で記載する。CHANGES.md の既存エントリ（`data_size` の u32 トランケーション、映像解像度の i16 符号反転）と同種の「暗黙的なビット切り捨て → 明示的エラー」パターン。
