# `TfdtBox` / `TfraBox` が version 2 を受理し、version 2 の full box を書き出す

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-tfdt-tfra-invalid-version
- Polished: {YYYY-MM-DD}

## 目的

`tfdt` と `tfra` の full box の version を 0 か 1 に限定し、仕様外の version 2 を受理・出力しないようにする。

## 現状

ISO/IEC 14496-12 の `TrackFragmentBaseMediaDecodeTimeBox` (`tfdt`) と `TrackFragmentRandomAccessBox` (`tfra`) の version は 0 か 1 である。`TfdtBox` は decode した version を公開フィールドとして保持し、`FullBox::full_box_version` が値の範囲内なら保存値を返す。`TfraBox` も同じ構造である。

そのため `TfdtBox { version: 2, .. }` を encode すると、32 ビットの `base_media_decode_time` を持つ version 2 のボックスを書き出す。decode 側も `version == 1` 以外をすべて 32 ビットとして読むため、version 2 の入力を version 0 のレイアウトとして受理する。

再現:

```rust
let tfdt = TfdtBox {
    version: 2,
    base_media_decode_time: 1000,
};
let bytes = tfdt.encode_to_vec().unwrap();
assert_eq!(bytes[8], 2);
```

確認結果:

```text
tfdt len=16, version_byte=2
decoded version=2, time=1000
```

## 設計方針

- `TfdtBox::decode` と `TfraBox::decode` で、version が 2 以上のとき `InvalidData` を返す
- `TfdtBox::encode` と `TfraBox::encode` で、保存された version が 0 か 1 であることを検査し、範囲外なら `InvalidInput` を返す
- `CttsBox` と `CslgBox` は既に version 2 以上を拒否しているため、同じ方針に揃える
- version 0 / 1 の roundtrip と、version 2 の拒否テストを追加する

## 完了条件

- version 2 の `tfdt` を decode すると `InvalidData` になる
- `TfdtBox { version: 2, .. }.encode_to_vec()` が `InvalidInput` になる
- version 2 の `tfra` でも decode / encode が同様にエラーになる
- version 0 / 1 の `tfdt` / `tfra` は従来どおり roundtrip する
- `cargo test --workspace` が成功する
