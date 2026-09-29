# `Fmp4SegmentMuxer` が handler の組が違う字幕サンプルエントリを同じトラックに入れる

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-subtitle-handler-mix
- Polished: {YYYY-MM-DD}

## 目的

`stpp` と `wvtt` のように、`hdlr` とメディアヘッダの組が違う字幕を、同じ fMP4 トラックの `stsd` に並べない。トラックに 1 組しか書けない属性と、`stsd` の中身が矛盾した init セグメントを出さないようにする。

## 現状

`src/mux_fmp4_segment.rs` の `subtitle_trak_attributes` は、次の組を返す。

- `stpp`: `subt` + `sthd`
- `wvtt`: `text` + `sthd`
- `tx3g`: `text` + `nmhd`

同じ関数のコメントは、この組が 1 トラック内で混在すると `stsd` には両方が並び、トラック側の属性は片方に固定されるので、呼び出し側が戻り値を突き合わせて検出する、と書いている。

`Mp4FileMuxer::append_sample` は、字幕トラックに既にあるサンプルエントリと新しいサンプルエントリで `subtitle_trak_attributes` の組が違うとき、`MuxError::MixedSampleEntries` を返す。

`Fmp4SegmentMuxer::build_init_trak` は、`sample_entries` の先頭だけで `derive_trak_attributes` を呼び、`hdlr` とメディアヘッダを決める。`resolve_segment_tracks` の `MixedSampleEntries` は、同一セグメントの中で sample entry のインデックスが変わったときだけである。別セグメントで、先頭が `stpp` のトラックに `wvtt` を足すと、両方 `sample_entries` に入り、`hdlr` は先頭の `subt` のまま `stsd` に `wvtt` が並ぶ。

## 設計方針

- 新しい字幕サンプルエントリをトラックに足すとき、既存の先頭エントリと `subtitle_trak_attributes` の組を比較する
- 組が違うときは `MuxError::MixedSampleEntries` を返し、そのサンプルエントリは `sample_entries` に残さない
- 比較は `Mp4FileMuxer::append_sample` と同じ関数の戻り値で行う
- 映像トラックの複数サンプルエントリは、今どおりこの検査の対象にしない

## 完了条件

- 字幕トラックに `stpp` を入れたあと、別セグメントで `wvtt` を足すと `MixedSampleEntries` になり、init セグメントの `stsd` には `stpp` だけが残る
- `stpp` だけの字幕トラック、または同じ組のサンプルエントリを複数入れる場合は成功する
- `Mp4FileMuxer` が同じ組み合わせを拒否する挙動は変わらない
