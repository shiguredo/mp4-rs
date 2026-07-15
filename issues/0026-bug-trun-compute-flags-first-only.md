# boxes_fmp4.rs の TrunBox::compute_flags が先頭サンプルのみで per-sample フラグを決定し後続フィールドが落ちる

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-trun-compute-flags-first-only
- Polished: YYYY-MM-DD

## 目的

`TrunBox::compute_flags` が per-sample フラグ（duration / size / flags / composition_time_offset の有無）を先頭サンプルのみで決定しており、先頭が `None`・後続が `Some` のときフラグが立たず後続フィールドがエンコード時に落ちる問題を修正する。

## 優先度根拠

公開 `TrunBox::encode` として、不整合入力を黙って潰してデータ消失を起こす。先頭 `duration=None`・2 番目 `duration=Some(100)` で encode → decode すると両方 `None` になる。ISO BMFF 上 trun の flag は run 全体共通だが、`Option` の不整合を黙って潰す点が問題。

## 現状

```rust
// src/boxes_fmp4.rs:569-582
if let Some(sample) = self.samples.first() {
    if sample.duration.is_some() {
        flags |= Self::FLAG_SAMPLE_DURATION_PRESENT;
    }
    if sample.size.is_some() {
        flags |= Self::FLAG_SAMPLE_SIZE_PRESENT;
    }
    if sample.flags.is_some() {
        flags |= Self::FLAG_SAMPLE_FLAGS_PRESENT;
    }
    if sample.composition_time_offset.is_some() {
        flags |= Self::FLAG_SAMPLE_COMPOSITION_TIME_OFFSETS_PRESENT;
    }
}
```

`self.samples.first()` のみで per-sample フラグを決定する。先頭が `None`・後続が `Some` だとフラグが立たず、encode ループ（619-642 行）で該当フィールドが全サンプルで出力されない。逆（先頭 `Some`・後続 `None`）は flag が立ち、後続は `unwrap_or(0)` で 0 が書かれる。

## 設計方針

全サンプルの `Option` 有無を OR で集約し、いずれかのサンプルで `Some` ならフラグを立てる。または、`Option` の有無がサンプル間で不整合な場合は `Err` を返す。ISO BMFF の trun flag は run 全体共通であるため、入力の整合性を保証する方が安全。

## 完了条件

- 先頭 `None`・後続 `Some` でも後続フィールドが正しくエンコードされること
- または不整合入力で `Err` を返すこと
- 全サンプルが `None` の場合は従来どおりフラグが立たないこと
- roundtrip でデータが一致すること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `compute_flags` で `self.samples.first()` の代わりに `self.samples.iter().any(|s| s.duration.is_some())` 等、全サンプルを確認する
2. または encode 前に全サンプルの `Option` 有無が一致するか検証し、不整合なら `Err` を返す
3. 先頭 `None`・後続 `Some` の roundtrip テストを追加する
