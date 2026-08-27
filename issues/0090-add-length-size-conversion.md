# lengthSizeMinusOne から LengthSize へ変換する API を追加する

- Created: 2026-08-27
- Completed: {YYYY-MM-DD}
- Branch: feature/add-length-size-conversion
- Polished: {YYYY-MM-DD}

## 目的

`avcC` / `hvcC` の `lengthSizeMinusOne` から、NAL ユニット処理 API が受け取る `LengthSize` へ安全に変換できるようにする。

利用側が同じ 0 / 1 / 3 の対応と予約値 2 の拒否を繰り返し実装する必要をなくし、ボックスから読み取った値を length-prefixed 変換へ渡す境界を共通化する。

## 現状

- `src/bitstream/nal.rs` の `LengthSize` は `OneByte` / `TwoBytes` / `FourBytes` を持つ
- `LengthSize::length_size_minus_one` は `LengthSize` から 0 / 1 / 3 への変換を提供する
- 逆方向の変換 API はなく、`AvccBox::length_size_minus_one` / `HvccBox::length_size_minus_one` を読む利用側が match を個別に書く必要がある
- closed issue 0062 では逆変換を呼び出し側の責任としたが、Hisui の HLS と decoder の移行検討により、同じ変換点が複数存在する具体的な再利用需要が確認できた
- ISO/IEC 14496-15 の `lengthSizeMinusOne` は 0 / 1 / 3 が正当で、2 は予約値である。3 バイト長のサポートを追加する必要はない

## 設計方針

`src/bitstream/nal.rs` の `LengthSize` に次の関連関数を追加する。

```rust
pub fn from_length_size_minus_one(value: u8) -> Result<Self>;
```

対応は次のとおり。

- 0 → `LengthSize::OneByte`
- 1 → `LengthSize::TwoBytes`
- 3 → `LengthSize::FourBytes`
- 2 → reserved として `ErrorKind::InvalidInput`
- 4 以上 → 2 ビット欄の範囲外として `ErrorKind::InvalidInput`

`TryFrom<u8>` は、入力値がバイト幅そのものか `lengthSizeMinusOne` かを型名だけでは判別できないため実装しない。既存 `length_size_minus_one(self)` と対称な、意味を明記した名前を使う。

`LengthSize` は `bitstream::h264` と `bitstream::h265` から同じ型が公開されているため、片方だけに別実装を追加しない。3 バイト length の variant も追加しない。

### テスト

- `tests/test_bitstream_h264.rs` に 0 / 1 / 3 の変換結果と 2 / 4 以上のエラーを追加する
- 3 variant だけの有限な対応表なので PBT は追加せず、決定的テストで全入力区分を確認する
- mock / stub、外部 command、ネットワークは使用しない

### 変更履歴

`CHANGES.md` の develop にある H.264 / H.265 bitstream の説明へ、`lengthSizeMinusOne` から `LengthSize` への検証付き変換を追記する。

## 完了条件

- `LengthSize::from_length_size_minus_one` が公開される
- 0 / 1 / 3 が対応する `LengthSize` に変換される
- 予約値 2 と範囲外の値が `ErrorKind::InvalidInput` になる
- 3 バイト length は引き続き型で表現できない
- 決定的テストが追加される
- `CHANGES.md` が更新される
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る
