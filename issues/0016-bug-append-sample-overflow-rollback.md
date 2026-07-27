# mux_mp4_file.rs の append_sample が Overflow 時に chunks をロールバックせず状態が残留する

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-append-sample-overflow-rollback
- Polished: YYYY-MM-DD
- Updated: 2026-07-27

## 目的

`Mp4FileMuxer::append_sample` が `MuxError::Overflow` 返却時に `self.tracks` への変更をロールバックせず、エラー後にリトライするとサンプルが二重登録される問題を修正する。

`append_sample` の失敗パスのうち `Overflow` だけが非アトミックであり、呼び出し側にミューサの破棄を強制している。この非対称を解消し、全エラー種別で内部状態が不変になるようにする。

## 優先度根拠

`Overflow` を受け取った後の `Mp4FileMuxer` は復旧できず、インスタンスを破棄するしかない。他のすべてのエラー種別は内部状態が完全に不変で再呼び出しできるため、`Overflow` だけが例外になっている。

到達には `advance_position()` で次の書き込み位置を `u64::MAX` 付近まで進める必要があり、実データでは到達しない（`data_size` は直前で `u32::MAX` 以下に制限されるため、`next_position > u64::MAX - u32::MAX` が必要）。ただし `advance_position()` は Rust の公開 API であり、C API（`crates/c-api/src/mux.rs:859` `mp4_file_muxer_advance_position`）からも到達できる。

「現実的には起きないが、暗黙のデータ破壊よりも明示的な正しさを優先する」という点で 0032 と同じ論法に立つ。ただし 0032 が「エラーを返さずに壊れた MP4 を出す」のに対し、本 issue は「エラーは返るが復旧できない」であり危険度は一段低い。

## 現状

```rust
// src/mux_mp4_file.rs:628-649
        let track_index = self.ensure_track_entry(sample.track_kind, sample.timescale)?;

        if let Some(sample_entry) = resolved_sample_entry {
            self.tracks[track_index].chunks.push(Chunk {
                offset: sample.data_offset,
                sample_entry,
                samples: Vec::new(),
            });
        }

        self.tracks[track_index]
            .chunks
            .last_mut()
            .expect("bug")
            .samples
            .push(metadata);

        self.next_position = self
            .next_position
            .checked_add(sample.data_size as u64)
            .ok_or(MuxError::Overflow)?;
        self.last_sample_kind = Some(sample.track_kind);
```

`self.tracks` への push の **後に** `next_position` の `checked_add` で `Overflow` が発生し得る。Overflow 時は `?` で return するため `next_position` / `last_sample_kind` は未更新だが、`self.tracks` への変更は残留する。残留する副作用は 3 段ある。

1. `ensure_track_entry()`（`src/mux_mp4_file.rs:657-679`）による新規 `TrackEntry` の push（新規トラック種別の場合）
2. `Chunk` の push（新規チャンクが必要な場合）
3. サンプル metadata の push（常に発生）

実測（`advance_position()` で次の書き込み位置を `u64::MAX - 10` まで進めてから `data_size = 100` のサンプルを投入）では、`Overflow` を受け取った後に同じ `data_offset` で再投入すると成功し、`finalize()` 後の `stsz` エントリ数が 2 ではなく 3 になることを確認した。

### ドキュメントの現状

`append_sample` の doc（`src/mux_mp4_file.rs:552-564`）は、この非アトミック性を実装どおりに記述している。

```rust
/// - [`MuxError::Overflow`]（次の書き込み位置の加算オーバーフロー）:
///   このサンプルの登録は完了した状態で残り、次の書き込み位置だけが未更新となる。
///   同じサンプルで再呼び出しすると二重に登録されるため、
///   このエラーを受け取った [`Mp4FileMuxer`] は復旧できないものとして破棄すること
```

起票時点の doc は「エラー時は内部状態を変更しない」と書いており実装と食い違っていたが、0046 の対応で実装に合わせて訂正された。したがって現在は「doc と実装の不一致」ではなく「弱い契約が doc で追認されている」状態であり、本 issue は契約そのものを強化する提案になる。

なお 0046 は「単純化のため clone-then-swap による rollback は採用しない」と判断している（`issues/closed/0046-add-mp4-file-muxer-subtitle.md:151`）。本 issue はその判断を覆すことになるため、後述のとおり clone を伴わない低コストな方法を採る。

## 設計方針

`next_position` の `checked_add` を `ensure_track_entry()` の呼び出し **前** に行い、加算結果をローカル変数に保持する。`self.tracks` への 3 段の push はすべて `Overflow` チェックを通過した後にだけ実行し、末尾で `self.next_position` に代入する。

`checked_add` の挿入位置は `MixedSampleEntries` チェックの直後・`ensure_track_entry()` の直前とし、既存のエラー優先順位（`PositionMismatch` → `EncodeError` → `MissingSampleEntry` → `MixedSampleEntries` → `TimescaleMismatch` → `Overflow`）を変えない。

`Fmp4SegmentMuxer` の clone-then-commit パターン（`src/mux_fmp4_segment.rs:314` と `:407-414`）と同型にする必要はない。`checked_add` を前倒しすれば副作用がそもそも発生しなくなるため、clone もロールバックも不要である。

## 完了条件

- `Overflow` エラー時に `self.tracks` / `next_position` / `last_sample_kind` がすべて変更されないこと。`self.tracks` については `TrackEntry` の追加・`Chunk` の追加・サンプル metadata の追加のいずれも残らないこと
- エラー後に同じ `data_offset` で再呼び出しすると正常に登録されること（二重登録なし）
- `build_moov_box()` の「chunks が空のままの `TrackEntry` は生成されない」という不変条件（`src/mux_mp4_file.rs:849-855` のコメント）を壊さないこと
- `append_sample` の doc（`src/mux_mp4_file.rs:552-564`）から `Overflow` の例外扱いを削除し、全エラー種別で内部状態が不変であると記述すること
- 既存の `test_append_sample_error_keeps_muxer_state`（`EncodeError` 経路のみを検証）が引き続き通ること
- `CHANGES.md` の `## develop` に対応するエントリを追加すること（doc に書かれた契約を変更するため）
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が通ること

## 解決方法

1. `append_sample` で `next_position` の `checked_add` を `ensure_track_entry()` 呼び出しの前に移動し、結果をローカル変数に保持する
2. `self.tracks` への push をすべて成功確定後に行い、末尾で `self.next_position` を更新する
3. `Overflow` 経路を検証するテストを新規に追加する。既存の `test_append_sample_error_keeps_muxer_state` は `#[cfg(target_pointer_width = "64")]` 付きで `EncodeError` 経路専用のためそのまま維持し、`Overflow` は 32-bit でも到達可能なので `cfg` を付けない別テストにする。テストは `advance_position()` で次の書き込み位置を `u64::MAX` 付近まで進めて `Overflow` を発生させ、その後に同じ `data_offset` で再投入して `finalize()` 後の `stsz` エントリ数から二重登録が無いことを検証する
4. `append_sample` の doc の「# エラー返却時の内部状態」節を更新し、`Overflow` を不変側の列挙に統合する
