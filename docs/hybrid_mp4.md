# Hybrid MP4 の取り扱い

`Mp4FileMuxer::advance_position()` を用いた Hybrid MP4 の書き出しについて、形式の定義と、本 crate と利用側の責任分界をまとめる。

## Hybrid MP4 とは

Hybrid MP4 は、録画中は Fragmented MP4 (fMP4) として書き込み、正常終了時に標準 MP4 として読める形へ変換するファイル形式である。OBS Studio が録画用途向けに導入したもので、録画中断時の耐性と、完成後の広い互換性を両立することを目的とする。

録画中は各フラグメントが独立した `moof` / `mdat` を持つため、書き込みが途中で止まっても最後に確定したフラグメントまでは再生できる。正常終了時には、ファイル末尾に標準 MP4 相当の完全な `moov` を書き、先頭付近を巨大な `mdat` ヘッダで上書きしてフラグメント構造を隠す。その結果、プレイヤーから見ると通常の MP4 として扱える。

## 標準 MP4 / fMP4 との違い

| 形式 | 録画中の構造 | 完成後の構造 | 主な性質 |
| --- | --- | --- | --- |
| 標準 MP4 | `ftyp` + 成長中の `mdat`（`moov` は未書き出し） | `ftyp` + `mdat` + 末尾 `moov` | `moov` 書き出し前に落ちると再生不能 |
| fMP4 | `ftyp` + サンプルテーブル無しの `moov` + `moof` / `mdat` の列 | 録画中と同じ | 途中停止に強いが、編集ソフト等での互換性が低いことがある |
| Hybrid MP4 | fMP4 と同じ構造 | `ftyp` + 巨大 `mdat`（録画中の内容を包含）+ 末尾 `moov` | 録画中の耐性と完成後の互換性を両立する |

Hybrid MP4 のポイントは、録画中に挿入した `moof` / `mdat` ヘッダを、完成後もファイル内に残したまま巨大な `mdat` のペイロードとして隠蔽する点にある。標準 MP4 の `moov` がサンプルの絶対オフセットを指すため、書き込み位置の管理が重要になる。

## 本 crate の対応方針と責任分界

本 crate が Hybrid MP4 向けに提供するのは、標準 MP4 用の `Mp4FileMuxer` に対する位置管理の補助である。`moof` の組み立てや fMP4 フラグメントの生成自体は本 crate の範囲外であり、利用側（または `Fmp4SegmentMuxer` を別途使う側）が担う。

### 本 crate が担当する範囲

- 内部書き込み位置 (`next_position`) の管理
- `Mp4FileMuxer::advance_position()` による非サンプルデータ分の位置前進
- `advance_position()` 直後の強制チャンク切り替え（サンプルデータの連続性が切れるため）
- `append_sample()` 時の `data_offset` と内部書き込み位置の整合検査
- `finalize()` による標準 MP4 相当の `moov` / `mdat` ヘッダの確定

### 利用側が担当する範囲

- 録画中の `moof` の構築（`trun.data_offset` の計算を含む）
- フラグメント用 `mdat` ヘッダの書き出し
- サンプルペイロードの実ファイルへの書き込み
- 非サンプルデータ書き込み後の `advance_position()` 呼び出しによる位置同期
- 完成時の変換処理（先頭領域の `mdat` 化、末尾への完全な `moov` 配置など）

## 書き出しの呼び出し順序

以下は Hybrid MP4 向けの骨格である。`moof` / `mdat` ヘッダの実バイト列の組み立ては範囲外とし、利用側が用意した非サンプルバイト列を挟む形だけを示す。

```rust
use std::num::NonZeroU32;

use shiguredo_mp4::{
    TrackKind,
    boxes::SampleEntry,
    mux::{Mp4FileMuxer, MuxError, Sample},
};

fn mux_hybrid_mp4(
    sample_entry: SampleEntry,
    // 利用側が組み立てた moof / mdat ヘッダなどの非サンプルデータ
    non_sample_header: &[u8],
    sample_payload: &[u8],
) -> Result<Vec<u8>, MuxError> {
    let mut muxer = Mp4FileMuxer::new()?;
    let mut output: Vec<u8> = muxer.initial_boxes_bytes().to_vec();

    // 利用側: 非サンプルデータを書き出す
    output.extend_from_slice(non_sample_header);
    // crate 側: 書き込み位置を非サンプルデータ分だけ進める
    muxer.advance_position(non_sample_header.len() as u64)?;

    // 利用側: サンプルペイロードを書き出す
    let data_offset = output.len() as u64;
    output.extend_from_slice(sample_payload);

    // crate 側: サンプルメタデータを登録する
    // data_offset は advance_position() 後の内部書き込み位置と一致させる
    let sample = Sample {
        track_kind: TrackKind::Video,
        sample_entry: Some(sample_entry),
        keyframe: true,
        timescale: NonZeroU32::new(30).expect("non-zero"),
        duration: 1,
        composition_time_offset: None,
        data_offset,
        data_size: sample_payload.len(),
    };
    muxer.append_sample(&sample)?;

    // 実際のフラグメントは通常複数のサンプルを含む。
    // その場合は「非サンプル書き出し 1 回 → advance_position 1 回 →
    // (サンプル書き出し + append_sample) × N」を 1 単位として、
    // フラグメント数分だけ繰り返す。
    // 同一フラグメント内の 2 サンプル目以降は sample_entry: None にできる
    // （直前のサンプルの sample_entry が引き継がれる）

    let finalized = muxer.finalize()?;

    // finalize() 後のボックス（末尾配置の moov など）は
    // これまでに書き出した output の末尾を超える位置にも配置されるため、
    // 必要な長さまで output を伸長してから書き戻す
    let final_len = finalized
        .offset_and_bytes_pairs()
        .map(|(offset, bytes)| offset as usize + bytes.len())
        .max()
        .unwrap_or(output.len());
    if final_len > output.len() {
        output.resize(final_len, 0);
    }

    for (offset, bytes) in finalized.offset_and_bytes_pairs() {
        let offset = offset as usize;
        output[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    Ok(output)
}
```

実際の Hybrid MP4 ライターでは、`Fmp4SegmentMuxer` でフラグメント用メタデータを生成しつつ、同じサンプル列を `Mp4FileMuxer` にも登録する二系統の構成になることが多い。

## 注意事項

- `advance_position(size)` で `size > 0` のとき、直後の `append_sample()` は強制的に新規チャンクを開始する。非サンプルデータの挿入でチャンク内のバイト連続性が失われるためである
- `append_sample()` に渡す `data_offset` は、crate 内部の書き込み位置と一致していなければならない。ずれると `MuxError::PositionMismatch` になる
- `advance_position()` 自体はバイトを書き出さない。利用側が先に非サンプルデータを出力し、そのサイズを伝えて位置だけを同期する
- `size == 0` の `advance_position()` は何もしない
- `finalize()` 済みの muxer に対する `advance_position()` / `append_sample()` は `MuxError::AlreadyFinalized` になる
- 本 crate の `Mp4FileMuxer::finalize()` が返す結果は標準 MP4 向けの後書き情報である。Hybrid MP4 特有の完成形への変換（録画中レイアウトから標準 MP4 相当への書き換え）は利用側が行う

## 参考リンク

- OBS Studio Hybrid MP4 の解説: <https://obsproject.com/blog/obs-studio-hybrid-mp4>
