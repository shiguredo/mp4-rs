# `BoxHeader` が `uuid` ボックスの largesize と usertype を ISO/IEC 14496-12 と逆の順で読み書きする

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-box-header-uuid-largesize-order
- Polished: 2026-09-25
- Refreshed: 2026-11-24

## 目的

`BoxHeader` のデコードとエンコードを、ISO/IEC 14496-12:2022 の 4.2.2 にある `aligned(8) class BoxHeader` の構文の順序に合わせる。

4.2.2 の構文では、ヘッダーは size、type、（size が 1 なら）largesize、（type が `uuid` なら）usertype の順に並ぶ。今の実装は usertype を largesize より前に置いている。このため、仕様どおりに書かれた largesize 形式の `uuid` ボックスを正しくデコードできず、largesize 形式の `uuid` ボックスをエンコードすると仕様と異なるバイト列になる。

## 現状

`src/basic_types.rs`:

- `impl Decode for BoxHeader`: size（4 バイト）、type（4 バイト）の順に読み、type が `uuid` なら usertype（16 バイト）を読み、その後で size が 1 なら largesize（8 バイト）を読む
- `impl Encode for BoxHeader`: 書き込む前に、0 以外でヘッダーの幅（`BoxHeader::external_size`）に満たないサイズを `ErrorKind::InvalidInput` として拒否する（ISO/IEC 14496-12:2015 の 4.2 を根拠とするコメント付き）。その後、size、type、usertype、largesize の順に書く
- `BoxSize` の doc にある仕様の引用（`aligned(8) class Box { unsigned int(32) size; if (size==1) { unsigned int(64) largesize; } ... }`）は、type フィールドがなく、2022 年版の 4.2.2 の `BoxHeader` の構文とも形が異なる

`pbt/tests/prop_basic_types.rs` の `BoxHeader` のラウンドトリップは、`box_header_normal_u32_roundtrip` / `box_header_uuid_u32_roundtrip` / `box_header_normal_u64_roundtrip` / `box_header_uuid_u64_roundtrip` の 4 つである。共通の `assert_box_header_roundtrip` は、エンコード結果の長さとデコード結果だけを確かめ、バイトの並びは確かめない。このため `box_header_uuid_u64_roundtrip` は、デコードとエンコードが同じ誤った順序のまま通る。`pbt/tests/prop_additional_boxes.rs` の `unknown_box_roundtrip` も `BoxType::Uuid` と `BoxSize::U64` の `UnknownBox` を往復させるが、同じ理由で通る。`tests/test_basic_types.rs` の `box_header_encode_size_limits` と `tests/test_boxes.rs` の `unknown_box_encode_size_mismatch` も両者を組み合わせるが、書き込む前に返るエラーだけを確かめるので、順序に依存しない。4.2.2 の順序のバイト列と突き合わせるテストは 1 件もない。

仕様どおりの largesize 形式の `uuid` ボックスをデコードすると、usertype の後半 8 バイトを largesize として読むため、ボックスサイズが化ける。その結果、`Too small box size` などのエラーになるか、誤った位置へ読み進む。`BoxHeader` を使うすべての経路が影響を受ける。たとえば次のもの:

- `Fmp4SegmentDemuxer::handle_init_segment` / `handle_media_segment` のトップレベルボックスの読み飛ばし
- `Fmp4FileDemuxer` のトップレベルボックスの読み飛ばし
- `UnknownBox` / `RootBox` のデコード

再現手順（develop で確認）:

1. 仕様どおりの順序で、largesize 形式の `uuid` ボックスのヘッダー（32 バイト）を組み立てる。size=1、type=`uuid`、largesize=40 とし、16 バイトの usertype の後半 8 バイトを big-endian の 5 にする
2. `BoxHeader::decode` に渡すと、usertype の後半 8 バイトを largesize として読み、`Too small box size: actual=5, expected=32 or more` になる
3. `BoxType::Uuid` と `BoxSize::U64(40)` を持つ `BoxHeader` をエンコードすると、size、type、usertype、largesize の順のバイト列になる（0 以外で 32 未満の largesize は、エンコードがヘッダーの幅の検査で拒否する）

issue 0094 で追加した `arb_leading_box`（`pbt/tests/prop_fmp4_segment_mux_demux.rs`）は、現在は `arb_skippable_box(ctx, &[])` に委譲している。共通生成器の `arb_skippable_box` は、`moof` より前、`moof` と `mdat` の間、`mdat` の後ろに置くボックスを生成し、`uuid` を候補から外している。

- 除外の理由は `arb_skippable_box` の doc コメントに 2 つ書かれている。largesize 形式の `uuid` はこの不具合のため、32 ビット size の `uuid` は形式を分けて生成する複雑さに見合わないためである
- `uuid` は `SKIPPABLE_BOX_TYPES` に含まれておらず、`arb_skippable_box` の任意の 4CC を引く枝の棄却条件で外している
- `arb_skippable_box` は size（と largesize）、種別、ペイロードだけを書き、usertype を書かない

0132（`issues/closed/0132-bug-box-size-invalid-encoding.md`）は `BoxHeader::encode` にヘッダーの幅の検査を加えたが、フィールドの順序は本 issue の対象として変えていない。0152（`BoxType::Normal(*b"uuid")` を encode で拒否する）は `BoxType::Uuid` のヘッダーのバイト列を変えないため、本 issue とどちらを先に直しても互いの内容は変わらない

## 設計方針

- `BoxHeader` のデコードとエンコードを、size、type、largesize、usertype の順に直す
- 32 ビット size の `uuid` ボックスと、`uuid` 以外の largesize 形式のボックスのバイト列は変わらない（usertype と largesize の両方を持つ場合だけ順序が変わる）
- `BoxHeader::external_size` / `BoxHeader::MAX_SIZE` の値は順序に依存しないため変更しない
- 順序が 4.2.2 の `BoxHeader` の構文に由来することを、資料名・節番号・クラス名と、将来の改訂で変わる可能性があることとともにコードコメントに書く
- 順序を前提にした既存のコメントと、仕様の引用も合わせて直す

## 完了条件

- 4.2.2 の順序で書かれた largesize 形式の `uuid` ボックスのヘッダーを正しくデコードできる
- largesize 形式の `uuid` ボックスのヘッダーを 4.2.2 の順序でエンコードする
- 32 ビット size の `uuid` ボックスと、`uuid` 以外のボックスのデコード・エンコード結果は変わらない
- `arb_leading_box` が、32 ビット size と largesize の両方の形式で `uuid` を生成する

## 解決方法

- `src/basic_types.rs`
  - `impl Decode for BoxHeader` と `impl Encode for BoxHeader` の読み書きの順序を直す
  - `BoxSize` の doc の仕様の引用を、4.2.2 の `BoxHeader` の構文に合わせる
- テスト（shiguredo-rust の役割分担に従い、正常系は PBT で検証する）
  - `pbt/tests/prop_basic_types.rs`: 既存の `box_header_uuid_u64_roundtrip` に、サンプルした usertype と largesize（0 または 32 以上）から 4.2.2 の順序で手で組み立てたバイト列が、エンコード結果と一致することの検証を加える。そのバイト列をデコードすると、元のヘッダーに戻ることも検証する
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs` の `arb_skippable_box`（`arb_leading_box` の委譲先）
    - `uuid` を明示的に選ぶ枝を追加し、16 バイトの usertype を生成する
    - ヘッダーは 32 ビット size なら size=`24 + ペイロード長`、`uuid`、usertype の順、largesize 形式なら size=1、`uuid`、largesize=`32 + ペイロード長`、usertype の順に組み立てる
    - 任意の 4CC を引く枝は usertype を書かないため、`uuid` の棄却は残す
    - doc コメントの「`moof` / `uuid` 以外の任意の 4CC」と除外理由の段落を、新しい生成方法に合わせて書き直す
- `CHANGES.md` に `[FIX]` として記載する
