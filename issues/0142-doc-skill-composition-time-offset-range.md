# `skills/shiguredo-mp4/SKILL.md` の `Fmp4SegmentMuxer` の `composition_time_offset` の範囲の記述を実装に合わせる

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-skill-composition-time-offset-range
- Polished: {YYYY-MM-DD}

## 目的

`skills/shiguredo-mp4/SKILL.md` の `Fmp4SegmentMuxer` の注意点にある `composition_time_offset` の範囲の記述を、`trun` の version 0 と version 1 を使い分ける実装に合わせる。

## 現状

### SKILL.md の記述

`skills/shiguredo-mp4/SKILL.md` の「`Fmp4SegmentMuxer` - fMP4 セグメントのマルチプレックス」の注意点には、次の 1 行がある。

```text
- `composition_time_offset` は `trun` に書くため `i32::MIN..=i32::MAX` の範囲に限られる。負値と `> i32::MAX` の値の混在はエラー
```

前半は、受け付ける範囲が `i32::MIN..=i32::MAX` に限られるとしている。一方、後半は `> i32::MAX` の値を前提にしており、前半と矛盾する。

### 実装

`src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer::build_moof` は、次のように `trun` を組み立てる。

- 各サンプルの `composition_time_offset` を `TrunSample::composition_time_offset` に写す
- トラックのサンプルのどれかが `Some` なら、`None` のサンプルには 0 を入れる
- 1 つのメディアセグメントで、1 トラックにつき 1 つの `trun` を作る

`src/boxes_fmp4.rs` の `TrunBox` の版と範囲は次のとおり。

- `TrunBox::uses_version_1` は、`composition_time_offset` に負値を持つサンプルが 1 つでもあれば version 1、なければ version 0 を選ぶ
- `TrunBox::encode` は、version 0 では値を `u32` に、version 1 では `i32` に変換する
- 変換できない値は `ErrorKind::InvalidInput` で拒否する
- `TrunSample::composition_time_offset` の doc も、version 0 は `0..=u32::MAX`、version 1 は `i32::MIN..=i32::MAX` としている

したがって、1 つの `trun` が受け付ける値は次のどちらかである。

- 負値を含まない場合: `0..=u32::MAX`（version 0）
- 負値を含む場合: `i32::MIN..=i32::MAX`（version 1）

負値と `> i32::MAX` の値が同じ `trun`、つまり同じメディアセグメントの同じトラックにあると、どちらの版でも書けないためエラーになる。

`Fmp4SegmentMuxer::create_media_segment_metadata` の doc も、version 0 で `0..=u32::MAX`、version 1 で `i32::MIN..=i32::MAX`、負値と `> i32::MAX` の値が 1 つの `trun` に混在するとエラー、と書いている。SKILL.md の記述はこの doc とも食い違う。

### 確認結果

develop で `Fmp4SegmentMuxer::create_media_segment_metadata` に映像トラックのサンプルを渡し、次の結果を確認した。

| 1 つのメディアセグメントに渡した `composition_time_offset` | 結果 |
| --- | --- |
| `i32::MAX + 1` と `u32::MAX` | 成功。`trun` は version 0 で、値はそのまま保持される |
| `i32::MIN` | 成功 |
| `-1` と `i32::MAX + 1` | `MuxError::EncodeError`（`ErrorKind::InvalidInput`、`trun version 1 requires composition_time_offset to be in i32 range, got 2147483648`） |
| `u32::MAX + 1` | `MuxError::EncodeError`（`ErrorKind::InvalidInput`、`trun version 0 requires composition_time_offset to be in u32 range, got 4294967296`） |

`-1` だけのメディアセグメントと `i32::MAX + 1` だけのメディアセグメントを同じ muxer で順に作ると、どちらも成功した。版は `trun` ごとに決まる。

## 設計方針

- SKILL.md の該当行を、次の内容に書き換える
  - `composition_time_offset` は `trun` に書くため、範囲はメディアセグメントのトラックごとに決まる
  - 負値を含まなければ `0..=u32::MAX`（version 0）、負値を含めば `i32::MIN..=i32::MAX`（version 1）に限られる
  - 同じメディアセグメントの同じトラックで、負値と `> i32::MAX` の値が混在するとエラーになる
- 書き方は `Fmp4SegmentMuxer::create_media_segment_metadata` の doc とそろえる
- 実装は変えない

## 完了条件

- SKILL.md の `Fmp4SegmentMuxer` の注意点の記述が、次の 3 点を表している
  - 負値を含まないトラックでは `0..=u32::MAX`
  - 負値を含むトラックでは `i32::MIN..=i32::MAX`
  - 混在はメディアセグメントのトラックごとに判定されてエラーになる
- その記述が `Fmp4SegmentMuxer::create_media_segment_metadata` の doc と矛盾しない
- SKILL.md の `Fmp4SegmentMuxer` の節に、受け付ける範囲を `i32::MIN..=i32::MAX` だけとする記述が残っていない
