# `SmhdBox::balance` が符号なし 8.8 のため、左寄りの balance を表せない

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-smhd-balance-sign
- Polished: 2026-10-05

## 目的

`smhd` の `balance` を、ISO/IEC 14496-12:2022 の符号付き 8.8 として読み書きする。今の型は全左（-1.0）を保持できず、そのバイト列を読むと 255.0 になる。

## 現状

ISO/IEC 14496-12:2022 の 12.2.2.2 は `template int(16) balance` である。12.2.2.3 は、これを固定小数 8.8 とし、0 が中央、全左が -1.0、全右が +1.0 としている。

`src/boxes_moov_tree.rs` の `SmhdBox::balance` の型は `FixedPointNumber<u8, u8>` である。doc は「0.0 が中央、-1.0 が全左、+1.0 が全右」と書いている。同じファイルの `MvhdBox::volume` と `TkhdBox::volume` は `FixedPointNumber<i8, u8>` である。

`FixedPointNumber` の encode / decode は整数部と小数部をその型のまま並べる。ワイヤ上の `0xFF00`（-1.0）は、整数部 `u8` の 255 と小数部 0 になる。-1.0 を作って encode することは、この型ではできない。

公開フィールドの型が変わる。`SmhdBox::DEFAULT_BALANCE` の型も変わる。

## 設計方針

- `balance` と `DEFAULT_BALANCE` を `FixedPointNumber<i8, u8>` にする
- 0 の既定値はそのままにする
- `SmhdBox::balance` の doc に、0 が中央、-1.0 が全左、+1.0 が全右であるという意味と、出典として ISO/IEC 14496-12:2022 の 12.2.2.3 を記載する（現行 doc に節番号は無いので追記する）
- `pbt/tests/prop_boxes.rs` と `pbt/tests/prop_container_boxes.rs` で `balance` を生成している箇所を `sample_u8` から `sample_i8` に追従させる
- `CHANGES.md` の develop に `[CHANGE]` を追記する。`FixedPointNumber<u8, u8>` から `FixedPointNumber<i8, u8>` への型変更で後方互換が無いためであり、種別順（CHANGE → ADD → UPDATE → FIX）に従って既存の `[FIX]` より前に置く
- Branch は `feature/fix-smhd-balance-sign` のままとする。作業の主目的はバグ修正であり、後方互換の無さは `CHANGES.md` の種別で表す（`MdhdBox::language` の型変更のときと同じ扱い）

## 完了条件

- サイズ 16、種別 `smhd`、version 0、flags 0、balance フィールドが `0xFF00`、reserved が 0 のボックスをデコードすると、整数部 -1、小数部 0 になる。今はこの balance フィールドが整数部 255、小数部 0 になる
- 整数部 -1、小数部 0 をエンコードすると、balance フィールドが `0xFF00` になる
- `0x0100` をデコードすると整数部 1、小数部 0 になり、整数部 1、小数部 0 をエンコードすると `0x0100` になる。`0x0000` も同様に往復する（負値対応のあとも、この 2 値の分解とバイト列が変わらないことの回帰確認）
- `SmhdBox::balance` と `SmhdBox::DEFAULT_BALANCE` の型が `FixedPointNumber<i8, u8>` になっている
- `SmhdBox::balance` の doc に、0 が中央、-1.0 が全左、+1.0 が全右であることと、ISO/IEC 14496-12:2022 の 12.2.2.3 が記載されている
- `pbt/tests/prop_boxes.rs` と `pbt/tests/prop_container_boxes.rs` の `balance` を生成する箇所が `sample_i8` に追従している
- `CHANGES.md` の develop に `[CHANGE]` エントリがある
- CI と同じ次のコマンドが warning なしで通る: `cargo fmt --all --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz -- -D warnings` / `cargo clippy --all-targets -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz`
