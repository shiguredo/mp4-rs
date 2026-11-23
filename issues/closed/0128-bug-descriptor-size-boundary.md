# `EsDescriptor` と子記述子の読み取りを宣言されたペイロード内に制限する

- Created: 2026-11-22
- Completed: 2026-11-23
- Branch: feature/fix-descriptor-size-boundary
- Polished: 2026-11-22

## 目的

各記述子を宣言されたペイロードの範囲内でデコードし、隣接する記述子のバイトを固定フィールド、URL 文字列、または子記述子として読まないようにする。

## 現状

`src/descriptors.rs` の `EsDescriptor::decode`、`DecoderConfigDescriptor::decode`、`SlConfigDescriptor::decode` は、`decode_tag_and_size` が返す宣言サイズを使わない。`EsDescriptor` は URL 文字列の長さを入力スライスの残量と比較し、`DecoderConfigDescriptor` はその残りに子のタグが現れるかを調べる。`DecoderSpecificInfo::decode` は自身の宣言サイズを検査するが、親から宣言範囲外のデータを渡されると、親の境界までは判断できない。

`DecoderConfigDescriptor::decode` に記載されているプロファイル・レベルの子記述子は未実装。`EsdsBox::decode` は外箱のペイロードから `EsDescriptor` を読むが、その消費長と箱の残量が一致するかは検査しない。

## 設計方針

- `decode_tag_and_size` が 0127 で宣言サイズを正確に復号することを前提とする。ここでは返されたヘッダー長と宣言サイズから、実際の入力内に収まるペイロードだけを取り出す。範囲の計算とスライス化に検査付きの操作を使い、算術オーバーフローや範囲外アクセスを起こさない
- `EsDescriptor`、`DecoderConfigDescriptor`、`SlConfigDescriptor` の固定フィールド、任意フィールド、子は、それぞれの宣言範囲に収めたスライスだけから読む。子のヘッダーと宣言ペイロードも、親が渡した残りのペイロードからはみ出してはならない。`DecoderSpecificInfo` の既存の境界検査は維持する
- 入力スライス自体がヘッダーや宣言ペイロードの終端に達しない場合、上記 3 型を単体でデコードするときは `ErrorKind::InsufficientBuffer` を返す。宣言範囲を確保できた記述子の中で、読み取るフィールドや子のヘッダー・宣言ペイロードがその境界を越える場合は、当該記述子から `ErrorKind::InvalidData` を返す。既存の未対応値に対する `ErrorKind::Unsupported` は維持する
- 既知の構文を読み終えた後に同じ記述子の宣言範囲内に残ったデータは、解釈せずに読み飛ばす。未対応要素の解析、保持、再エンコードは対象外。成功時に返す消費バイト数はヘッダー長と宣言ペイロード長の合計とする。宣言内に未解釈の末尾がある場合は、その分も消費バイト数に含めるため、従来の戻り値と異なることがある。`EsdsBox` 自体の境界や残余の扱いは変えない

## 完了条件

- 公開の `Decode` の PBT で、正常な記述子を順に読めて、成功時の消費バイト数が次の記述子の先頭を指すことを確認する。宣言範囲内の未解釈部分を読み飛ばす場合も同じ消費長の規則を確認する
- 実入力の不足と、親の宣言範囲内に必須フィールドや子が収まらない場合を区別して、公開デコードのエラーパスのテストでそれぞれのエラー種別を確認する。任意の子の有無の判定も親の外を参照しないことを確認する
- 既存の記述子と `esds` を含むファイルのラウンドトリップが引き続き成功し、`cargo test --workspace` が通る

## 解決方法

- `src/descriptors.rs` に private な共通関数を 2 つ追加した
  - `decode_descriptor_payload`: タグを検査してから、宣言されたペイロードを切り出す。ヘッダー長と宣言サイズの和は `checked_add` で求め、和がオーバーフローする場合や入力に収まらない場合は `ErrorKind::InsufficientBuffer` を返す。ペイロードの中身はこの時点では読まない
  - `within_declared_payload`: 宣言されたペイロードの中を読む処理で出た `ErrorKind::InsufficientBuffer` だけを `ErrorKind::InvalidData`（理由 `"<型名> contents exceed declared descriptor size"`）に置き換える。`location` は読み取りに失敗した位置のまま残す
- `EsDescriptor`、`DecoderConfigDescriptor`、`SlConfigDescriptor` に private な `decode_payload` を追加した。固定フィールド、任意フィールド、子のディスクリプター、`DecoderSpecificInfo` の有無の判定は、すべて宣言されたペイロードのスライスだけから行う。成功時の消費バイト数はヘッダー長と宣言サイズの合計で、読み終えた後にペイロードに残ったデータは読み飛ばす。URL 文字列と `buffer_size_db` の明示的な検査の文言は `... exceeds declared descriptor size` に変えた
- `DecoderSpecificInfo::decode` の既存の境界検査と `EsdsBox` は変更していない。`DecoderSpecificInfo` が共通関数を使わない理由はコメントに残した
- テスト
  - `tests/test_descriptors.rs` に mod `descriptor_declared_payload_tests` を追加した
    - 3 型それぞれの `*_payload_exceeds_input`: 入力が宣言されたペイロードの終端に達しない場合に `InsufficientBuffer` を返すことを、中身だけなら読める入力と、中身を読むとエラーになる入力の 2 ケースで確認する
    - `*_outside_declared_payload` と `*_exceeds_declared_payload`: フィールド・URL 文字列・子のヘッダーや宣言されたペイロードが親の宣言されたペイロードに収まらない場合に `InvalidData` を返すことを確認する。どの入力にも宣言されたペイロードの外に続きのバイトを置き、それを読まないことも確かめる
  - 既存の `sl_config_descriptor_unsupported_predefined` は `Unsupported` のまま返ることを確認するようにした。`buffer_size_db` の既存テストは新しい mod に移して改名した
  - `src/descriptors.rs` の `mod tests` に、宣言サイズが `usize::MAX` の 3 型が `InsufficientBuffer` を返すテストを追加した。fuzz 由来の回帰テストは `esds_fuzz_input_with_short_decoder_config_descriptor` に改名し、理由文字列も確認するようにした
  - `pbt/tests/prop_descriptors.rs`: 型ごとの roundtrip 3 本を `descriptors_decode_sequentially` に統合した。連結したディスクリプターを順に decode して値と消費バイト数を確認する。自身や子の宣言されたペイロードの末尾に解釈しないバイト列を足す場合と、サイズ欄を冗長な長さにする場合を含む。ケースのおよそ半分では、`DecoderSpecificInfo` を持たない `DecoderConfigDescriptor` の直後に `DecoderSpecificInfo` を並べ、それを子として読まないことも確認する
- `cargo test --workspace` が esds を含む既存ファイルのラウンドトリップを含めて成功することと、wasm32-wasip1 を対象にしたビルドで追加したテストが通ることを確認した
- `CHANGES.md` の `## develop` に [FIX] を追記した
