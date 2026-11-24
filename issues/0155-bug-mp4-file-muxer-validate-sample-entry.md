# `Mp4FileMuxer::append_sample` でエンコードできないサンプルエントリーを拒否する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-mp4-file-muxer-validate-sample-entry
- Polished: {YYYY-MM-DD}

## 目的

`Mp4FileMuxer::append_sample` で、エンコードできないサンプルエントリーを受け取った時点で `MuxError::EncodeError` を返す。

今はエラーが `Mp4FileMuxer::finalize` まで遅れる。利用側はサンプルデータを出力に書き終えた後で、初めて失敗を知る。しかも、追加済みのサンプルエントリーを取り除く手段がないため、その muxer では MP4 ファイルを完成させられない。

## 現状

`src/mux_mp4_file.rs`:

- `Mp4FileMuxer::append_sample` は、新しいチャンクが必要なときにサンプルエントリーを解決し、`Chunk::sample_entry` にそのまま保持する。解決するのは `sample.sample_entry` で、省略時は同じトラックの直前のチャンクのものを使う。エントリーをエンコードできるかは確かめない。字幕トラックの `MuxError::MixedSampleEntries` の検査はあるが、エンコードの可否とは別の検査である
- `Mp4FileMuxer::finalize` は、`build_moov_box` で組み立てた `moov` を `encode_to_vec` する。サンプルエントリーはここで初めてエンコードされ、失敗すると `MuxError::EncodeError` を返す
- `finalize` が失敗しても `finalized_boxes` は設定されない。ただし保持しているチャンクは変わらないため、もう一度呼んでも同じエラーになる
- 公開 API（`new` / `with_options` / `initial_boxes_bytes` / `advance_position` / `append_sample` / `finalize` / `finalized_boxes`）には、追加済みのサンプルやサンプルエントリーを取り除く手段がない

エンコード時に検査されるサンプルエントリーの例:

- 宣言サイズが、ヘッダーとペイロードの長さの合計と食い違う `UnknownBox` を含むもの（`SampleEntry::Unknown` や、各サンプルエントリーの `unknown_boxes`）。`UnknownBox::encode` が `ErrorKind::InvalidInput` を返す
- `metadata_blocks` が空か、先頭が STREAMINFO でない `DflaBox` を持つ `SampleEntry::Flac`。`DflaBox::encode` が `ErrorKind::InvalidInput` を返す

`CHANGES.md` の `## develop` には、`UnknownBox::encode` と `DflaBox::encode` の検査を追加した項目がある。どちらの項目も、こうしたサンプルエントリーを渡した場合に `Mp4FileMuxer::finalize` / `Fmp4SegmentMuxer::init_segment_bytes` が `MuxError::EncodeError` を返す、と書いている。つまり、エラーは `finalize` で返る。

再現手順（develop で確認）:

1. `Mp4FileMuxer::new` で muxer を作る
2. 音声サンプルを `append_sample` に渡す。`sample_entry` は `SampleEntry::Unknown(UnknownBox { box_type: BoxType::Normal(*b"zzzz"), box_size: BoxSize::U32(100), payload: vec![0; 4] })` とする。`Ok(())` が返る。続けて `sample_entry: None` のサンプルを渡しても `Ok(())` が返る
3. `finalize` を呼ぶと、`MuxError::EncodeError` が返る。理由は `UnknownBox size mismatch: box_size=U32(100), header_size=8, payload_len=4` である
4. もう一度 `finalize` を呼んでも同じエラーになり、`finalized_boxes()` は `None` のままである
5. `DflaBox { metadata_blocks: vec![] }` を持つ `SampleEntry::Flac` でも同じになる。`append_sample` は `Ok(())` を返し、`finalize` は理由が `dfLa box must contain at least one metadata block (STREAMINFO)` の `MuxError::EncodeError` を返す

比較として、`src/mux_fmp4_segment.rs` の `Fmp4SegmentMuxer` でもエラーが遅れる。

- `create_media_segment_metadata` は、`resolve_segment_tracks` で新しいサンプルエントリーを `TrackEntry::sample_entries` に加えるだけで、エンコードの可否を確かめない。返す `moof` と `mdat` のヘッダーにはサンプルエントリーが含まれないため、上と同じエントリーを渡しても成功する
- その後の `init_segment_bytes` は、`build_init_moov` で組み立てた `moov` をエンコードするときに `MuxError::EncodeError` を返す。`mfra_bytes` も内部で `init_segment_bytes` を呼ぶため、同じエラーになる。エントリーは `sample_entries` に残るので、どちらもその後ずっと失敗する

## 設計方針

- `append_sample` で、新しいチャンクのために `sample.sample_entry` から解決したサンプルエントリーをエンコードできるか確かめる。できなければ `MuxError::EncodeError` を返す。理由には、エンコードが返したエラー（これまで `finalize` が返していたもの）をそのまま使う
- 検査は `ensure_track_entry` より前に置く。`append_sample` 内のコメントにあるとおり、`ensure_track_entry` より後に失敗経路を足すと、`TrackEntry` を追加した後でエラーを返せてしまう。そうなると、`append_sample` の doc にある「エラーを返した場合も内部状態は変わらない」という契約が壊れる
- `sample_entry` を省略したサンプルは、直前のチャンクのエントリーを引き継ぐ。引き継ぐエントリーは追加時に検査済みなので、改めて検査しない
- `append_sample` の doc に、このエラー条件を書く
- `finalize` のエンコードとエラーは変えない
- `Fmp4SegmentMuxer` の `create_media_segment_metadata` / `create_media_segment_metadata_with_sidx` は対象外とする。この issue の対象は `Mp4FileMuxer` に限り、`Fmp4SegmentMuxer` は投入の経路（`resolve_segment_tracks`）も、エラーの出る API（`init_segment_bytes` / `mfra_bytes`）も異なるため、別に扱う
- 映像の幅・高さが `i16::MAX` を超える場合も、`finalize` まで `MuxError::EncodeError` が遅れる（`extract_video_dimensions`）。ただし、これはサンプルエントリーのエンコードとは別の検査なので対象外とする

## 完了条件

- `tests/test_mux_mp4_file.rs` で、次の 3 点を確認する
  - トラックの最初のサンプルに、エンコードできないサンプルエントリーを渡すと、`append_sample` が `MuxError::EncodeError` を返す。宣言サイズとペイロード長が食い違う `UnknownBox` を含むものと、`metadata_blocks` が空の `dfLa` を持つ `SampleEntry::Flac` の両方で確かめ、理由の文字列まで照合する
  - 同じトラックの途中で、エンコードできない新しいサンプルエントリーを渡した場合も、`MuxError::EncodeError` を返す
  - エラーの後、同じ `data_offset` に正しいサンプルエントリーのサンプルを `append_sample` で渡すと成功し、`finalize` も成功する。`moov_box()` には拒否したエントリーが含まれず、トラックとサンプルの数は成功した `append_sample` の分だけである
- `cargo test --workspace` が成功する
