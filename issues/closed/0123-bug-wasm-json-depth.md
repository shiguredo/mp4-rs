# wasm の JSON 入力に深さと長さの上限を設ける

- Created: 2026-11-14
- Completed: 2026-11-14
- Branch: feature/change-wasm-bindgen
- Polished: {YYYY-MM-DD}

## 目的

`crates/wasm` の公開 API はトラック情報・サンプル情報・サンプルエントリーを JSON 文字列で受け取る。`nojson` のパーサは再帰実装であり深さの上限を持たないため、深くネストした JSON を渡すと wasm のスタックが枯渇し、trap した後にインスタンスが復帰不能になる。公開 API の入口で入力を拒否し、利用側の誤りや攻撃的な入力で wasm インスタンス全体が使えなくなるのを防ぐ。

## 現状

- 実装済み。`crates/wasm/src/json_limit.rs` がパース前に深さ 128 段、文字列長 8 MiB を検査し、`appendSample` / `createMediaSegment` / `createMediaSegmentWithSidx` の 3 入口すべてに適用している
- 深さを超える JSON は `RuntimeError` ではなく `JsError` になり、同じインスタンスの他のオブジェクトは使い続けられる
- 回帰テストは `crates/wasm/tests/wasm_api.test.mjs` に追加し、深い JSON を渡した後に `version()` が成功することを検証している
- 数値配列の要素数は、文字列長の上限で間接的に制限している（要素数専用の上限は設けていない）

## 設計方針

- JSON をパースする前に、文字列リテラルとエスケープを考慮した線形の深さ検査を行う
  - JSON のスキーマ上の深さは数段しかないため、上限は 128 程度でよい
  - 上限を超えた場合は `JsError` を返し、trap させない
- 入力 JSON の文字列長と、数値配列の要素数にも上限を設ける
  - wasm32 の線形メモリは 4 GB であり、`nojson` は入力サイズに対してメモリを増幅する
- 検査は `crates/wasm` 側の入口に置く。`nojson` に深さ制限を追加できる場合はそちらに寄せる
- 上限値は定数として 1 箇所にまとめ、README に記載する

## 完了条件

- 深くネストした JSON を `appendSample` / `createMediaSegment` に渡しても trap せず、`JsError` になる
- 拒否した後も同じインスタンスで `version()` と他のオブジェクトの操作が続けられる
- 上限を超える長さの JSON と数値配列も `JsError` になる
- 上記を検証するテストが `crates/wasm/tests/` に追加され、CI で実行される
- README に上限値を記載する

## 解決方法

- `crates/wasm/src/json_limit.rs` を追加し、文字列リテラルとエスケープを考慮した線形走査でネストの深さを数え、128 段を超える場合は例外にした
- 同じ検査で JSON 文字列の長さを 8 MiB に制限した。wasm32 の線形メモリは 4 GiB であり、巨大な入力をそのままパースするとメモリ不足で abort して解放できなくなるため
- `Mp4FileMuxer::appendSample` / `Fmp4SegmentMuxer::createMediaSegment` / `createMediaSegmentWithSidx` の入口で、パース前に検査するようにした
- `crates/wasm/src/json_limit.rs` の単体テストと、深い JSON を渡しても `version()` が成功することを確認する統合テストを追加した
- 数値配列の要素数は文字列長の上限で間接的に制限している。要素数専用の上限が必要になった場合は別途追加する
