# `TrunBox` が `first_sample_flags` とサンプルごとの `flags` を同時に書ける

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-trun-first-sample-flags-and-sample-flags
- Polished: 2026-10-06

## 目的

`first_sample_flags` とサンプルごとの `flags` が両方ある `TrunBox` をエンコードしない。ISO/IEC 14496-12:2022 が同時指定を禁止している組み合わせのファイルを、公開 API から書けないようにする。

## 現状

ISO/IEC 14496-12:2022 の 8.8.8.1 は、`first-sample-flags-present` を使うなら `sample-flags-present` を立ててはならない、としている。このフラグは、先頭サンプルについて既定のフラグを上書きする。

`src/boxes_fmp4.rs` の `TrunBox::compute_flags` は、`first_sample_flags` が `Some` なら `FLAG_FIRST_SAMPLE_FLAGS_PRESENT` を立て、いずれかのサンプルの `flags` が `Some` なら `FLAG_SAMPLE_FLAGS_PRESENT` を立てる。両方立つ。`validate_sample_option_consistency` が見るのは、サンプル間で `Option` の有無が揃っているかだけである。`Encode for TrunBox` はその検証のあとで両方のフィールドを書く。

デコード側の `Fmp4SegmentDemuxer` は、サンプル 0 では `first_sample_flags` を優先する。両方書かれたファイルは、サンプル 0 の per-sample `flags` を読まない。

`Fmp4SegmentMuxer::build_moof` は `first_sample_flags: None` で、サンプルの `flags` は `Some` にする。この muxer の出力はこの組み合わせにならない。公開の `TrunBox` を直接 encode する経路で書ける。

この組み合わせを前提にした既存テストがある。`tests/test_boxes_fmp4.rs` の `trun_box_multiple_samples` は `first_sample_flags` とサンプルごとの `flags` を両方 `Some` にして encode の成功と roundtrip を検証しており、`pbt/tests/prop_fmp4_boxes.rs` の `arb_trun_box` も両者を独立に抽選するため、`trun_box_roundtrip` をはじめとする PBT がこの組み合わせを生成する。今回の変更後は、これらのテストを新しい規則（両方 `Some` は `InvalidInput`）に合わせるか、この組み合わせを生成しないようにする必要がある。

## 設計方針

- `first_sample_flags` が `Some` で、かついずれかのサンプルの `flags` が `Some` のとき、`Encode for TrunBox` が `ErrorKind::InvalidInput` を返す
- 検証は `validate_sample_option_consistency` と同じタイミング（ヘッダを書く前）に置く
- デコードは、既存ファイルを読むために今の優先順のままにする。この issue では demux の解釈は変えない

## 完了条件

- `first_sample_flags` とサンプルの `flags` が両方 `Some` の `TrunBox` の encode が `InvalidInput` になる
- `first_sample_flags` だけが `Some` の encode と、サンプルの `flags` だけが `Some` の encode は成功する
- `Fmp4SegmentMuxer` が書く `trun` のバイト列は変わらない
