# `TfdtBox` と `TfraBox` で未対応のバージョンを既知の構文として扱わない

- Created: 2026-11-22
- Completed: 2026-11-23
- Branch: feature/fix-tfdt-tfra-invalid-version
- Polished: 2026-11-22

## 目的

このライブラリが対応する `tfdt` と `tfra` のバージョンを 0 と 1 に限定する。未対応のバージョンを既知の構文としてデコードせず、エンコードでも出力しない。

## 現状

ISO/IEC 14496-12:2015 の 8.8.12.2 と 8.8.12.3 は `tfdt` のバージョン 0 と 1 を定義する。`tfra` は同規格の 8.8.10.2 でバージョン 1 とそれ以外に構文を分けているが、ここでは規格が 2 以上を禁止すると断定せず、ライブラリの対応範囲を 0 と 1 とする。同規格の 4.2 は未認識バージョンのボックスを読み飛ばすとしている。

`src/boxes_fmp4.rs` の `TfdtBox::decode` と `TfraBox::decode` は、バージョン 1 以外を 32 ビットの時刻・位置の構文として読み、バージョンを公開フィールドに保持する。エンコード時は `full_box_version` が、時刻や位置が 32 ビットに収まらなければ 1 を返し、収まれば保持値を返す。そのため、保持値が範囲外でも、値によってはそのままヘッダーに書かれるか、バージョン 1 に置き換わる。

親の `TrafBox::decode` と `MfraBox::decode` は現在、各子のデコードエラーを伝播させる。単体のデコーダーで未対応バージョンを拒否するだけでは、親が読み飛ばすことにはならない。

## 設計方針

- `TfdtBox::decode` と `TfraBox::decode` を単体で呼んだ場合、FullBox ヘッダーに 0・1 以外があれば、バージョン別フィールドを読む前に `ErrorKind::InvalidData` を返す
- `TrafBox::decode` と `MfraBox::decode` は、入力範囲に収まる子ヘッダーと FullBox ヘッダーを読めた場合に限り、未対応バージョンの `tfdt` または `tfra` を、その子の検証済みの範囲の末尾まで読み進めて捨てる。子の外形の検査でエラーになった場合や、子を通常どおり型付きでデコードしてエラーになった場合は、そのエラーを返す。`tfdt` は重複判定より先に未対応バージョンを除外する。それ以外の重複は変更せず、最初の `tfdt` を採用済みで、重複した子の FullBox ヘッダーを読めない場合も、外形が正しければ従来どおり `UnknownBox` として扱う。この読み飛ばしの分岐では、親の残り全体を子の範囲とはみなさず、子の `size` が 0 なら `ErrorKind::InvalidData` を返す。対応バージョンと既存の重複処理のサイズ契約は変えない。未対応バージョンの本文やフラグの意味は解釈しない。`tfdt` がないときのデマルチプレクサーの時刻計算は 0110 の対象とする。`mfra` を再エンコードする際の `mfro.size` は 0125 で扱うため、0125 を先に実装する
- `TfdtBox::encode` と `TfraBox::encode` は、ヘッダーの書き込み前に、算出後のバージョンではなく `self.version` を検査する。0・1 以外を `ErrorKind::InvalidInput` とし、有効な 0 を 64 ビットが必要なときに 1 へ昇格する既存の挙動は維持する

## 完了条件

- 単体デコードで未対応バージョンを `ErrorKind::InvalidData` とし、エンコードでは 32 ビットに収まる値かどうかに関係なく、範囲外の保持値を `ErrorKind::InvalidInput` とする
- 親をデコードすると未対応バージョンの子は結果に含まれず、後続の対応バージョンの子を読める。未対応の `tfdt` は、対応バージョンの `tfdt` の後にあっても `UnknownBox` として保持されない。対応バージョンの子のデコードエラーは引き続き親から返る
- 既存の往復 PBT で、バージョン 0 と、時刻・位置が 32 ビットに収まる場合も含むバージョン 1 が保持されることを確認する。既存の生成器は、32 ビットに収まる値で 0 と 1 をそれぞれ明示的に生成できるようにし、`tfdt_box_roundtrip` にはデコード後のバージョンの比較を加える。有効なバージョン 0 の指定で 64 ビットが必要な場合は、出力がバージョン 1 になることを両型で確認する
- `cargo test --workspace` が成功する

## 解決方法

- `src/boxes_fmp4.rs` に、対応するバージョンの上限を表す定数 `MAX_SUPPORTED_TFDT_TFRA_VERSION`（1）と、3 つの補助関数を追加した。規格の根拠（ISO/IEC 14496-12:2015 の 8.8.12.2 / 8.8.12.3 / 8.8.10.2 / 4.2）と、将来の改訂で変わる可能性があることはコードコメントに残した
  - `check_encode_version`: 範囲外のバージョンを `ErrorKind::InvalidInput` にする
  - `check_decode_version`: 未対応バージョンを `ErrorKind::InvalidData` にする
  - `unsupported_version_child_size`: 子ヘッダーと FullBox ヘッダーを読めて、かつ未対応バージョンの子なら、その子の範囲の長さを返す。子のサイズが 0 なら、子の `box_type` を設定した `ErrorKind::InvalidData` を返す。外形を読めない場合は `None` を返し、通常の処理に任せる
- `TfdtBox::decode` / `TfraBox::decode` は FullBox ヘッダーを読んだ直後に `check_decode_version` を呼び、バージョン別のフィールドを読む前に `InvalidData` を返すようにした
- `TfdtBox::encode` / `TfraBox::encode` は、ヘッダーを書き込む前に `self.version` を `check_encode_version` で検査するようにした。`TfraBox` では既存の長さフィールドの検査より前に置いた。有効なバージョン 0 で 64 ビットが必要な値をバージョン 1 で出力する挙動は変えていない
- `TrafBox::decode` は、子の種別による分岐（重複判定を含む）より前に、未対応バージョンの `tfdt` を読み飛ばすようにした。`MfraBox::decode` も同様に、未対応バージョンの `tfra` を `TfraBox::decode` の前に読み飛ばす。読み飛ばした子は保持しない。`TrafBox` / `MfraBox` / `TfdtBox` / `TfraBox` の doc にこの挙動を追記した
- テスト
  - `tests/test_boxes_fmp4.rs` に mod `tfdt_tfra_unsupported_version` を追加した。単体 decode の `InvalidData`、単体 encode の `InvalidInput`（値の大きさと、`Vec` / 空のバッファの別を問わない）、サイズ 0 の未対応バージョンの子の `InvalidData`、親の範囲を超える子と壊れた対応バージョンの子のエラー、FullBox ヘッダーを読めない重複した `tfdt` が従来どおり `UnknownBox` になることを確認する
  - `pbt/tests/prop_fmp4_boxes.rs` の `arb_tfdt_box` / `arb_tfra_box` を、32 ビットに収まる値のバージョン 0 と 1、64 ビットの値のバージョン 1 を明示的に生成するようにした。`tfdt_box_roundtrip` はデコード結果全体（バージョンを含む）を比較する
  - PBT に、バージョン 0 で 64 ビットが必要な値がバージョン 1 で出力されることの確認（`tfdt` / `tfra`）と、未対応バージョンの子の扱いの確認（`traf` での読み飛ばし、`tfdt` 以外の子は `unknown_boxes` に残ること、`mfra` での読み飛ばし）を追加した。未対応バージョンの子は通常ヘッダーと largesize ヘッダーの両方で生成する
- `tfdt` がないときのデマルチプレクサーの時刻計算（0110）と、`mfra` を再エンコードする際の `mfro.size`（0125）は変更していない
- `CHANGES.md` の `## develop` に [FIX] を追記した
