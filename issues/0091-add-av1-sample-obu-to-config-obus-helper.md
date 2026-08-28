# AV1 の Sample 文脈 OBU を ConfigObus 用バイト列へ正規化するヘルパーを追加する

- Created: 2026-08-28
- Completed: {YYYY-MM-DD}
- Branch: feature/add-av1-sample-obu-to-config-obus-helper
- Polished: {YYYY-MM-DD}

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

- Sequence Header の payload（または `Av1Obu`）から、ConfigObus 規則を満たす 1 OBU 分のバイト列を返すヘルパーを `bitstream::av1` に追加する
- 生成結果は `obu_has_size_field = 1` とし、`build_av01_box` / `build_av01_box_from_config_obus` にそのまま渡せることを保証する
- 必要なら `encode_leb128` も公開し、ヘルパー実装と利用側の双方で使えるようにする（公開する場合は `decode_leb128` と対になる契約を rustdoc に書く）
- 既存の `parse_obus` / `build_av01_box*` の受理条件は狭めない

## 完了条件

- Sample 文脈で得た Sequence Header から ConfigObus 用バイト列を作れる公開 API がある
- そのバイト列を `build_av01_box` または `build_av01_box_from_config_obus` に渡して `Av01Box` を構築できる
- size 省略された Sequence Header OBU を入力にしても、正規化後は ConfigObus 規則を満たす
- ユニットテストで上記を検証している
- `cargo test` が pass する
