# basic_types.rs の decode_header_and_payload で size=0 の特別処理が到達不能なデッドコードになっている

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-31
- Model: opencode-go glm-5.2
- Branch: feature/fix-decode-header-payload-size-zero
- Polished: 2026-07-31

## 目的

`BoxHeader::decode_header_and_payload` で size=0 の特別処理（バッファ末尾まで使用）が `box_size < header_size` 判定の後にあり到達不能であり、ドキュメントと実装が矛盾している問題を修正する。

## 優先度根拠

size=0 ボックス（ファイル末尾の可変長ボックス）をデコードできず常に `InvalidData` になる。ドキュメントは size=0 を「バッファ全体を使用する」と書いているが実装は常にエラー。`BoxHeader::decode` 自体は size=0 を許可しているため非対称。

## 現状

`BoxHeader::decode_header_and_payload`（`src/basic_types.rs`）の現行実装は次の順序である。

```rust
pub fn decode_header_and_payload(buf: &[u8]) -> Result<(Self, &[u8])> {
    let (header, header_size) = Self::decode(buf)?;

    let mut box_size = usize::try_from(header.box_size.get())
        .map_err(|_| Error::invalid_data("too large box size"))?;
    if box_size < header_size {
        return Err(Error::invalid_data("box size is smaller than header size"));
    }
    Error::check_buffer_size(box_size, buf)?;

    // サイズが0の場合は、バッファ全体を使用する（ファイル末尾の可変長ボックスと想定）
    if box_size == 0 {
        box_size = buf.len();
    }

    Ok((header, &buf[header_size..box_size]))
}
```

`box_size == 0` のとき `box_size < header_size`（`0 < 8` 等）が常に真でエラーになり、その後の `check_buffer_size` と size=0 の特別処理には到達しない。`header_size` は通常 8（`BoxType::Uuid` や `BoxSize::U64` なら 16 以上。最小は `BoxHeader::MIN_SIZE`）。

一方 `Decode for BoxHeader` は `box_size.get() != 0` のときだけサイズ下限を検査しており、size=0 自体は decode では許容される。`decode_header_and_payload` のドキュメントも size=0 を「渡されたバッファ全体をペイロードとして扱う」と書いているが、実装は常にエラーで矛盾している。

## 設計方針

size=0（32bit `size` フィールドが 0、つまり `BoxSize::U32(0)` / `BoxSize::VARIABLE_SIZE`）の場合のみ「ボックスがバッファ末尾まで延びる」として扱い、`box_size < header_size` の判定をスキップして `box_size = buf.len()` にする。戻り値のペイロードは従来どおり `&buf[header_size..box_size]`（ヘッダー直後からバッファ末尾）とする。

一方、size=1 + largesize=0（`BoxSize::U64(0)` / `BoxSize::LARGE_VARIABLE_SIZE`）は ISO/IEC 14496-12 で意味が未定義であり、著名な実装（FFmpeg / GPAC / Bento4 / mp4parse-rust）すべてがエラー扱いにしているため、従来どおり `box_size < header_size` でエラーにする。

### 他実装での扱い

| 実装 | size==0 (U32) | largesize==0 (U64) |
| --- | --- | --- |
| FFmpeg (`libavformat/mov.c` `mov_read_default`) | 親サイズ残りを設定 | `a.size = -8` で `break` |
| GPAC (`src/isomedia/box_funcs.c` `gf_isom_parse_box_ex`) | root box のみ許可 | `size < hdr_size` でエラー |
| Bento4 (`Source/C++/Core/Ap4AtomFactory.cpp` `CreateAtomFromStream`) | サポート (stream size まで) | `size < 16` でエラー |
| mp4parse-rust (`mp4parse/src/lib.rs` `read_box_header`) | `MediaDataBox` のみ許可 | `offset > size` でエラー |

ISO/IEC 14496-12 4.2 では size==0 のみ「box は file の最後まで拡張される」と明示され、largesize==0 は特別な意味が定義されていない。

### 補足

- `BoxSize::LARGE_VARIABLE_SIZE = U64(0)` はエンコード用途として残る。`Mp4FileMuxer`（`src/mux_mp4_file.rs`）が `mdat` ヘッダー初期化で `BoxHeader::new(MdatBox::TYPE, BoxSize::LARGE_VARIABLE_SIZE)` を使っており、「エンコード側で使えない」わけではない。`finalize_box_size` は結果が `BoxSize::U32` 以外ならエラーにするが、mux の経路は `finalize_box_size` を経由しない。デコード側で `U64(0)` を可変長として扱わないのは、上記の仕様・他実装に合わせた方針であり、エンコード可否とは独立である。
- 本修正後、`BoxSize::LARGE_VARIABLE_SIZE` の既存ドキュメント（「基本的には `VARIABLE_SIZE` と同じ」）はデコード上の意味としては偽になるため、エンコード用の特別値である旨が分かるように更新する。
- demux 系（`src/demux_*.rs`）は `BoxHeader::decode` のあと `box_size.get() == 0` で U32/U64 を区別せず末尾扱いしており、本件の `decode_header_and_payload`（U32(0) のみ末尾）とは別レイヤである。demux 側の変更はスコープ外とする。
- size=0 の位置制限（GPAC / mp4parse-rust のように「トップレベルのみ」「mdat のみ」など）は本 issue のスコープ外とし、必要なら別 issue で扱う。

## 完了条件

- size=0（`BoxSize::U32(0)`）のボックスを `decode_header_and_payload` でデコードしたとき、ボックスがバッファ末尾まで延び、ペイロードとして `&buf[header_size..]`（ヘッダー直後からバッファ末尾）が返ること
- largesize=0（`BoxSize::U64(0)`）は従来どおりエラーを返すこと
- `decode_header_and_payload` のドキュメントと実装が一致すること（size=0 は 32bit の場合のみバッファ末尾扱いであること）
- `BoxSize::LARGE_VARIABLE_SIZE` のドキュメントが、デコードでは `VARIABLE_SIZE` と同義ではないこと（エンコード用の特別値であること）と矛盾しないこと
- size > 0 で `box_size < header_size` の場合は従来どおりエラーを返すこと
- `cargo test` / `cargo clippy` が通ること

## 解決方法

設計方針どおり、`BoxSize::VARIABLE_SIZE`（`U32(0)`）のみバッファ末尾扱いとし、`U64(0)` は下限検査でエラー継続とした。

- `BoxHeader::decode_header_and_payload`（`src/basic_types.rs`）で `VARIABLE_SIZE` 判定を `box_size < header_size` の前に置き、真のとき `box_size = buf.len()` にするようにした。`check_buffer_size` はその後に残した
- `BoxSize` / `VARIABLE_SIZE` / `LARGE_VARIABLE_SIZE` / `decode_header_and_payload` のドキュメントを、デコードの可変長は 32-bit のみであることと整合するよう更新した
- `tests/test_basic_types.rs` に回帰テストを追加した（`VARIABLE_SIZE` 成功・空ペイロード・`U64(0)` エラー・size < header・`InsufficientBuffer`）
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加した
