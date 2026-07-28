# sidx 付き media segment と mfra の moof_offset が不整合になる

- Priority: High
- Created: 2026-07-15
- Completed: 2026-07-28
- Model: opencode-go glm-5.2
- Branch: feature/fix-sidx-mfra-moof-offset
- Polished: 2026-07-28

## 目的

`Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx()` が sidx を返却バイト列の先頭に載せる一方で `media_bytes_written` に sidx サイズを加算しないため、`mfra_bytes()` の `tfra.moof_offset` が実際のファイル上の moof 位置より sidx 分だけ前を指す問題を修正する。

## 優先度根拠

`examples/fmp4.rs:189,237` が `create_media_segment_metadata_with_sidx` と `mfra_bytes` の組み合わせを使用しており、実例で発火する。生成された MP4 の `tfra.moof_offset` が不正確になると、シーク・ランダムアクセスで moof の位置を外し、再生・解析ツールが誤動作する。High。

## 現状

```rust
// src/mux_fmp4_segment.rs:297-300 (create_media_segment_metadata_with_sidx)
let sidx_bytes = sidx_box.encode_to_vec()?;
let mut result = sidx_bytes;
result.extend_from_slice(&media_segment);
Ok(result)
```

```rust
// src/mux_fmp4_segment.rs:310 (build_media_segment_bytes 冒頭)
let moof_relative_offset = self.media_bytes_written;
```

```rust
// src/mux_fmp4_segment.rs:389-397 (build_media_segment_bytes 内、tfra エントリを push する)
for (traf_pos, resolved_track) in resolved_tracks.iter().enumerate() {
    let ti = resolved_track.track_index;
    let entry = TfraSegmentEntry {
        time: self.tracks.get(ti).map_or(0, |track| track.decode_time),
        moof_relative_offset,
        traf_number: u32::try_from(traf_pos + 1).expect("traf count exceeds u32::MAX"),
    };
    next_tfra_entries[ti].push(entry);
}
```

```rust
// src/mux_fmp4_segment.rs:407-411 (build_media_segment_bytes 内)
self.media_bytes_written = self
    .media_bytes_written
    .checked_add(segment.len() as u64)
    .and_then(|written| written.checked_add(mdat_payload_size))
    .ok_or(MuxError::Overflow)?;
```

```rust
// src/mux_fmp4_segment.rs:454 (mfra_bytes)
    moof_offset: init_segment_size + e.moof_relative_offset,
```

`build_media_segment_bytes` は `moof_relative_offset = media_bytes_written`（sidx を含まない値）を記録し、その値を含む tfra エントリを直後に `self.tfra_entries` へ push する。`media_bytes_written` にも moof + mdat のみを加算する。`create_media_segment_metadata_with_sidx` はその後 sidx を先頭に付けるが、当該セグメントの tfra エントリはすでに sidx を含まないオフセットで確定済みで、`media_bytes_written` にも sidx サイズは加算されない。

実ファイル: `init + [sidx + moof + mdat] + ...`
mfra が指す位置: `init + [moof + mdat] 累積`（当該セグメント自身も後続セグメントも sidx 分ずれる）

PBT `mfra_bytes_roundtrip` は sidx なし経路のみ検証しているため未検出。

## 設計方針

sidx サイズを、当該セグメントの tfra エントリと `media_bytes_written` の両方に反映する。`create_media_segment_metadata_with_sidx` で sidx をエンコードした後に、以下の 2 箇所へ sidx バイト数を `checked_add` する:

1. **当該セグメントで新規追加された tfra エントリの `moof_relative_offset`**: sidx は当該 media segment の直前に付加されるため、当該セグメント自身のオフセットも sidx サイズ分だけ後ろにずらす必要がある
2. **`self.media_bytes_written`**: 後続セグメントが `moof_relative_offset` の起点とするため、こちらにも sidx サイズを含める必要がある

いずれか一方だけでは、当該セグメントの tfra または後続セグメントの tfra のどちらかが sidx 分ずれたまま残る。特に、当該セグメントの tfra エントリは `build_media_segment_bytes` の内部で sidx を認識するタイミングより前に確定するため、`media_bytes_written` の加算だけでは直せない。

`build_media_segment_bytes` に sidx サイズを事前に渡す代替案もあるが、sidx サイズは同関数が返す media segment サイズに依存するため、事前計算するには `SidxBox` の構造から独立に予測する必要があり、sidx 実装の変更に脆くなる。したがって、tfra エントリを事後補正する方針を採る。

もう一つの代替案として「sidx 使用時の mfra 併用を明示的に拒否する」もあるが、`examples/fmp4.rs` が両者を併用しているため、整合させる方を採用する。

## 完了条件

- sidx 付き media segment と mfra を併用したとき、`tfra.moof_offset` が実際の moof 位置と一致すること
- sidx なし経路の従来挙動が変わらないこと
- `mfra_bytes_roundtrip` に sidx 付きのケースを追加し、オフセットが正しいことを検証すること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

`feature/fix-sidx-mfra-moof-offset` ブランチで対応した。

### 実施内容

- `src/mux_fmp4_segment.rs` の `create_media_segment_metadata_with_sidx` に、sidx エンコード後の事後補正を追加した
  - `build_media_segment_bytes` 呼び出し前にトラックごとの `self.tfra_entries[track_index].len()` をスナップショットする
  - sidx エンコード後、`self.media_bytes_written.checked_add(sidx_size)` を先に評価してオーバーフロー検査する
  - スナップショットより長くなったトラックの末尾 1 件の `moof_relative_offset` に sidx サイズを加算する（`media_bytes_written` の検査後は必ずオーバーフローしない不変条件を `.expect(...)` で明示している）
  - 最後に `self.media_bytes_written` を更新する
- `pbt/tests/prop_fmp4_segment_mux_demux.rs` に PBT を 2 本追加した
  - `mfra_bytes_roundtrip_with_sidx_mix`: 単一映像トラックで sidx あり／なし混在時の `tfra.moof_offset` が実 moof 位置を指すことを検証する
  - `mfra_bytes_roundtrip_with_sidx_mix_multi_track`: 映像 + 音声のマルチトラックで、両トラック同時セグメント / 音声後発 / 既存トラックで今回サンプル無しの各分岐をカバーする
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追記した

### 計画から外れた点

- **PBT を 2 本に分けた。** 元の計画では `mfra_bytes_roundtrip` に sidx 付きケースを追加する 1 本の想定だったが、実装後のコードレビューでマルチトラック分岐（`pre_tfra_lens.get(track_index) = None → unwrap_or(0)`）が単一トラック PBT では踏まれないことが判明したため、マルチトラック専用 PBT を別途新設した

### レビューを受けて追加で対応した内容

- 日本語コメント中の "media segment" 表記を "メディアセグメント" に統一した（本 issue の追加コードだけでなく、既存の他ファイルも含めて統一）
- `create_media_segment_metadata_with_sidx` の doc / 内部コメントを整理した
  - doc から private フィールド（`tfra_entries` / `media_bytes_written`）への言及を外し、観測可能な保証（`mfra_bytes()` の返す `tfra.moof_offset` が実 `moof` 位置と整合する）だけの記述に絞った
  - `pre_tfra_lens` 前後の三重掲載になっていたシフト理由コメント 2 行を削減した
  - `moof_relative_offset` の `checked_add` の `.expect` メッセージに、`moof_relative_offset <= media_bytes_written` の不変条件を明記した
- 補正ループの `pre_tfra_lens.get(track_index).copied().unwrap_or(0)` について、`build_media_segment_bytes` が新規トラックを追加した場合に `pre_tfra_lens` に対応要素が無く「以前は存在しなかった = 0 件」を意味することをコメントで明示した
