# `Mp4FileDemuxer` のエラー状態が、一度エラーを返すと解除される

- Created: 2026-09-29
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-mp4-file-demuxer-error-state
- Polished: 2026-10-06

## 目的

不正な入力でエラー状態になった `Mp4FileDemuxer` を、そのエラーを 1 回返しただけでは通常状態に戻さない。`required_input` はエラー状態のあいだ `None` のままにし、`tracks` と `next_sample` は同じエラーを返し続ける。

## 現状

`src/demux_mp4_file.rs` の `Mp4FileDemuxer::handle_input` の doc は、要求範囲を満たさない入力のあとエラー状態になり、そのあと `required_input` は常に `None` を返し、`tracks` と `next_sample` の次の呼び出しはエラーを返す、と書いている。同じ段落の rustdoc リンクは、`tracks` と `next_sample` の閉じバッククォートが無く、リンクになっていない。

`required_input` は `handle_input_error` が `Some` のとき `None` を返す。ここまでは doc と一致する。

`ensure_initialized` は `handle_input_error.take()` でエラーを取り出して返す。`tracks` または `next_sample` が 1 回エラーを返すと、`handle_input_error` は `None` に戻る。その次の `required_input` はフェーズの要求を返し、`None` ではない。その次の `tracks` は、保存していたエラーではなく `InputRequired` を返す。

`handle_input` は、`handle_input_error` が既に `Some` のとき、入力が要求を満たすかの検査をしない。`handle_input_inner` が成功しても、古いエラーは残る。内側がフェーズを進めたあとに `tracks` を呼ぶと、進んだ状態ではなく古いエラーが返り、その取り出しでエラー状態が解除される。

## 設計方針

- エラーは `take` しない。`tracks` と `next_sample` は、エラーが残っているあいだ同じエラーを返す
- エラーが残っているあいだ `handle_input` はフェーズを進めず、入力を捨てる。新しいエラーで上書きしない
- `required_input` は、エラーが残っているあいだ `None` を返す今の条件を維持する
- 同じ doc 段落の閉じバッククォートを補い、リンクが `tracks` と `next_sample` を指すようにする

## 完了条件

- 要求を満たさない `handle_input` のあと、`required_input` が `None` である
- そのあと `tracks` を 2 回呼んでも、両方とも同じエラーになり、2 回目のあとでも `required_input` は `None` である
- エラーが残っている状態で、正しい範囲の `handle_input` を呼んでもフェーズは進まず、その後の `tracks` は最初のエラーを返す
- `handle_input` の doc で、`tracks` と `next_sample` が rustdoc のリンクになっている
