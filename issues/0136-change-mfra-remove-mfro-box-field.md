# `MfraBox` から、エンコードに使われない `mfro_box` フィールドを外す

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/change-mfra-remove-mfro-box-field
- Polished: {YYYY-MM-DD}

## 目的

`MfraBox` の公開フィールド `mfro_box` を外し、エンコード結果に影響しない値の指定を利用者に求めないようにする。これにより、`mfra` を decode → encode → decode したときに `MfraBox` の値が変わらなくなる。

## 現状

`src/boxes_fmp4.rs` の `MfraBox` は、公開フィールド `tfra_boxes: Vec<TfraBox>` と `mfro_box: MfroBox` を持つ。`#[non_exhaustive]` は付いていないため、構造体リテラルで組み立てるには `mfro_box` の値が必要になる。

`MfraBox::encode` は `self.mfro_box` を使わない。各 `tfra` の後に `MfroBox { size: 0 }` を書き、`BoxHeader::finalize_box_size` で `mfra` のサイズを確定する。その後、末尾 4 バイト（`mfro.size` フィールド）を `mfra` 全体のバイト数で上書きする。`MfraBox::mfro_box` と `MfroBox::size` の doc にも、エンコード時は値を使わないことが書かれている。

`MfraBox::decode` は、入力の `mfro.size` の値をそのまま `mfro_box` に入れる。`mfro` が複数あれば後のものが残り、1 つもなければ `ErrorKind::InvalidData`（`Missing mandatory 'mfro' box in 'mfra' box`）を返す。

このため、入力の `mfro.size` がエンコード後の実サイズと異なると、decode → encode → decode で `mfro_box.size` が変わる。`mfro_box` の doc にもその旨が書かれている。実サイズと異なる場合は次のとおり。

- decode で読み飛ばした未知の子ボックスがあり、出力が短くなる場合
- 64 ビットサイズ（largesize）のヘッダーを持つ `mfra` を 32 ビットのヘッダーで書き直し、出力が短くなる場合
  - 確認では、largesize のヘッダーと `tfra` 1 個、`mfro` を持つ 67 バイトの `mfra` が 59 バイトで出力され、末尾の `mfro.size` は 59 になった
- 入力の `mfro.size` の値そのものが誤っている場合

### `mfro_box` を参照している箇所

`src/` で `mfro_box` を読むのは、`impl BaseBox for MfraBox` の `children` だけである。`tfra_boxes` の各要素の後に `&self.mfro_box` を返す。

`children` を通じた汎用の走査は、次の箇所が使っている。

- `examples/dump_wasm/src/lib.rs` のボックスの木の出力
- `tests/test_decode_encode.rs` の `collect_unknown_box_types`

`mfro_box` を設定するのは `src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::mfra_bytes` だけである。`MfroBox { size: 0 }` を入れ、出力に使われない旨をコメントに書いている。

テストでは、次の 2 ファイルが `mfro_box.size` を読む。

- `pbt/tests/prop_fmp4_boxes.rs`: `mfra_box_skips_unsupported_version_tfra` と `mfra_box_decode_encode_rewrites_mfro_size`
- `pbt/tests/prop_fmp4_segment_mux_demux.rs`: `mfra_bytes_roundtrip`、`mfra_bytes_roundtrip_with_sidx_mix`、`mfra_bytes_roundtrip_with_sidx_mix_multi_track`
  - いずれも `mfra_box.mfro_box.size` が `mfra` のバイト数と一致することを確かめている

### 影響しない箇所

- `Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` は `mfra` をデコードしない
  - `Fmp4FileDemuxer::read_top_level_box_header` は `moof` 以外のトップレベルボックスを読み飛ばす
  - `Fmp4SegmentDemuxer::handle_media_segment` は `moof` と `mdat` 以外のトップレベルボックスを種別を問わず読み飛ばす
- `crates/wasm/src/fmp4_segment_mux.rs` が公開するのは、`mfra_bytes` のバイト列を返す `mfra` メソッドだけで、`MfraBox` は公開していない
- `examples/fmp4.rs` は `mfra_bytes` の出力の末尾 4 バイトを読んで確認しており、`MfraBox` のフィールドを使わない
- fuzz ターゲットの `fuzz_mfra_box` と `fuzz_mfro_box` は decode と encode を呼ぶだけで、フィールドを参照しない

### `MfroBox` の位置づけ

`MfroBox` は `src/boxes.rs` から公開されており、単体でも encode と decode ができる。doc には「ファイル末尾から逆方向にシークするために使用される」とあり、ファイル末尾の 16 バイトから `mfra` の先頭を求める読み手が単体で使う。

## 設計方針

- `MfraBox` から `mfro_box` フィールドを削除する。公開フィールドの削除なので、`CHANGES.md` には `[CHANGE]` として書く
- `MfraBox::encode` の出力は変えない
  - 各 `tfra` の後に `mfro` を 1 つ書き、その `size` に `mfra` 全体のバイト数を書く今の処理をそのまま使う
- `MfraBox::decode` の受理範囲は変えない
  - これまでどおり子の `mfro` を `MfroBox::decode` で読む。`mfro` の構文の誤りは今と同じエラーになる
  - `mfro` が 1 つもなければ `ErrorKind::InvalidData` を返す
  - 読んだ `mfro.size` の値は保持しない
  - 入力の `mfro.size` を確かめたい利用者は、`mfra` の末尾 16 バイトを `MfroBox::decode` で読む。この方法を `MfraBox` の doc に書く
- `impl BaseBox for MfraBox` の `children` は、引き続き最後に `mfro` を 1 つ返す
  - エンコード結果には必ず `mfro` が 1 つ含まれるため、走査の結果をバイト列の子ボックスの構成と一致させる
  - フィールドがなくなるので、定数の `MfroBox`（`size` は 0）への参照を返す
  - `BaseBox` が公開するのは `box_type`、`is_unknown_box`、`children` だけなので、走査から `size` の値は観測できない。この扱いを `children` の実装のコメントに書く
- `MfroBox` は公開型として残し、単体の encode と decode の挙動も変えない
  - `MfroBox::size` の doc から `MfraBox::mfro_box` への参照を外す
  - 代わりに「`MfraBox` のエンコードでは、エンコード後の `mfra` 全体のバイト数を書く」と書く
- `Fmp4SegmentMuxer::mfra_bytes` から `MfroBox` の指定と、そのコメントを削除する。出力のバイト列は変わらない

### 採らなかった案

- `mfro_box` を残し、doc で値を使わないと明記する（現状のまま）
  - 構造体リテラルで、意味のない値の指定を求め続けることになる
  - decode → encode → decode で値が変わる状態も残る
- `mfro_box` を非公開にし、decode した値を読むアクセサを設ける
  - このクレートのボックス型は、公開フィールドと構造体リテラルで組み立てる形にそろっている
  - `MfraBox` だけ、コンストラクタが必要になる
- `children` から `mfro` を外す
  - エンコード結果には必ず `mfro` があるのに、`examples/dump_wasm` などの走査の結果から消える

### 関連 issue

- issue 0137（`MfraBox` で未知の子ボックスを保持する）は、`MfraBox` に公開フィールド `unknown_boxes` を追加する
- 同じ構造体の公開フィールドを変えるが、どちらの issue も単独で成立し、先に入った方に後の方を合わせればよい
- 0137 で未知の子ボックスを保持しても、largesize のヘッダーを持つ `mfra` と `mfro.size` の値が誤っている入力では、入力の `mfro.size` と出力の実サイズが食い違う
  - そのため、本 issue は 0137 とは別に必要である

## 完了条件

- `MfraBox` に `mfro_box` フィールドがない
- `Fmp4SegmentMuxer::mfra_bytes` の出力のバイト列が変わらない
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs` の `mfra_bytes_roundtrip` 系 3 つを書き換えて確認する
  - `mfra_box.mfro_box.size` の比較は、出力の末尾 16 バイトを `MfroBox::decode` で読んだ `size` の比較に置き換える
- `pbt/tests/prop_fmp4_boxes.rs` の `mfra_box_decode_encode_rewrites_mfro_size` を書き換える
  - `decoded.mfro_box.size` を確かめる箇所は削除する
  - 出力の末尾 16 バイトを `MfroBox::decode` で読んだ `size` が出力のバイト数と一致することを確かめる
  - 入力の `mfro.size` が実サイズと異なる場合も、decode した値と再デコードした値が等しいことを確かめる
- `mfra_box_skips_unsupported_version_tfra` から `decoded.mfro_box.size` の比較を削除し、`tfra_boxes` の比較は残す
- `mfro` を含まない `mfra` の decode が `ErrorKind::InvalidData` を返すことを、`tests/test_boxes_fmp4.rs` の単体テストで確認する
- `MfraBox::children` が `tfra_boxes` の各要素の後に、種別が `mfro` の子ボックスを 1 つ返すことをテストで確認する
- `MfraBox` の doc に、エンコードでは `mfro.size` に `mfra` 全体のバイト数を書くこと、入力の `mfro.size` は末尾 16 バイトを `MfroBox::decode` で読めば確かめられることが書かれている
- `MfroBox::size` の doc が `MfraBox::mfro_box` を参照していない
- `CHANGES.md` の `## develop` に `[CHANGE]` を追加する
- `cargo test --workspace` が成功し、`fuzz/` で `cargo check --all-targets` が成功する
