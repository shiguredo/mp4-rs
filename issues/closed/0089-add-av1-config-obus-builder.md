# AV1 configOBUs から sample entry を構築する API を追加する

- Created: 2026-08-27
- Completed: 2026-08-27
- Branch: feature/add-av1-config-obus-builder
- Polished: {YYYY-MM-DD}

## 目的

エンコーダーの codec private 情報として得られる `configOBUs` だけから、内容と `av1C` の構造化フィールドが一致する `Av01Box` を構築できるようにする。

利用側が同じバイト列に対して OBU 列挙、Sequence Header 抽出、Sequence Header 解析、既存 builder 呼び出しを毎回組み立てる必要をなくす。

## 現状

- `src/bitstream/av1.rs` は `parse_obus`、`parse_sequence_header`、`build_av01_box` を公開している
- `build_av01_box` は呼び出し側が渡した `Av1SequenceHeader` と `config_obus` 内の Sequence Header が一致することを検証する
- AV1 binding は `configOBUs` の Sequence Header を省略可能としているため、既存 `build_av01_box` が `Av1SequenceHeader` を別引数で受ける設計は維持する必要がある
- 一方、encoder の extradata に Sequence Header OBU が含まれる典型経路では、呼び出し側が `parse_obus` と `parse_sequence_header` を先に呼び、同じ `config_obus` を `build_av01_box` に再度渡す定型処理が必要になる
- Hisui の `src/video/av1.rs` は `config_obus` を検証せずコピーし、`Av1cBox` の profile / level / bit depth / chroma 欄を固定値で埋めているため、実ストリームと不一致になりうる

参照仕様は AV1 Codec ISO Media File Format Binding v1.3.0 とする。

<https://aomediacodec.github.io/av1-isobmff/v1.3.0.html>

## 設計方針

`src/bitstream/av1.rs` に次の convenience API を追加する。

```rust
pub fn build_av01_box_from_config_obus(
    config_obus: &[u8],
    config: &Av1SampleEntryConfig,
) -> Result<Av01Box>;
```

処理は次の順に行う。

1. `parse_obus(config_obus, Av1ObuParseContext::ConfigObus)` で列挙する
2. 先頭 OBU が Sequence Header であり、Sequence Header がちょうど 1 個だけあることを確認する
3. Sequence Header の payload を `parse_sequence_header` で解析する
4. 解析結果、元の `config_obus`、`config` を既存 `build_av01_box` に渡す

この API は Sequence Header が `configOBUs` に含まれる入力専用とする。空入力、Sequence Header がない入力、先頭以外にある入力、複数ある入力は `ErrorKind::InvalidInput` とする。

Sequence Header を別経路から得る場合や、binding が許容する Sequence Header なしの `configOBUs` を扱う場合は、既存 `build_av01_box` を引き続き使用する。既存 API の受理条件は狭めない。

幅・高さの呼び出し側上書き、sample 文脈の OBU 解析、RAP 判定は追加しない。

### テスト

- `tests/test_bitstream_av1.rs` に実 fixture `tests/testdata/black-av1-config-obus.bin` から `Av01Box` を構築できることを追加する
- 構築結果の profile / level / bit depth / chroma / width / height / `config_obus` が Sequence Header と一致することを確認する
- 空入力、Sequence Header なし、先頭以外、複数 Sequence Header を拒否することを確認する
- OBU / Sequence Header パーサー自体の PBT は既存 `pbt/tests/prop_bitstream_av1.rs` で担保されるため、この薄い合成 API 専用の PBT は追加しない
- mock / stub、外部 command、ネットワークは使用しない

### 変更履歴

`CHANGES.md` の develop にある AV1 bitstream の `[ADD]` エントリーへ、`configOBUs` から `Av01Box` を構築する API を追記する。

## 完了条件

- `build_av01_box_from_config_obus` が公開される
- `configOBUs` 内の Sequence Header から `Av01Box` の全導出フィールドが構築される
- Sequence Header がない、先頭でない、または複数ある入力を拒否する
- 既存 `build_av01_box` の受理範囲が変わらない
- 実 fixture を使う決定的テストが追加される
- `CHANGES.md` が更新される
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る

## 解決方法

- `src/bitstream/av1.rs` に `build_av01_box_from_config_obus` を追加した
- OBU 列挙後に、空入力・先頭が Sequence Header でない・複数ある入力を `ErrorKind::InvalidInput` で拒否し、先頭 Sequence Header の payload を解析して既存 `build_av01_box` に委譲する
- `tests/test_bitstream_av1.rs` に実 fixture `black-av1-config-obus.bin` からの構築と、導出フィールドが Sequence Header と一致することのテストを追加した
- 拒否系 (空入力、Sequence Header なし、先頭以外、複数) のテストも追加した
- `CHANGES.md` に `[ADD]` エントリーを追記した
