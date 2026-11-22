# `build_av01_box` で予約された色差サンプル位置と不整合な `twelve_bit` を拒否する

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
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
