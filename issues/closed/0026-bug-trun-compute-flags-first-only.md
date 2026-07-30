# boxes_fmp4.rs の TrunBox::compute_flags が先頭サンプルのみで per-sample フラグを決定し後続フィールドが落ちる

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-30
- Model: opencode-go glm-5.2
- Branch: feature/fix-trun-compute-flags-first-only
- Polished: 2026-07-30

## 目的

`TrunBox::compute_flags` が per-sample フラグ（duration / size / flags / composition_time_offset の有無）を先頭サンプルのみで決定しており、先頭が `None`・後続が `Some` のときフラグが立たず後続フィールドがエンコード時に落ちる問題を修正する。ISO BMFF の trun flag は run 全体共通であるため、サンプル間で `Option` 有無が不整合な入力は黙って潰さず `Err` で拒否する。

## 優先度根拠

公開 `TrunBox::encode` として、不整合入力を黙って潰してデータ消失を起こす。先頭 `duration=None`・2 番目 `duration=Some(100)` で encode → decode すると両方 `None` になる。ISO BMFF 上 trun の flag は run 全体共通だが、`Option` の不整合を黙って潰す点が問題。

## 現状

```rust
// src/boxes_fmp4.rs の TrunBox::compute_flags
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

`self.samples.first()` のみで per-sample フラグを決定する。先頭が `None`・後続が `Some` だとフラグが立たず、`Encode for TrunBox` のサンプル書き出しループで該当フィールドが全サンプルで出力されない。逆（先頭 `Some`・後続 `None`）は flag が立ち、後続は `unwrap_or(0)` で 0 が書かれる。

`TrunSample` の doc では duration / size / flags の `None` は `TfhdBox` / `TrexBox` の default への委譲を意味する。OR 集約でフラグを立てると `None` が `unwrap_or(0)` により明示値 0 として書き出され、別種の黙殺になるため採用しない。

なお `mux_fmp4_segment` の moof 構築は duration / size / flags を常に `Some` にし、`composition_time_offset` も `has_any_cto` で全サンプルの有無を揃えているため、本バグは主に `TrunBox` を直接組み立てて encode する経路で顕在化する。`pbt/tests/prop_fmp4_boxes.rs` の `arb_trun_box` もサンプル間の Option 一貫性を強制している。

## 設計方針

サンプル間で各 per-sample フィールド（duration / size / flags / composition_time_offset）の `Option` 有無が一致しない場合は、`Encode for TrunBox` から `Error::invalid_input` を返す。全サンプルで揃っている場合のみ、従来どおりその有無に応じてフラグを立てる。

closed `0014`（負値と `> i32::MAX` の CTO 混在を encode 時に拒否）と同じく、trun として表現できない入力は黙殺せずエラーにする。

検証は `FullBoxHeader::from_box(self).encode(...)` より前に行うこと。現行の `Encode for TrunBox` はヘッダ書き込み時に `full_box_flags()` → `compute_flags()` を呼ぶため、検証を後段だけに置くと不正フラグがバッファに書かれたあとに `Err` になり得る。

整合性が保証された入力では `samples.first()` でも `iter().any(...)` でもフラグ結果は同じになる。実装は検証ロジックと `compute_flags` のどちらをどう整理してもよいが、不整合入力がヘッダに不正フラグを残さないことと、整合入力の挙動が従来どおりであることを満たすこと。

## 完了条件

- duration / size / flags / composition_time_offset のいずれかについて、サンプル間で `Option` 有無が不整合な入力に対し `TrunBox::encode` が `Error::invalid_input` を返すこと
- 全サンプルが `None` の場合は従来どおり対応フラグが立たないこと
- 全サンプルが `Some` の場合は対応フラグが立ち、encode → decode の roundtrip でデータが一致すること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

### 実装

1. `src/boxes_fmp4.rs` の `Encode for TrunBox` に `validate_sample_option_consistency` を追加し、`FullBoxHeader` 書き込みより前に呼ぶ。duration / size / flags / composition_time_offset の `Option` 有無が全サンプルで一致するかを先頭サンプルと順次比較し、不整合ならサンプル index・フィールド名・両側の Option 有無を含む `Error::invalid_input` を返す
2. `compute_flags` を `self.samples.first()` ベースから `self.samples.iter().any(...)` ベースに変更し、`uses_version_1` と流儀を揃える。これで `FullBox::full_box_flags()` を直接呼ばれても「どのサンプルかに Some があればフラグを立てる」決定論的な値を返す

### テスト

1. `tests/test_boxes_fmp4.rs` に `trun_sample_option_consistency` モジュールを追加し、先頭 `None`・後続 `Some` と先頭 `Some`・後続 `None` の 8 ケース（4 フィールド × 2 方向）で `InvalidInput` を検証する
2. `pbt/tests/prop_fmp4_boxes.rs` に `arb_trun_box_inconsistent` strategy と `trun_box_inconsistent_option_is_invalid_input` を追加し、サンプル数 2〜5・反転位置 1〜(count-1) をランダマイズして `iter().skip(1)` の縮退バグ検出網を張る
3. 単体・PBT の両方で `err.reason.contains("inconsistent Option presence")` を確認し、`TrunBox::encode` の別 `InvalidInput` 経路（cto 範囲外など）を「合格」と誤認するリスクを消す

### ドキュメント

- `TrunBox` の doc に「事前条件」節を追加し、per-sample `Option` 有無が run 全体で一致すべきこと・違反時は `Encode::encode` が `InvalidInput` を返すことを明記する
- `TrunSample` の doc にその事前条件へのポインタを追加する
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加する（両方向の情報損失例・`Fmp4SegmentMuxer` 内部利用への影響なし・`compute_flags` の `iter().any()` 化を明記）

### 残作業

- `TrunBox` を直接組み立てる fuzz ターゲット（`fuzz_trun_box_encode` 相当）は未追加。既存 `fuzz_trun_box` は decode 起点で validate 経路に到達しない。必要になれば別 issue で対応する
- テストヘルパー名 (`empty_sample` / `trun_with_inconsistent_field`) やモジュール名 (`encode_variable_uint_insufficient_buffer`) の命名・シグネチャは改善余地あり（実害なし）
