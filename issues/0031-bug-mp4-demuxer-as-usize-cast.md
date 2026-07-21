# demux_mp4_file.rs の Mp4FileDemuxer で box_size の u64 → usize 変換が as キャストであり 32 bit プラットフォームで切り詰められる

- Priority: Medium
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/fix-mp4-demuxer-as-usize-cast
- Polished: 2026-07-20

## 目的

`Mp4FileDemuxer` の `read_ftyp_box_header` および `read_moov_box_header` で、`header.box_size.get()`（u64）を `as usize` でキャストしている。32 bit プラットフォームで値が `u32::MAX` を超える場合に暗黙の切り詰めが発生する。同じプロジェクトの `Fmp4FileDemuxer` や `Mp4FileKindDetector` では `usize::try_from` でエラー処理しており一貫性がない。

## 優先度根拠

32 bit プラットフォーム（wasm32 を除く組み込み等）でのみ発生する。mp4-rs の主要ターゲット（64 bit デスクトップ・wasm32）では実害はほぼないが、一貫性と防御的プログラミングの観点から修正すべき。

## 現状

`src/demux_mp4_file.rs:449`（`read_ftyp_box_header` 内）:

```rust
let box_size = Some(header.box_size.get() as usize).filter(|n| *n > 0);
```

`src/demux_mp4_file.rs:479`（`read_moov_box_header` 内、moov 以外のボックスをスキップする分岐。`as usize` を含まないため書き換え対象外）:

```rust
let box_size = Some(header.box_size.get()).filter(|n| *n > 0);
```

`src/demux_mp4_file.rs:493`（同関数内、moov ボックスの分岐。479 行目の `Option<u64>` を `Option<usize>` に変換）:

```rust
let box_size = box_size.map(|n| n as usize);
```

対比: `demux_fmp4_file.rs:254-256` では `usize::try_from` でエラー処理している:

```rust
let box_size = usize::try_from(header.box_size.get()).map_err(|_| {
    DemuxError::DecodeError(Error::invalid_data("ftyp box size exceeds usize::MAX"))
})?;
```

### `box_size == 0` のセマンティクス差異

`Mp4FileDemuxer` は `box_size == 0` を `None`（EOF まで読み込み）として扱う。これは MP4 仕様上 `box_size == 0` が「ボックスがファイル末尾まで拡張される」を意味するため正当。一方 `Fmp4FileDemuxer` / `Mp4FileKindDetector` は `box_size == 0` をエラーとして扱う。本 issue は `usize::try_from` の導入のみを扱い、`box_size == 0` のセマンティクスは変更しない。

## 設計方針

`as usize` を `usize::try_from` に置き換え、変換失敗時に `DemuxError::DecodeError` を返す。`box_size == 0 → None` のセマンティクスは維持する。

### 書き換え後のコード

`read_ftyp_box_header`（449 行目）:

```rust
let raw_size = header.box_size.get();
let box_size = if raw_size == 0 {
    None
} else {
    Some(usize::try_from(raw_size).map_err(|_| {
        DemuxError::DecodeError(Error::invalid_data("ftyp box size exceeds usize::MAX"))
    })?)
};
```

`read_moov_box_header`（493 行目）:

```rust
let box_size = box_size
    .map(|n| {
        usize::try_from(n).map_err(|_| {
            DemuxError::DecodeError(Error::invalid_data("moov box size exceeds usize::MAX"))
        })
    })
    .transpose()?;
```

エラーメッセージは参照先の実装（`demux_fmp4_file.rs`）に合わせてボックス名を含める。

### スコープ外

- 同ファイル内の `Input::slice_range`（209 行目）の `position.checked_sub(self.position)? as usize` も u64 → usize の `as` キャストだが、これは box_size ではなく position offset の変換であり本 issue のスコープ外
- 689 行目の `sample_accessor.data_size() as usize` は u32 → usize のキャストであり、Rust の全ターゲットで `usize >= u32` が保証されるため切り詰めが起きない。対象外

## 完了条件

- 449 行目・493 行目の `as usize` が `usize::try_from` に置き換えられ、変換失敗時に `DemuxError::DecodeError` が返ること
- `box_size == 0 → None` のセマンティクスが維持されること
- 既存のテストが通ること
- `cargo clippy` が通ること

## 解決方法

設計方針の「書き換え後のコード」に従って `as usize` を `usize::try_from` に置き換える。

## 後方互換

`usize::try_from` の導入は 32 bit プラットフォームでのみ動作が変わり（切り詰め → エラー）、64 bit プラットフォームでは挙動不変。API シグネチャの変更もない（既に `Result` を返している）。

## CHANGES.md

`[FIX]` で記載する。
