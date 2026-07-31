# basic_types.rs の decode_header_and_payload で size=0 の特別処理が到達不能なデッドコードになっている

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-decode-header-payload-size-zero
- Polished: YYYY-MM-DD

## 目的

`BoxHeader::decode_header_and_payload` で size=0 の特別処理（バッファ末尾まで使用）が `box_size < header_size` 判定の後にあり到達不能であり、ドキュメントと実装が矛盾している問題を修正する。

## 優先度根拠

size=0 ボックス（ファイル末尾の可変長ボックス）をデコードできず常に `InvalidData` になる。ドキュメントは size=0 を「バッファ全体を使用する」と書いているが実装は常にエラー。`BoxHeader::decode` 自体は size=0 を許可しているため非対称。

## 現状

```rust
// src/basic_types.rs:161-177
pub fn decode_header_and_payload(buf: &[u8]) -> Result<(Self, &[u8])> {
    let (header, header_size) = Self::decode(buf)?;

    let mut box_size = usize::try_from(header.box_size.get())
        .map_err(|_| Error::invalid_data("too large box size"))?;
    if box_size < header_size {
        return Err(Error::invalid_data("box size is smaller than header size"));
    }
    // サイズが0の場合は、バッファ全体を使用する（ファイル末尾の可変長ボックスと想定）
    if box_size == 0 {
        box_size = buf.len();
    }

    Ok((header, &buf[header_size..box_size]))
}
```

`box_size == 0` のとき `box_size < header_size`（`0 < 8`）が常に真でエラーになり、size=0 の特別処理（172-174 行）には到達しない。`header_size` は通常 8（uuid+large なら 16 以上）。

一方 `BoxHeader::decode` は `get() != 0` のときだけサイズ下限を検査しており、size=0 自体は decode では許容される。ドキュメント（151-159 行）も「バッファ全体をペイロードとして扱う」と書いているが実装は矛盾。

## 設計方針

size=0 (32bit `size` フィールドが 0) の場合のみ「バッファ末尾まで」として扱い、`box_size < header_size` の判定をスキップして `buf.len()` を使う。
一方、size=1 + largesize=0 (`BoxSize::U64(0)`) は ISO/IEC 14496-12 で意味が未定義であり、著名な実装 (FFmpeg / GPAC / Bento4 / mp4parse-rust) すべてがエラー扱いにしているため、従来どおり `box_size < header_size` でエラーにする。

### 他実装での扱い

| 実装 | size==0 (U32) | largesize==0 (U64) |
| --- | --- | --- |
| FFmpeg (`libavformat/mov.c` `mov_read_default`) | 親サイズ残りを設定 | `a.size = -8` で `break` |
| GPAC (`src/isomedia/box_funcs.c` `gf_isom_parse_box_ex`) | root box のみ許可 | `size < hdr_size` でエラー |
| Bento4 (`Source/C++/Core/Ap4AtomFactory.cpp` `CreateAtomFromStream`) | サポート (stream size まで) | `size < 16` でエラー |
| mp4parse-rust (`mp4parse/src/lib.rs` `read_box_header`) | `MediaDataBox` のみ許可 | `offset > size` でエラー |

ISO/IEC 14496-12 4.2 では size==0 のみ「box は file の最後まで拡張される」と明示され、largesize==0 は特別な意味が定義されていない。

### 補足

- `BoxSize::LARGE_VARIABLE_SIZE = U64(0)` は既存の定義として残っているが、`finalize_box_size` (`basic_types.rs:129-134`) が「U32 に収まらないとエラー」で弾く実装になっており、エンコード側では実質的に使えない。デコード側でも仕様準拠で扱わない方針とする。
- size=0 の位置制限 (GPAC / mp4parse-rust のように「トップレベルのみ」「mdat のみ」など) は本 issue のスコープ外とし、必要なら別 issue で扱う。

## 完了条件

- size=0 (`BoxSize::U32(0)`) のボックスをデコードしたときバッファ全体をペイロードとして返すこと
- largesize=0 (`BoxSize::U64(0)`) は従来どおりエラーを返すこと
- ドキュメントと実装が一致すること
- size > 0 で `box_size < header_size` の場合は従来どおりエラーを返すこと
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `matches!(header.box_size, BoxSize::U32(0))` の判定を `box_size < header_size` の前に移動し、真のとき `box_size = buf.len()` にする
2. その後 `check_buffer_size(box_size, buf)?` を呼ぶ
3. テストを追加する
   - size=0 (`BoxSize::U32(0)`) のボックスをデコードしバッファ全体がペイロードとして得られること
   - largesize=0 (`BoxSize::U64(0)`) のボックスはエラーになること
   - size > 0 で `box_size < header_size` の場合はエラーになること (回帰防止)
4. 必要に応じてドキュメント (`basic_types.rs:151-159`) の文言を「size=0 (32bit の場合のみ)」と明示化する
