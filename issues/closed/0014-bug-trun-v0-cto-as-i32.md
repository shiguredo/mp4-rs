# boxes_fmp4.rs の trun version 0 の composition_time_offset を as i32 で格納しており符号が化ける

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-28
- Model: opencode-go glm-5.2
- Branch: feature/fix-trun-v0-cto-as-i32
- Polished: 2026-07-28

## 目的

`TrunBox` の decode で version 0 の `composition_time_offset`（仕様上 unsigned 32-bit）を `as i32` で格納しており、`> i32::MAX` の正当な値が負値に化ける問題を修正する。

## 優先度根拠

ISO/IEC 14496-12 で version 0 は unsigned、version 1 は signed。格納型が `Option<i32>` のため u32 全域を保持できず、境界で黙って破壊的変換している。`ctts` 側は `i64` で保持しており表現力が不整合で、fMP4 demux の `composition_time_offset` が汚染され PTS が誤る。

## 現状

```rust
// src/boxes_fmp4.rs:764-773 (TrunBox::decode)
let composition_time_offset =
    if flags & Self::FLAG_SAMPLE_COMPOSITION_TIME_OFFSETS_PRESENT != 0 {
        if version == 1 {
            Some(i32::decode_at(payload, &mut offset)?)
        } else {
            Some(u32::decode_at(payload, &mut offset)? as i32)
        }
    } else {
        None
    };
```

```rust
// src/boxes_fmp4.rs:830 (TrunSample)
pub composition_time_offset: Option<i32>,
```

version 0 で `u32 as i32` は `0x8000_0000 ..= 0xFFFF_FFFF` を負の i32 に再解釈する。対照的に `CttsEntry.sample_offset` は `i64` で保持されており（`src/boxes_moov_tree.rs:2109`）、`CttsBox::decode` は version 0 を `u32 as i64` で格納している（`src/boxes_moov_tree.rs:2187-2190`）。

## 設計方針

`TrunSample.composition_time_offset` の型を `Option<i64>` に変更し、version 0 は `u32 as i64`、version 1 は `i32 as i64` で格納する。`ctts` 側と表現力を揃える。demuxer / muxer 経由の公開 API では既に `composition_time_offset: Option<i64>` として公開されており（`src/demux_mp4_file.rs:122`、`src/mux_mp4_file.rs:257`）、demuxer 出力の表現力とも整合する。

`TrunSample` は `src/boxes.rs:8` で pub 再エクスポートされた公開 API 型のため、この型変更はクレート利用者に対する破壊的変更となる。

### encode 側の版選択と範囲チェック

`Option<i64>` に広げた結果、以下の 4 領域が発生する。実装は `ctts`（`src/boxes_moov_tree.rs:2146,2154` で `i32::try_from` / `u32::try_from` を用いてエラー化）に倣い、`try_from` で厳密に検証する。

1. `0..=i32::MAX`: version 0 でエンコード可（version 1 でも可だが、旧挙動維持のため version 0 を優先）
2. `(i32::MAX as i64 + 1)..=(u32::MAX as i64)`: version 0 でしかエンコード不可
3. `(i32::MIN as i64)..=-1`: version 1 でしかエンコード不可
4. 上記以外（`< i32::MIN as i64` または `> u32::MAX as i64`）: どちらのバージョンでもエンコード不可

`uses_version_1` の新規則: いずれかのサンプルが負値なら version 1、それ以外は version 0。負値と `> i32::MAX` の値が同一 `TrunBox` に混在する場合はどちらのバージョンでも全サンプルを表現できないため、`Encode` 実装から `Error::invalid_input` を返す。

## 完了条件

- version 0 の `composition_time_offset` が `> i32::MAX` でも正しく保持されること
- version 1 の signed 値も従来どおり正しく保持されること
- encode / decode の往復（`0`、`i32::MAX`、`i32::MAX as i64 + 1`、`u32::MAX`、`-1`、`i32::MIN` の境界値）が正しいこと
- 領域外の値（`< i32::MIN` または `> u32::MAX`）および両バージョン表現不能な混在は encode 時に `Error::invalid_input` を返すこと
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `src/boxes_fmp4.rs`
   - `TrunSample.composition_time_offset` の型を `Option<i64>` に変更する
   - `TrunBox::decode`（現行 764-773 行）で version 0 は `u32 as i64`、version 1 は `i32 as i64` で格納する
   - `TrunBox` の encode（現行 664-674 行付近）で、version 0 は `u32::try_from`、version 1 は `i32::try_from` を使って範囲外をエラー化する
   - `uses_version_1`（現行 618-626 行）を「いずれかのサンプルが負値なら version 1」に変更し、負値と `> i32::MAX` の値が同一 `TrunBox` に混在した場合は encode 時に `Error::invalid_input` を返すよう `Encode` 実装で検証する
2. `src/mux_fmp4_segment.rs`
   - 内部型 `ResolvedSegmentSample.composition_time_offset`（現行 124 行）を `Option<i64>` に広げ、`Sample` から `ResolvedSegmentSample` を作る変換部（現行 898-907 行）にある `i32::try_from` 境界検証は撤廃し、`TrunBox::encode` 側の新しい範囲検証に一本化する
   - `moof` 構築部（現行 749-763 行付近）を新しい `TrunSample.composition_time_offset: Option<i64>` に整合するよう更新する
   - `create_media_segment_metadata` の doc コメント（現行 209-211 行付近）で「trun に書けるのは `i32::MIN..=i32::MAX` の範囲に限られる」と書かれた記述を、新しい許容範囲（version 0 で `0..=u32::MAX`、version 1 で `i32::MIN..=i32::MAX`、両立不能な混在は不可）に更新する
3. `src/demux_fmp4_segment.rs`
   - 現行 402-404 行の `trun_sample.composition_time_offset.map(i64::from)` を、型変更後は不要になるため単純代入に置き換える
4. `pbt/tests/prop_fmp4_boxes.rs`
   - `arb_trun_box` の `cto_strategy`（現行 158-162 行付近）を `any::<i64>()` ベースに広げ、version 0 の `(i32::MAX, u32::MAX]` と version 1 の `[i32::MIN, -1]` の両範囲を PBT で探索できるようにする
   - 境界値（`0`、`i32::MAX`、`i32::MAX as i64 + 1`、`u32::MAX`、`-1`、`i32::MIN`）の roundtrip 単体テストを追加する
   - encode 側の範囲エラー（`< i32::MIN`、`> u32::MAX`、負値と `> i32::MAX` の混在）を検証する単体テストを追加する

## 解決方法

### 本体の型変更と範囲検証

1. `src/boxes_fmp4.rs` の `TrunSample.composition_time_offset` を `Option<i32>` から `Option<i64>` に変更した。`TrunBox::decode` で version 0 は `i64::from(u32::decode_at(...))`、version 1 は `i64::from(i32::decode_at(...))` に変えて、version 0 で `> i32::MAX` の値が負値に化けていた既存バグを解消した。
2. `TrunBox::encode` で `unwrap_or(0)` した `cto: i64` を、version 1 では `i32::try_from`、version 0 では `u32::try_from` で厳密に検証し、範囲外は `Error::invalid_input` を返すようにした。`uses_version_1()` は「いずれかのサンプルが負値なら version 1」に整理し、負値と `> i32::MAX` の混在は encode 時にエラーとして扱う設計にした。
3. `src/mux_fmp4_segment.rs` の `ResolvedSegmentSample.composition_time_offset` を `Option<i64>` に広げ、`resolve_segment_tracks` にあった `i32::try_from` の境界検証を撤廃した。範囲検証は `TrunBox::encode` 側に一本化した。`create_media_segment_metadata` の doc も新しい許容範囲に書き換えた。
4. `src/demux_fmp4_segment.rs` の `trun_sample.composition_time_offset.map(i64::from)` は型変更で不要になったため単純代入に置き換えた。

### C API doc の追従

5. `crates/c-api/src/fmp4_segment_mux.rs` と `crates/c-api/src/mux.rs` の `composition_time_offset` の doc を新仕様（version 0 で `0..=u32::MAX`、version 1 で `i32::MIN..=i32::MAX`、混在は不可）に更新し、cbindgen 経由で `crates/c-api/include/mp4.h` を再生成した。

### テスト

6. `pbt/tests/prop_fmp4_boxes.rs` の `arb_trun_box` の `cto_strategy` を `i64` ベースに広げ、TrunBox 単位で「符号あり側 (`i32::MIN..=i32::MAX`)」か「符号なし側 (`0..=u32::MAX`)」のどちらか一方を選ぶ構造にした（混在は encode でエラーになるため）。
7. 境界値（`0`、`i32::MAX`、`i32::MAX + 1`、`u32::MAX`、`-1`、`i32::MIN`）の roundtrip 単体テスト、および範囲外エラー（`> u32::MAX`、`< i32::MIN`、負値と `> i32::MAX` の混在）の単体テストを追加した。

### レビュー指摘への対応

8. コメント整理: `mux_fmp4_segment.rs` に浮いていた「範囲外の composition_time_offset は…」コメント、`ResolvedSegmentSample` フィールドの重複した 2 行目、`create_media_segment_metadata` doc の冗長な最終文、テストの doc と重複していた行内コメントを削除した。
9. 境界値テスト内の `.expect("cto={cto} …")` は書式展開されなかったため `.unwrap_or_else(|e| panic!("… cto={cto} …: {e:?}"))` に置き換え、失敗時にどの `cto` で落ちたかとエラー内容を追えるようにした。

### 変更履歴

`CHANGES.md` に `[CHANGE]` エントリを 1 件追加した。型変更・decode バグ修正・`Fmp4SegmentMuxer` / C API 経由で `> i32::MAX` を受け付けるようになる動作変更・混在時のエラー挙動を記載した。
