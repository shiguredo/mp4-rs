# `Fmp4SegmentMuxer::create_media_segment_metadata` でエンコードできないサンプルエントリーを拒否する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-fmp4-segment-muxer-validate-sample-entry
- Polished: {YYYY-MM-DD}

## 目的

`Fmp4SegmentMuxer::create_media_segment_metadata` / `create_media_segment_metadata_with_sidx` で、エンコードできないサンプルエントリーを受け取った時点で `MuxError::EncodeError` を返す。

今はエラーが `init_segment_bytes` / `mfra_bytes` まで遅れる。利用側はメディアセグメントを出力した後で、初めて失敗を知る。しかも、取り込んだサンプルエントリーを取り除く手段がないため、その muxer では以降ずっと init segment と `mfra` を作れない。

## 現状

`src/mux_fmp4_segment.rs`:

- `build_media_segment_bytes` は、`self.tracks` を複製した `next_tracks` に対して `resolve_segment_tracks` を呼び、成功した場合だけ `self.tracks` を置き換える。そのため、エラーを返した場合は muxer の内部状態が変わらない（`create_media_segment_metadata_with_sidx` の doc にもこの契約がある）
- `resolve_segment_tracks` は、サンプルの `sample_entry` が既知のものと異なれば `TrackEntry::sample_entries` に加えるだけで、エンコードできるかは確かめない。返す `moof` と `mdat` のヘッダーにはサンプルエントリーが含まれないため、`create_media_segment_metadata` は成功する
- `init_segment_bytes` は `build_init_moov` で組み立てた `moov` をエンコードし、サンプルエントリーはここで初めてエンコードされる。失敗すると `MuxError::EncodeError` を返す
- `mfra_bytes` は内部で `init_segment_bytes` を呼ぶため、同じエラーになる
- 取り込んだサンプルエントリーは `sample_entries` に残るため、`init_segment_bytes` と `mfra_bytes` はその後ずっと失敗する。公開 API に、取り込んだサンプルエントリーを取り除く手段はない

エンコード時に検査されるサンプルエントリーの例:

- 宣言サイズが、ヘッダーとペイロードの長さの合計と食い違う `UnknownBox` を含むもの。`UnknownBox::encode` が `ErrorKind::InvalidInput` を返す
- `metadata_blocks` が空か、先頭が STREAMINFO でない `DflaBox` を持つ `SampleEntry::Flac`。`DflaBox::encode` が `ErrorKind::InvalidInput` を返す

`CHANGES.md` の `## develop` にある `UnknownBox::encode` と `DflaBox::encode` の検査の項目は、こうしたサンプルエントリーを渡した場合に `Fmp4SegmentMuxer::init_segment_bytes` が `MuxError::EncodeError` を返す、と書いている。

`Mp4FileMuxer::append_sample` でも同じ種類の遅れがあり、issue 0155 で扱う。投入の経路（`resolve_segment_tracks`）とエラーの出る API（`init_segment_bytes` / `mfra_bytes`）が異なるため、本 issue で別に扱う。

## 設計方針

- `resolve_segment_tracks` で、新しいサンプルエントリーを `sample_entries` に加える前に、エンコードできるか確かめる。できなければ `MuxError::EncodeError` を返す。理由には、エンコードが返したエラー（これまで `init_segment_bytes` が返していたもの）をそのまま使う
- 検査は `next_tracks` に対して行うため、エラーを返した場合も muxer の内部状態は変わらない。この契約を崩さない
- 既知のサンプルエントリーと同じものや、`sample_entry` を省略したサンプルは、取り込み時に検査済みなので改めて検査しない
- `create_media_segment_metadata` / `create_media_segment_metadata_with_sidx` の doc に、このエラー条件を書く
- `init_segment_bytes` / `mfra_bytes` のエンコードとエラーは変えない

## 完了条件

- `tests/test_mux_fmp4_segment.rs` で、次の 3 点を確認する
  - エンコードできないサンプルエントリーを持つサンプルを渡すと、`create_media_segment_metadata` が `MuxError::EncodeError` を返す。宣言サイズとペイロード長が食い違う `UnknownBox` を含むものと、`metadata_blocks` が空の `dfLa` を持つ `SampleEntry::Flac` の両方で確かめ、理由の文字列まで照合する
  - `create_media_segment_metadata_with_sidx` でも同じエラーを返す
  - エラーの後、正しいサンプルエントリーのサンプルで `create_media_segment_metadata` を呼ぶと成功し、`init_segment_bytes` と `mfra_bytes` も成功する。init segment の `stsd` には拒否したエントリーが含まれない
- `cargo test --workspace` が成功する
