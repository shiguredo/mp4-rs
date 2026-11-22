# ディスクリプターのサイズの復号で `usize` に収まらない値を拒否する

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-descriptor-size-overflow
- Polished: 2026-11-22

## 目的

ディスクリプターのサイズを正確に復号する。宣言された値が実行ターゲットの `usize` に収まらない場合は、切り詰めずエラーにする。

## 現状

`src/descriptors.rs` の `decode_tag_and_size` は、サイズを復号するたびに `usize::checked_shl` で 7 ビット左へシフトする。[Rust の契約](https://doc.rust-lang.org/std/primitive.usize.html#method.checked_shl) では、これはシフト量が型のビット幅以上かどうかを検査するだけで、結果から上位ビットが失われても `None` にはならない。影響は 64 ビットに限らず、`usize` の幅に依存する。

この関数はタグ、宣言サイズ、ヘッダーの消費バイト数を返す。ペイロードは解析せず、既存の `tag_and_size` テストもヘッダー解析をペイロードの検証と切り離して確認している。

## 設計方針

- `decode_tag_and_size` の数値の組み立てをチェック付きの乗算と加算に置き換え、各桁の更新時点で `usize` に収まらなければ `ErrorKind::InvalidData` を返す。そこまでの更新が範囲内で、次の桁を読む際に入力が尽きた場合は、従来どおり `ErrorKind::InsufficientBuffer` を返す
- 宣言サイズとペイロードの残量の比較はこの関数に追加しない。`DecoderSpecificInfo::decode` の既存の境界検査は維持し、外側の記述子の宣言境界は 0128 で扱う。エンコードと、サイズ欄のバイト数に関する受理方針は変えない

## 完了条件

- 内部のサイズ更新を対象とする単体テストで、通常の値が正しく計算され、`usize` に収まらない更新が `ErrorKind::InvalidData` になることを確認する。テストを対象の `usize` 幅に依存させない
- 既存の `tag_and_size` テストで、タグ、サイズ、ヘッダーの消費バイト数が変わらない。既存の公開 API のラウンドトリップとエラーパス、4 バイトのサイズ欄を含む既存の AAC ファイルのラウンドトリップを含め、`cargo test --workspace` が成功する
