# `EsDescriptor` / `DecoderConfigDescriptor` が宣言された size を境界として使わない

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-descriptor-size-boundary
- Polished: {YYYY-MM-DD}

## 目的

ディスクリプターの decode で、親が宣言した size をペイロードの境界として使い、宣言より後ろのバイトを子ディスクリプターとして読まないようにする。

## 現状

`src/descriptors.rs` の `EsDescriptor::decode` と `DecoderConfigDescriptor::decode` は、`decode_tag_and_size` が返した size を `_size` として捨て、入れ子の `decode_at` をバッファ全体に対して行う。URL 文字列の長さ検査も、親の size ではなくバッファの残量と比較している。

`DecoderSpecificInfo::decode` だけは自身の size を境界に使うが、その外側のディスクリプターは size を境界にしない。

再現:

- 最小の `EsDescriptor` を encode すると 23 バイト、size フィールドは 21 になる
- size フィールドだけを 0 に書き換えて `EsDescriptor::decode` に渡す
- `Ok` で `es_id=1`、`object_type_indication=0x40`、消費 23 バイトになる

確認結果:

```text
encoded len=23, size byte=21
decoded ok: es_id=1, oti=64, consumed=23
```

size 0 のディスクリプターはペイロードを持たないはずだが、後続のバイトを子ディスクリプターとして読んで成功している。破損した `esds` が別の設定として静かに受理される。

## 設計方針

- `decode_tag_and_size` の戻り値から size を受け取り、そのディスクリプターのペイロードを `buf[offset..offset + size]` で切り出す
- 入れ子のディスクリプターは、切り出したペイロードに対してだけ decode する
- 子ディスクリプターの decode 後に、消費バイト数が親の size と一致することを検査する
- URL 文字列の長さ検査も、親の size で切り出したペイロードに対して行う
- 既存の正常な `EsDescriptor` / `DecoderConfigDescriptor` の roundtrip は変えない

## 完了条件

- 上記の size を 0 に改変した `EsDescriptor` が `InvalidData` になる
- 子ディスクリプターが親の size を超える入力が `InvalidData` になる
- 正常な `EsDescriptor` / `DecoderConfigDescriptor` の roundtrip テストが引き続き成功する
- `tests/test_descriptors.rs` に size 改変のテストが追加されている
- `cargo test --workspace` が成功する
