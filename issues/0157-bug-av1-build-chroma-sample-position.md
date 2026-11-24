# `build_av01_box` で 4:2:0 以外と monochrome の構成の 0 でない `chroma_sample_position` を拒否する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-av1-build-chroma-sample-position
- Polished: {YYYY-MM-DD}

## 目的

`build_av01_box` で、`chroma_sample_position` が 0 でなければならない構成の `Av1SequenceHeader` を拒否する。対象は、monochrome の構成と、monochrome でない 4:2:0 以外の構成である。

今は 0 でない値をそのまま `av1C` に書き出すため、AV1 Codec ISOBMFF Binding v1.3.0 の 2.3.4 に反する `av1C` を作れる。

## 現状

[AV1 の 5.5.2 Color config syntax](https://aomediacodec.github.io/av1-spec/#color-config-syntax) の color_config では、`chroma_sample_position` は次のように決まる。

- `mono_chrome` が 1 の場合、`subsampling_x = 1`、`subsampling_y = 1` とともに `chroma_sample_position = CSP_UNKNOWN` を代入する。[6.4.2 Color config semantics](https://aomediacodec.github.io/av1-spec/#color-config-semantics) の表では、`CSP_UNKNOWN` は 0 である
- `mono_chrome` が 0 の場合、`chroma_sample_position` を `f(2)` で読むのは `subsampling_x && subsampling_y`（4:2:0）のときだけである。それ以外（sRGB の 4:4:4、profile 1 の 4:4:4、profile 2 の 4:2:2 など）では、読みも代入もしない

[AV1 ISOBMFF Binding v1.3.0 の 2.3.4](https://aomediacodec.github.io/av1-isobmff/v1.3.0.html#av1codecconfigurationbox-semantics) は、`av1C` の `chroma_sample_position` を Sequence Header OBU の値と等しいとする。続けて次のように定める。

> When not specified in the Sequence Header OBU, and not defined by the conditions specified in the [AV1] color_config, the values of chroma_subsampling_x, chroma_subsampling_y, and chroma_sample_position SHALL be 0.

このため、`av1C` の値は次のとおりどちらも 0 になる。

- monochrome の構成では、color_config が代入する 0 になる
- monochrome でない 4:2:0 以外の構成では、上の SHALL により 0 になる

`src/bitstream/av1.rs`:

- `read_color_config`（`parse_sequence_header` が使う）は、monochrome の場合と、`subsampling_x == 1 && subsampling_y == 1` でない場合に 0 を返す。構文どおりにしか 0 以外の値を返さない
- `build_av01_box` は `seq.chroma_sample_position` を 0..=2 の範囲でだけ検査し（予約値 3 を拒否する）、`chroma_subsampling_x` / `chroma_subsampling_y` / `monochrome` との組み合わせは検査しない。値はそのまま `Av1cBox::chroma_sample_position` に写す
- `config_obus` に Sequence Header OBU がある場合、`build_av01_box` は解析結果と `seq` を照合する。解析結果は上の条件で 0 なので、0 でない値は不一致として拒否される。照合を経ないのは、`config_obus` に Sequence Header OBU がない場合である
- `Av1SequenceHeader::chroma_sample_position` の doc は「(0..=2)。構文に現れないときは 0。」と書くが、`build_av01_box` がこの条件を検査するとは書いていない

再現手順（develop で確認）。いずれも `config_obus` は空とし、他の欄は 320x240 の正常値とする。

- `seq_profile = 1`、`chroma_subsampling_x = 0`、`chroma_subsampling_y = 0`（4:4:4）、`chroma_sample_position = 1`: `Ok` が返り、`av1C` の `chroma_sample_position` は 1 になる
- `seq_profile = 2`、`high_bitdepth = true`、`chroma_subsampling_x = 1`、`chroma_subsampling_y = 0`（4:2:2）、`chroma_sample_position = 2`: `Ok` が返り、2 になる
- `seq_profile = 0`、`monochrome = true`、`chroma_subsampling_x = 1`、`chroma_subsampling_y = 1`、`chroma_sample_position = 2`: `Ok` が返り、2 になる

`build_av01_box` は、同じ構図の `twelve_bit`（Sequence Header の構文に現れない条件で真になっているもの）をすでに拒否している。そのコメントには、拒否する理由として「黙って 0 に置き換えると誤りを隠すことになるため、置き換えずに拒否する」と書かれている。

## 設計方針

- `build_av01_box` で、`seq.chroma_subsampling_x == 1 && seq.chroma_subsampling_y == 1 && !seq.monochrome` が成り立たないのに `seq.chroma_sample_position != 0` である入力を、`ErrorKind::InvalidInput` で拒否する
  - 既存の 0..=2 の範囲検査の後、`Av1cBox` の構築前に置く。範囲外の値が既存の理由で拒否されるようにし、拒否理由を一意にするためである
- 0 に正規化せず、拒否する。判断材料は次のとおり
  - `parse_sequence_header` はこの条件で必ず 0 を返す。そのため 0 でない値は、呼び出し側が `Av1SequenceHeader` を手で組み立てた場合にしか生じない
  - `config_obus` に Sequence Header OBU がある場合は、すでに照合の不一致で拒否している。拒否にそろえれば、挙動が変わるのは Sequence Header OBU がない場合だけになる
  - `twelve_bit` と同じ扱いになる。正規化すると呼び出し側の構築誤りを隠し、渡した `Av1SequenceHeader` と `av1C` の値が食い違う
- 根拠（AV1 spec の 5.5.2 と 6.4.2、Binding v1.3.0 の 2.3.4）と、将来の改訂で変わる可能性があることをコードコメントに書く
- `build_av01_box` の doc のエラー条件と、`Av1SequenceHeader::chroma_sample_position` のフィールド doc に、この検査を追記する
- 次のものは対象外とする
  - monochrome とサブサンプリングの整合（monochrome なら (1, 1)）や、プロファイルとサブサンプリングの整合など、他の組み合わせの検査
  - `parse_sequence_header`、および `Av1cBox` の直接のエンコード・デコードの変更

## 完了条件

- `tests/test_bitstream_av1.rs` の `mod build` で、次の 3 つの構成を確かめる。いずれも、既存のヘルパー `config_obus_without_sequence_header` が返す 2 通りの `config_obus` で確かめる
  - 構成は、monochrome（profile 0、(1, 1)）、monochrome でない 4:4:4（profile 1、(0, 0)）、4:2:2（profile 2、`high_bitdepth`、(1, 0)）の 3 つ
  - `chroma_sample_position = 0` は構築でき、`av1C` に 0 が写る
  - `chroma_sample_position` が 1 と 2 の場合は `ErrorKind::InvalidInput` で拒否され、理由の文字列がこの検査のものである
- 4:2:0 で 1 と 2 を構築できることを確かめている既存のテストが成功する（`out_of_range_chroma_sample_position_rejected` の対照と、`build_av01_box_matches_sequence_header` PBT）
- `cargo test --workspace` が成功する
