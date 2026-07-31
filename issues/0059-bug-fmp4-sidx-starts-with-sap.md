# `Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` の `sidx.starts_with_sap` / `sap_type` が EPT サンプルの実際の SAP 状態を反映していない

- Priority: Medium
- Created: 2026-07-30
- Completed: YYYY-MM-DD
- Model: claude-code claude-opus-4-7
- Branch: feature/fix-fmp4-sidx-starts-with-sap
- Polished: 2026-07-31

## 目的

`Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` が組み立てる `sidx.references[0]` の `starts_with_sap` / `sap_type` を、EPT が指すサンプル自身の SAP 状態と整合させる。現行実装は samples[] 内で参照トラックに該当する最初のサンプル（実質 `samples[0]`）の `keyframe` を採るため、負 CTO を持つ B フレームが presentation 順先頭になる入力で ISO/IEC 14496-12 8.16.3.3 の `starts_with_SAP` 定義と齟齬が生じる。

## 優先度根拠

`starts_with_sap` は DASH クライアントが subsegment を SAP から復号できるかを判定する材料として使う。EPT のサンプルが B フレームであるにもかかわらず `starts_with_sap=1` が付いていると、SAP を期待して復号を開始したプレイヤーが破綻する、シーク位置がずれる、初回再生時に黒フレームが挿入されるなどの誤動作が起き得る。0023（`earliest_presentation_time` の CTO 反映）が closed になったことで、EPT が B フレームサンプルを指すケースが正しく計算されるようになり、`starts_with_sap` との不整合が顕在化しやすくなった。B フレーム負 CTO を含む映像で fMP4 sidx を DASH 用途に使う限られた場面で顕在化するが、実害は具体的（DASH プレイヤーの再生破綻）。

## 現状

`src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` は次のロジックで `starts_with_sap` を決めている:

```rust
let first_sample_is_keyframe = samples
    .iter()
    .find(|s| s.track_kind == first_track_kind)
    .map(|s| s.keyframe)
    .unwrap_or(false);
// ...
let sidx_box = SidxBox {
    // ...
    references: vec![SidxReference {
        // ...
        starts_with_sap: first_sample_is_keyframe,
        sap_type: if first_sample_is_keyframe { 1 } else { 0 },
        sap_delta_time: 0,
    }],
};
```

`first_track_kind = samples[0].track_kind` としているため、`.find(|s| s.track_kind == first_track_kind)` は必ず `samples[0]` にマッチする。したがって `first_sample_is_keyframe` は実質「samples[] 内で参照トラックに該当する最初のサンプル（= `samples[0]`）の `keyframe`」を採っている（`.find(...).map(...).unwrap_or(false)` の 3 段は `samples[0].keyframe` と等価な冗長イディオムでもある）。

一方 EPT は `compute_earliest_presentation_time` により **参照トラックの各サンプルの PTS（`DTS + composition_time_offset`）の最小値** で決まる。負 CTO を持つ B フレームが後段にある場合、EPT はその B フレームサンプルの PTS を指し得る。ISO/IEC 14496-12 8.16.3.3 の `starts_with_SAP` は「参照される subsegment が SAP から始まる」の意味であり、EPT に対応するアクセスユニットが SAP かどうかを問う。したがって現行の `samples[0]` の keyframe 情報は EPT サンプルの SAP 状態と一致しない。

失敗する具体例（映像単一トラック / 参照トラック = Video / セグメント先頭のトラック累積 `decode_time = 0` / 各サンプル `duration = 100`）:

| samples[] index | 種別 | `keyframe` | DTS | CTO | PTS |
| --- | --- | --- | --- | --- | --- |
| 0 | I フレーム | `true` | 0 | +50 | 50 |
| 1 | B フレーム | `false` | 100 | -40 | 60 |
| 2 | B フレーム | `false` | 200 | -180 | 20 (最小 = EPT) |

`compute_earliest_presentation_time` は正しく `EPT = 20` を返すが、`starts_with_sap` は `samples[0]` の I フレームを見て `true`、`sap_type` も `1` が付く。EPT = 20 に対応するのはサンプル 2 の B フレームであり、SAP ではないため、この 2 フィールドは虚偽になる。

## 設計方針

以下の 3 案から選ぶ:

- **案 A**: `Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` の doc に「`starts_with_sap` は `samples[0]` の `keyframe` を採る近似値であり、B フレームが presentation 順先頭になる場合は不正確になる」旨を明記する（実装は変えない）
- **案 B**: EPT を採ったサンプル（`compute_earliest_presentation_time` で `min_pts` を採ったサンプル）の `keyframe` を `starts_with_sap` に採用する。`sap_type` は「`keyframe` → `1`、非 `keyframe` → `0`」の近似を維持し、SAP type 1〜6 の厳密判定は行わない
- **案 C**: ISO/IEC 14496-12 8.16.3 が参照する SAP type 1〜6 の定義（詳細は同規格の SAP semantics に基づく）に沿って厳密判定する。SAP type 1（closed GoP）/ 2（closed GoP with leading pictures）/ 3（open GoP）等の区別は B フレーム前後の参照解析（NALU 内部のパースとコーデック依存の DPB 状態追跡）を新規に持ち込む必要があり、mp4-rust が現在触っていない領域

**推奨は案 B**。上記失敗例では `starts_with_sap = false, sap_type = 0` が正しく出るようになり、`samples[0]` の keyframe を採ることによる虚偽が消える。`sap_type` については案 C の厳密判定を行わないため open GoP の I フレームが EPT のときは `sap_type = 1` の近似誤差が残るが、これは現行実装と同じ近似度であり悪化しない。案 A は不正確な値がそのまま残る。案 C は NALU 解析の新規導入が必要で本 issue のスコープを超える。

**案 B の実装方針**（高レベル）:

- `compute_earliest_presentation_time` の戻り値を `(u64, bool)` タプルに拡張し、`min_pts` を更新したサンプルの `keyframe` を第 2 要素で返す
- 呼び出し側 `create_media_segment_metadata_with_sidx` は返された第 2 要素を `SidxReference::starts_with_sap` に、その値から導いた `0` / `1` を `sap_type` に渡す

関数名維持・PTS 同値時の第 2 要素の扱い（先勝ち）・`sap_delta_time` 据置・現行 `first_sample_is_keyframe` イディオムの削除・doc 更新の具体手順は「## 解決方法」に記す

## 完了条件

- `samples[0]` が I フレーム、EPT を採るサンプルが B フレーム（負 CTO で PTS が最小）である入力で `sidx.references[0].starts_with_sap == false && sap_type == 0` になること
- 従来ケース（EPT サンプル == `samples[0]` == I フレーム）では `starts_with_sap == true && sap_type == 1` が引き続き成り立つこと
- 参照トラック以外のサンプル（Audio 等）の `keyframe` に引きずられないこと（`compute_earliest_presentation_time` は既に参照トラック filter を持つため、案 B の戻り値拡張がその filter を継承する形で書けば自動的に満たされる。将来の実装変更で filter が外れる回帰を検出するため、単体テストで固定する）
- 上記境界を `tests/test_mux_fmp4_segment.rs` で固定入力の単体テストとして検証すること
- `cargo test` / `cargo clippy --all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が通ること

## 解決方法

### 実装

1. `src/mux_fmp4_segment.rs` の `compute_earliest_presentation_time` の戻り値を `Result<u64, MuxError>` から `Result<(u64, bool), MuxError>` に拡張し、`min_pts` を更新したサンプルの `keyframe` を第 2 要素で返す。関数名は現行維持
2. 第 2 要素は「PTS が厳密に減少するときのみ更新する」形で追跡し、PTS 同値時は先出現のサンプルの `keyframe` を保持する。現行の `min_pts.map_or(pts, |current| current.min(pts))` は `Ord::min` が等値時にレシーバ側（先出現）を返すため `min_pts` 値の先勝ちが自然成立するが、第 2 要素の keyframe 追跡は `<=` で更新すると後勝ちに化けるため、明示的に厳密減少で書く
3. `create_media_segment_metadata_with_sidx` は拡張後の戻り値から `(ept, sap_at_ept)` を取り出し、`SidxReference` の `starts_with_sap` に `sap_at_ept`、`sap_type` に `if sap_at_ept { 1 } else { 0 }` を渡す
4. `SidxReference::sap_delta_time` は現行の `0` のまま変更しない（EPT サンプルが SAP でない場合に subsegment 内の SAP を厳密に探索して `T_SAP - EPT` を報告するのは案 C 相当の解析が必要になるため、本 issue のスコープ外とする）
5. 現状の引用ブロックにある `first_sample_is_keyframe` を計算する `.find(...).map(...).unwrap_or(false)` イディオムを削除する（EPT サンプル基準の値に置換されるため不要になる）

### テスト

`tests/test_mux_fmp4_segment.rs` に単体テスト 3 本を追加する:

- EPT サンプルが B フレームである入力で `starts_with_sap = false, sap_type = 0` になること
- EPT サンプルが I フレームである入力で `starts_with_sap = true, sap_type = 1` になること
- 非参照トラック（Audio）のサンプルが全て `keyframe = true` でも、参照トラック（Video）の EPT サンプルが B フレームなら `starts_with_sap = false` になること（参照トラック filter の回帰防止）。Video を `samples[0]` に置き、Video 群を前半・Audio 群を後半に `data_offset` 連続で配置する（参照トラックは `samples[0].track_kind` により決まり、そのトラックの `track_id` が `sidx.reference_id` に書かれる。また `resolve_segment_tracks` は同一トラック内での `data_offset` 連続配置を要求する。既存の `sidx_ept_ignores_non_reference_track_samples` が同じ配置パターンで先例）

既存のテストヘルパー `video_sample_with_timing` / `audio_sample_with_timing` は `keyframe: true` を固定しているため、上記テストで `keyframe = false` を組み立てるにはいずれかを選ぶ:

- ヘルパーに `keyframe: bool` 引数を追加し、既存呼び出しでは `true` を明示する（呼び出し箇所の一括修正が必要）
- 新規テストでのみ `Sample { .. }` を直接組み立てる（既存呼び出しには影響しない）

### ドキュメント

- `compute_earliest_presentation_time` の doc に、戻り値第 2 要素が「`min_pts` を採ったサンプルの `keyframe`」であることと、PTS 同値時に samples[] 内で先のサンプルを採る挙動を追記する
- `create_media_segment_metadata_with_sidx` の doc に、`starts_with_sap` / `sap_type` が EPT サンプル基準で決まること、および `sap_type` は「`keyframe` → `1`、非 `keyframe` → `0`」の近似で SAP type 1〜6 の厳密判定は行わないことを追記する
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加する
