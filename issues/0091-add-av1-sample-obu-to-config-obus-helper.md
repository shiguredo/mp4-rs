# AV1 の Sample 文脈 OBU を ConfigObus 用バイト列へ正規化するヘルパーを追加する

- Created: 2026-08-28
- Completed: {YYYY-MM-DD}
- Branch: feature/add-av1-sample-obu-to-config-obus-helper
- Polished: 2026-10-06

## 目的

MP4 サンプルから取り出した Sequence Header OBU を、そのまま `av1C.configOBUs` や `build_av01_box` に渡せるバイト列へ変換できるようにする。Sample と ConfigObus で `obu_has_size_field` の規則が異なるため、利用側が LEB128 付き OBU を手組みしなくて済むようにする。

## 現状

- `src/bitstream/av1.rs` の `Av1ObuParseContext` は `ConfigObus` と `Sample` を区別する
  - `ConfigObus`: すべての OBU で `obu_has_size_field = 1` が必須
  - `Sample`: 最後の OBU だけ size 省略が許される
- `parse_obus(..., Sample)` で得た `Av1Obu::obu` は、それがサンプル末尾の OBU だと size フィールドを持たないことがある
- `build_av01_box` / `build_av01_box_from_config_obus` は入力を `ConfigObus` 規則で再解析するため、size 無しの OBU バイト列を渡すと拒否される
- `decode_leb128` は公開されているが、対応する `encode_leb128` は公開されていない。利用側が size 付き OBU を組み立て直すとき、LEB128 符号化を自前で持つことになる

## 設計方針

`bitstream::av1` に次の公開 API を追加する。

```rust
pub fn normalize_obu_to_config_obus(obu: Av1Obu<'_>) -> Result<Vec<u8>>
```

- 入力は [`parse_obus`] が返す [`Av1Obu`] とする。[`Av1ObuParseContext::Sample`] で得たものを想定するが、OBU 単体の正規化なので文脈は問わない。主な用途は MP4 サンプル内の Sequence Header OBU であり、Sequence Header 以外の種別も同じ規則で正規化できる
- 出力は OBU header（`obu_has_size_field` ビットを 1 に更新する。extension header があればそのまま保持する）+ payload 長の最短表現 LEB128 + payload を連結した `Vec<u8>` とし、ConfigObus 規則を満たす
- 入力がすでに `obu_has_size_field = 1` でも、常に上記の再構成を行う（非最短表現の LEB128 は最短へ正規化される）
- エラー条件は payload が `u32::MAX` を超える場合のみとする（AV1 spec §4.10.5 の `obu_size` は u32 幅）
- 再構成に必要な LEB128 符号化は `bitstream::av1` の内部実装として追加し、公開 API にはしない。利用側が LEB128 付き OBU を手組みする必要をなくすのが本 issue の目的であり、公開 API をそれ以上増やさない
- 既存の `parse_obus` / `build_av01_box*` の受理条件は狭めない

## 完了条件

- Sample 文脈で得た Sequence Header から ConfigObus 用バイト列を作れる公開 API がある
- そのバイト列を `build_av01_box` または `build_av01_box_from_config_obus` に渡して `Av01Box` を構築できる
- size 省略された Sequence Header OBU を入力にしても、正規化後は ConfigObus 規則を満たす
- ユニットテストで上記を検証している
- `cargo test` が pass する
