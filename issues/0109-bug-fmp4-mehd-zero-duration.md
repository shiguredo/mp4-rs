# `Fmp4SegmentMuxer` が全体長不明のまま `mehd.fragment_duration` に 0 を書く

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-mehd-zero-duration
- Polished: {YYYY-MM-DD}

## 目的

全体の長さが決まっていない init セグメントに、長さ 0 の `mehd` を書かない。長さが未知ならボックスを省略し、読み手に全体長不明と分かるようにする。

## 現状

ISO/IEC 14496-12:2022 の 8.8.2.3 では、`fragment_duration` はフラグメントを含むムービー全体の長さ（`MovieHeaderBox` の timescale）であり、最長トラックの尺に対応する。リアルタイムで事前に長さが分からない場合は、このボックスを省略してよい、とある。8.8.2.1 は、movie fragment があり movie duration が 0 のとき、ボックスが無ければ長さを不定と解釈する、としている。この文の PDF は "MediaExtendsHeaderBox" と書いている。同じ節のクラス名は `MovieExtendsHeaderBox`（`mehd`）なので、この語はその誤記と読む。

`src/boxes_moov_tree.rs` の `MehdBox` の doc は、ボックスが無いことは継続時間不明だと書いている。

`src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::build_init_moov` は、常に `MehdBox { fragment_duration: 0 }` を `mvex` に入れる。`mvhd.duration` も 0 である。init セグメントはサンプルを足す前に書け、後から `fragment_duration` を更新しない。0 は「長さ不明」ではなく「全体の長さは 0」になる。

`mvhd` / `tkhd` の duration 0 は、フラグメントがあるときのトラック duration としてこのまま残す。変えるのは `mehd` の有無だけである。

## 設計方針

- `build_init_moov` は `mehd_box: None` にする
- 全体長を後から書き戻す API は足さない。この muxer は init を先に確定し、その後のセグメントで全体長を知らない

## 完了条件

- `Fmp4SegmentMuxer` の init セグメントの `mvex` に `mehd` が無い
- `mvhd.duration` と `tkhd.duration` は 0 のままである
- 既存の init セグメントをデコードする経路は、`mehd` が無い `mvex` を今どおり読める
