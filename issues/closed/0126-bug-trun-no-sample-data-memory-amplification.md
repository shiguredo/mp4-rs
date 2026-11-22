# `TrunBox` でサンプルごとのフィールドがないランの件数をコンパクトに保持する

- Created: 2026-11-22
- Completed: 2026-11-23
- Branch: feature/fix-trun-no-sample-data-memory-amplification
- Polished: 2026-11-22

## 目的

サンプルごとのフィールドを持たない `trun` の符号化・復号で、サンプル数に比例するメモリ確保やループを行わず、論理サンプル数を維持する。

## 現状

ISO/IEC 14496-12:2015 の 8.8.8.1〜8.8.8.3 では、`trun` の各サンプル行に含まれるフィールドはラン全体に共通のフラグで決まり、行にはフィールドがなくてもよい。したがって `sample_count` の妥当性は `trun` や親の `moof` の長さだけからは判定できない。

`src/boxes_fmp4.rs` の `TrunBox::decode` は、サンプルごとのフィールドがない場合にも個々の `TrunSample` を `Vec` に格納する。`TrunBox::encode` はこの `Vec` の長さを件数として書く。単に空の `Vec` にすると、本来の件数が失われる。

`Fmp4SegmentDemuxer` は、省略された duration・size・flags に `tfhd`、次いで `trex` の既定値を使う。`first_sample_flags` があれば先頭サンプルのフラグにのみ優先し、省略された `composition_time_offset` は `None` のまま返す。対応トラックではサンプルごとの結果を返し、読み飛ばすトラックでは `skipped_traf_data_end` がデータ末尾だけを求める。

## 設計方針

- `TrunBox::samples` は、件数だけの形式と、サンプルごとの値を持つ形式を区別する列挙型に変更する。公開 Rust API の型変更として扱い、ボックスの公開エクスポート、`Fmp4SegmentMuxer::build_moof`、`Fmp4SegmentDemuxer` と既存のテストを新しい型に合わせる
- デコードは各行の `duration`、`size`、`flags`、`composition_time_offset` がすべて省略されている場合に限り、件数だけを保持する。ラン単位の `data_offset` と `first_sample_flags` の有無はこの判定に含めない。エンコードはその件数を直接使い、両フィールドを保持する。サンプルごとのフラグとバージョンの計算も個別行を走査せずに行う。サンプルごとの値を持つ形式では、エンコード時の既存の有無の整合性検査と、デコード時のペイロード境界検査を維持する。従来の全フィールドが `None` の配列は、同じ論理件数の件数だけの形式に置き換える
- サンプルごとのフィールドがないランの復号では、受理範囲を変えないため既存の件数上限を維持する。この形式の件数上限の根拠に `trun` や `moof` のバイト長は使わない
- 対応トラックでは、件数だけのランを処理する際に既定値を使い、空の `TrunSample` の配列は作らない。読み飛ばすトラックでは、件数が 0 なら既定サイズを解決せず、そうでなければ既定サイズと件数の積からチェック付き算術でデータ末尾を求める。後続ランと `traf` の基準位置の規則は変えない
- この課題はボックスの空行の実体化をなくすことに限定する。デマルチプレクサーの一括出力の契約や、出力全体に対する資源制限は変更しない

## 完了条件

- 少数のサンプルを持つ通常のボックスを用いた PBT で、サンプルごとのフィールドがないランは件数だけの形式になり、論理件数、`data_offset`、`first_sample_flags` が再エンコードと再デコードを経ても維持されることを確認する
- 対応トラックでは、件数だけのランと等価な値を各サンプル行に明示したランを比較する。サンプルの尺、サイズ、キーフレーム判定が一致し、`composition_time_offset` が両方とも `None` であることを確認する。先頭には残りのサンプルと異なるキーフレーム判定になるフラグを用い、行にフラグを明示する比較側では `first_sample_flags` を併用しない。直後の `data_offset` を持たないランのサンプル位置も比較する
- 読み飛ばすトラックでは、件数だけのランから求めたデータ末尾を、`base_data_offset` がなく `default_base_is_moof` が false の次の `traf` が使うケースを確認する。どちらのテストも、`moof` の長さの違いを考慮し、各セグメントの `mdat` のペイロード内で同じ範囲を指すことを確認する
- サンプルごとの値を持つランの検証と `Fmp4SegmentMuxer` の既存出力は変わらず、`cargo test --workspace` が成功する

## 解決方法

- `src/boxes_fmp4.rs` に公開の列挙型 `TrunSamples`（`CountOnly(u32)` / `PerSample(Vec<TrunSample>)`）を追加し、`TrunBox::samples` の型を `Vec<TrunSample>` から `TrunSamples` に変更した。`src/boxes.rs` の公開エクスポートにも追加した
  - デコードは、`tr_flags` にサンプルごとのフィールドのフラグが 1 つも立っていない場合と、`sample_count` が 0 の場合に `CountOnly` を返し、サンプル行を実体化しない。`data_offset` と `first_sample_flags` は判定に含めない
  - 件数 0 も `CountOnly(0)` にしたのは、行が 0 個なら「各行のフィールドがすべて省略されている」に当たり、decode → encode → decode で値が変わらないようにするため
  - `CountOnly` の件数上限（16,777,216）は受理範囲を変えないために維持した。上限の根拠に `trun` や `moof` のバイト長は使っていない
  - エンコードは `CountOnly` の件数を直接書き、`compute_flags` / `uses_version_1` / `validate_sample_option_consistency` は `CountOnly` ではサンプル行を走査せずに決まる
  - `PerSample` では、エンコード時の `Option` の有無の整合性検査と、デコード時のペイロード境界検査を維持した
  - `TrunBox` / `TrunSamples` / `first_sample_flags` の doc に、両形式の関係、既定値の参照順、上限とエンコードの非対称を書いた
- `src/demux_fmp4_segment.rs` を新しい型に合わせた
  - 対応トラックでは、`trun_sample_rows` が `CountOnly` の件数分の `None` を返し、空の `TrunSample` の配列を作らずに既定値を使う
  - 読み飛ばすトラックの `skipped_traf_data_end` は、`CountOnly(0)` では既定サイズを解決せず、それ以外は既定サイズと件数の積からチェック付き算術でデータ末尾を求める。エラー文言は従来どおり `sample data offset overflow`
- `src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::build_moof` は `TrunSamples::PerSample` を使う。出力は変わらない
- テスト
  - `pbt/tests/prop_fmp4_boxes.rs`: `trun_box_without_per_sample_fields_keeps_count` を追加し、`CountOnly` と全フィールド `None` の配列が同じバイト列になり、論理件数・`data_offset`・`first_sample_flags` が再エンコード・再デコードで維持されることを確認した。`arb_trun_box` は 0 件とフィールドなしを `CountOnly` で生成し、`trun_box_roundtrip` を等価比較に強めた
  - `pbt/tests/prop_fmp4_segment_mux_demux.rs`: `count_only_trun_matches_explicit_rows_on_supported_track`（対応トラックで `CountOnly` と値を明示したランを比較。先頭だけキーフレーム、比較側は `first_sample_flags` なし、直後の `data_offset` なしのランも比較、既定値は `tfhd` と `trex` の両方）と、`count_only_trun_data_end_is_used_by_next_traf_on_skipped_track`（読み飛ばすトラックの `CountOnly` のデータ末尾を、`base_data_offset` なし・`default_base_is_moof = false` の次の `traf` が使う）を追加した。どちらも各セグメントの `mdat` の payload 内で同じ範囲を指すことを確認する
  - `tests/test_boxes_fmp4.rs`: 件数上限の境界、件数 0 でフラグが立った `trun` の正規化、サンプル行のペイロード境界検査の単体テストを追加した
  - `tests/test_demux_fmp4_segment.rs`: 読み飛ばすトラックの `CountOnly` について、件数と既定サイズの積、件数 0 で `trex` が要らないこと、`trex` がない場合のエラー、データ末尾のオーバーフローの単体テストを追加した
  - 変異（件数を掛けない、`first_sample_flags` を使わない、行数を減らす、件数 0 で既定サイズを解決する、境界検査を外す、正規化を外す、加算を wrapping にする）を入れ、対応するテストが失敗することを確認した
- `skills/shiguredo-mp4/SKILL.md` の `TrunBox` の説明を更新した
- `CHANGES.md` の `## develop` に `[CHANGE]` を追加した。ブランチは `feature/fix-` だが公開 API の型変更のため、`TrunSample` の型を変えた先例と同じく `[CHANGE]` とした
