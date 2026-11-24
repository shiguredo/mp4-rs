# `MfraBox` の decode で未知の子ボックスを捨てずに保持する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-mfra-unknown-boxes
- Polished: {YYYY-MM-DD}

## 目的

`mfra` の decode で、`tfra` と `mfro` 以外の子ボックスを `MfraBox` に保持する。decode → encode で子ボックスが失われないようにし、ほかのコンテナのボックスと同じ扱いにそろえる。

## 現状

### `MfraBox` の扱い

`src/boxes_fmp4.rs` の `MfraBox::decode` は、子ボックスを順に読み、種別ごとに次のように扱う。

- `TfraBox::TYPE` のうち、このライブラリが対応しないバージョンのもの: `unsupported_version_child_size` で読み飛ばす
- それ以外の `TfraBox::TYPE`: `tfra_boxes` に入れる
- `MfroBox::TYPE`: `mfro_box` に入れる。複数ある場合は後のもので上書きする
- 上記以外: `BoxHeader::decode_header_and_payload` でサイズだけを求めて読み飛ばす（コメントは「未知のボックスはスキップ」）

`MfraBox` には未知の子ボックスを入れるフィールドがない。そのため、読み飛ばした子ボックスは encode で失われる。issue 0125 の再現例では、16 バイトの `free` を含む 75 バイトの `mfra` を decode → encode すると、出力は 59 バイトになった。

issue 0125 は `mfro.size` が出力の実サイズと食い違う不具合を直した。この食い違いの原因の 1 つが、子ボックスの消失だった。0125 の設計方針では、未知の子ボックスの保持を対象外としていた。

読み飛ばした子ボックスは、`BaseBox::children` を通じた走査にも現れない。そのため、`tests/test_decode_encode.rs` の `collect_unknown_box_types` は `mfra` の未知の子ボックスを検出できない。`examples/dump_wasm` の出力にも、`unknown` として現れない。

### ほかのコンテナの扱い

次のコンテナは `unknown_boxes: Vec<UnknownBox>` を持つ。

- fMP4 のコンテナ: `src/boxes_fmp4.rs` の `MoofBox`、`TrafBox`、`src/boxes_moov_tree.rs` の `MvexBox`
- `moov` 配下のコンテナ: `MoovBox`、`TrakBox`、`MdiaBox`、`MinfBox`、`StblBox` など

これらは、既知の種別に当たらない子ボックスを `UnknownBox::decode_at` で読んで `unknown_boxes` に入れる。encode では既知の子ボックスの後に書き、`children` でも最後に返す。

`src/` の中で、子ボックスを走査して未知の種別を保持せずに読み飛ばすのは、`MfraBox::decode` だけである。`TrafBox::decode` も対応しないバージョンの `tfdt` は読み飛ばすが、未知の種別の子ボックスは `unknown_boxes` に保持する。

### 受理範囲にかかわる今の挙動

develop で次のことを確認した。

- 32 ビットの size=0 の子ボックスを読み飛ばす
  - 読み飛ばしの処理は `BoxHeader::decode_header_and_payload` を使う。そのため、size=0 の子ボックスは `mfra` のペイロードの末尾まで続くものとして読み飛ばされる
  - 確認では、`tfra`、`mfro`、size=0 の未知のボックスの順に並べた 71 バイトの `mfra` の decode が成功し、71 バイトを消費した
- `UnknownBox::decode` は、size=0 を `ErrorKind::InvalidData`（`UnknownBox does not accept size=0`）で拒否する
  - `UnknownBox::decode_top_level` の doc は、可変長のサイズが仕様上有効なのはファイル末尾のトップレベルボックスに限るとしている
- 対応しないバージョンの `tfra` は `tfra_boxes` にも入らず、捨てられる
  - これは `TrafBox` が対応しないバージョンの `tfdt` を `unknown_boxes` に入れないのと同じ扱いである
  - そうした `tfra` のサイズが 0 の場合は `ErrorKind::InvalidData` になる
- `mfro` が 2 つある場合は後のものが残る
  - 確認では、`size` が 111 と 222 の `mfro` を順に並べると、decode した `mfro_box.size` は 222 になった

## 設計方針

- `MfraBox` に公開フィールド `unknown_boxes: Vec<UnknownBox>` を追加する
  - doc は `MoofBox::unknown_boxes` と同じく「上記のいずれにも該当しなかった子ボックス群（未知の box_type を含む）」とする
  - `MfraBox` には `#[non_exhaustive]` が付いていないため、フィールドの追加は構造体リテラルで組み立てるコードを壊す。このクレートでは `Fmp4SegmentMuxer::mfra_bytes` がそれにあたり、空の `Vec` を入れる
  - 後方互換のない変更なので、`CHANGES.md` には `[CHANGE]` として書く。未知の子ボックスが失われていた不具合の修正であることもあわせて書く
- decode では、`tfra`（対応するバージョンのもの）と `mfro` 以外の子ボックスを、ほかのコンテナと同じく `UnknownBox::decode_at` で読んで `unknown_boxes` に入れる
  - このため、これまで読み飛ばしていた 32 ビットの size=0 の子ボックスは `ErrorKind::InvalidData` になる
  - size=0 のまま保持すると、encode でその子ボックスが `mfro` より前に size=0 で書かれる。読み手はそれを末尾まで続くボックスとして読むため、`mfro` を見つけられなくなる
  - サイズ 0 の対応しないバージョンの `tfra` を `ErrorKind::InvalidData` にする今の扱いとも整合する
- 次の扱いは変えない
  - 対応しないバージョンの `tfra` は読み飛ばし、`unknown_boxes` にも入れない
  - `mfro` は `unknown_boxes` に入れない。`mfro` が複数ある場合に後のものを採る扱いも変えない
    - 前の `mfro` を `unknown_boxes` に入れると、encode でそれが入力の `size` のまま最後の `mfro` より前に書かれ、実サイズと異なる `mfro.size` が出力に残る
- encode では、通常ヘッダー、`tfra_boxes`、`unknown_boxes`、`mfro` の順に書く
  - `mfro` を最後に書き、末尾 4 バイトを `mfra` 全体のバイト数で上書きする今の処理は変えない
  - 入力で `mfro` の後ろにあった子ボックスは、encode では `mfro` の前に移る
  - `tfra` と未知の子ボックスの間の相対的な順序は保たない。これは既知の子ボックスの後に `unknown_boxes` を書く `MoofBox` や `TrafBox` と同じ扱いである
- `impl BaseBox for MfraBox` の `children` は、`tfra_boxes`、`unknown_boxes`、`mfro` の順に返す
- 未知の子ボックスを読み飛ばす前提で書かれた記述を直す
  - `MfraBox::encode` のコメントにある、`mfro_box.size` を使わない理由のうち「decode で読み飛ばした未知の子ボックス」を外す。64 ビットサイズのヘッダーの 32 ビット化の理由は残す
  - `MfraBox::mfro_box` の doc は、decode → encode → decode で値が変わりうることを書いている。この記述が未知の子ボックスの消失を前提にしていないか見直す

### 関連 issue

- issue 0136（`MfraBox` から `mfro_box` フィールドを外す）は、同じ `MfraBox` の公開フィールドを変える
- どちらも単独で成立し、先に入った方に後の方を合わせればよい
- 0136 が先に入った場合、上の `MfraBox::mfro_box` の doc の見直しは不要になる

## 完了条件

- `pbt/tests/prop_fmp4_boxes.rs` の `mfra_box_decode_encode_rewrites_mfro_size` を書き換える
  - 今は「`free` が decode で読み飛ばされ、出力が `free` のバイト数だけ短くなる」ことを確かめている
  - 書き換え後は、`free` が `unknown_boxes` に入り、出力の長さが入力の長さと等しくなることを確かめる
  - 出力が `tfra` 列、`free`、`mfro` の順に並び、末尾の `mfro.size` が出力のバイト数と一致することも確かめる
- 次のことを PBT で確認する
  - `tfra` 列の任意の位置に、`tfra` と `mfro` 以外の種別の子ボックスを 0 個以上挟んだ `mfra` を decode すると、挟んだ子ボックスが入力の順で `unknown_boxes` に入る
  - その `MfraBox` を encode して decode すると、`tfra_boxes` と `unknown_boxes` が元と等しい
- 入力で `mfro` の後ろにある未知の子ボックスについて、単体テストで次のことを確認する
  - decode では `unknown_boxes` に入る
  - encode では `mfro` の前に書かれる
  - 出力の末尾の `mfro.size` が出力のバイト数と一致する
- 32 ビットの size=0 の未知の子ボックスを含む `mfra` の decode が `ErrorKind::InvalidData` を返すことを、単体テストで確認する
- `mfro` を 2 つ含む `mfra` の decode で、`unknown_boxes` に `mfro` が入らないことを単体テストで確認する
- `mfra_box_skips_unsupported_version_tfra` が成功し、対応しないバージョンの `tfra` が `unknown_boxes` に入らないことを確認する
- `Fmp4SegmentMuxer::mfra_bytes` の出力のバイト列が変わらず、`mfra_bytes_roundtrip` 系の PBT が成功する
- `CHANGES.md` の `## develop` に `[CHANGE]` を追加する
- `cargo test --workspace` が成功する
