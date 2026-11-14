# wasm の JSON 入力に深さと長さの上限を設ける

- Created: 2026-11-14
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-wasm-json-depth
- Polished: {YYYY-MM-DD}

## 目的

`crates/wasm` の公開 API はトラック情報・サンプル情報・サンプルエントリーを JSON 文字列で受け取る。`nojson` のパーサは再帰実装であり深さの上限を持たないため、深くネストした JSON を渡すと wasm のスタックが枯渇し、trap した後にインスタンスが復帰不能になる。公開 API の入口で入力を拒否し、利用側の誤りや攻撃的な入力で wasm インスタンス全体が使えなくなるのを防ぐ。

## 現状

- `crates/wasm` は `nojson` 0.3 で JSON をパースしており、`appendSample` / `createMediaSegment` / `createMediaSegmentWithSidx` の入口で深さと長さの検査を行っていない
- 約 15 KB の JSON（7,500 段の配列を未使用メンバーに含むもの）を `appendSample` に渡すと `RuntimeError: memory access out of bounds` になる
- trap した後は `version()` も `free()` も失敗し、同じ WebAssembly インスタンス上の他のオブジェクトも実用上使えなくなる
- 巨大な数値配列（1,600 万要素、JSON で約 153 MB）でも 4 GB のメモリ上限に達して abort し、オブジェクトを解放できなくなる
- どちらも `develop` の旧 C ABI 実装でも同じ深さ・サイズで再現する。`crates/wasm` の変更に起因する退行ではない
- `crates/wasm/tests/` に回帰テストは無い

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

{実装後に記載する}
