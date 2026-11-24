# fMP4 の CENC (DRM) 向け初期化データのボックスに対応する

- Created: 2026-11-25
- Completed: 2026-11-25
- Branch: feature/add-cenc-init-boxes
- Polished: 2026-11-25

## 目的

CMSF (draft-ietf-moq-cmsf) は、保護された CMAF トラックの初期化データに `sinf` (`schm` と、`schi` 内の `tenc`) を含めることを MUST としている (CMSF 4.2)。CENC で保護されたサンプルエントリーは `encv` (映像) / `enca` (音声) になり、`sinf` を子ボックスに持つ。また、init segment の `moov` には DRM システムごとの `pssh` が置かれることがある。

`shiguredo_mp4` は現状これらのボックスを型付きで扱えない。`encv` / `enca` は `SampleEntry::Unknown` に、`pssh` は `MoovBox::unknown_boxes` に生バイトで落ちるため、次のことができない。

- 受信した init segment から `tenc` の `default_KID` / `default_Per_Sample_IV_Size` / `default_isProtected` を読む
- CMSF 向けに `sinf` / `tenc` を含む init segment を生成する
- `pssh` を保持したまま init segment を再構築する

## 現状

- `src/boxes.rs` に CENC / DRM 系のボックス型はない。
- `src/boxes_sample_entry.rs` の `SampleEntry::decode` は、`encv` / `enca` を `UnknownBox` にフォールバックして `SampleEntry::Unknown` として返す。各サンプルエントリーは `unknown_boxes` を持つためバイト列は保持されるが、`sinf` を型付きで読めない。
- `src/boxes_moov_tree.rs` の `MoovBox` は `pssh` を `unknown_boxes` に入れる。`Fmp4SegmentMuxer::build_init_moov` は `unknown_boxes: Vec::new()` で `moov` を組み立てるため、`pssh` を init segment に出力できない。
- `Fmp4SegmentDemuxer::handle_init_segment` は `moov` をデコードするが、`pssh` を利用側に露出しない。
- `SampleEntry` を網羅 match しているのは `src/codec_string.rs` の `from_sample_entry` と `crates/wasm/src/boxes.rs` の `fmt_json` だけである。`parse_json` は `kind` 文字列の match なので `SampleEntry` の網羅性は要求されない。`src/mux_fmp4_segment.rs` の `extract_video_dimensions` / `build_ftyp` / `subtitle_trak_attributes` と、`src/mux_mp4_file.rs` の `build_final_ftyp_box` は `_` アームを持つ。
- `issues/0071-other-position-timing-boxes-inventory.md` は、暗号化 / DRM をスコープ外として `saio` / `saiz` を低優先度にしている。本 issue で初期化データ側の対応を始める。

## 設計方針

### 1. CENC ボックスの追加

`src/boxes_cenc.rs` を追加し、`src/lib.rs` に `mod boxes_cenc;` を足したうえで、`src/boxes.rs` から再エクスポートする。

- `PsshBox` (ISO/IEC 23001-7 8.1)
  - `version: u8`、`system_id: [u8; 16]`、`kids: Vec<[u8; 16]>`、`data: Vec<u8>`
  - version 0 / 1 のみ受理し、それ以外は `InvalidData` を返す
  - version 0 では `kids` が空であることを必須とし、version 1 では `KID_count == 0` も許容する (ムービー内の全 KID に適用する表現)
  - `DataSize` が `u32` を超える場合は `InvalidInput` を返す
- `FrmaBox` (ISO/IEC 14496-12 8.12)
  - `data_format: [u8; 4]`
- `SchmBox` (ISO/IEC 14496-12 8.12)
  - FullBox version 0 のみ
  - `scheme_type: [u8; 4]`、`scheme_version: u32`、`scheme_uri: Option<Utf8String>`
  - `scheme_version` は flags に関係なく常に存在する
  - flags は `scheme_uri` がある場合だけ `0x000001` を立てる
- `SchiBox` (ISO/IEC 14496-12 8.12)
  - `tenc_box: Option<TencBox>`、`unknown_boxes: Vec<UnknownBox>`
- `SinfBox` (ISO/IEC 14496-12 8.12)
  - `frma_box: FrmaBox` は必須
  - `schm_box: Option<SchmBox>`、`schi_box: Option<SchiBox>`、`unknown_boxes: Vec<UnknownBox>`
  - `schm` / `schi` は「両方存在する」か「両方存在しない」かのどちらかとする (14496-12 8.12.1.1 c)
  - エンコード時は `schm` / `schi` の組を検査し、CENC 系 scheme なら `schi.tenc` を必須として検査する。decode も片方だけの組は `InvalidData` とする
  - `cens` / `cbcs` では `tenc` がパターン暗号化パラメーターを持つ version 1 であること、`cenc` / `cbc1` ではパターン暗号化パラメーターを持たない version 0 であることを検査する
- `TencBox` (ISO/IEC 23001-7 8.2)
  - `default_is_protected: u8` (0 / 1)、`default_per_sample_iv_size: u8` (0 / 8 / 16)、`default_kid: [u8; 16]`
  - `default_constant_iv: Option<Vec<u8>>`
  - `default_crypt_byte_block: Option<Uint<u8, 4>>`、`default_skip_byte_block: Option<Uint<u8, 4>>`
  - version 0 / 1 のみ受理する。パターン暗号化パラメーターがある場合は version 1、ない場合は version 0 としてエンコードする
  - パターン暗号化パラメーターは 2 つそろっていることを必須とする
  - `default_isProtected == 1 && default_Per_Sample_IV_Size == 0` のときだけ `default_constant_IV` を必須とし、それ以外で指定されていれば `InvalidInput` を返す
  - `default_constant_IV` の長さは 8 または 16 のいずれかとする
  - 上記の範囲外の値は `InvalidInput` / `InvalidData` として拒否する

### 2. 暗号化サンプルエントリーの追加

`src/boxes_sample_entry.rs` に `EncvBox` / `EncaBox` を追加し、`SampleEntry` に `Encv` / `Enca` を追加する。

- `EncvBox { visual: VisualSampleEntryFields, sinf_box: SinfBox, unknown_boxes: Vec<UnknownBox> }`
- `EncaBox { audio: AudioSampleEntryFields, sinf_box: SinfBox, unknown_boxes: Vec<UnknownBox> }`
- `sinf` は必須。子ボックスのうち最初の `sinf` を型付きで保持し、それ以外は `unknown_boxes` に保持する
- コーデック設定ボックス (`avcC` / `hvcC` / `vpcC` / `av1C` / `esds` / `dOps` / `dfLa` など) は `unknown_boxes` に生バイトで保持する
- `EncvBox` / `EncaBox` のエンコードでは、CMSF 4.2 が要求する CENC の保護情報を欠いた init segment を生成しないよう、`sinf.schm` と `sinf.schi.tenc` が存在することを必須とする。デコードでは、CENC 以外の保護スキームを読めるように `sinf` の存在だけを必須とする
- `SampleEntry` のヘルパー (`audio_channel_count` / `audio_sample_rate` / `audio_sample_size` / `video_resolution`) を `Encv` / `Enca` に対応させる
- `src/codec_string.rs` の `from_sample_entry` は、`Encv` / `Enca` でも `frma` と `unknown_boxes` 内の設定ボックスから元のコーデック文字列を生成する
  - `avc1` → `avcC`、`hev1` / `hvc1` → `hvcC`、`vp08` / `vp09` → `vpcC`、`av01` → `av1C`、`mp4a` → `esds` を `unknown_boxes` から探してデコードする
  - `Opus` / `fLaC` は設定ボックスを必要とせず、4CC をそのまま返す
  - 上記以外の `frma`、または必要な設定ボックスが見つからない場合は `Unsupported` を返す

### 3. moov と mux / demux への統合

- `MoovBox` に `pssh_boxes: Vec<PsshBox>` を追加し、encode / decode / `children()` を対応させる
  - `MoovBox` の構造体リテラルは `src/mux_mp4_file.rs` の `build_moov_box`、`src/mux_fmp4_segment.rs` の `build_init_moov`、`tests/test_boxes_moov_tree.rs`、`pbt/tests/prop_container_boxes.rs`、`examples/transcode_wasm/src/mp4.rs` にある。これらに `pssh_boxes: Vec::new()` を追加する
  - `MoovBox` のフィールド追加、`SampleEntry` への `Encv` / `Enca` 追加、`SegmentMuxerOptions` への `pssh_boxes` 追加は、いずれも公開 API の破壊的変更になるため `CHANGES.md` に `[CHANGE]` として記載する
- `SegmentMuxerOptions` に `pssh_boxes: Vec<PsshBox>` を追加し、`Fmp4SegmentMuxer::build_init_moov` で `MoovBox` に設定する
- `Fmp4SegmentMuxer` の `extract_video_dimensions` を `Encv` に対応させる
- `Fmp4SegmentMuxer` の `build_ftyp` は、`Encv` の `frma` を見て `hev1` / `hvc1` / `av01` ブランドを立てる。`avc1` は `default_base_is_moof` の制約があるため従来どおり追加しない
- `Fmp4SegmentDemuxer` は init segment の `pssh` を保持し、`pssh_boxes(&self) -> &[PsshBox]` で返す。init segment 処理前は空スライスを返す。`Fmp4FileDemuxer` も内部の `Fmp4SegmentDemuxer` に委譲して同じ値を `pssh_boxes(&self) -> &[PsshBox]` で返す
- `Mp4FileMuxer` は `MoovBox` の構造体リテラルを更新する。`Mp4FileDemuxer` / `Fmp4FileDemuxer` は `MoovBox::decode` を呼ぶだけで構造体リテラルを持たないため、リテラルの更新は不要である
- 非 fragmented MP4 の `pssh` 出力、`Encv` / `Enca` を渡した `Mp4FileMuxer` の暗号化トラック出力 (ftyp のブランド更新を含む) は今回のスコープ外とする

### 4. wasm クレート

- `crates/wasm/src/boxes.rs` の `fmt_json` は `SampleEntry` を網羅 match しているため、`Encv` / `Enca` の分岐を追加する。`parse_json` は `kind` 文字列の match なので追加は不要
- `crates/wasm/src/fmp4_segment_mux.rs` の `SegmentMuxerOptions` リテラルは全フィールドを明示しているため、`pssh_boxes: Vec::new()` を追加する
- DRM サンプルエントリーの JSON 表現と wasm からの `pssh` 指定は今回のスコープ外とし、未知のサンプルエントリーと同じ扱いにする

## 完了条件

- `PsshBox` / `FrmaBox` / `SchmBox` / `SchiBox` / `SinfBox` / `TencBox` の encode / decode が実装され、PBT の往復テストと異常系テストが通る
- `EncvBox` / `EncaBox` の encode / decode が実装され、`sinf` / `tenc` を型付きで読める
- `EncvBox` / `EncaBox` のエンコードで、`schm` または `schi.tenc` を欠いた CENC 用の入力が拒否される
- `MoovBox` で `pssh` を保持して往復できる
- `Fmp4SegmentMuxer` が `pssh` と、`schm` / `schi.tenc` を含む `encv` / `enca` の init segment を生成できる
- `Fmp4SegmentDemuxer` が `pssh_boxes()` を返し、各トラックの最初のサンプルの `Sample.sample_entry` に `Encv` / `Enca` (型付きの `sinf` / `tenc` 付き) が入る。映像トラックは `Encv`、音声トラックは `Enca` で確認する。`Fmp4FileDemuxer::pssh_boxes()` も同じ値を返す
- `codec_string::from_sample_entry` が `Encv` / `Enca` でも元のコーデック文字列を返す
- `cargo test --workspace`、`cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz -- -D warnings`、`cargo fmt --all --check`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` が成功する
- `CHANGES.md`、`skills/shiguredo-mp4/SKILL.md`、`issues/0071-other-position-timing-boxes-inventory.md` が更新されている

## 解決方法

- `src/boxes_cenc.rs` を追加し、`PsshBox` / `FrmaBox` / `SchmBox` / `SchiBox` / `SinfBox` / `TencBox` を実装した
- `src/boxes_sample_entry.rs` に `EncvBox` / `EncaBox` と `SampleEntry::Encv` / `Enca` を追加した
- `src/boxes_moov_tree.rs` の `MoovBox` に `pssh_boxes` を追加し、`Fmp4SegmentMuxer` の `SegmentMuxerOptions::pssh_boxes` から init segment に `pssh` を書き出せるようにした
- `Fmp4SegmentDemuxer::pssh_boxes()` と `Fmp4FileDemuxer::pssh_boxes()` を追加した
- `src/mux_fmp4_segment.rs` で `Encv` / `Enca` の解像度と ftyp ブランドを扱うようにした
- `src/codec_string.rs` で `Encv` / `Enca` のコーデック文字列を生成できるようにした
- `crates/wasm` は `fmt_json` の `Encv` / `Enca` 分岐と `SegmentMuxerOptions` リテラルを最小対応した
- `tests/test_boxes_cenc.rs` / `pbt/tests/prop_boxes_cenc.rs` などを追加し、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz --no-deps` を確認した
- `CHANGES.md`、`skills/shiguredo-mp4/SKILL.md`、`issues/0071-other-position-timing-boxes-inventory.md` を更新した

## 対象外

- `senc` / `saiz` / `saio` とメディアセグメントのサンプル単位の暗号化情報
- `sbgp` / `sgpd` / `seig` による鍵ローテーション
- 非 fragmented MP4 の `stbl` 内のサンプル補助情報
- `Mp4FileMuxer` の暗号化トラック出力 (ftyp のブランド更新を含む)
- CENC 以外の保護スキームの `sinf` / `encv` / `enca` の生成
- トップレベルに置かれた `pssh`
- `enct` / `encs` (字幕の保護サンプルエントリー)
- wasm の JSON API での DRM サンプルエントリー対応と wasm からの `pssh` 指定
- `pssh` の `Data` に含まれる DRM システム固有データの解釈
- 暗号化・復号そのもの (このライブラリの責務ではない)

## 関連 issue

- `issues/0071-other-position-timing-boxes-inventory.md` (暗号化 / DRM をスコープ外としていた前提が変わる。本 issue で該当箇所を更新する)
