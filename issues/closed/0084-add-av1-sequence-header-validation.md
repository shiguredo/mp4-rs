# bitstream::av1 の Sequence Header に operating point 情報を公開し予約値を検証する

- Created: 2026-08-26
- Completed: 2026-08-26
- Branch: feature/add-av1-sequence-header-validation
- Polished: {YYYY-MM-DD}

## 目的

`bitstream::av1` の `parse_sequence_header` が読み捨てている operating point 情報を `Av1SequenceHeader` に公開し、予約値の `chroma_sample_position` を検証する。
利用側が「単一 operating point かつ `operating_point_idc[0] == 0`」「`chroma_sample_position` の予約値拒否」のような Sequence Header のポリシーを、Sequence Header を再解析せずに適用できるようにする。

## 現状

- `src/bitstream/av1.rs` の `parse_sequence_header` は `operating_points_cnt_minus_1`（f(5)）と `operating_point_idc`（f(12)）を読み、`seq_level_idx[0]` / `seq_tier[0]` だけを `Av1SequenceHeader` に返す。operating point の個数と idc の値は公開しない
- 同関数の `read_color_config` は `chroma_sample_position` を f(2) で読むが、AV1 spec の Color config semantics が `CSP_RESERVED`（値 3）と定める予約値を検証しない
- issue 0064 の設計方針は、複数 operating point がある正当な入力を crate 側で拒否しないこと（利用側のポリシーに委ねる）としている。ただし値そのものを公開しないと、利用側が単一 operating point 制限を適用するために Sequence Header を自前解析する必要が生じる

## 設計方針

### `Av1SequenceHeader` への field 追加

`Av1SequenceHeader` に次を追加する。

- `operating_points_cnt_minus_1: u8`
- `operating_point_idc_0: u16`（`operating_point_idc[0]` の値）

`reduced_still_picture_header == 1` のときは AV1 spec の Sequence header OBU syntax が定める暗黙値（`operating_points_cnt_minus_1 = 0`、`operating_point_idc[0] = 0`）を代入する。
field 追加は後方互換のある追加とし、既存の `build_av01_box` / `parse_frame_header_prefix` の挙動は変えない。
複数 operating point の拒否や選択・正規化は本 issue で行わない（issue 0064 の方針を維持する）。

### `chroma_sample_position` の予約値検証

`parse_sequence_header` は `seq_profile` の予約値（3..=7）拒否と同様に、`chroma_sample_position == 3`（`CSP_RESERVED`）を `crate::Error` で拒否する。

### 対象外

- 複数 operating point 自体の拒否と `operating_point_idc` の解釈（利用側のポリシーに委ねる）
- RTP / SDP での operating point の扱い
- C API / WASM バインディング

## 完了条件

- `Av1SequenceHeader` に `operating_points_cnt_minus_1` / `operating_point_idc_0` が公開される
- `reduced_still_picture_header == 1` の Sequence Header で両 field が暗黙値になる
- `parse_sequence_header` が `chroma_sample_position == 3` を `crate::Error` で拒否する
- 決定的テスト（`tests/` 配下）が追加され、mock / stub、sleep、`#[ignore]`、外部 command、ネットワークを使用しない
- `CHANGES.md` の develop に `[ADD]` として記載される
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る

## 解決方法

`src/bitstream/av1.rs` の `Av1SequenceHeader` に `operating_points_cnt_minus_1` と `operating_point_idc_0` を追加し、`parse_sequence_header` が値を公開するようにした。`reduced_still_picture_header == 1` のときは AV1 spec の暗黙値（いずれも 0）を代入する。複数 operating point 自体の拒否は行わない。

`read_color_config` で `chroma_sample_position == 3`（`CSP_RESERVED`）を `crate::Error` として拒否するようにした。

決定的テストを `tests/test_bitstream_av1.rs` に追加し、`CHANGES.md` の develop に `[ADD]` を記載した。
