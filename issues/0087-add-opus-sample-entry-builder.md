# Opus の sample entry 構築 API を追加する

- Created: 2026-08-27
- Completed: {YYYY-MM-DD}
- Branch: feature/add-opus-sample-entry-builder
- Polished: {YYYY-MM-DD}

## 目的

Opus の codec private 情報から、ISOBMFF の固定値と `dOps` の対応関係を満たす `OpusBox` を構築できるようにする。

利用側で `AudioSampleEntryFields` と `DopsBox` を手組みする必要をなくし、AAC や映像コーデックと同様に sample entry 構築の正規形を `bitstream` モジュールで提供する。

## 現状

- `src/boxes_sample_entry.rs` には `OpusBox` と `DopsBox` があるが、両者を仕様どおりに組み立てる API はない
- `DopsBox` の encode / decode は `ChannelMappingFamily = 0` だけに対応している
- Hisui の `src/audio/opus.rs` は `AudioSampleEntryFields`、`DopsBox`、空の `unknown_boxes` を利用側で直接組み立てている
- Opus の ISOBMFF mapping は `AudioSampleEntryFields::samplesize = 16`、`samplerate = 48000` を要求し、`dOps` の `OutputChannelCount` などを codec private 情報から写す

参照仕様は Encapsulation of Opus in ISO Base Media File Format とする。

<https://www.opus-codec.org/docs/opus_in_isobmff.html>

## 設計方針

`src/bitstream/opus.rs` を新設し、`src/bitstream.rs` から `pub mod opus;` として公開する。

公開 API は次の形とする。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelCount {
    Mono,
    Stereo,
}

impl ChannelCount {
    pub const fn as_u8(self) -> u8;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpusSampleEntryConfig {
    pub channel_count: ChannelCount,
    pub pre_skip: u16,
    pub input_sample_rate: u32,
    pub output_gain: i16,
}

pub fn build_opus_box(config: &OpusSampleEntryConfig) -> OpusBox;
```

構築時の固定値は次のとおり。

- `AudioSampleEntryFields::data_reference_index` = `DEFAULT_DATA_REFERENCE_INDEX`
- `AudioSampleEntryFields::samplesize` = `DEFAULT_SAMPLESIZE`（16）
- `AudioSampleEntryFields::samplerate` = 48000 Hz
- `AudioSampleEntryFields::channelcount` = `channel_count` の `as_u8()`
- `OpusBox::unknown_boxes` = 空 `Vec`
- `DopsBox` の 4 フィールド = `OpusSampleEntryConfig` の対応する値

現行 `DopsBox` が固定する `ChannelMappingFamily = 0` は mono / stereo の family なので、チャンネル数は `ChannelCount` enum の `Mono` / `Stereo` だけで表現する。実行時検証ではなく型で不正値を排除し、対応していない multistream の box を生成できないようにする。

`pre_skip` / `input_sample_rate` / `output_gain` は codec private 情報の値をそのまま保持する。Hisui 固有の stereo / 48 kHz input / gain 0 は固定しない。Audio Sample Entry の `samplerate = 48000` と `DopsBox::input_sample_rate` は別の意味なので混同しない。

### テスト

- `tests/test_bitstream_opus.rs` に `ChannelCount` の値と mono / stereo の構築結果を追加する
- `pbt/tests/prop_bitstream_opus.rs` に mono / stereo と任意の `pre_skip` / `input_sample_rate` / `output_gain` が失われず `OpusBox` に写ることを確認する PBT を追加する
- mock / stub、外部 command、ネットワークは使用しない

### 変更履歴

`CHANGES.md` の develop に `[ADD]` として `bitstream::opus` と `OpusBox` 構築 API の追加を記載する。

## 完了条件

- `bitstream::opus::ChannelCount` / `OpusSampleEntryConfig` / `build_opus_box` が公開される
- family 0 で正しい mono / stereo の `OpusBox` が構築される
- Audio Sample Entry の sample size と sample rate が仕様の固定値になる
- 対応していないチャンネル数が型として存在しない
- 決定的テストと PBT が追加される
- `CHANGES.md` が更新される
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る
