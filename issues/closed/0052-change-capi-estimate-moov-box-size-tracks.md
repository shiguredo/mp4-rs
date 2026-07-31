# C API の `mp4_estimate_maximum_moov_box_size()` が音声・映像の 2 トラック分しか見積もれない

- Priority: Medium
- Created: 2026-07-27
- Completed: 2026-07-31
- Model: Opus 5
- Branch: feature/change-capi-estimate-moov-box-size-tracks
- Polished: 2026-07-31

## 目的

C API の `mp4_estimate_maximum_moov_box_size()` が音声・映像の 2 引数固定になっており、字幕トラックを含む 3 トラック構成の `moov` サイズを見積もれない問題を解消する。

`Mp4FileMuxer` が字幕トラックを受け入れるようになったため、C API / WASM の利用者は faststart 用の予約領域を正しく決められない状態にある。

## 優先度根拠

Medium。生成される MP4 が壊れるわけではなく、faststart が黙って無効になり `moov` がファイル末尾に回るだけの縮退で済む。ただし利用者がこの関数の戻り値をそのまま渡しても目的（faststart の有効化）を達成できず、しかも縮退したことを検知する手段が無い。

## 現状

### 見積もり関数が 2 トラック固定

`crates/c-api/src/mux.rs` の `mp4_estimate_maximum_moov_box_size` 関数:

```rust
pub extern "C" fn mp4_estimate_maximum_moov_box_size(
    audio_sample_count: u32,
    video_sample_count: u32,
) -> u32 {
    shiguredo_mp4::mux::estimate_maximum_moov_box_size(&[
        audio_sample_count as usize,
        video_sample_count as usize,
    ]) as u32
}
```

Rust 本体の `src/mux_mp4_file.rs` の `estimate_maximum_moov_box_size` 関数は `&[usize]` で任意トラック数を受けられるため、制約は C API 側の引数だけにある。

### 実測

映像 / 音声 / 字幕を 1 本ずつ交互に `append_sample()` した構成で、2 トラック見積もりと 3 トラック見積もりを比較した結果、11 ケース中 6 ケースで faststart の有無が変わった。

```
v=  10 a=  10 s= 100 | 2track 見積= 2880 faststart=false 実 moov= 3583 | 3track 見積= 5504 faststart=true
v=  50 a=  50 s= 300 | 2track 見積= 4160 faststart=false 実 moov= 9603 | 3track 見積= 9984 faststart=true
v=   1 a=   1 s=1000 | 2track 見積= 2592 faststart=false 実 moov=16047 | 3track 見積=19616 faststart=true
```

「字幕分のサンプル数を `audio_sample_count` に足す」という回避策も、`v=50 a=50 s=300` のケースでは不足する（`512 + 1024 * 2 + 400 * 16 = 8960 < 9603`）。トラック数そのものが見積もり式の項（`PER_TRACK_OVERHEAD`）に効くためである。

### 縮退を検知する手段が無い

Rust 側の `src/mux_mp4_file.rs` の `FinalizedBoxes::is_faststart_enabled` メソッドは C API に公開されていない。`crates/c-api/src/mux.rs` の公開関数一覧にも faststart の成否を問い合わせるものは無いため、C API 利用者は見積もりが不足したことを知る方法がない。

## 設計方針

以下の方針で実装する（3 つの主要な設計判断は決定済み）。

### 見積もり関数のシグネチャ

`mp4_estimate_maximum_moov_box_size()` を、任意トラック数を受け取れる配列 + 長さのシグネチャに変更する。

- 変更後: `mp4_estimate_maximum_moov_box_size(const uint32_t *sample_counts, uint32_t sample_counts_len) -> uint32_t`
- Rust 側 `shiguredo_mp4::mux::estimate_maximum_moov_box_size(&[usize])` と 1:1 で対応する形。トラック種別は Rust 側の見積もり式が使わないため C 側でも受けない。
- 引数の扱いは以下で確定する（`sample_counts_len > 0` かつ `sample_counts` が NULL の組み合わせによる UB を避けるため、NULL 判定を長さ判定より先に行う）:
    - `sample_counts` が NULL の場合は `sample_counts_len` の値によらず `0` を返す（誤用扱い。他の C API 関数が NULL 引数に対して空文字や `MP4_ERROR_NULL_POINTER` を返すのと同じ方針で、戻り値が `u32` のため `0` を返す）。
    - `sample_counts` が NULL でなく `sample_counts_len == 0` の場合は空スライスとして Rust 側の `estimate_maximum_moov_box_size(&[])` を呼び、`BASE_MOOV_OVERHEAD` 相当（現状 512）を返す。

### 既存 2 引数版の扱い

既存の `(audio_sample_count, video_sample_count)` 版は破壊的に置き換える（deprecated として残さない）。

- `c-api` クレートは 0.1.0 のためメジャーバージョン到達前で破壊的変更は許容範囲。
- CHANGES.md に破壊的変更として記録する（`shiguredo-changelog` に従う）。
- 既存の呼び出し箇所も新シグネチャに書き換える:
    - `crates/c-api/tests/simple_mux_demux.c` の `main` 内 `mp4_estimate_maximum_moov_box_size` 呼び出し
    - `crates/wasm/examples/mux.js` の `mp4_estimate_maximum_moov_box_size` 呼び出し
    - `crates/c-api/src/mux.rs` の `mp4_estimate_maximum_moov_box_size` 関数直上の doc コメント内使用例（`# 使用例` ブロックの C コード）と、同ファイルの `mp4_file_muxer_set_reserved_moov_box_size` 関数の doc コメント内使用例。cbindgen が `crates/c-api/include/mp4.h` にそのまま反映するため、doc コメントを新シグネチャに揃えないと公開ヘッダーに旧シグネチャの使用例が残る。
    - `crates/c-api/src/mux.rs` の `mp4_estimate_maximum_moov_box_size` 関数直上の `# NOTE` セクション（「この関数は音声・映像の 2 トラック分しか見積もれない」）は削除する。任意トラック数対応後は事実と食い違うため。

### faststart 成否の問い合わせ関数

今回は追加しない。

- Rust 側の `FinalizedBoxes::is_faststart_enabled()` を C API に公開する対応は行わない。
- 見積もり関数が任意トラック数に対応することで、上記実測 6 ケースを含む通常構成では faststart が有効になることが期待できる。
- faststart を確実に有効にしたい利用者は、余裕を持たせて `mp4_file_muxer_set_reserved_moov_box_size()` に直接指定する従来の運用で対応可能。
- 「## 現状」の「縮退を検知する手段が無い」は今回のスコープ外として残す（必要になった時点で別 issue を起票する）。

### その他

`crates/c-api/include/mp4.h` は `crates/c-api/build.rs` の cbindgen が毎ビルド再生成するため、ヘッダーを手で編集する必要はない。

## 完了条件

- `mp4_estimate_maximum_moov_box_size()` が任意トラック数を受け取れる（配列 + 長さ）シグネチャに置き換わっていること
- C API から字幕トラックを含む 3 トラック以上の構成の `moov` サイズを見積もれること
- 「### 実測」節の表に示した 3 ケース（`v=10 a=10 s=100` / `v=50 a=50 s=300` / `v=1 a=1 s=1000`）で、3 トラック見積もりを使った場合に faststart が有効になること
- 既存の呼び出し箇所（`crates/c-api/tests/simple_mux_demux.c`、`crates/wasm/examples/mux.js`、および `crates/c-api/src/mux.rs` の doc コメント内使用例と `# NOTE`）が新シグネチャに追従していること
- `crates/c-api/tests/` に見積もり関数のテストが追加されていること
- CHANGES.md に破壊的変更として追記されていること
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `cargo test -p c-api --lib` が通ること

## 解決方法

### C API シグネチャの破壊的置き換え

`crates/c-api/src/mux.rs` の `mp4_estimate_maximum_moov_box_size` を、旧 2 引数固定 `(audio_sample_count, video_sample_count)` から任意トラック数を受ける `(sample_counts: *const u32, track_count: u32) -> u32` に破壊的に置き換えた。Rust 側 `shiguredo_mp4::mux::estimate_maximum_moov_box_size(&[usize])` をそのまま呼び出す形で、内部で `u32 → usize` に変換する。

引数の扱いは以下で確定した:

- `sample_counts` が NULL のときは `track_count` の値によらず `0` を返す（誤用扱い）
- `sample_counts` が非 NULL で `track_count == 0` のときは空スライスとして扱い、トラックなしの基本オーバーヘッド相当を返す

長さ引数は当初 `sample_counts_len` としていたが、レビュー指摘を反映して c-api 内の他関数の慣行（`sample_count` / `nalu_array_count` など）に合わせて `track_count` に変更した。

### 数値の飽和処理

- `src/mux_mp4_file.rs` の `estimate_maximum_moov_box_size` の加算・乗算を `saturating_*` に置き換え、wasm32 で `usize` が 32bit のときに起きる release wrap を防ぐ
- C API 側の `as u32` を `u32::try_from(...).unwrap_or(u32::MAX)` に置き換え、見積もり結果が `u32::MAX` を超えたときの silent truncation を防ぐ

### 呼び出し箇所の追従

- `crates/c-api/tests/simple_mux_demux.c` を新シグネチャに書き換えた
- `crates/wasm/examples/mux.js` を新シグネチャに書き換えた。`mp4_alloc` が align 1 契約であることを踏まえ、`Uint32Array` コンストラクタの `byteOffset` の 4 バイト境界要件を満たさない可能性を避けるため `DataView.setUint32` で書き込む
- `crates/c-api/src/mux.rs` の doc コメント内使用例（`mp4_estimate_maximum_moov_box_size` と `mp4_file_muxer_set_reserved_moov_box_size`）を新シグネチャに揃えた

### テスト追加

`crates/c-api/tests/test_mux.rs` を新設し、以下 7 件のテストを追加した:

- `estimate_returns_zero_for_null_pointer`: NULL 引数のとき `0` を返すこと
- `estimate_returns_base_overhead_for_empty_slice`: 空スライスで基本オーバーヘッド相当を返すこと
- `estimate_matches_formula_for_various_track_counts`: 1〜3 トラックで式と一致すること
- `estimate_handles_zero_counts_mixed`: 要素値 0 混在で式と一致すること
- `estimate_saturates_at_u32_max`: `u32::MAX` 要素で `u32::MAX` に飽和すること
- `estimate_handles_many_tracks`: 16 トラックで式と一致すること
- `estimate_enables_faststart_for_interleaved_three_tracks`: 実測 3 ケースで faststart が有効になることを実 `Mp4FileMuxer` を回して検証すること

### ドキュメントと CHANGES.md

- `crates/c-api/src/mux.rs` の doc コメントを補強した:
    - 引数の順序が任意である旨、`uint32_t` の 4 バイト境界要件を明示
    - `# NOTE` セクション（見積もりが不足しうる場合と、faststart 保証のための保険策）を追加
    - `# 関連関数` セクションで `mp4_file_muxer_set_reserved_moov_box_size` への逆リンクを追加
    - cbindgen で `crates/c-api/include/mp4.h` に反映
- CHANGES.md に `[CHANGE]` エントリを追加した
