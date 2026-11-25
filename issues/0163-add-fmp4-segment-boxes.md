# fMP4 のセグメント種別ボックス (`styp`) に対応する

- Created: 2026-11-25
- Completed: {YYYY-MM-DD}
- Branch: feature/add-fmp4-segment-boxes
- Polished: 2026-11-25

## 目的

`shiguredo_mp4` は fMP4 の中核ボックス (`moof` / `mfra` / `sidx` 系) と、CENC の初期化データ向けボックスを実装してきたが、`styp` (Segment Type Box) を持たない。

`styp` は DASH や CMAF のメディアセグメントで、セグメントの種別とブランドを示すために広く使われている。現状の `Fmp4SegmentMuxer` は `styp` を書き出せないため、CMAF などのセグメント形式に準拠した出力を作れない。

本 issue では `styp` を実装し、mux で書き出し、demux で型付きに扱えるようにする。

## 現状

- `src/boxes_fmp4.rs` に `MoofBox` / `MfhdBox` / `TrafBox` / `TfhdBox` / `TfdtBox` / `TrunBox` / `SidxBox` / `MfraBox` / `TfraBox` / `MfroBox` がある。
- `styp` の型は存在しない。
- `Fmp4SegmentDemuxer::handle_media_segment` は `moof` の前後にあるトップレベルボックスを「種別を問わず中身を解釈せずに読み飛ばす」。`styp` もこの経路で捨てられる。
- `Fmp4SegmentMuxer::create_media_segment_metadata()` の出力は `moof` + `mdat` ヘッダー、`create_media_segment_metadata_with_sidx()` の出力は `sidx` + `moof` + `mdat` ヘッダーである。
- `build_media_segment_bytes()` は `moof_relative_offset = self.media_bytes_written` を記録し、`mfra_bytes()` はそれを `init_segment_size` に足して `tfra.moof_offset` を計算する。`create_media_segment_metadata_with_sidx()` は `sidx` のサイズを `media_bytes_written` と `tfra` のエントリーに加算している。
- `Fmp4FileDemuxer` の `read_top_level_box_header()` は `moof` 以外のトップレベルボックスを読み飛ばし、`read_media_segment()` は `moof` から `mdat` 末尾までの範囲だけを内部の `Fmp4SegmentDemuxer` に渡す。
- `FtypBox` は `src/boxes.rs` にあり、`major_brand` / `minor_version` / `compatible_brands` を持つ。

## 設計方針

### 1. `styp` の構造と配置

ISO/IEC 14496-12:2022 8.16.2 の `styp` は `ftyp` と同じ構造で、Container はトップレベルである。`styp` は「セグメントの先頭ボックスでなければならない」とされており、先頭以外に現れたものは受信側が無視してよい。

- `StypBox` を `src/boxes_fmp4.rs` に追加し、`src/boxes.rs` で再エクスポートする
- フィールドは `FtypBox` と同じ `major_brand` / `minor_version` / `compatible_brands` にする
- `BaseBox` / `Encode` / `Decode` を実装する。`compatible_brands` のバイト数が 4 の倍数でない場合はデコードを `InvalidData` にする
- 実装コメントには、根拠 (ISO/IEC 14496-12:2022 8.16.2) と将来変更される可能性を明記する

### 2. mux の `styp` 書き出し

`SegmentMuxerOptions` に `styp: Option<StypBox>` を追加し、指定されたときだけ書き出す。既定は `None` で、既存の出力バイト列は変えない。

`styp` はセグメントの先頭に置く必要があるため、出力順は次のとおりとする。

- `create_media_segment_metadata()`: `styp` + `moof` + `mdat` ヘッダー
- `create_media_segment_metadata_with_sidx()`: `styp` + `sidx` + `moof` + `mdat` ヘッダー

`styp` のサイズは、既存の `sidx` と同じ扱いで次に反映する。

- 現在のセグメントの `tfra` エントリーの `moof_relative_offset` に `styp` のサイズを加算する
- `media_bytes_written` に `styp` のサイズを加算する
- `sidx.referenced_size` は `moof` と `mdat` の合計とし、`styp` を含めない。`styp` は `sidx` より前にあるため `sidx` の参照範囲に入らない
- `sidx.first_offset` は 0 とする。`sidx` の直後が `moof` であるため、既存の定義 (sidx 直後から最初のサブセグメント先頭までのバイト数) に従う
- `mfra_bytes()` の `tfra.moof_offset` が、`styp` を含む実際の `moof` 位置と一致しなければならない

`create_media_segment_metadata()` と `create_media_segment_metadata_with_sidx()` の doc、`mfra_bytes()` の doc を、`styp` を含む出力に合わせて更新する。

### 3. demux の `styp` 取得

`Fmp4SegmentDemuxer` に `pub fn styp(&self) -> Option<&StypBox>` を追加する。

- 初期化セグメント未処理のとき、および直前の成功したメディアセグメントに `styp` が無いときは `None`
- 採用するのは、そのメディアセグメントの先頭にある `styp` だけとする。2 個目以降の `styp` と `mdat` の後ろにある `styp` は無視する
- `moof` の前後にあるトップレベルボックスを読み飛ばす既定の挙動は維持する。ただし先頭の `styp` だけは型付きで読み、「ボックス全体が入力内にあり、デコードに成功した場合だけ」保持する。デコードに失敗した場合は従来どおり読み飛ばし、エラーにしない
- 保持する値の更新は、メディアセグメントの検証がすべて通った後にまとめて行う。`handle_media_segment()` がエラーを返した場合は内部状態を変えない (既存の契約を維持する)

`Fmp4FileDemuxer` への `styp()` 追加は行わない。`Fmp4FileDemuxer` は `moof` より前のトップレベルボックスを内部の `Fmp4SegmentDemuxer` に渡さないため、単純な委譲では値を返せない。実装するならファイルデマルチプレクサのフェーズを増やす設計が必要になり、本 issue の範囲を超える。

### 4. 公開 API の変更

`SegmentMuxerOptions` へのフィールド追加は後方互換でない。`..Default::default()` を使わない構造体リテラルを構築している利用者は修正が必要になる。

- `crates/wasm/src/fmp4_segment_mux.rs` の `SegmentMuxerOptions` リテラルに `styp: None` を追加する
- wasm の JSON API で `styp` を指定できるようにはしない (今回の対象外)。その旨を `CHANGES.md` に書く

### 5. テスト

- `tests/test_boxes_fmp4.rs`: 既知バイト列の encode / decode、box 種別違い、`compatible_brands` の端数
- `pbt/tests/prop_fmp4_boxes.rs`: encode / decode の往復
- `tests/test_mux_fmp4_segment.rs`: `styp` を指定したときの出力順、指定しないときの出力が変わらないこと
- `tests/test_demux_fmp4_segment.rs`: `styp()` の値、`styp` なしの場合、2 個目の `styp` を無視すること、不正な `styp` を読み飛ばすこと、`styp` を保持した後にメディアセグメントのデコードが失敗した場合に `styp()` が変わらないこと
- `tests/test_mux_fmp4_segment.rs`: `create_media_segment_metadata()` (`sidx` なし) に `styp` を指定した場合の `mfra` の `tfra.moof_offset` が実際の `moof` 位置と一致すること
- `pbt/tests/prop_fmp4_segment_mux_demux.rs`: `sidx` なしの `styp` + `moof` + `mdat` でも `tfra.moof_offset` が実際の `moof` 位置と一致すること
- `pbt/tests/prop_fmp4_segment_mux_demux.rs`: mux → demux の `styp` 往復、`styp` + `sidx` + `mfra` の `tfra.moof_offset` が実際の `moof` 位置と一致すること、`sidx.referenced_size` が `moof` + `mdat` と一致し `first_offset` が 0 になること、既存の読み飛ばし PBT とエラー時状態不変 PBT が通り続けること

## 完了条件

- `StypBox` の `Encode` / `Decode` / `BaseBox` が実装され、PBT と異常系テストが通る
- `SegmentMuxerOptions::styp` を指定したときの出力順が `styp` + `moof` + `mdat`、および `styp` + `sidx` + `moof` + `mdat` になる
- `styp` を指定しない場合の出力バイト列が既存と一致する
- `styp` を指定した場合の `mfra_bytes()` の `tfra.moof_offset` が、実際の `moof` の位置と一致する
- `styp` を指定した場合の `sidx.referenced_size` が `moof` と `mdat` の合計と一致し、`sidx.first_offset` が 0 になる
- `Fmp4SegmentDemuxer::styp()` が先頭の `styp` だけを返し、`styp` なしのセグメントでは `None` になる
- 不正な `styp` を持つセグメントが、従来どおり読み飛ばされてエラーにならない
- `handle_media_segment()` がエラーを返した場合に `styp()` の値が変わらない
- `crates/wasm` がコンパイルできる
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz --no-deps` が成功する
- `CHANGES.md`、`skills/shiguredo-mp4/SKILL.md` が更新されている
- `src/demux_fmp4_segment.rs` の `handle_media_segment` の公開 doc が、先頭の `styp` だけは型付きで読む旨に更新されている
- `src/mux_fmp4_segment.rs` の `create_media_segment_metadata` / `create_media_segment_metadata_with_sidx` / `mfra_bytes` の公開 doc が、`styp` を含む出力に合わせて更新されている
- `src/boxes.rs` の `Brand` の公開 doc が、`FtypBox` と `StypBox` の両方で使うことが分かる記述に更新されている
- `crates/wasm/src/fmp4_segment_mux.rs` の公開 doc が、wasm では `styp` を指定しないため戻り値に含まれない旨に更新されている
- `StypBox` の実装コメントに、根拠 (ISO/IEC 14496-12:2022 8.16.2) と将来変更される可能性が書かれている
- `tests/test_demux_fmp4_segment.rs` と `pbt/tests/prop_fmp4_segment_mux_demux.rs` の「`styp` の中身を解釈しない」というコメントが、先頭の `styp` だけは型付きで読む旨に更新されている

## 解決方法

- `src/boxes_fmp4.rs` に `StypBox` を追加する (`FtypBox` の実装を参考にする)
- `src/boxes.rs` の再エクスポートに `StypBox` を追加する
- `src/mux_fmp4_segment.rs` の `SegmentMuxerOptions` に `styp` を追加し、`create_media_segment_metadata` 系でセグメント先頭に書き出す。`styp` のサイズは `moof_relative_offset`、`media_bytes_written`、新規 `tfra` エントリーに加算する。`sidx.referenced_size` / `sidx.first_offset` には加算しない (`styp` は `sidx` より前にあるため参照範囲に入らない)
- `src/mux_fmp4_segment.rs` の doc (`create_media_segment_metadata` / `create_media_segment_metadata_with_sidx` / `mfra_bytes`) を更新する
- `src/demux_fmp4_segment.rs` で先頭の `styp` を型付きで読み、`styp()` を追加する。デコードに失敗した場合は読み飛ばす
- `src/demux_fmp4_segment.rs` の `handle_media_segment` の公開 doc にある「`styp` を含むトップレベルボックスは中身を解釈せずに読み飛ばす」という記述を、先頭の `styp` だけは型付きで読む旨に更新する
- `crates/wasm/src/fmp4_segment_mux.rs` の公開 doc にある「`styp` を含む場合はそれを先頭に返す」という記述を、wasm では `styp` を指定しないため返らない旨に更新する
- `tests/test_demux_fmp4_segment.rs` と `pbt/tests/prop_fmp4_segment_mux_demux.rs` の「demuxer は `styp` の中身を解釈しない」というコメントを、先頭の `styp` だけは型付きで読む旨に更新する
- `crates/wasm/src/fmp4_segment_mux.rs` の `SegmentMuxerOptions` リテラルに `styp: None` を追加する
- `CHANGES.md` に `[ADD]` (`StypBox` / `SegmentMuxerOptions::styp` / `Fmp4SegmentDemuxer::styp()`) を記録し、`[CHANGE]` に `SegmentMuxerOptions` のフィールド追加と移行手順、wasm の JSON では指定できないことを書く。`## develop` の既存 `[FIX]` にある「読み飛ばすボックスの中身は種別を問わず解釈しない」という記述を、`styp` だけは型付きで読む旨に更新する
- `skills/shiguredo-mp4/SKILL.md` の fMP4 ボックス一覧に `StypBox` を、`Fmp4SegmentDemuxer` の API 表に `styp()` を、`SegmentMuxerOptions` の説明に `styp` を追加する。`Fmp4SegmentMuxer` のメソッド表にある `create_media_segment_metadata` / `create_media_segment_metadata_with_sidx` の出力説明を `styp` を含む形に更新し、`handle_media_segment` の説明を「`styp` は先頭のみ型付きで読む」に更新する
- `src/boxes.rs` の `Brand` の doc を、`FtypBox` と `StypBox` の両方で使うことが分かる記述に更新する

## 対象外

除外の根拠は、利用頻度の推測ではなく、仕様上の依存関係と本ライブラリの現状で示す。

- `ssix` / `leva`
  - ISO/IEC 14496-12 8.16.4 の `ssix` は、8.8.13 の `leva` が定義する level と部分サブセグメントのバイト範囲を対応付けるボックスである。`ssix` 単体では意味を持たない
  - `leva` の `assignment_type` は、グループ指定 (0 / 1)、トラック指定 (2 / 3)、サブトラック指定 (4) のいずれかである
  - グループ指定 (0 / 1) の解決には `sbgp` / `sgpd` が必要で、本ライブラリでは未実装である。サブトラック指定 (4) には `sub_track_ID` を定義するサブトラック (`strk` 系) が必要で、これも未実装である
  - トラック指定 (2 / 3) だけは `TrackInfo::track_id` で解決できるが、その場合も `ssix` が返すのは level とバイト範囲の対応だけで、本ライブラリに level を使う機能が無い。`styp` に限定する本 issue では、依存する `sbgp` / `sgpd` とサブトラックを同時に実装しないため、`leva` / `ssix` を対象外とする
- `prft`
  - ISO/IEC 14496-12 8.16.5 のオプションのボックスで、low-latency 配信で NTP / UTC とメディア時刻を対応付けるために使う
  - low-latency 配信の実装者による説明でも、`prft` は低遅延配信に必須ではなく、既定では無効にされている (https://github.com/streamlinevideo/low-latency-preview/issues/1)
  - 本ライブラリの muxer は wall clock を保持しないため生成できず、demux で型付きに読むだけでは利用者の用途が無い。必要になった時点で別 issue にする
- `emsg`
  - `emsg` は ISO/IEC 23009-1 (DASH) が定義するボックスであり、ISO/IEC 14496-12 の fMP4 のボックスではない。本 issue の対象は 14496-12 のボックスに限る
- `Fmp4FileDemuxer::styp()` (`moof` より前のボックスを内部 demuxer に渡す設計変更が必要)
- wasm の JSON API での `styp` 指定
- CMAF のブランドの追加と `ftyp` / `styp` の CMAF ブランド出力 (0164 で扱う)
- `styp` の `major_brand` / `compatible_brands` の妥当性検査

## 関連 issue

- `issues/0164-add-cmaf-segment-brand.md`: CMAF の構造ブランドと単一メディアトラックの制約。`styp` の書き出し自体は本 issue の `SegmentMuxerOptions::styp` を使い、0164 はブランドと `ftyp` の切り替えを扱う
- `issues/0162-add-cenc-sample-aux-info.md`: fMP4 のサンプル補助情報 (`senc` / `saiz` / `saio`)、`src/mux_fmp4_segment.rs` と `src/demux_fmp4_segment.rs` を共有するため本 issue の前に実装する
- `issues/closed/0161-add-cenc-init-boxes.md`: 初期化データ向けの CENC ボックス対応
