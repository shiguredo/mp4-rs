# `BoxHeader` が `uuid` ボックスの largesize と usertype を ISO/IEC 14496-12 と逆の順で読み書きする

- Created: 2026-09-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-box-header-uuid-largesize-order
- Polished: {YYYY-MM-DD}

## 目的

`BoxHeader` のデコードとエンコードを、ISO/IEC 14496-12:2022 の 4.2.2 にある Box の構文の順序に合わせる。

4.2.2 の構文では、ヘッダーは size、type、（size が 1 なら）largesize、（type が `uuid` なら）usertype の順に並ぶ。今の実装は usertype を largesize より前に置いている。このため、仕様どおりに書かれた largesize 形式の `uuid` ボックスを正しくデコードできず、largesize 形式の `uuid` ボックスをエンコードすると仕様と異なるバイト列になる。

## 現状

`src/basic_types.rs`:

- `impl Decode for BoxHeader`: size（4 バイト）、type（4 バイト）の順に読み、type が `uuid` なら usertype（16 バイト）を読み、その後で size が 1 なら largesize（8 バイト）を読む
- `impl Encode for BoxHeader`: size、type、usertype、largesize の順に書く

デコードとエンコードが同じ誤った順序なので、`pbt/tests/prop_basic_types.rs` のラウンドトリップでは検出できない。usertype と largesize を両方持つヘッダーのバイト列を固定したテストもない。

仕様どおりの largesize 形式の `uuid` ボックスをデコードすると、usertype の後半 8 バイトを largesize として読むため、ボックスサイズが化ける。その結果、`Too small box size` などのエラーになるか、誤った位置へ読み進む。`BoxHeader` を使うすべての経路が影響を受ける。たとえば次のもの:

- `Fmp4SegmentDemuxer::handle_init_segment` / `handle_media_segment` のトップレベルボックスの読み飛ばし
- `Fmp4FileDemuxer` のトップレベルボックスの読み飛ばし
- `UnknownBox` / `RootBox` のデコード

issue 0094 の対応で追加する PBT（`pbt/tests/prop_fmp4_segment_mux_demux.rs` の `arb_leading_box`）は、この問題があるため `uuid` を `moof` より前に置くボックスの候補から外している。

## 設計方針

- `BoxHeader` のデコードとエンコードを、size、type、largesize、usertype の順に直す
- 32 ビット size の `uuid` ボックスと、`uuid` 以外の largesize 形式のボックスのバイト列は変わらない（usertype と largesize の両方を持つ場合だけ順序が変わる）
- `BoxHeader::external_size` / `BoxHeader::MAX_SIZE` は順序に依存しないため変更しない
- 仕様由来の順序であることを、資料名・節番号とともにコードコメントに書く

## 完了条件

- 4.2.2 の順序で書かれた largesize 形式の `uuid` ボックスのヘッダーを正しくデコードできる
- largesize 形式の `uuid` ボックスのヘッダーを 4.2.2 の順序でエンコードする
- 32 ビット size の `uuid` ボックスと、`uuid` 以外のボックスのデコード・エンコード結果は変わらない

## 解決方法

- `src/basic_types.rs` の `impl Decode for BoxHeader` と `impl Encode for BoxHeader` の読み書きの順序を直す
- テスト
  - `tests/test_basic_types.rs`: 仕様どおりの順序の固定バイト列（size=1、`uuid`、largesize、usertype）をデコード・エンコードする単体テストを追加する
  - `pbt/tests/prop_basic_types.rs`: largesize 形式の `uuid` ヘッダーのラウンドトリップに、バイト列の並びの検証を加える
  - issue 0094 の対応がマージ済みであれば、`arb_leading_box` で `uuid` を候補に戻し、除外理由のコメントを削除する
- `CHANGES.md` に `[FIX]` として記載する
