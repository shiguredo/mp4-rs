# `Mp4FileMuxer` / `Fmp4SegmentMuxer` で同一 `TrackKind` の複数トラックを許容する

- Priority: Low
- Created: 2026-07-24
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-multiple-tracks-per-kind
- Polished: YYYY-MM-DD

## 目的

`Mp4FileMuxer` / `Fmp4SegmentMuxer` の両 muxer は現状「同一 `TrackKind` のトラックは 1 本まで」に制限している。以下のようなユースケースに対応するため、同一 kind の複数トラックを扱えるようにする。

- 多言語音声（Audio × 2 本以上）
- 多言語字幕（Subtitle × 2 本以上、`stpp` / `wvtt` / `tx3g` の混在）
- 副音声 / 解説音声などの alternate tracks

## 優先度根拠

Low。現時点で具体的な要求は無い。バグ由来でも無い。0046 完了時点で `Mp4FileMuxer` / `Fmp4SegmentMuxer` の両方が `Vec<TrackEntry>` パターンで実装され、多本化の下地は整うが、設計判断（後述）を確定してから着手する必要があるため pending として起票する。

## 現状

- `src/mux_fmp4_segment.rs:22-25` モジュール doc に「現時点では同一 `TrackKind` のトラックは 1 本までに制限している（音声 / 映像 / 字幕 各 1 本）」「将来、同種複数トラックに対応する場合は file muxer と合わせて拡張する想定である」と明記
- `src/mux_fmp4_segment.rs:905-935` `ensure_track_entry` は同一 kind の既存 entry があればそこに合流させる（新規 push しない）実装
- 0046 完了後の `src/mux_mp4_file.rs` にも同名パターンで `ensure_track_entry` が導入される予定（`issues/0046-add-mp4-file-muxer-subtitle.md` 参照）

## pending にした理由

以下の設計判断が未確定のため、いま実装に着手できない。方針が固まった時点で reopened にする。

### 決めるべき設計方針

1. **`track_id` の割り当てポリシー**: 追加順で通し番号を振る現状パターンを維持するか、kind ごとに範囲を分ける（例: Audio は 100 番台、Video は 200 番台）か
2. **同一 kind 内での SampleEntry の区別方法**: `TrackEntry` を `Vec<TrackEntry>` にそのまま増やすか、`HashMap<TrackKind, Vec<TrackEntry>>` 相当の構造に変えるか。`ensure_track_entry` のシグネチャが変わる
3. **`tkhd` の `alternate_group` の扱い**: ISO/IEC 14496-12 の alternate tracks メカニズムをサポートするか（例: 多言語音声を同一 `alternate_group` に置く）
4. **既存 API への影響**: `Sample::track_kind` だけでは複数トラックを区別できないため、`Sample` に `track_index` フィールドを追加するか、`ensure_track_entry` の呼び出し側で track ID を明示するか。前者は semver 上 `[CHANGE]` になる
5. **言語情報の扱い**: `mdhd.language` (`src/boxes_moov_tree.rs` `MdhdBox`) を Sample 側から指定できるようにするか。多言語音声 / 字幕では必須
6. **C API / WASM 露出**: 複数 track_id を返す API 変更が必要になる

### 判断に必要な材料

- 実運用での要求（どの機能が最優先か。多言語音声 / 副音声 / 多言語字幕のどれから対応するか）
- 対応するデコーダ / プレイヤーの `alternate_group` 対応状況
- 既存 API 破壊を許容するか、`Sample` に optional な `track_index` を後方互換で追加するか

## 完了条件

（設計判断確定後に詳細化する）

- `Mp4FileMuxer` / `Fmp4SegmentMuxer` の両方で同一 `TrackKind` の複数トラックを mux できる
- `Mp4FileDemuxer` / `Fmp4FileDemuxer` / `Fmp4SegmentDemuxer` 3 経路で同一 kind 複数トラックが取り出せる
- `src/mux_fmp4_segment.rs:22-25` の doc「同一 `TrackKind` のトラックは 1 本まで」の制限記述を削除する
- 既存 Audio / Video / Subtitle 1 本ずつの mux 挙動が変わらない
- PBT / 単体テストの追加

## 解決方法

（設計判断確定後に詳細化する）
