# `Mp4FileDemuxer::prev_sample` で同時刻のトラックを順方向と逆の順序で選ぶ

- Created: 2026-11-22
- Completed: 2026-11-24
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

## 解決方法

- `src/demux_mp4_file.rs` の `Mp4FileDemuxer::prev_sample` で、候補の比較を `timestamp <= s.0` から `timestamp < s.0` に変えた。トラック添字の昇順に候補を見るため、同じ比較時刻ではトラック添字が最大の候補（後から見つかった方）を選ぶ。時刻の計算、`next_sample` の順序、選んだトラックだけの `next_sample_index` を戻す挙動、`seek` は変えていない
- `next_sample` と `prev_sample` の比較の箇所に、両者が逆順の関係にあり、比較を変えるときは両方を合わせる必要があることをコメントで書いた
- 公開ドキュメント
  - `next_sample`: 時刻の定義（`Sample::timestamp` をトラックのタイムスケールで秒に換算し、ナノ秒未満を切り捨てた値）と、最も早い時刻のサンプルが複数のトラックにある場合は `tracks()` の並びで最も前にあるトラックのサンプルを返すことを書いた
  - `prev_sample`: 最も遅い時刻のサンプルが複数のトラックにある場合は `tracks()` の並びで最も後ろにあるトラックのサンプルを返すこと、ファイル先頭から `seek` を挟まずに `next_sample` とこのメソッドを呼ぶ場合の往復の関係（逆順で返すこと、戻した直後の `next_sample` が同じサンプルを返すこと）、`seek` の後はこれらの関係を保証しないことを書いた。意味の取りにくかった既存の 1 文は置き換えた
- テスト
  - `pbt/tests/prop_mux_demux.rs` に `prev_sample_reverses_next_sample_across_tracks` を追加した。既存の映像・音声・字幕のサンプル生成を使い、映像と音声に半数のケースで字幕を加えた MP4 を mux する。各トラックの先頭サンプルは時刻 0 で同時刻になり、さらに半数のケースでは timescale を揃えて途中のサンプルも同時刻にする
  - サンプルはトラック ID・タイムスタンプ・データ位置・データサイズで区別し、次のことを確かめる: 順方向に 1 つ進めるたびに直後の `prev_sample` と続く `next_sample` が直前に進めたサンプルを返すこと、同時刻のサンプルを `next_sample` が `tracks()` の並びの順に返すこと、最後まで進めた後の `prev_sample` の連続呼び出しが逆順を返して最後に `None` になること、先頭まで戻した後の `next_sample` が先頭のサンプルを返すこと
  - 修正前の比較に戻すと、2 個目のサンプルへ進めた直後の `prev_sample` の検査で必ず失敗することを確かめた。既存の単一トラックの `prev_sample_roundtrip` も成功する
- `CHANGES.md` の `## develop` に [FIX] を追記した
