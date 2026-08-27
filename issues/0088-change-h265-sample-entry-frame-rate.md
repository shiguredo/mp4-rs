# H.265 sample entry のフレームレート設定を呼び出し側指定にする

- Created: 2026-08-27
- Completed: {YYYY-MM-DD}
- Branch: feature/change-h265-sample-entry-frame-rate
- Polished: {YYYY-MM-DD}

## 目的

H.265 の sample entry 構築時に、呼び出し側が把握している平均フレームレートと constant frame rate の状態を `hvcC` に記録できるようにする。

フレームレートを知らない場合の 0 / 0 は維持しつつ、録画・エンコード系の利用者が構築後の `HvccBox` を直接書き換えなくてもよい公開契約にする。

## 現状

- `src/bitstream/h265.rs` の `H265SampleEntryConfig` は `length_size` だけを持つ
- `build_hvcc_box_and_visual` は `HvccBox::avg_frame_rate` と `constant_frame_rate` を常に 0 に固定する
- ISO/IEC 14496-15:2022 8.3.2.1.3 では `avgFrameRate = 0` は未指定を表すが、非ゼロ値を呼び出し側が指定することもできる
- 同節の `constantFrameRate` は 0 / 1 / 2 に意味があり、3 は予約値である
- Hisui の `src/video/h265.rs` は出力パイプラインのフレームレートと CFR 方針を把握しているが、現行 API へ移行すると構築後の `HvccBox` を書き換える必要がある
- 公開 struct にフィールドを追加すると既存の struct literal がコンパイルできなくなるため、値のデフォルトが 0 であっても Rust の API としては後方互換のない変更になる

## 設計方針

`src/bitstream/h265.rs` に `constantFrameRate` の 3 状態だけを表せる enum を追加する。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum H265ConstantFrameRate {
    Unknown,
    Constant,
    ConstantPerTemporalLayer,
}
```

各 variant は ISO/IEC 14496-15:2022 8.3.2.1.3 の 0 / 1 / 2 に対応させる。予約値 3 は型で表現できないようにする。Copy な enum の変換メソッドは `self` で受ける。

`H265SampleEntryConfig` を次の形に変更する。

```rust
pub struct H265SampleEntryConfig {
    pub length_size: LengthSize,
    pub avg_frame_rate: u16,
    pub constant_frame_rate: H265ConstantFrameRate,
}
```

`avg_frame_rate` は `hvcC` の 16 ビット raw 値とし、0 は未指定、非ゼロ値は 256 秒あたりのフレーム数を表す。利用側固有の `FrameRate` 型や丸め方は持ち込まない。

`build_hev1_box` / `build_hvc1_box` と Annex B 版の構築関数は、設定値を `build_hvcc_box_and_visual` 経由で `HvccBox` に写す。既存の VPS / SPS / PPS 解析や VUI からの推定は変更しない。

あわせて `src/boxes_sample_entry.rs` の `HvccBox::avg_frame_rate` / `constant_frame_rate` の doc コメントを仕様の単位と意味に合わせる。「CBR / VBR」はビットレートの用語なので、constant frame rate の説明には使わない。

### テスト

- `tests/test_bitstream_h265.rs` の既存 config literal を新しい公開契約へ更新する
- 0 / `Unknown` が従来と同じ box を生成することを確認する
- 非ゼロ `avg_frame_rate` と `Constant` / `ConstantPerTemporalLayer` が `HvccBox` に失われず写ることを確認する
- `pbt/tests/prop_bitstream_h265.rs` の生成 config を更新し、任意の `u16` と 3 状態が構築結果へ写ることを確認する
- mock / stub、外部 command、ネットワークは使用しない

### 変更履歴

`CHANGES.md` の develop に `[CHANGE]` として、`H265SampleEntryConfig` の公開フィールド追加と既存 struct literal の更新が必要なことを記載する。

## 完了条件

- `H265SampleEntryConfig` から `avg_frame_rate` と constant frame rate 状態を指定できる
- `constantFrameRate` の予約値 3 を公開設定型で表現できない
- 0 / `Unknown` で従来の 0 / 0 が生成される
- Hisui 固有の `FrameRate` 型や CFR 強制方針が追加されない
- `HvccBox` の関連 doc コメントが仕様と一致する
- 決定的テストと PBT が更新される
- `CHANGES.md` が更新される
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る
