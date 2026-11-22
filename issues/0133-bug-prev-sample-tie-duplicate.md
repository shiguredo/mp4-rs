# `Mp4FileDemuxer::prev_sample` で同時刻のトラックを順方向と逆の順序で選ぶ

- Created: 2026-11-22
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-prev-sample-tie-duplicate
- Polished: 2026-11-23

## 目的

ファイル先頭から `seek` を挟まずに走査するとき、同時刻のサンプルも `next_sample` で進んだ順序の逆順で `prev_sample` が返すようにする。`prev_sample` で戻したサンプルを、直後の `next_sample` で再取得する挙動は維持する。

## 現状

`src/demux_mp4_file.rs` の `Mp4FileDemuxer::next_sample` と `prev_sample` は、各トラックの DTS を `Duration::from_secs(timestamp) / timescale` で比較する。比較値が等しいと、どちらも内部のトラック添字が小さい方を選ぶ。順方向の走査では先に返した方を逆方向でも選ぶため、最後に進めたトラックとは別のカーソルが戻ることがある。

既存の `tests/testdata/black-h265-video.mp4` と `tests/testdata/beep-aac-audio.mp4` から、先頭の各 2 サンプルを元の尺とタイムスケールで、映像、音声、映像、音声の順に追加した MP4 で確認した。先頭から `next_sample` を 2 回呼ぶと DTS が 0 の映像 1、音声 1 の順に返る。直後の `prev_sample` とそれに続く `next_sample` は現行ではどちらも映像 1 を返す。映像と音声の順序はこのファイルのトラック順によるもので、メディア種別の優先順ではない。

## 設計方針

- `prev_sample` は各トラックのカーソルの直前候補から比較時刻が最大のものを選ぶ。同じ比較時刻では、`next_sample` とは逆に内部トラック添字が最大の候補を選ぶ。時刻の計算、`next_sample` の順序、選んだトラックだけの `next_sample_index` を戻す挙動は変えない
- `prev_sample` の公開ドキュメントに、同時刻ではトラック添字の降順で選ぶことと、ファイル先頭から `seek` を挟まず走査した場合の往復の関係を記載する。`seek` による各トラックの位置決めは変更せず、シーク後に同じサンプルへ直ちに往復できるとは保証しない

## 完了条件

複数トラックの試験では、サンプルをタイムスタンプだけでなくトラック ID とデータ位置でも区別する。

- トラック順が映像、音声で先頭の比較時刻が等しく、両トラックとも後続サンプルがあればその比較時刻が先頭より後になる MP4 を使い、`next_sample` が映像 1、音声 1 を返した直後に、`prev_sample` と続く `next_sample` がどちらも音声 1 を返すことを確認する
- 3 トラック以上に同時刻の候補がある場合も、`prev_sample` の連続呼び出しが順方向で取得したサンプルを逆順で返すことを確認する。複数トラックのケースは `pbt/tests/prop_mux_demux.rs` の既存の映像・音声・字幕の生成を活用する
- 既存の単一トラックの `prev_sample_roundtrip` PBT と `cargo test --workspace` が成功する
