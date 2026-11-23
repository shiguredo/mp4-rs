# `build_av01_box` で予約された色差サンプル位置と不整合な `twelve_bit` を拒否する

- Created: 2026-11-22
- Completed: 2026-11-24
- Branch: feature/fix-av1-build-reserved-fields
- Polished: 2026-11-23

## 目的

`build_av01_box` で `chroma_sample_position` の予約値と、Sequence Header の構文に現れない条件で真になっている `twelve_bit` を拒否する。対象はこの 2 項目の生成時の検査に限る。

## 現状

[AV1 の 6.4.2 Color config semantics](https://aomediacodec.github.io/av1-spec/#color-config-semantics) は色差サンプル位置の 3 を `CSP_RESERVED` とする。[5.5.2 Color config syntax](https://aomediacodec.github.io/av1-spec/#color-config-syntax) では `twelve_bit` は `seq_profile == 2` かつ `high_bitdepth == true` の場合だけ現れる。[AV1 ISOBMFF Binding v1.3.0 の 2.3.4](https://aomediacodec.github.io/av1-isobmff/v1.3.0.html#av1codecconfigurationbox-semantics) は、Sequence Header OBU に `twelve_bit` がない場合、`av1C` の値を 0 とする。

`src/bitstream/av1.rs` の `parse_sequence_header` は、`read_color_config` で `CSP_RESERVED` を `ErrorKind::InvalidInput` として拒否し、構文に現れない `twelve_bit` は `false` とする。`build_av01_box` は `config_obus` に Sequence Header OBU があれば解析して引数の `seq` と照合するが、公開型 `Av1SequenceHeader` は Sequence Header OBU がなくても渡せる。`seq` に対する検査では `chroma_sample_position` をビット幅だけで判定し、予約値を除外しない。また `twelve_bit` の構文条件も検査しない。

## 設計方針

- `build_av01_box` では `chroma_sample_position` が `0..=2` に収まらない入力を `ErrorKind::InvalidInput` とする。`twelve_bit` が真の場合は、`seq_profile == 2` かつ `high_bitdepth == true` を満たさなければ同じエラーを返す。どちらも `Av1cBox` の構築前に検査し、`build_av01_box` の公開ドキュメントのエラー条件に記載する
- 入力を丸めたり 0 に置き換えたりせず、正常値の写像は維持する。`parse_sequence_header` と `config_obus` の照合、`Av1cBox` の直接のエンコード・デコード、およびそれ以外の項目の検査は変更しない

## 完了条件

- `tests/test_bitstream_av1.rs` の公開 API のテストで、正常な設定から片方ずつ条件を外し、予約された色差サンプル位置と、構文に現れない `twelve_bit` がそれぞれ `ErrorKind::InvalidInput` になることを確認する。`twelve_bit` の検査は、プロファイルと `high_bitdepth` の各条件が欠ける場合を別々に確認する。`config_obus` に Sequence Header OBU を含めず、引数の `Av1SequenceHeader` 自体が検査されることを確認する
- 既存の正常系 `build_av01_box_matches_sequence_header` PBT に、色差サンプル位置が非ゼロの正常値となる 4:2:0 の profile 2・12 ビットの生成を追加する。`Av01Box` を構築でき、`av1C` の `high_bitdepth`、`twelve_bit`、`chroma_sample_position` が解析結果と一致することを確認する
- `cargo test --workspace` が成功する

## 解決方法

- `src/bitstream/av1.rs` の `build_av01_box` に、`Av1cBox` の構築前の検査を 2 つ追加した。どちらも `ErrorKind::InvalidInput` を返し、入力を丸めたり 0 に置き換えたりしない
  - `seq.chroma_sample_position` が 0..=2 の範囲外 (予約値 3 = `CSP_RESERVED` を含む) なら拒否する。これまでの検査は 2 ビットの幅 (0..=3) だけだった
  - `seq.twelve_bit` が真で、`seq.seq_profile` が 2 でないか `seq.high_bitdepth` が偽なら拒否する
  - 根拠 (AV1 spec §6.4.2、§5.5.2、Binding v1.3.0 §2.3.4) と、将来の改訂で変わる可能性があることをコードコメントに残した
- `build_av01_box` の doc のエラー条件に 2 つの検査を追記した。あわせて、既存の検査のうち列挙から漏れていた「`config_obus` 内の Sequence Header の解析に失敗する」と「`seq.seq_profile` が 0..=2 の範囲外」を加え、いずれの場合も `InvalidInput` を返すことと、`config_obus` に Sequence Header OBU がない場合も `seq` を検査することを書いた
- `Av1SequenceHeader` の `twelve_bit` / `chroma_sample_position` のフィールド doc に、`build_av01_box` も拒否することを書いた
- `parse_sequence_header`、`config_obus` の照合、`Av1cBox` のエンコード・デコード、それ以外の項目の検査は変えていない。`config_obus` に Sequence Header OBU がある場合は、これまでも `config_obus` の検査、解析、`seq` との照合のいずれかでこれらの値を拒否していたため、挙動が変わるのは Sequence Header OBU がない場合だけである
- テスト
  - `tests/test_bitstream_av1.rs` の `mod build` に 2 件を追加した。`config_obus` は Sequence Header OBU を含まない 2 通り (空と、Metadata OBU だけ) で、エラー種別と理由の文字列まで照合する
    - `out_of_range_chroma_sample_position_rejected`: 正常値の上限 2 を対照に、3、4、255 を拒否する
    - `twelve_bit_outside_syntax_rejected`: 正常な設定 (profile 2、`high_bitdepth`、`twelve_bit`) から、プロファイルの条件 (0 と 1) と `high_bitdepth` の条件を片方ずつ外して拒否する
  - `pbt/tests/prop_bitstream_av1.rs` の Sequence Header の生成器を `ReducedStillColor` で一般化し、`build_av01_box_matches_sequence_header` で profile 0 の 8 ビット 4:2:0 と、色差サンプル位置が 1 または 2 の profile 2 の 12 ビット 4:2:0 を半々に生成するようにした。av1C の `high_bitdepth`、`twelve_bit`、`chroma_sample_position` が解析結果と一致することを確かめる
- `CHANGES.md` の `## develop` に [FIX] を追記した
