# CMAF の構造ブランドと単一トラックのセグメント出力に対応する

- Created: 2026-11-25
- Completed: {YYYY-MM-DD}
- Branch: feature/add-cmaf-segment-brand
- Polished: 2026-11-25

## 目的

`shiguredo_mp4` は `styp` を実装するが、CMAF の構造ブランドを持たないため、出力を CMAF の構造ブランドとして宣言できない。

CMAF (ISO/IEC 23000-19) の `cmfc` は、1 ファイル 1 メディアトラック、1 `moof` 1 `traf` といった構造を前提とする。`Fmp4SegmentMuxer` は音声・映像・字幕を 1 つの init segment にまとめて複数の `traf` を書けるため、ブランドを付けるだけで準拠を名乗ることはできない。

本 issue では、CMAF の構造ブランド `cmfc` を出力できるようにし、ブランドを付けてよい条件 (単一メディアトラック) を muxer で検査する。CMAF の全制約の保証は利用側の構成に依存するため、本 issue の対象外とする。

## 現状

- `src/boxes.rs` の `Brand` に CMAF のブランドが無い。
- `Fmp4SegmentMuxer` の `ftyp` は major brand `iso5`、互換 brand は `iso5` / `iso6` / `mp41` と映像コーデックに応じた `hev1` / `hvc1` / `av01` である。
- `Fmp4SegmentMuxer` は最大 3 トラック (`TrackKind::Audio` / `Video` / `Subtitle`) を 1 つの init segment に書き、`moof` には解決済みトラックごとに `traf` を書く。
- `styp` の実装と `SegmentMuxerOptions::styp` は別 issue (0163) で追加する。
- CMAF の構造制約を検査する仕組みは無い。

## 設計方針

### 1. CMAF の構造ブランド

`Brand` に次を追加する。

- `CMFC` (`cmfc`): CMAF Track File の構造ブランド (ISO/IEC 23000-19:2024 7.6)
- `CMFS` (`cmfs`): `styp` で CMAF Segment を示すブランド (ISO/IEC 23000-19:2024/Amd 2:2026 の Table 1 で SegmentTypeBox 用とされている)

`cmf2` は `cmfc` をさらに制限する構造ブランドであり (`elst` を使わない、`trun` version 1 の負の composition time offset を使う、`tfhd` の既定値を `traf` ごとに繰り返す)、本 issue では対象外とする。`cmfl` は定義と配置を一次資料で確認できていないため対象外とする。

### 2. ブランドを付けてよい条件

CMAF の構造ブランドを宣言できるのは、次の条件を満たす場合だけとする。

- 観測済みの `TrackKind` が 1 種類だけである (音声・映像・字幕を混在させない)
- init segment の `trak` が 1 個である
- 1 つの `moof` に書く `traf` が 1 個である

条件を満たさない場合、`create_media_segment_metadata` 系が `MuxError::EncodeError(Error::invalid_input(...))` を返す。

トラックを観測するのは `create_media_segment_metadata` 系だけであり、`self.tracks` は成功した呼び出しでしか増えない。そのため検査は `create_media_segment_metadata` 系に置き、累積の `TrackKind` 数と、その呼び出しで書く `traf` の数の両方を見る。`init_segment_bytes()` は検査済みの状態だけを持つため、追加の検査は行わない。

### 3. 公開 API

`SegmentMuxerOptions` に `cmaf: Option<CmafTrackFileBrand>` を追加する。

- `CmafTrackFileBrand` は `Cmfc` のみを持つ enum にする (将来の `cmf2` 追加時に拡張できる形)
- 既定値は `None` で、指定しない場合の出力バイト列は変えない
- wasm の JSON API では指定できない (0163 の `styp` と同じ扱い)

### 4. `ftyp` と `styp` の内容

`cmaf: Some(CmafTrackFileBrand::Cmfc)` を指定した場合、次のとおりにする。

- `ftyp`: `major_brand = cmfc`、`minor_version = 0`。互換 brand は既存の `iso5` / `iso6` / `mp41` と映像コーデックブランドを残し、`cmfc` も含める
- `styp`: `major_brand = cmfs`、`minor_version = 0`。互換 brand はその時点の `ftyp` の互換 brand と同じ集合にする
- `styp` は各メディアセグメントの生成時に、その時点で観測済みのトラックから組み立てる。`with_options()` の時点ではトラックが 0 本であり、後から観測するコーデックブランドを `styp` に含められないため
- `styp` の書き出し自体は 0163 が追加する `SegmentMuxerOptions::styp` の処理を使う。本 issue では、CMAF 指定時に 0163 の `styp` 出力へ渡す `StypBox` を組み立てるだけにして、書き出し処理を二重実装しない
- `cmaf` と 0163 の `styp` を同時に指定した場合は、どちらを優先するかを黙って決めない。`MuxError::EncodeError(Error::invalid_input("cmaf and styp cannot both be specified"))` を返す

### 5. 検証の範囲

本 issue では、ブランドを付けてよい条件 (単一メディアトラック) の検査だけを行う。`colr` / `pasp` の存在、`elst` の制限、clear と encrypted の混在禁止、SAP の先頭配置など、CMAF の他の構造制約の検証は行わない。これらは利用側の構成に依存するため、本 issue の完了条件には含めない。

## 完了条件

- `Brand` に `cmfc` と `cmfs` が追加され、各定数の rustdoc に「どのボックスで使うか」「根拠」「将来変更される可能性」が書かれている
- `SegmentMuxerOptions::cmaf` を指定しない場合の init segment とメディアセグメントのバイト列が既存と一致する
- `cmaf: Some(Cmfc)` を指定した場合の `ftyp` が `major_brand = cmfc`、`minor_version = 0`、互換 brand に `iso5` / `iso6` / `mp41` / `cmfc` を含む。映像トラックを観測した場合は、対応するコーデックブランド (`hev1` / `hvc1` / `av01`) も含む
- `cmaf: Some(Cmfc)` を指定した場合の `styp` が `major_brand = cmfs`、互換 brand が `ftyp` と同じ集合になる
- `cmaf` を指定して複数の `TrackKind` を観測した場合、`create_media_segment_metadata` 系が `MuxError::EncodeError` を返し、状態が変わらない
- `cmaf` と `styp` を同時に指定した場合に `MuxError::EncodeError` を返す
- 生成したセグメントを `Fmp4SegmentDemuxer` で読み戻し、`styp()` が `major_brand = cmfs` の `StypBox` を返す
- 読み戻したサンプルの `data_offset` が指すバイト列が元の payload と一致し、`data_size` / `keyframe` が一致する (`data_offset` の数値は muxer と demuxer で基準が異なるため比較しない)
- `composition_time_offset` は、テスト入力を全サンプル `Some` または全サンプル `None` のどちらかに揃えたうえで往復が一致する (muxer は 1 つでも `Some` があれば全サンプルを `Some` に正規化するため、混在した入力は比較しない)
- `crates/wasm` がコンパイルできる
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz --no-deps` が成功する
- `CHANGES.md` と `skills/shiguredo-mp4/SKILL.md` が更新されている

## 解決方法

- `src/boxes.rs` の `Brand` に `CMFC` と `CMFS` を追加し、各定数の rustdoc に「どのボックスで使うか」「根拠 (ISO/IEC 23000-19:2024 7.6、Amd 2:2026 Table 1)」「将来変更される可能性」を書く
- `src/mux_fmp4_segment.rs` に `CmafTrackFileBrand` を追加し、`SegmentMuxerOptions` に `cmaf: Option<CmafTrackFileBrand>` を追加する
- `src/mux.rs` の `pub use crate::mux_fmp4_segment::{...}` に `CmafTrackFileBrand` を追加する
- `src/mux_fmp4_segment.rs` の `build_ftyp` で、`cmaf` 指定時に major brand と互換 brand を CMAF 向けに切り替える
- `src/mux_fmp4_segment.rs` で、`cmaf` 指定時に `styp` を組み立てて 0163 の `styp` 出力へ渡す。同時指定の検査と、単一メディアトラックの検査を追加する
- `crates/wasm/src/fmp4_segment_mux.rs` の `SegmentMuxerOptions` リテラルに `cmaf: None` を追加し、同ファイルの公開 doc に wasm の JSON API では指定できない旨を書く
- `tests/test_mux_fmp4_segment.rs` に `ftyp` / `styp` の期待値、複数トラックでのエラー、`cmaf` と `styp` の同時指定エラー、往復のテストを追加する
- `tests/test_demux_fmp4_segment.rs` に `styp()` の往復を追加する
- `CHANGES.md` に `[ADD]` (`Brand` の CMAF 定数、CMAF 向け `ftyp` / `styp` 出力) と `[CHANGE]` (`SegmentMuxerOptions` のフィールド追加、`..Default::default()` を使わない構造体リテラルの修正が必要、wasm の JSON では指定不可) を記録する
- `skills/shiguredo-mp4/SKILL.md` に CMAF ブランドと `SegmentMuxerOptions::cmaf` の説明、単一メディアトラックの制約を追記する

## 対象外

- `cmf2` の制約実装 (`elst` を使わない、`trun` version 1 の負の composition time offset、`tfhd` の既定値の繰り返し)
- `cmfl` と `cmf1` (`Amd 2:2026` で追加されたブランド。定義と配置を一次資料で確認できていない)
- CMAF の構造制約の検証 API (単一トラック以外の制約。出力のブランド付与とは独立した機能のため、必要になった時点で別 issue にする)
- `colr` / `pasp` / `elst` の制約、clear と encrypted の混在禁止、SAP の先頭配置の検証
- 字幕トラックの CMAF 対応 (CMAF の `cmfc` は 1 メディアトラックのみ)
- `emsg` (ISO/IEC 23009-1 の DASH のボックスであり、ISO/IEC 14496-12 の fMP4 ボックスではない)

## 関連 issue

- `issues/0163-add-fmp4-segment-boxes.md`: `styp` の実装と `SegmentMuxerOptions::styp`。本 issue はその書き出し処理を再利用する
- `issues/0162-add-cenc-sample-aux-info.md`: fMP4 のサンプル補助情報 (`senc` / `saiz` / `saio`) と `Sample` の `encryption_info`。`Sample` のリテラルを共有するため本 issue の前に実装する
- `issues/closed/0161-add-cenc-init-boxes.md`: 初期化データ向けの CENC ボックス対応
