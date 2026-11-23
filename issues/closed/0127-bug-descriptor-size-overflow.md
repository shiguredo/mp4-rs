# ディスクリプターのサイズの復号で `usize` に収まらない値を拒否する

- Created: 2026-11-22
- Completed: 2026-11-23
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

## 解決方法

- `src/descriptors.rs` の `decode_tag_and_size` で行っていたサイズの組み立てを、新しく追加した private 関数 `shift_in_size_bits` に切り出した。`usize::checked_shl` をやめ、`checked_mul(1 << 7)` と `checked_add` で 7 ビットずつ組み立てる。結果が `usize` に収まらない場合は、その桁の時点で `ErrorKind::InvalidData`（理由 `Descriptor size exceeds usize::MAX`）を返し、次のバイトは読まない。値が `usize` に収まる範囲で入力が尽きた場合は、従来どおり `ErrorKind::InsufficientBuffer` を返す
- 根拠コメントは ISO/IEC 14496-34:2025 の 7.5 Expandable classes を引いた。ISO/IEC 14496-1 は 2026 年の第 5 版で SDL の定義を 14496-34 に移しており、2010 年版では 8 章 Syntactic Description Language で定義されていたことも併記した
- テスト
  - `src/descriptors.rs` の `mod tests`
    - `shift_in_size_bits_computes_size`: 通常の値の更新を確認する
    - `shift_in_size_bits_overflow_boundary`: 結果が `usize::MAX` ちょうどの更新は成功し、オーバーフローする最小の値と最大の値からの更新は `InvalidData` になることを確認する。どちらも `usize::MAX` から値を求めるため、`usize` の幅に依存しない
    - `decoder_specific_info_payload_exceeds_boundary_with_usize_max_size`: 宣言サイズが `usize::MAX` でも、`DecoderSpecificInfo` の境界検査がオーバーフローしないことを確認する。修正により、既存の回帰テストの入力はサイズ欄の時点で止まり、境界検査まで届かなくなったため追加した
    - 既存の回帰テスト `decoder_specific_info_size_overflow` は、`is_err()` ではなくエラー種別まで確認するようにした
  - `tests/test_descriptors.rs`
    - `decoder_specific_info_size_exceeds_usize_max`: 2 の 64 乗 + 2 を宣言した入力が、修正前のように 2 バイトのペイロードとして受理されず、`InvalidData` になることを確認する
    - `decoder_specific_info_size_field_truncated`: 値が `usize` に収まらなくなった後で入力が尽きた場合は `InvalidData`、値が収まる範囲で入力が尽きた場合は `InsufficientBuffer` になることを確認する
    - 既存テストのコメント `size = 129` を、実際の値 `16257` に直した
  - `pbt/tests/prop_descriptors.rs`: `DecoderSpecificInfo` のペイロード長を `0..300` に広げ、サイズ欄が 2 バイトになる正常系を生成するようにした。DSI の roundtrip は `arb_decoder_specific_info` を使う形にまとめ、使われなくなった `sample_vec` を削除した
- 既存の `tag_and_size` テストは変更していない。4 バイトのサイズ欄を含む AAC ファイルのラウンドトリップを含め、`cargo test --workspace` が成功することを確認した。wasm32-wasip1 を対象にしたビルドでも、追加したテストが通ることを確認した
- `CHANGES.md` の `## develop` に [FIX] を追記した
