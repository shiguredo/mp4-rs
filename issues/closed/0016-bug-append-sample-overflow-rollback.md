# mux_mp4_file.rs の append_sample が Overflow 時に tracks への変更を残したままエラーを返す

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-29
- Model: opencode-go glm-5.2
- Branch: feature/fix-append-sample-overflow-state
- Polished: 2026-07-29
- Updated: 2026-07-27

## 目的

`Mp4FileMuxer::append_sample` が `MuxError::Overflow` 返却時に `self.tracks` への変更を残したまま return し、エラー後もインスタンスを使い続けるとサンプルが二重登録される問題を修正する。

`append_sample` の失敗パスのうち `Overflow` だけが非アトミックであり、呼び出し側にミューサの破棄を強制している。この非対称を解消し、全エラー種別で内部状態が不変になるようにする。修正は clone やロールバックではなく、副作用が発生する前に `checked_add` を完了させる方式とする。

## 優先度根拠

`Overflow` を受け取った後の `Mp4FileMuxer` は復旧できず、インスタンスを破棄するしかない。他のすべてのエラー種別は内部状態が完全に不変で再呼び出しできるため、`Overflow` だけが例外になっている。

到達には `advance_position()` で次の書き込み位置を `u64::MAX` 付近まで進める必要があり、実データでは到達しない（`data_size` は直前で `u32::MAX` 以下に制限されるため、`next_position > u64::MAX - u32::MAX` が必要）。ただし `advance_position()` は Rust の公開 API であり、C API（`crates/c-api/src/mux.rs:863` `mp4_file_muxer_advance_position`）からも到達できる。

「現実的には起きないが、暗黙のデータ破壊よりも明示的な正しさを優先する」という点で、同種の防御的修正（例: `issues/0032-bug-mux-stbl-saturating-add.md`）と同じ論法に立つ。ただし 0032 が「エラーを返さずに壊れた MP4 を出す」のに対し、本 issue は「エラーは返るが、破棄せず使い続けると二重登録で壊れる」であり危険度は一段低い。

0046 は Overflow 時の副作用を維持し、clone-then-commit による rollback を採用しないと判断した（`issues/closed/0046-add-mp4-file-muxer-subtitle.md:149-151`）。ただし 0046 本文は同一 `data_offset` の再試行が `PositionMismatch` になり事実上 rollback 不能と見積もっており、これは誤りである。`Overflow` 時は `next_position` が更新されないため、同一 `data_offset` の再試行は `PositionMismatch` を通過し、呼び出しのたびに `self.tracks` への push が積み増される。0046 が実装に合わせて書いた doc（`src/mux_mp4_file.rs:561-564`）は二重登録を正しく警告している。本 issue は、その誤った見積もりを正したうえで、clone を伴わない低コストな方法で契約を強化する。

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

実測では、先に通常サイズのサンプルを 1 つ登録したうえで `advance_position()` により次の書き込み位置を `u64::MAX - 10` まで進め、`data_size = 100` のサンプルを投入して `Overflow` を受け取る。`advance_position()` は `size > 0` のとき `last_sample_kind` を `None` にリセットするため、直後の `append_sample` では `is_new_chunk_needed` が `true` になり、**新規 `Chunk` の push とそのサンプル metadata の push** が 1 回の失敗で残留する。その直後に `finalize()` すると、意図した `stsz` / `stco` エントリ数 1 に対してそれぞれ 2 になる。同一サンプル（同一 `data_offset` / `data_size`）で再試行しても `Overflow` は再現するが、そのたびに新規 `Chunk` と metadata がさらに積み増される。

### ドキュメントの現状

`append_sample` の doc（`src/mux_mp4_file.rs:552-564`）は、この非アトミック性を実装どおりに記述している。

```rust
/// - [`MuxError::Overflow`]（次の書き込み位置の加算オーバーフロー）:
///   このサンプルの登録は完了した状態で残り、次の書き込み位置だけが未更新となる。
///   同じサンプルで再呼び出しすると二重に登録されるため、
///   このエラーを受け取った [`Mp4FileMuxer`] は復旧できないものとして破棄すること
```

起票時点の doc は「エラー時は内部状態を変更しない」と書いており実装と食い違っていたが、0046 の対応で実装に合わせて訂正された。したがって現在は「doc と実装の不一致」ではなく「弱い契約が doc で追認されている」状態であり、本 issue は契約そのものを強化する提案になる。

## 設計方針

`next_position` の `checked_add` を `ensure_track_entry()` の呼び出し **前**（`MixedSampleEntries` チェックの直後）に行い、加算結果をローカル変数に保持する。`self.tracks` への 3 段の push はすべて `Overflow` チェックを通過した後にだけ実行し、末尾で `self.next_position` / `self.last_sample_kind` を更新する。

この順序では、既存トラックの `timescale` 不一致と `Overflow` が同時に成立する入力に対し、返るエラーが現状の `TimescaleMismatch` から `Overflow` に変わる。両方が同時に成立する入力は病理的であり、どちらのエラーでも呼び出し側は入力を変えない限り再試行できない。副作用なしの timescale 事前検査を足して優先順位を厳密維持する案は採らない。検査の二重化と、`ensure_track_entry()` 内の `TimescaleMismatch` 分岐の到達不能化を招くためである。単純な前倒しを採り、優先順位のこの点だけが変わることを CHANGES.md の `[FIX]` エントリにも短く書く。

`Fmp4SegmentMuxer` の clone-then-commit パターン（`src/mux_fmp4_segment.rs` の `build_media_segment_bytes` 内、tracks の clone 開始は 364 行付近、確定代入は 463 行付近）と同型にする必要はない。副作用前に検査を完了させれば clone もロールバックも不要である。

## 完了条件

- `Overflow` エラー時に `self.tracks` / `next_position` / `last_sample_kind` がすべて変更されないこと。`self.tracks` については `TrackEntry` の追加・`Chunk` の追加・サンプル metadata の追加のいずれも残らないこと
- エラー後に同じ `data_offset` かつオーバーフローしない `data_size` で再呼び出しすると正常に登録されること（二重登録なし）
- `build_moov_box()` の「chunks が空のままの `TrackEntry` は生成されない」という不変条件（`src/mux_mp4_file.rs:854-859` のコメント）を壊さないこと
- `append_sample` の doc（`src/mux_mp4_file.rs:552-564`）から `Overflow` の例外扱いを削除し、全エラー種別で内部状態が不変であると記述すること
- 既存の `test_append_sample_error_keeps_muxer_state`（`EncodeError` 経路のみを検証）および既存の `TimescaleMismatch` テストが引き続き通ること
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加すること。主旨は Overflow 時の状態残留の修正である。病理的入力での `TimescaleMismatch` / `Overflow` 優先順位の変化は、同一 `[FIX]` エントリ内の注記とし、別立ての `[CHANGE]` は不要とする（どちらも拒否であり、同時成立は `u64::MAX` 近傍でのみ起きる）
- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm -- -D warnings` / `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が通ること

## 解決方法

`feature/fix-append-sample-overflow-state` ブランチで対応した。

### 実施内容

- `append_sample` で `next_position` の `checked_add` を `MixedSampleEntries` チェックの直後・`ensure_track_entry()` 呼び出しの前に移し、加算結果をローカル変数に保持してから `self.tracks` へ push するようにした
- `Overflow` 直後に `tracks` / `chunks` / `samples` が増えていないことと、収まる `data_size` での再投入後に `stsz` が二重登録されないことを検証するテストを追加した
- 既存トラックの `timescale` 不一致と `Overflow` が同時に成立するとき `Overflow` が先に返る回帰テストを追加した
- `append_sample` の「エラー返却時の内部状態」doc を、エラー時は状態不変で再呼び出し可能である旨に簡潔化した
- `CHANGES.md` の `## develop` に `[FIX]` を追記した（病理的入力での優先順位変化も同一エントリ内に注記）

### 計画から外れた点

- issue 文面ではエラー種別を列挙して「全エラー種別で不変」と書く想定だったが、修正後は種別ごとの例外が無くなったため、状態不変と再呼び出し可能性だけを書く形にした
- `Sample::timescale` の doc に Overflow 優先の補足を書く案は採らなかった（フィールドの本来の契約は `TimescaleMismatch` であり、病理的同時成立は CHANGES と回帰テストで足りる）

### 検証

- `cargo fmt` / `cargo clippy -D warnings` / 関連単体テストが通ることを確認した
- `/review-diff-code` の重要指摘（expect 日本語化、優先順位回帰、CHANGES 文言、中間状態アサート、doc 簡潔化）を反映した
