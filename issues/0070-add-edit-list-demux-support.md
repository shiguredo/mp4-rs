# `edts` / `elst` の内容を demux 結果に反映する

- Created: 2026-08-19
- Completed: {YYYY-MM-DD}
- Branch: feature/add-edit-list-demux-support
- Polished: {YYYY-MM-DD}

## 目的

現在の MP4 / fMP4 demuxer は `TrakBox::edts_box` を無視しており、他社の一般的な encoder が出力する `edts` / `elst` の内容を demux 結果のタイムラインに反映しない。

これによって B-frame ありの H.264 / H.265 を含む MP4 (iPhone / QuickTime / ffmpeg などの典型的な出力) では、先頭の empty edit や offset edit が反映されず、demux 結果の `Sample::timestamp` が「利用側が期待する presentation timeline」から `edit_duration` / `media_time` の分だけずれる。

本 issue は「edit list を demux に反映する場合の設計選択肢」を残すことを目的とする。実装スコープと採用選択肢は「実害の実例 (どの `elst` パターンで、どのユースケースで、どれだけずれるか) が確認できた時点」で確定し、実装用の別 issue を切って進める。

## 現状

- `src/boxes_moov_tree.rs` の `EdtsBox` / `ElstBox` / `ElstEntry` は parse/encode 済み。`pbt/tests/prop_boxes.rs` にラウンドトリップ PBT も存在
- `src/demux_mp4_file.rs` の `Mp4FileDemuxer::read_moov_box` / `build_sample` / `next_sample` / `prev_sample` / `seek` はいずれも `TrakBox::edts_box` を参照しない。`Sample::timestamp` は `SampleTableAccessor` (`src/auxiliary.rs` の `SampleAccessor::timestamp`) 由来の raw DTS (media timescale)
- `Sample::composition_time_offset` は `ctts` の値をそのまま露出し、`Sample` 型 doc コメント (`src/demux_mp4_file.rs`) 上「PTS = timestamp + composition_time_offset」の契約
- `src/demux_fmp4_file.rs` / `src/demux_fmp4_segment.rs` も `edts_box` を無視する。fMP4 側は本 issue のスコープでは低優先度扱い (`issues/0071-other-position-timing-boxes-inventory.md` に方針記載)
- `src/auxiliary.rs` の `SampleTableAccessor<T>` は `StblBox` のみをラップしており、`edts` は `TrakBox` 側にあるため対象外

## 設計方針

### 設計軸

edit list を反映する実装に着手する時点で、以下 6 点は必ず何らかの形で確定する必要がある。ここでは判断そのものは行わず、軸だけ列挙する。

1. `Sample::timestamp` を media timeline (DTS) のまま維持するか、presentation timeline に付け替えるか
2. `edts` によって切り落とされたメディアサンプル (edit の範囲外にある実サンプル) を `next_sample()` から出すか、間引くか
3. empty edit (`media_time = -1`) が作る先頭ギャップを、`Sample::timestamp` の起点シフトで表すか、無音 / 黒フレームなどのプレースホルダーで埋めるか (プレースホルダー生成はライブラリの責務外にする選択肢もある)
4. `Mp4FileDemuxer::seek(Duration)` の Duration の意味を media timeline に保つか、presentation timeline に付け替えるか
5. rate change (`media_rate ≠ 1`) や複数 edit のサポート範囲。empty edit と単一 offset edit のサブセットに絞るか、全 edit を扱うか、扱えない edit が来たときに `DemuxError` を返すか無視するか
6. fMP4 側 (`Fmp4SegmentDemuxer` / `Fmp4FileDemuxer`) にも同じ変換を通すか。fMP4 は当面スコープ低優先度だが、後で対応するときに MP4 側と整合が取れる設計にしておく必要はある

### 選択肢

以下はフラットに残す。優先度・推奨は現時点で付けない。

#### A. `Sample::timestamp` を presentation timeline に付け替える (破壊的変更)

- `Sample::timestamp` の意味を「DTS」から「edit list 反映後の presentation time」に付け替える
- `Sample::composition_time_offset` との関係 (現行は `PTS = timestamp + cto`) が壊れるので、`composition_time_offset` の意味も再定義が必要
- `seek(Duration)` の Duration は presentation timeline 基準に切り替え
- 破壊的変更のため CHANGES.md には `[CHANGE]` 相当の記述が必要

#### B. `Sample` に `presentation_time: Option<u64>` を追加する

- 現行の `Sample::timestamp` / `Sample::composition_time_offset` の意味は変えない
- `edts` が存在する場合のみ `presentation_time = Some(...)` を返す
- 切り落とされたメディアサンプルは `presentation_time = None` で表すか、別のマーカーで示す
- `next_sample()` の並び順を DTS ベースにするか PTS ベースにするかは別軸で決める必要がある
- 非破壊。CHANGES.md には `[ADD]`

#### C. `SampleTableAccessor` と並立する presentation timeline 用アクセサを新設する

- `PresentationTimelineAccessor<'a, T> { table: &SampleTableAccessor<T>, elst: &ElstBox }` のような別型を追加
- `SampleTableAccessor` は現行のまま (`StblBox` のみ扱う。`edts` は `TrakBox` 側なので範囲外) を維持
- 複数 edit / rate change / splice のような複雑ケースの居場所を分離できる
- `Mp4FileDemuxer` が新アクセサをどう使うか (デフォルト採用か、オプションで切り替えるか) は結局 A / B / D と同じ判断が残る
- 非破壊。CHANGES.md には `[ADD]`

#### D. `Mp4FileDemuxerOptions` に `apply_edit_list` フラグを設ける (デフォルトは現状動作維持)

- 呼び出し側が用途に応じて `edts` 反映の on/off を選ぶ
- デフォルトを false (現状動作維持) にするか true (仕様に沿う) にするかは別途要検討
- `Sample::timestamp` の意味がオプションで切り替わるため、doc の記述負荷は増える
- 非破壊。CHANGES.md には `[ADD]`

### 対象外

- 採用選択肢の確定と実装スコープの確定 (本 issue では選択肢を残すのみ)
- `edts` / `elst` を muxer で生成する対応 (`issues/0071-other-position-timing-boxes-inventory.md` の umbrella に方針記載)
- `cslg` / `prft` / `saio` / `saiz` / `sbgp` / `sgpd` / `stps` / `stsh` / `stdp` / `padb` / `tref` の対応 (同 umbrella に集約)

## 完了条件

実害の実例 (対象ファイル、`elst` の内容、ズレの実運用上の意味) が確認できた段階で、上記選択肢のうち採用する方針を確定し、実装用の別 issue を切ってそちらに追跡を移した時点で本 issue を close する。選択肢の追加 / 削除 / 修正が必要になった場合は本 issue を更新する。

## 関連 issue

- `issues/0071-other-position-timing-boxes-inventory.md` (位置・時刻調整系ボックスのアンブレラ)
