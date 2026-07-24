# 字幕系 `compatible_brands` (`msubs` 等) を `Mp4FileMuxer` / `Fmp4SegmentMuxer` の `ftyp` に追加する

- Priority: Low
- Created: 2026-07-24
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/add-subtitle-compatible-brands
- Polished: YYYY-MM-DD

## 目的

生成される MP4 / fMP4 の `ftyp` の `compatible_brands` に字幕関連ブランド（`msubs` 等）を追加し、実プレイヤーでの字幕再生（DASH.js / Safari / VLC / QuickTime Player 等での認識）互換性を高める。

現状、字幕トラック（`SampleEntry::Stpp` / `Wvtt` / `Tx3g`）を含む MP4 / fMP4 を生成しても `compatible_brands` には字幕系ブランドが追加されないため、一部プレイヤーで字幕トラックが認識されない可能性がある。

## 優先度根拠

Low。0043 / 0044 / 0045 / 0046 の各「### `compatible_brands` の方針」節でも「本 issue の範囲では追加しない」で送られており、実プレイヤー互換性は 4 issue とも別 issue に切り出した扱い。現時点で具体的な要求は無い。

## 現状

- `src/mux_mp4_file.rs:724-758` `build_final_ftyp_box` は `SampleEntry::Avc1` / `Hev1` / `Hvc1` / `Av01` のみ `compatible_brands` に反映（`AVC1` / `HEV1` / `HVC1` / `AV01`）
- `src/mux_fmp4_segment.rs:503-540` `build_ftyp` も同様のロジック
- 字幕系 `SampleEntry`（`Stpp` / `Wvtt` / `Tx3g`）は `compatible_brands` への影響を持たない
- `src/boxes.rs` の `Brand` 定数に `MSUBS` は現状定義されていない
- 0043 (`issues/closed/0043-add-subtitle-stpp.md:197-201`)、0044 (`issues/closed/0044-add-subtitle-wvtt.md:243-247`)、0045 (`issues/closed/0045-add-subtitle-tx3g.md:465-469`)、0046 (`issues/0046-add-mp4-file-muxer-subtitle.md`) はいずれも「本 issue の範囲では追加しない、必要になれば別 issue で対応」と保留

## pending にした理由

以下の設計判断が未確定のため、いま実装に着手できない。方針が固まった時点で reopened にする。

### 決めるべき設計方針

1. **どのブランドを追加するか**: `msubs` のみか、`stpp` / `wvtt` / `tx3g` を独立したブランドとして扱うか、DASH-IF や 3GPP の推奨ブランドセット（例: `dash` / `dsms` / `msdh` / `3gp6`）にどこまで追随するか
2. **追加の条件**: `SampleEntry::Stpp` / `Wvtt` / `Tx3g` のいずれかを含めば追加するか、方式ごとに異なるブランドを対応表で追加するか
3. **各方式に対応するブランド**:
   - `stpp` → `stpp` 単独か、`msubs` を追加するか（ISO/IEC 14496-30 の推奨は未確認）
   - `wvtt` → `wvtt` 単独か、DASH-IF の推奨に従うか
   - `tx3g` → `3gp*` 系ブランドを追加するか、`msubs` に統一するか
4. **`Brand::MSUBS` 等の関連定数の追加**: `src/boxes.rs` に `impl Brand { pub const MSUBS: Self = ...; }` を追加する必要がある
5. **既存プレイヤーでの検証**: DASH.js / Safari / VLC / QuickTime Player の各プレイヤーで実際に字幕再生できるか実測する

### 判断に必要な材料

- DASH-IF Interoperability Guidelines の字幕対応推奨（`msubs` 等の brand 定義箇所）
- ISO/IEC 14496-30 の brand 記述箇所
- 3GPP TS 26.245 の brand 記述箇所
- 実プレイヤーでの動作検証結果

## 完了条件

（設計判断確定後に詳細化する）

- `Mp4FileMuxer::build_final_ftyp_box` / `Fmp4SegmentMuxer::build_ftyp` が字幕系 `SampleEntry` の存在に応じて `compatible_brands` に必要なブランドを追加する
- 追加されるブランド定数（`Brand::MSUBS` 等）が `src/boxes.rs` に定義される
- 既存 Audio / Video のみの mux 生成物の `compatible_brands` が変わらない
- PBT / 単体テストの追加
- 主要プレイヤー（DASH.js / Safari / VLC 等）で字幕トラックが認識されることの実測記録

## 解決方法

（設計判断確定後に詳細化する）
