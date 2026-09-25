# `Fmp4SegmentMuxer` が `default-base-is-moof` を立てたまま `ftyp` の互換 brand に `isom` / `avc1` を入れている

- Created: 2026-09-25
- Completed: 2026-09-25
- Branch: feature/fix-fmp4-segment-muxer-brands-with-default-base-is-moof
- Polished: 2026-09-25

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
- `hev1` / `hvc1` / `av01` / `mp41` は ISO/IEC 14496-12 で定義される brand ではない（`hev1` / `hvc1` は ISO/IEC 14496-15、`mp41` は ISO/IEC 14496-14、`av01` は AV1 ISOBMFF 仕様）。この issue では ISO/IEC 14496-12 の brand だけを対象にするため、これらは変えない
- 関連 issue: issue 0050（pending）は、字幕系の brand を `build_ftyp` に足す。同じ関数を変えるが、目的は別である

## 完了条件

- `Fmp4SegmentMuxer` の init セグメントの `ftyp` の `compatible_brands` に、`isom` と `avc1` が含まれない
- ほかの brand（`iso5` / `iso6` / `mp41` と、サンプルエントリーに応じた `hev1` / `hvc1` / `av01`）は今と同じ

## 解決方法

- `src/mux_fmp4_segment.rs`
  - `build_ftyp` の `compatible_brands` から `Brand::ISOM` と `Brand::AVC1` を外した。`build_moof` がすべての `tfhd` で `default_base_is_moof` を true にするため、ISO/IEC 14496-12:2022 の 8.8.7.1 と附属書 E の NOTE に従い、この制約の対象になる `isom` と `avc1` を含めないようにした
  - `iso5` / `iso6` は制約の対象外であり、`mp41` は ISO/IEC 14496-14 の brand で 8.8.7.1 と附属書 E の NOTE が扱う ISO/IEC 14496-12 の brand ではないため、これらは従来どおり残した。根拠（資料名・節番号・将来の改訂で変わりうること）は `build_ftyp` のコメントに書いた
  - `has_avc1` の判定を削除した。H.264 のサンプルエントリーは `compatible_brands` に影響しなくなったためである
- テスト
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: `ftyp_compatible_brands_are_compatible_with_default_base_is_moof` を追加した。4 種類の映像サンプルエントリー（`avc1` / `hev1` / `hvc1` / `av01`）から空でない部分集合を選び、選んだサンプルエントリーごとに 1 サンプルのメディアセグメントを作る。1 セグメントには複数のサンプルエントリーを置けないため、複数のセグメントに分けてトラックへ観測させ、init セグメントの `stsd` に複数のサンプルエントリーが並ぶ状況を作る
  - 同テストで、init セグメントの `ftyp` の `compatible_brands` が、`iso5` / `iso6` / `mp41` とサンプルエントリーに応じた `hev1` / `hvc1` / `av01` だけを従来と同じ並びで含むことを確認する。音声（Opus）と字幕（Stpp）のトラックを加える組み合わせも含め、これらが brand に影響しないことも確認する
  - `avc1` は brand に影響しなくなったため、観測させた映像サンプルエントリーが init セグメントの `stsd` に含まれることと、`avc1` を選んだケースが 1 つ以上あったことをあわせて確認し、「観測させられなかった」場合と「観測したが意図どおり brand から外した」場合を区別できるようにした
  - `tests/test_mux_fmp4_segment.rs` には単体テストを追加しなかった。brand の組み合わせは PBT で網羅できるためである
  - 映像サンプルエントリーを 1 つも観測していない init セグメントについても、既存の `audio_only_roundtrip` で `compatible_brands` が `iso5` / `iso6` / `mp41` だけになることを確認するようにした
- `CHANGES.md` に `[FIX]` として記載した。init セグメントの `ftyp` のバイト列が変わることも書いた
- 関連 issue への申し送り: 字幕系 brand を追加する issue 0050 は「既存 Audio / Video のみの mux 生成物の `compatible_brands` が変わらない」ことを完了条件にしているが、本変更で `Fmp4SegmentMuxer` のその出力は `isom`（H.264 の場合は `avc1` も）が外れて変わる。0050 の着手時にこの変更後の brand 集合を基準にすること

### 確認したこと

- `src/mux_fmp4_segment.rs` の変更を `git stash push` で戻した状態では、`ftyp_compatible_brands_are_compatible_with_default_base_is_moof` が `compatible_brands` の比較で失敗し、`isom` と `avc1` が含まれることを確認した（`left: [Brand("isom"), Brand("iso5"), Brand("iso6"), Brand("mp41"), Brand("avc1"), Brand("hvc1"), Brand("av01")]`）
- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`cargo test -p c-api --lib`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ることを確認した
