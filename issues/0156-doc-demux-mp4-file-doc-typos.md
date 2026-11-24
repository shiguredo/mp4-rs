# `demux_mp4_file` の doc コメントの誤字と記法の誤りを直す

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-demux-mp4-file-doc-typos
- Polished: {YYYY-MM-DD}

## 目的

`src/demux_mp4_file.rs` の doc コメント（`///` / `//!`）にある誤字・脱字、余分な空白と空行、閉じバッククォートの抜け、全角と半角の間の空白の抜けを直す。rustdoc に出る文面を正し、AGENTS.md の表記規約に合わせる。

## 現状

`src/demux_mp4_file.rs` で、次の誤りを確認した。

誤字・脱字:

- `Sample::timestamp`（demux 側の `Sample`。`mux::Sample` ではない）の doc にある「求めらている」は、「求められている」の誤字である
- `DemuxError::InputRequired` の doc にある「供給するが可能となる」は、「こと」が抜けている

余分な空白と空行:

- `Mp4FileDemuxer::handle_input` の doc にある「エラー状態に遷移した後は、 [」で、読点の後に余分な半角空白がある
- `RequiredInput::size` の doc で、空の `///` 行が 2 行続いている

閉じバッククォートの抜け:

- `Mp4FileDemuxer::handle_input` の doc の次の行で、2 つのリンクのどちらも閉じバッククォートが抜けている。同じ doc の他のリンクは、[`Mp4FileDemuxer::required_input()`] の形で書いている

```text
    /// [`Mp4FileDemuxer::tracks()] や [`Mp4FileDemuxer::next_sample()] の次の呼び出しはエラーを返すようになる
```

全角と半角の間の空白の抜け（AGENTS.md の「全角と半角の間には半角スペースを入れること」に反する）:

- モジュール doc（`//!`）の「MP4ファイル」
- `TrackInfo::track_id` の doc の「トラックID」

## 完了条件

- 上に挙げた誤りがすべて直っている
- `rg -n '求めらている|供給するが可能|後は、 \[' src/demux_mp4_file.rs` の結果が空になる
- `rg -n '^\s*//[/!].*([A-Za-z0-9][\p{Han}\p{Hiragana}\p{Katakana}]|[\p{Han}\p{Hiragana}\p{Katakana}][A-Za-z0-9])' src/demux_mp4_file.rs` の結果が空になる
- `Mp4FileDemuxer::handle_input` の doc で、`tracks()` と `next_sample()` へのリンクが、[`Mp4FileDemuxer::required_input()`] と同じ形で書かれている
- `RequiredInput::size` の doc で、空の `///` 行が続いていない
