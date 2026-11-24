# ディスクリプターの decode のエラー理由に失敗したディスクリプターの型名を含める

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/update-descriptor-error-reason-tag
- Polished: {YYYY-MM-DD}

## 目的

ディスクリプターの decode が失敗したとき、エラー理由（`Error::reason`）から、どのディスクリプターで失敗したかを特定できるようにする。

今は、別のディスクリプターで起きた失敗が、同じ理由文字列と同じ `location` になる経路がある。壊れた `esds` を調べるとき、理由を読んでも、どのディスクリプターが壊れているかを判断できない。

## 現状

`src/descriptors.rs` が返すエラー理由は次のとおり。

| 発生箇所 | 種別 | 理由 |
| --- | --- | --- |
| `decode_descriptor_payload` のタグ検査（`EsDescriptor` / `DecoderConfigDescriptor` / `SlConfigDescriptor` で共通） | `InvalidData` | `Unexpected descriptor tag: expected={expected_tag}, actual={tag}` |
| `DecoderSpecificInfo::decode` のタグ検査 | `InvalidData` | 同上 |
| `shift_in_size_bits`（`decode_tag_and_size` から呼ぶ。4 型で共通） | `InvalidData` | `Descriptor size exceeds usize::MAX` |
| `within_declared_payload`（上記 3 型で、宣言されたペイロードの中を読む途中に入力が尽きた場合） | `InvalidData` | `{型名} contents exceed declared descriptor size` |
| `EsDescriptor::decode_payload` の URL の検査 | `InvalidData` | `URL string exceeds declared descriptor size` / `Invalid UTF-8 in URL string` |
| `DecoderConfigDescriptor::decode_payload` の `buffer_size_db` の検査 | `InvalidData` | `buffer_size_db exceeds declared descriptor size` |
| `DecoderSpecificInfo::decode` の境界検査 | `InvalidData` | `DecoderSpecificInfo payload exceeds buffer boundary` |
| `SlConfigDescriptor::decode_payload` | `Unsupported` | ``Unsupported `SLConfigDescriptor.predefined` value: {predefined}`` |

`EsdsBox::decode`（`src/boxes_moov_tree.rs`）は、`with_box_type` でエラーの `box_type` に `esds` を入れる。そのため、`Display` で `[esds]` までは分かる。一方、ディスクリプターには `box_type` に当たる情報がない。入れ子のどこで失敗したかを知る手がかりは、理由文字列と `location` だけである。しかも `within_declared_payload` は `location` を書き換えない。

壊れた `esds` を `EsdsBox::decode` に渡して、develop で確かめた。次の 3 つの系統では、失敗したディスクリプターを特定できない。

### (a) 子のディスクリプターが親の宣言されたペイロードに収まらない

次の 3 つの入力は、いずれも理由が `EsDescriptor contents exceed declared descriptor size` になる。

- `EsDescriptor` の中の `DecoderConfigDescriptor` の宣言サイズが、`EsDescriptor` のペイロードの残りを超える
- `EsDescriptor` の中の `SlConfigDescriptor` の宣言サイズが、`EsDescriptor` のペイロードの残りを超える
- `EsDescriptor` のペイロードが、`DecoderConfigDescriptor` のタグの直後で尽きる

前の 2 つでは、子の `decode_descriptor_payload` が `InsufficientBuffer` を返し、親の `within_declared_payload` がそれを置き換える。このとき `location` はどちらも `decode_descriptor_payload` の中を指す。超えたのが `DecoderConfigDescriptor` なのか `SlConfigDescriptor` なのかは、理由からも `location` からも区別できない。

### (b) サイズ欄の値が `usize` に収まらない

次の 2 つの入力は、いずれも理由が `Descriptor size exceeds usize::MAX` になる。

- `DecoderConfigDescriptor` のサイズ欄が `usize` に収まらない
- その中の `DecoderSpecificInfo` のサイズ欄が `usize` に収まらない

`location` はどちらも `shift_in_size_bits` の中を指す。どのディスクリプターのサイズ欄で失敗したかは分からない。

### (c) タグが期待と異なる

理由は `Unexpected descriptor tag: expected=4, actual=5` のように、タグの数値だけを含む。期待したタグの値から位置を推測することはできる。ただし、型名を含む他の理由とは書き方が揃っていない。

### 0146 との関係

今は、`DecoderConfigDescriptor` の中の `DecoderSpecificInfo` が親のペイロードを超えると、`DecoderSpecificInfo payload exceeds buffer boundary` になる。0146 で `DecoderSpecificInfo::decode` を `decode_descriptor_payload` に揃えると、この理由は `within_declared_payload` が付ける `DecoderConfigDescriptor contents exceed declared descriptor size` に変わる。その結果、この経路も (a) と同じ形になり、`DecoderSpecificInfo` の名前が理由から消える。

## 設計方針

- エラー種別（`ErrorKind`）と `location` は変えない。`DecoderSpecificInfo` のエラー種別の非対称は 0146 で扱う
- 既存の理由文字列は残し、失敗したディスクリプターの型名を付け加える
  - 既存テストは、理由を `contains` で照合している。照合している部分文字列は `EsDescriptor`、`DecoderConfigDescriptor`、`SlConfigDescriptor`、`buffer_size_db`、`URL string`、`Descriptor size exceeds usize::MAX`、`DecoderSpecificInfo payload exceeds buffer boundary` である。変更後の理由にもこれらを含める
  - `DecoderSpecificInfo payload exceeds buffer boundary` は、0146 が先に入った場合はなくなる
- ボックスの `box_type` が、エラーを最初に出したボックスを示すのと同じ考え方を取る。入れ子の経路全体（`EsDescriptor` からたどった並び）は理由に含めず、失敗した位置に最も近いディスクリプターを示す
  - (a) では、親の型名と、読もうとした子の型名の両方を含める。子のタグを読む前に親のペイロードが尽きる場合（`SlConfigDescriptor` のタグの位置でペイロードが尽きる場合など）も含む。子の型名は、親が子を decode する呼び出し側で付ける
  - (b) では、そのサイズ欄を持つディスクリプターの型名を含める
  - (c) では、期待したディスクリプターの型名を含める
- トップレベルで返る `InsufficientBuffer` の理由は、空のまま変えない
  - 呼び出し側は、decode した型を知っている
  - クレートの中で `InsufficientBuffer` を作るのは `Error::insufficient_buffer` だけで、理由を付けない。ボックスの decode が返す `InsufficientBuffer` も同じである

## 完了条件

- `tests/test_descriptors.rs` の公開 API のテストで、次を確認する
  - `EsDescriptor` の中の `DecoderConfigDescriptor` が超える場合と、`SlConfigDescriptor` が超える場合とで、理由が異なる。どちらの理由も、`EsDescriptor` と、超えた子の型名を含む
  - `DecoderConfigDescriptor` のサイズ欄が `usize` に収まらない場合と、その中の `DecoderSpecificInfo` のサイズ欄が収まらない場合とで、理由がそれぞれの型名を含む
  - 4 型のタグ不一致で、理由が期待した型名を含む。既存の `*_invalid_tag` の 4 件を、`is_err()` だけでなく理由まで照合する形にする
- 既存テストのエラー種別の照合が変更なしで通り、`cargo test --workspace` が成功する
