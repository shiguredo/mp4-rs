# `Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` の `sidx.starts_with_sap` / `sap_type` が EPT サンプルの実際の SAP 状態を反映していない

- Priority: Medium
- Created: 2026-07-30
- Completed: YYYY-MM-DD
- Model: claude-code claude-opus-4-7
- Branch: feature/fix-fmp4-sidx-starts-with-sap
- Polished: YYYY-MM-DD

## 目的

`Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` が組み立てる `sidx.references[0]` の `starts_with_sap` / `sap_type` を、EPT が指すサンプル自身の SAP 状態と整合させる。現行実装は decode 順先頭の参照トラックサンプルの `keyframe` を採るため、負 CTO を持つ B フレームが presentation 順先頭になる入力で ISO/IEC 14496-12 8.16.3.3 の `starts_with_SAP` 定義と齟齬が生じる。

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
        // ...
    }],
};
```

`first_track_kind = samples[0].track_kind` としているため、`.find(|s| s.track_kind == first_track_kind)` は必ず `samples[0]` にマッチする。したがって `first_sample_is_keyframe` は実質「decode 順で先頭のサンプルの `keyframe`」を採っている（`.find(...).map(...).unwrap_or(false)` の 3 段は `samples[0].keyframe` と等価な冗長イディオムでもある）。

一方 EPT は `compute_earliest_presentation_time` により **各サンプルの PTS（`DTS + composition_time_offset`）の最小値** で決まる。負 CTO を持つ B フレームが後段にある場合、EPT はその B フレームサンプルの PTS を指し得る。ISO/IEC 14496-12 8.16.3.3 の `starts_with_SAP` は「参照される subsegment が SAP から始まる」の意味であり、EPT に対応するアクセスユニットが SAP かどうかを問う。したがって現行の decode 順先頭の keyframe 情報は EPT サンプルの SAP 状態と一致しない。

失敗する具体例（映像単一トラック / 参照トラック = Video）:

- サンプル 0: I フレーム, `keyframe=true`, CTO=+50 → PTS=50
- サンプル 1: B フレーム, `keyframe=false`, CTO=-40 → PTS=60
- サンプル 2: B フレーム, `keyframe=false`, CTO=-180 → PTS=20 (これが EPT)

`compute_earliest_presentation_time` は正しく `EPT=20` を返すが、`starts_with_sap` は decode 順先頭の I フレームを見て `true`、`sap_type` も `1` が付く。EPT=20 に対応するのはサンプル 2 の B フレームであり、SAP ではないため、この 2 フィールドは虚偽になる。

## 設計方針

以下の 3 案から選ぶ:

- **案 A**: `Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx` の doc に「`starts_with_sap` は decode 順先頭の keyframe を採る近似値であり、B フレームが presentation 順先頭になる場合は不正確になる」旨を明記する（実装は変えない）
- **案 B**: EPT を採ったサンプル（`compute_earliest_presentation_time` で `min_pts` を採ったサンプル）の `keyframe` を `starts_with_sap` に採用する。`sap_type` は `keyframe ? 1 : 0` を採る
- **案 C**: ISO/IEC 14496-12 8.16.3.2 の SAP type 1〜6 の定義に沿って厳密判定する（B フレーム前後の再生開始点を評価する必要があり、コーデック依存の解析が要る）

**推奨は案 B**。上記失敗例では `starts_with_sap=false, sap_type=0` が正しく出るようになり、`starts_with_sap=1` の虚偽が消える。実装コストは小さい。案 C は仕様上完全だが、mp4-rust 側で SAP type を厳密判定するには B フレーム前後の参照解析が必要で、本 issue のスコープを超える。案 A は実装コスト最小だが実害が残る。

案 B の実装方針:

- `compute_earliest_presentation_time` が `min_pts` を採ったサンプルの `keyframe` も返せるように戻り値を拡張する（例: `Result<(u64, bool), MuxError>`）
- 呼び出し側 `create_media_segment_metadata_with_sidx` は返された `(ept, sap_at_ept)` を `SidxReference` にそのまま渡す
- 現行の `first_sample_is_keyframe` を計算する `.find(...).map(...).unwrap_or(false)` イディオムは不要になり、削除する（隣接コードの冗長も自然解消）

## 完了条件

- decode 順で先頭が I フレーム、EPT を採るサンプルが B フレーム（負 CTO で PTS が最小）である入力で `sidx.references[0].starts_with_sap == false && sap_type == 0` になること
- 従来ケース（EPT サンプル == decode 順先頭サンプル == I フレーム）では `starts_with_sap == true && sap_type == 1` が引き続き成り立つこと
- 参照トラック以外のサンプル（Audio 等）の `keyframe` に引きずられないこと
- 上記境界を `tests/test_mux_fmp4_segment.rs` で固定入力の単体テストとして検証すること
- `cargo test` / `cargo clippy --all-targets --all-features -- -D warnings` / `cargo fmt --all -- --check` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が通ること

## 解決方法

1. `src/mux_fmp4_segment.rs` の `compute_earliest_presentation_time` の戻り値を `Result<u64, MuxError>` から `Result<(u64, bool), MuxError>` に拡張し、`min_pts` を更新したサンプルの `keyframe` を第 2 要素で返す
2. `create_media_segment_metadata_with_sidx` は上記の戻り値から `(ept, sap_at_ept)` を取り出し、`SidxReference` の `starts_with_sap` に `sap_at_ept`、`sap_type` に `if sap_at_ept { 1 } else { 0 }` を渡す
3. 従来の `first_sample_is_keyframe` を計算する `.find(...).map(...).unwrap_or(false)` イディオムを削除する（EPT サンプル基準に置換されるため）
4. 上記「完了条件」に対応する単体テストを 2 本追加する
   - EPT サンプルが B フレームである入力で `starts_with_sap=false, sap_type=0` になること
   - EPT サンプルが I フレームである入力で `starts_with_sap=true, sap_type=1` になること
5. `CHANGES.md` の `## develop` に `[FIX]` エントリを追加する
