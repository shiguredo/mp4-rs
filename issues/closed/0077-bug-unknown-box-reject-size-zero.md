# UnknownBox のデコードで size=0 を原則拒否する

- Created: 2026-08-19
- Completed: 2026-08-20
- Branch: feature/fix-unknown-box-reject-size-zero
- Polished: {YYYY-MM-DD}

## 目的

`UnknownBox` のデコードで size=0（`BoxSize::VARIABLE_SIZE`）を原則拒否することで、コンテナボックス末尾に 4 バイト以上のゼロ埋めが混入した壊れた入力が「1 個の未知 box（type = `\0\0\0\0`）」として黙って吸収されるパターンを検出できるようにする。

現状はコンテナ内部のどこであっても size=0 の未知 box が受理され、残りペイロード全体を 1 個の `UnknownBox { box_type=<null 相当>, box_size=U32(0), payload=残り全部 }` として成功扱いにしてしまう。呼び出し側にエラーは出ず、返却値の見た目上は「未知 box が 1 個入っていた」ように見えるため、下流（mp4-py 等）でも異常を検出しづらい。

### 本 issue の効果は「size=0 という仕様外パターン限定」の検出改善である

`UnknownBox` は「未知の box_type を受け入れる」設計なので、壊れた入力でも `size` が非ゼロで妥当な範囲に収まっていれば、その中身が何であれ 1 個の未知 box として成功する。これは `UnknownBox` の設計上の性質であり、本 issue で解決するものではない。

たとえば末尾に `[0x00, 0x00, 0x00, 0x08, b'x', b'x', b'x', b'x']` のようなゴミが並んでいれば、それは size=8 / type=`"xxxx"` の unknown box として成功する。本 issue の対応後もこれは検出できない。

本 issue で検出できるようになるのは以下 1 パターンに限る:

- コンテナ内部の未知 box を読む位置に、先頭 4 バイトが `0x00000000`（size=0）で始まるバイト列が現れる場合

`size=0` は ISO/IEC 14496-12 4.2 で「box が file の最後まで拡張される」と定義され、実質的にはトップレベルの `mdat` 相当でしか使われない。コンテナ内部の未知 box に size=0 が現れることは仕様上ありえず、正当な MP4 では誤検出しない。したがってこの 1 パターンだけは 100% 誤検出なく潰せる、という改善に本 issue のスコープを限定する。

## 現状

`UnknownBox::decode`（`src/boxes.rs`）は `BoxHeader::decode_header_and_payload`（`src/basic_types.rs`）をそのまま呼び出しており、size=0（`BoxSize::VARIABLE_SIZE` = `U32(0)`）が入っても「バッファ末尾までがペイロード」として成功する（この挙動自体は issue 0025 で意図的に確立されたもので、`decode_header_and_payload` の docstring にも「呼び出し側はファイル末尾のボックスに限って使うこと」と注意書きがある）。

一方、コンテナ box の実装は末尾で以下のようなループで未知 box を吸収している（`src/boxes_sample_entry.rs` の `StppBox::decode`、`src/boxes_fmp4.rs` の各 `TrafBox` / `MoofBox` などの内部 loop、`Tx3gBox::decode` の末尾 loop 等、多数）:

```rust
while offset < payload.len() {
    unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
}
```

このループにゼロ埋めが混ざったとき、先頭 4 バイトが `0x00000000` になれば `UnknownBox::decode` が `BoxSize::VARIABLE_SIZE` として残りペイロード全体を吸収してループが 1 回で終わる。エラーは出ない。

具体例として、下流 mp4-py の issue 0050 で観測された stpp 誤パースがある。stpp のペイロード末尾が連続 null で終わっているとき、`Utf8String::decode` は空文字列 3 つ（namespace / schema_location / auxiliary_mime_types）として正常に消費し、残ったゼロパディングが 1 個の未知 box に化けて成功扱いになる。stpp 特有ではなく、末尾で `while offset < payload.len() { UnknownBox::decode_at(...)? }` パターンを持つ全てのコンテナ box で同じ穴が空いている。

現行の設計思想（`decode_header_and_payload` の docstring）どおり、`size=0` は **ファイル末尾のトップレベル box に限り** 意味のある値である。`UnknownBox` はコンテナ内部の未知 box 受け皿として最も頻用されるが、そこで size=0 を無検査に許してしまうのは docstring の警告を型・実装レベルで強制できていないことを意味する。

## 設計方針

`UnknownBox::decode` を **常に size=0 を拒否** する実装に変える（`InvalidData` 相当のエラー）。呼び出し側規約に頼らず、UnknownBox 側で強制する。

- 影響範囲は「`UnknownBox` のデコード時に size=0 が来た場合」に限定する。`BoxHeader::decode_header_and_payload` 自体の挙動は issue 0025 で確立された仕様（VARIABLE_SIZE をバッファ末尾扱い）を変更しない
- したがって既知の top-level box（`MdatBox` など）が `decode_header_and_payload` 経由で size=0 を受理する挙動は影響を受けない
- `RootBox::decode`（`src/boxes.rs`）は未知の box_type に対して `UnknownBox::decode_top_level` を呼んで `RootBox::Unknown(...)` を作る。`RootBox::decode` は **top-level で size=0 を許容すべき数少ない経路** であり、`UnknownBox::decode` の strict 化とは別に `UnknownBox::decode_top_level` を追加して従来どおり size=0 を受理させる

`decode_top_level` を追加する理由:

- 末尾にゼロパディング（8 バイト以上）を持つ実ファイルは `Mp4File::decode`（`RootBox::decode` 経由）で従来は末尾を 1 個の未知 box として吸収して成功していた。`UnknownBox::decode` を strict にしただけではこのパターンがエラーになり、実ファイルの互換性を損なう
- 一方、コンテナ内部（`stpp` 等の unknown_boxes ループ）の size=0 は仕様上ありえず、本 issue の検出対象。トップレベルとコンテナ内部で扱いを分ける

### 他実装

issue 0025 の調査で示されたように、GPAC / mp4parse-rust は size=0 の受理位置に強い制約（GPAC は root box のみ、mp4parse-rust は `MediaDataBox` のみ）を設けており、コンテナ内部で size=0 の未知 box を許すのは意図された動作ではない。

## 完了条件

- `UnknownBox::decode` に対して先頭 4 バイトが `0x00000000` のバッファを与えたとき、エラー（`InvalidData` 相当）を返すこと
- 上記に伴い、コンテナ box 内部の `while offset < payload.len() { UnknownBox::decode_at(...)? }` パターンで、末尾ゼロ埋めが 4 バイト以上あった場合にループがエラーで停止すること
- `RootBox::decode` の既知型分岐（`MdatBox` などの top-level size=0 が意味を持つ box）は従来どおり成功すること
- `RootBox::decode` の未知型分岐（`UnknownBox::decode_top_level` を呼ぶ経路）は、size=0 の未知型 top-level box を従来どおり受理すること
- 影響を受ける可能性のあるコンテナ box（`StppBox` を含む `src/boxes_sample_entry.rs` の末尾 unknown_boxes loop、`src/boxes_fmp4.rs` の末尾 unknown_boxes loop、`src/boxes_moov_tree.rs` の該当箇所）の既存テストが引き続き通ること
- 上記挙動の変化をカバーする回帰テストを追加すること

（本 issue が保証するのは「先頭 4 バイトが `0x00000000` のパターンだけ検出する」ところまでで、末尾に非ゼロ size の壊れたバイト列（例: `[0x00, 0x00, 0x00, 0x08, b'x', b'x', b'x', b'x']`）が並ぶケースは対象外。これは `UnknownBox` の設計上、任意の未知 type / size を受け入れるため避けられない）

### 未知型 top-level + size=0 を許容する理由

- ISO/IEC 14496-12 4.2 の size==0 の意味は「box が file の最後まで拡張される」であり、ファイル末尾のトップレベル box に限り仕様上有効
- 本 issue の検出対象はコンテナ内部（unknown_boxes ループ）のゼロ埋めであり、トップレベル末尾のゼロパディングは対象外
- `Mp4File::decode`（`RootBox::decode` 経由）で末尾ゼロパディングを持つ実ファイルが読めなくなる互換リスクを避けるため、`UnknownBox::decode_top_level` で従来どおり受理する

## 解決方法

- `src/boxes.rs` の `impl Decode for UnknownBox` を修正し、`BoxHeader::decode_header_and_payload` 呼び出し前後で `header.box_size == BoxSize::VARIABLE_SIZE` を判定してエラー（`Error::invalid_data("UnknownBox does not accept size=0")` 相当）を返す
  - 実装順序としては `BoxHeader::decode` だけ先に行い、`box_size` を確認してから `decode_header_and_payload` を呼ぶ形にすれば、size=0 を判別してからペイロードを取りに行けて自然
- `UnknownBox` の docstring に「`UnknownBox::decode` は size=0 を受理しない。トップレベルの可変長サイズの未知 box を受理したい場合は `UnknownBox::decode_top_level` を使う」と明記する
- `UnknownBox::decode_top_level` を追加し、`RootBox::decode` の未知型分岐から使う
- 回帰テスト（`src/boxes.rs` のテストモジュール、または `tests/` に新規テストファイル）で以下を検証する
  - `UnknownBox::decode` に size=0 のバッファ（例: `[0x00, 0x00, 0x00, 0x00, b'x', b'x', b'x', b'x']` の後にペイロード）を与えたときにエラーになること
  - `UnknownBox::decode` に size=8（ヘッダーのみで空ペイロード）のバッファを与えたときに成功すること（回帰確認）
  - `UnknownBox::decode_top_level` に size=0 のバッファを与えたときに成功すること
  - コンテナ経由のシナリオとして、`StppBox::decode` に「先頭 8 バイトの予約領域と data_reference_index の後、3 つの null 終端空文字列に続いてゼロ埋めが 4 バイト以上並ぶ」ペイロードを与えたときにエラーになること（size=0 パターン限定の再現テスト。ペイロード全体を `stpp` box として組み立ててデコードする）
  - `RootBox::decode` に size=0 の未知型 top-level box を与えたときに成功すること（従来どおりの挙動）
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加する

## スコープ外

- 既知型で size=0 を許容している経路（`MdatBox` など）の挙動変更は行わない
- `BoxHeader::decode_header_and_payload` そのものの挙動変更は行わない（issue 0025 で確立された挙動を維持）
- 末尾ゼロ埋めを許容側に倒す（tolerant parsing）方針は取らない。方針としては「壊れた入力のうち size=0 パターンだけを検出する」側を選ぶ
- 壊れた入力全般の検出は本 issue の対象外。非ゼロ size で妥当な範囲に収まった未知 box は、内容が何であれ従来どおり成功する

## 補足

- 本 issue は下流 mp4-py 側で観測された stpp 誤パース（mp4-py issue 0050）の根本原因調査で判明。mp4-py 側は特性化テストで結了済みだが、根因である「コンテナ末尾の unknown_boxes ループが size=0 を吸収する」挙動は mp4-rs 内に残っている
- 関連: issue 0025（`decode_header_and_payload` の size=0 特別処理を有効化した際に「size=0 の位置制限は別 issue で扱う」と明記した follow-up がまさに本 issue に相当する）
