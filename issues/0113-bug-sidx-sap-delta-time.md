# `Fmp4SegmentMuxer` の `sidx` が、サブセグメントの先頭 SAP と `SAP_delta_time` を書いていない

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-sidx-sap-delta-time
- Polished: {YYYY-MM-DD}

## 目的

`sidx` の `starts_with_SAP` と `SAP_delta_time` を、ISO/IEC 14496-12:2022 の 8.16.3.3 と Table 6 に合わせる。EPT を採ったサンプルがキーフレームかどうかではなく、デコード順で最初の同期サンプルがサブセグメントの先頭かどうかで書く。

## 現状

8.16.3.3 の `starts_with_SAP` は、参照されるサブセグメントが SAP で始まるかを示す。`SAP_delta_time` は、デコード順で最初の SAP の TSAP を示す。TSAP は Annex I の用語であり、そのアクセスユニットの presentation time（TPTF）とは限らない。SAP があるときは、サブセグメントの earliest presentation time と TSAP の差である。サブセグメントが SAP で始まるとき、この差は 0 になり得る。SAP が無いときは 0 で予約である。

Table 6 では、メディア参照（`reference_type = 0`）について次のように分かれる。

- `starts_with_SAP = 1` かつ `SAP_type` が 1 以上: そのタイプの SAP でサブセグメントが始まる
- `starts_with_SAP = 0` かつ `SAP_type` が 1 以上: SAP を含むが、それで始まらないことがある。最初のそのタイプの SAP が `SAP_delta_time` に対応する
- `starts_with_SAP = 0` かつ `SAP_type = 0`: SAP の情報は無い

`src/mux_fmp4_segment.rs` の `create_media_segment_metadata_with_sidx` は、EPT を採ったサンプルの `keyframe` を `starts_with_sap` にし、`sap_type` をその 0 か 1 にし、`sap_delta_time` を常に 0 にする。doc は、SAP type 1 から 6 の区別（open GOP の I フレームは type 3 に相当する、など）はしない、と書いている。

デコード順の 2 個目がキーフレームで、その PTS が最小のとき、実装は `starts_with_SAP = 1` かつ `SAP_delta_time = 0` を書く。これは「先頭が SAP で、その TSAP が EPT と一致する」という主張になる。先頭サンプルが非同期なら、Table 6 の「SAP で始まる」には当たらない。

逆に、デコード順の先頭が同期サンプルで、EPT が後続サンプルの PTS のとき、実装は `starts_with_SAP = 0` かつ `SAP_type = 0` になり、先頭の同期サンプルを「情報無し」にする。

issue 0059 は、`starts_with_sap` を `samples[0]` ではなく EPT を採ったサンプルの `keyframe` にする修正で closed になっている。`sap_delta_time` は 0 のままである。`tests/test_mux_fmp4_segment.rs` の `sidx_starts_with_sap_false_when_ept_sample_is_b_frame` は、デコード順の先頭が同期サンプル（PTS 50）で EPT が後続の B フレーム（PTS 20）のとき、`starts_with_sap` が false、`sap_type` が 0、`sap_delta_time` が 0 であることを固定している。今の実装はこの値を返す。Table 6 では、デコード順の先頭が SAP なら `starts_with_SAP = 1` である。

このテストの時刻は、先頭サンプルの PTS が 50（TPTF）、EPT が 20（TEPT）である。type 1 は `TEPT = TDEC = TSAP = TPTF`、type 2 は `TEPT = TDEC = TSAP < TPTF` である。後続サンプルが先頭の同期サンプルからデコードできるなら type 2 であり、サブセグメントがその SAP で始まるときの TSAP は EPT と一致するので、`SAP_delta_time` は 0 になる。デコード順先頭の PTS は TPTF であり、EPT より後ろでも TSAP ではない。PTS 50 と EPT 20 の差 30 は書かない。後続がデコードできない type 3（`TEPT < TDEC = TSAP`）は、type 2 を `sap_type = 2` と書く作業とともに、この issue に含めない。`sap_type` は同期サンプルを 1、それ以外の情報無しを 0 のままにする。直したあとのこのテストの期待値は、`starts_with_sap` が true、`sap_type` が 1、`sap_delta_time` が 0 である。

## 設計方針

- 参照トラックをデコード順に見て、最初の `keyframe == true` のサンプルを最初の SAP とする。この muxer が書く同期サンプルは type 1 の近似のままにする。type 2 も 1 と書く
- そのサンプルがデコード順の先頭なら `starts_with_sap = true`、`sap_type = 1`、`sap_delta_time = 0` とする。サブセグメントが SAP で始まるときの type 1 と type 2 は TSAP が EPT と一致するため、先頭サンプルの PTS から EPT を引いた値は書かない
- 先頭でなく、後ろに同期サンプルがあるなら `starts_with_sap = false`、`sap_type = 1` とする。`sap_delta_time` は、type 1 の近似としてその同期サンプルの PTS からこのサブセグメントの EPT を引いた値とする。0 も許す。差が 28 ビットに収まらないときはエラーにする
- 同期サンプルが 1 つも無いなら `starts_with_sap = false`、`sap_type = 0`、`sap_delta_time = 0` とする
- doc の「EPT サンプルが SAP かどうか」という説明を、Table 6 の「始まる」と「含む」の区別に直す。type 1 から 6 を区別しないことは残す

## 完了条件

- デコード順で先頭が非同期、2 個目が同期で、2 個目の PTS が EPT のとき、`starts_with_sap` が false、`sap_type` が 1、`sap_delta_time` が 0 になる
- デコード順の先頭が同期で、EPT が後続サンプルのより前の PTS のとき、`starts_with_sap` が true、`sap_type` が 1、`sap_delta_time` が 0 になる。先頭 PTS と EPT の差は書かない
- 同期サンプルが無いとき、`starts_with_sap` が false、`sap_type` が 0、`sap_delta_time` が 0 になる
- `sap_type` に 2 以上を書く経路は足さない
