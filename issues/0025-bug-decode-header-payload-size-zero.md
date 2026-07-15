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

`box_size == 0` のとき `box_size < header_size` の判定をスキップし、`buf.len()` を使うよう条件順序を修正する。size=0 のときは `check_buffer_size` も `buf.len()` に対して行う。

## 完了条件

- size=0 のボックスをデコードしたときバッファ全体をペイロードとして返すこと
- ドキュメントと実装が一致すること
- size > 0 で `box_size < header_size` の場合は従来どおりエラーを返すこと
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `box_size == 0` の判定を `box_size < header_size` の前に移動する
2. size=0 のとき `box_size = buf.len()` にしてから `check_buffer_size` を行う
3. size=0 のデコードテストを追加する
