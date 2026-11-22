# `EsDescriptor` と子記述子の読み取りを宣言されたペイロード内に制限する

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
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
