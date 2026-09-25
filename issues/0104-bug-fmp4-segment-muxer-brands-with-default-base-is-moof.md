# `Fmp4SegmentMuxer` が `default-base-is-moof` を立てたまま `ftyp` の互換 brand に `isom` / `avc1` を入れている

- Created: 2026-09-25
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-segment-muxer-brands-with-default-base-is-moof
- Polished: {YYYY-MM-DD}

## 目的

`Fmp4SegmentMuxer` が出力する init セグメントの `ftyp` を、ISO/IEC 14496-12:2022 に合わせる。

`Fmp4SegmentMuxer` はすべての `tfhd` で `default-base-is-moof` フラグを立てる。仕様では、このフラグは `iso5` より前の brand を含むファイルでは使ってはならない。ところが `ftyp` の互換 brand には `isom` と、H.264 を含む場合は `avc1` が入っており、仕様に反する出力になっている。

## 現状

- `src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::build_ftyp`: `major_brand` は `iso5`、`compatible_brands` は `isom` / `iso5` / `iso6` / `mp41` に、使っているサンプルエントリーに応じて `avc1` / `hev1` / `hvc1` / `av01` を足す
- `Fmp4SegmentMuxer::build_moof`: すべての `tfhd` で `default_base_is_moof` を true にする
- ISO/IEC 14496-12:2022
  - 8.8.7.1: `default-base-is-moof` フラグは、`iso5` より前の brand や互換 brand では使ってはならない（shall not）。NOTE で、このフラグは以前の brand と互換がないため、以前の brand が FileTypeBox に含まれるときは立てられないとしている
  - 附属書 E の `isom`（E.2）、`avc1`（E.3）、`iso2`（E.4）、`iso3`（E.6）、`iso4`（E.7）は、いずれも NOTE で、その brand を付けたファイルでは `default-base-is-moof` フラグを立てられないとしている
- 既存の実装: FFmpeg の `libavformat/movenc.c` の `mov_write_ftyp_tag` は、`default_base_moof` を使う場合は「iso5 より前の brand は付けられない」として `isom` / `iso2` / `avc1` を書かない（`mp41` は書く）

## 設計方針

- `build_ftyp` の `compatible_brands` から `isom` と `avc1` を外す
- `hev1` / `hvc1` / `av01` / `mp41` は ISO/IEC 14496-12 では定義されておらず、同じ制約を確認できないため変えない
- 関連 issue: issue 0050（pending）は、字幕系の brand を `build_ftyp` に足す。同じ関数を変えるが、目的は別である

## 完了条件

- `Fmp4SegmentMuxer` の init セグメントの `ftyp` の `compatible_brands` に、`isom` と `avc1` が含まれない
- ほかの brand（`iso5` / `iso6` / `mp41` と、サンプルエントリーに応じた `hev1` / `hvc1` / `av01`）は今と同じ

## 解決方法

- `src/mux_fmp4_segment.rs` の `build_ftyp` を変更し、brand を選ぶ理由（8.8.7.1 と附属書 E の NOTE）をコードコメントに書く
- テスト
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: 任意のサンプルエントリーの組み合わせで init セグメントを作り、`ftyp` の `compatible_brands` が完了条件どおりになることを確認するプロパティを追加する
- `CHANGES.md` に `[FIX]` として記載する。init セグメントの `ftyp` のバイト列が変わることも書く
