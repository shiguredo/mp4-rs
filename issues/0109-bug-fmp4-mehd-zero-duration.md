# `Fmp4SegmentMuxer` が全体長不明のまま `mehd.fragment_duration` に 0 を書く

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-mehd-zero-duration
- Polished: 2026-10-05

## 目的

全体の長さが決まっていない init セグメントに、長さ 0 の `mehd` を書かない。長さが未知ならボックスを省略し、読み手に全体長不明と分かるようにする。

## 現状

ISO/IEC 14496-12:2022 の 8.8.2.3 では、`fragment_duration` はフラグメントを含むムービー全体の長さ（`MovieHeaderBox` の timescale）であり、最長トラックの尺に対応する。リアルタイムで事前に長さが分からない場合は、このボックスを省略してよい、とある。8.8.2.1 は、movie fragment があり movie duration が 0 のとき、ボックスが無ければ長さを不定と解釈する、としている。この文の PDF は "MediaExtendsHeaderBox" と書いている。同じ節のクラス名は `MovieExtendsHeaderBox`（`mehd`）なので、この語はその誤記と読む。

`src/boxes_moov_tree.rs` の `MehdBox` の doc は、ボックスが無いことは継続時間不明だと書いている。

`src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::build_init_moov` は、常に `MehdBox { fragment_duration: 0 }` を `mvex` に入れる。`mvhd.duration` も 0 である。init セグメントは最初のメディアセグメントを観測した時点で出力できる（`init_segment_bytes` はトラックが未登録だと `MuxError::EmptyTracks` を返すため、サンプルを 1 つも渡さないうちは取得できない）。その時点では全体長が確定しておらず、出力後の init の `fragment_duration` を更新する API も無い。0 は「長さ不明」ではなく「全体の長さは 0」になる。

## 設計方針

- `build_init_moov` は `mehd_box: None` にする
- 全体長を後から書き戻す API は足さない。この muxer はセグメントを 1 つずつ受け取るため最終フラグメントまで全体長が確定せず、init は最初のセグメント直後に出力されうる
- `mvhd` / `tkhd` の duration 0 は、フラグメントがあるときのトラック duration としてこのまま残す。変えるのは `mehd` の有無だけである
- `CHANGES.md` の develop に `[FIX]` を追記する。init セグメントの `mvex` から `mehd` が消えてバイト列が変わるが、公開 API のシグネチャは変わらず、仕様に適合しない出力を直す修正であるため

## 完了条件

- `Fmp4SegmentMuxer` の init セグメントの `mvex` に `mehd` が無い
- `mvhd.duration` と `tkhd.duration` は 0 のままである（回帰確認）
- `MoovBox::decode` と `MvexBox::decode` が `mehd` の無い `mvex` を読み、`Fmp4SegmentDemuxer::handle_init_segment` が `mehd` の無い init セグメントからトラックを返す（回帰確認。既存の手組みテストと `pbt/tests/prop_fmp4_segment_mux_demux.rs` の mux→demux 往復で確認できる）
- `Fmp4SegmentMuxer` の公開 API に `fragment_duration` や `mehd` を設定・更新するメソッドを追加していない
- `CHANGES.md` の develop に `[FIX]` エントリがある
- `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz -- -D warnings` と `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` が通る
