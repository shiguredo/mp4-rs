# `SmhdBox::balance` が符号なし 8.8 のため、左寄りの balance を表せない

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-smhd-balance-sign
- Polished: {YYYY-MM-DD}

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
- 中央 0、全左 -1.0、全右 +1.0 は、資料名と 12.2.2.3 を doc に残す

## 完了条件

- サイズ 16、種別 `smhd`、version 0、flags 0、balance フィールドが `0xFF00`、reserved が 0 のボックスをデコードすると、整数部 -1、小数部 0 になる。今はこの balance フィールドが整数部 255、小数部 0 になる
- 整数部 -1、小数部 0 をエンコードすると、balance フィールドが `0xFF00` になる
- `0x0100`（+1.0）と `0x0000`（中央）の往復は今と同じ意味になる
