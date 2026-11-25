//! Fragmented MP4 (fMP4) のマルチプレックス機能を提供するモジュール
//!
//! このモジュールは、複数のメディアトラック（音声・映像・字幕）からのサンプルを
//! 初期化セグメントとメディアセグメントに分けて生成する機能を提供する。
//!
//! # fMP4 の構造
//!
//! fMP4 は以下の 2 種類のセグメントで構成される:
//!
//! - **初期化セグメント**: `ftyp` + `moov` (サンプルテーブルの代わりに `mvex/trex` を含む)
//! - **メディアセグメント**: `moof` + `mdat` のペア（繰り返し）
//!   - セグメントの先頭に `styp` や `sidx` が付くことがある
//!
//! # `Mp4FileMuxer` との主な違い
//!
//! fMP4 の sample entry 自体は `stsd` にしか格納できないが、
//! [`Fmp4SegmentMuxer`] は `create_media_segment_metadata()` に渡されたサンプルから
//! トラック情報と sample entry を学習し、その時点までに観測した内容を反映した
//! init segment を [`init_segment_bytes()`](Fmp4SegmentMuxer::init_segment_bytes) で返す。
//!
//! そのため、`Mp4FileMuxer` と同様にサンプルごとに `track_kind` / `timescale` /
//! `sample_entry` を受け取る設計になっている。
//! 現時点では同一 [`TrackKind`] のトラックは 1 本までに制限している（音声 / 映像 / 字幕 各 1 本）。
//! 将来、同種複数トラックに対応する場合は file muxer と合わせて拡張する想定である。
//!
//! # Examples
//!
//! ```no_run
//! use std::num::NonZeroU32;
//!
//! use shiguredo_mp4::TrackKind;
//! use shiguredo_mp4::mux::{Fmp4SegmentMuxer, Sample};
//!
//! # fn main() -> Result<(), Box<dyn 'static + std::error::Error>> {
//! let sample_entry = todo!("build a sample entry for the codec being used");
//! let mut muxer = Fmp4SegmentMuxer::new()?;
//!
//! // 返り値は moof + mdat header であり、payload 自体は含まれない
//! let samples = vec![Sample {
//!     track_kind: TrackKind::Video,
//!     timescale: NonZeroU32::new(90000).expect("non-zero"),
//!     sample_entry: Some(sample_entry),
//!     duration: 3000,
//!     keyframe: true,
//!     composition_time_offset: None,
//!     data_offset: 0,
//!     data_size: 1024,
//!     encryption_info: None,
//! }];
//! let segment_bytes = muxer.create_media_segment_metadata(&samples)?;
//!
//! // その時点までに観測した内容を反映した init segment を取得する
//! let init_bytes = muxer.init_segment_bytes()?;
//! # Ok(())
//! # }
//! ```
use alloc::{format, vec, vec::Vec};
use core::{num::NonZeroU32, time::Duration};

use crate::{
    BoxHeader, BoxSize, Decode, Either, Encode, Error, FixedPointNumber, Mp4FileTime, SampleFlags,
    TrackKind,
    boxes::{
        Brand, DinfBox, FtypBox, HdlrBox, MdatBox, MdhdBox, MdiaBox, MediaHeader, MehdBox, MfhdBox,
        MfraBox, MfroBox, MinfBox, MoofBox, MoovBox, MvexBox, MvhdBox, NmhdBox, PsshBox, SaioBox,
        SaizBox, SampleEncryptionInfo, SampleEntry, SbgpBox, SbgpEntry, SeigEntry, SencBox,
        SencSample, SgpdBox, SgpdEntry, SidxBox, SidxReference, SmhdBox, StblBox, StcoBox, SthdBox,
        StscBox, StsdBox, StszBox, SttsBox, StypBox, TencBox, TfdtBox, TfhdBox, TfraBox, TfraEntry,
        TkhdBox, TrafBox, TrakBox, TrexBox, TrunBox, TrunSample, TrunSamples, UnknownBox, VmhdBox,
    },
    boxes_cenc::SEIG_GROUPING_TYPE,
    boxes_moov_tree::EdtsBox,
    mux_mp4_file::{MuxError, Sample, TrackMetadata},
};

/// [`SegmentMuxerOptions::cmaf`] で指定する、CMAF の構造ブランド
///
/// CMAF (ISO/IEC 23000-19:2024 7.6) の構造ブランドのうち、本ライブラリが出力を
/// 名乗れるものを表す。指定すると、初期化セグメントの `ftyp` とメディアセグメントの
/// `styp` が、対応する CMAF のブランドになる。
///
/// 現在は CMAF Track File の構造ブランド `cmfc` のみを扱う。
/// より制約の強い構造ブランド (`cmf2` 等) を追加する場合は、この enum に variant を追加する。
/// なお、CMAF のブランドの定義と配置は ISO/IEC 23000-19:2024 に基づくものであり、
/// 将来の改訂で変わる可能性がある。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CmafTrackFileBrand {
    /// CMAF Track File の構造ブランド `cmfc`
    ///
    /// `ftyp` の `major_brand` を `cmfc`、`styp` の `major_brand` を `cmfs` にする。
    /// 1 ファイル 1 メディアトラック、1 つの `moof` に 1 つの `traf` という構造を
    /// 前提とするため、これに反する出力になる場合は
    /// [`Fmp4SegmentMuxer::create_media_segment_metadata()`] 系がエラーを返す。
    Cmfc,
}

/// [`Fmp4SegmentMuxer`] 用のオプション
#[derive(Debug, Clone, Default)]
pub struct SegmentMuxerOptions {
    /// ファイル作成時刻（構築される fMP4 内のメタデータとして使われる）
    ///
    /// デフォルト値は UNIX エポック（1970年1月1日 00:00:00 UTC）
    pub creation_timestamp: Duration,

    /// 音声トラックのメタデータ（`mdhd.language` / `hdlr.name`）
    ///
    /// 現状は同じ `TrackKind` の全トラックに共通の値が適用される。
    /// トラックごとの個別指定は将来の対応
    pub audio_track: TrackMetadata,

    /// 映像トラックのメタデータ（`mdhd.language` / `hdlr.name`）
    ///
    /// 同一 `TrackKind` 内での扱いは [`Self::audio_track`] を参照
    pub video_track: TrackMetadata,

    /// 字幕トラックのメタデータ（`mdhd.language` / `hdlr.name`）
    ///
    /// 同一 `TrackKind` 内での扱いは [`Self::audio_track`] を参照
    pub subtitle_track: TrackMetadata,

    /// 初期化セグメントの `moov` に格納する `pssh` ボックス群
    ///
    /// DRM システムごとの初期化データを指定する。複数指定できる。
    pub pssh_boxes: Vec<PsshBox>,

    /// メディアセグメントの先頭に書き出す `styp` ボックス
    ///
    /// 指定した場合は、[`create_media_segment_metadata()`](Fmp4SegmentMuxer::create_media_segment_metadata)
    /// と [`create_media_segment_metadata_with_sidx()`](Fmp4SegmentMuxer::create_media_segment_metadata_with_sidx)
    /// が返すバイト列の先頭に `styp` を含める。
    /// 既定値は `None` で、指定しない場合の出力バイト列は変わらない。
    ///
    /// [`Self::cmaf`] とは同時に指定できない。
    pub styp: Option<StypBox>,

    /// CMAF の構造ブランドとして出力する場合の種別
    ///
    /// `Some(CmafTrackFileBrand::Cmfc)` を指定すると、初期化セグメントの `ftyp` を
    /// `major_brand = cmfc`、メディアセグメントの `styp` を `major_brand = cmfs` にして出力する。
    /// `styp` の互換 brand は、各メディアセグメントの生成時点で観測済みのトラックから
    /// 組み立てるため、その時点の `ftyp` の互換 brand に `cmfs` を加えた集合になる。
    ///
    /// CMAF の構造ブランドは 1 ファイル 1 メディアトラックを前提とするため、
    /// 複数の [`TrackKind`] を観測した場合、1 つの `moof` に複数の `traf` を書く場合、
    /// 初期化セグメントの `trak` が複数になる場合は、
    /// [`create_media_segment_metadata()`](Fmp4SegmentMuxer::create_media_segment_metadata) 系が
    /// [`MuxError::EncodeError`] を返す。
    ///
    /// [`Self::styp`] とは同時に指定できない。
    /// 既定値は `None` で、指定しない場合の出力バイト列は変わらない。
    pub cmaf: Option<CmafTrackFileBrand>,
}

impl SegmentMuxerOptions {
    /// [`TrackKind`] に対応するトラックメタデータを返す
    pub(crate) fn track_metadata(&self, kind: TrackKind) -> &TrackMetadata {
        match kind {
            TrackKind::Audio => &self.audio_track,
            TrackKind::Video => &self.video_track,
            TrackKind::Subtitle => &self.subtitle_track,
        }
    }
}

#[derive(Debug, Clone)]
struct TrackEntry {
    track_kind: TrackKind,
    timescale: NonZeroU32,
    sample_entries: Vec<SampleEntry>,
    track_id: u32,
    /// 累積デコード時間（タイムスケール単位）
    decode_time: u64,
    current_sample_entry_index: Option<usize>,
}

/// tfra ボックス用の 1 セグメント分のエントリー
#[derive(Debug, Clone)]
struct TfraSegmentEntry {
    /// このセグメントの先頭サンプルのデコード時間
    time: u64,
    /// メディアセグメント列の先頭を 0 としたときの moof ボックスの相対オフセット
    moof_relative_offset: u64,
    /// moof 内でのこのトラックの traf の 1 ベースインデックス
    traf_number: u32,
}

/// [`Fmp4SegmentMuxer::build_media_segment_bytes()`] の結果
#[derive(Debug)]
struct BuiltMediaSegment {
    /// `moof` + `mdat` ヘッダーのバイト列（`styp` は含まない）
    ///
    /// `sidx` を付ける場合、呼び出し側は `styp` の後ろ、このバイト列の前に `sidx` を置く。
    metadata: Vec<u8>,

    /// `styp` ボックスのバイト列
    ///
    /// [`SegmentMuxerOptions::styp`] と [`SegmentMuxerOptions::cmaf`] のどちらも
    /// 指定されていない場合は空になる。
    styp: Vec<u8>,

    /// `mdat` payload のバイト数
    mdat_payload_size: u64,
}

#[derive(Debug)]
struct ResolvedSegmentTrack {
    track_index: usize,
    samples: Vec<ResolvedSegmentSample>,
    total_duration: u64,
    sample_description_index: Option<u32>,
    first_data_offset: u64,
    payload_end: u64,
    /// `tenc.default_is_protected = 1` のトラックの、サンプル順の暗号化情報
    ///
    /// `senc` / `saiz` / `saio` を書き出すときに使う。暗号化されていないトラックは `None`。
    encryption_samples: Option<Vec<SampleEncryptionInfo>>,

    /// `seig` サンプルグループで KID を切り替える場合に `traf` へ書き出すボックス群
    ///
    /// すべてのサンプルの実効 KID が `tenc.default_kid` と同じ場合は `None` になり、
    /// `sgpd` / `sbgp` を書き出さない。
    seig_sample_groups: Option<SeigSampleGroups>,
}

/// `seig` サンプルグループで KID を切り替えるときに `traf` に書くボックス群
#[derive(Debug, Clone)]
struct SeigSampleGroups {
    /// KID ごとの `seig` エントリーを持つ `sgpd`
    sgpd_box: SgpdBox,

    /// 連続する同一 KID ごとの `sbgp`
    sbgp_box: SbgpBox,
}

#[derive(Debug)]
struct ResolvedSegmentSample {
    duration: u32,
    keyframe: bool,
    // `TrunSample::composition_time_offset` に転写するため公開 API と同じ i64 で保持する
    composition_time_offset: Option<i64>,
    data_size: usize,
    encryption_info: Option<SampleEncryptionInfo>,
}

/// サンプルエントリーから取り出した CENC のトラック暗号化情報
#[derive(Debug, Clone)]
struct TrackEncryption {
    tenc_box: TencBox,
}

impl TrackEncryption {
    /// サンプルエントリーから CENC のトラック暗号化情報を取り出す
    ///
    /// `Encv` / `Enca` 以外のサンプルエントリーと、`tenc` を持たないサンプルエントリーは `None` になる。
    fn from_sample_entry(sample_entry: &SampleEntry) -> Option<Self> {
        let sinf_box = match sample_entry {
            SampleEntry::Encv(encv_box) => &encv_box.sinf_box,
            SampleEntry::Enca(enca_box) => &enca_box.sinf_box,
            _ => return None,
        };
        let tenc_box = sinf_box.schi_box.as_ref()?.tenc_box.clone()?;
        Some(Self { tenc_box })
    }

    /// サンプルに付与された暗号化情報が `tenc` の既定値と整合するかを検査する
    ///
    /// 選択的暗号化 (ISO/IEC 23001-7:2023 3.1.9) は対象外である。そのため
    /// `default_is_protected = 1` のトラックは全サンプルに暗号化情報を要求し、
    /// `default_is_protected = 0` のトラックは暗号化情報を禁止する。
    fn validate_sample_encryption(
        &self,
        encryption_info: Option<&SampleEncryptionInfo>,
    ) -> Result<(), MuxError> {
        if self.tenc_box.default_is_protected == 1 {
            let Some(encryption_info) = encryption_info else {
                return Err(MuxError::EncodeError(Error::invalid_input(
                    "encrypted track requires sample encryption info for every sample",
                )));
            };
            let expected_iv_size = usize::from(self.tenc_box.default_per_sample_iv_size);
            if encryption_info.iv.len() != expected_iv_size {
                return Err(MuxError::EncodeError(Error::invalid_input(format!(
                    "sample IV size {} does not match tenc default_per_sample_iv_size {expected_iv_size}",
                    encryption_info.iv.len(),
                ))));
            }
            if encryption_info.subsamples.len() > usize::from(u16::MAX) {
                return Err(MuxError::EncodeError(Error::invalid_input(
                    "sample subsample count exceeds u16::MAX",
                )));
            }
            if encryption_info.kid.is_some() {
                // KID の切り替えは per-sample IV のトラックだけを対象とする。
                // constant IV のトラックでは `seig` の IV サイズの上書きになり、
                // `senc` では表現できない
                if self.tenc_box.default_per_sample_iv_size == 0 {
                    return Err(MuxError::EncodeError(Error::invalid_input(
                        "sample KID rotation requires per-sample IV",
                    )));
                }
                // `seig` のパターン暗号化パラメーターは常に 0 / 0 であり、`tenc` に
                // 0 以外のパターン値がある場合はそれを上書きして復号結果が変わってしまう。
                // version 1 の 0 / 0 はパターン暗号化を使わないことを表すため、
                // demux 側の `check_seig_entry_against_tenc()` と同じく値で判定する。
                // version 0 は `None` なので 0 として扱い、従来どおり KID を切り替えられる。
                let crypt_byte_block = self
                    .tenc_box
                    .default_crypt_byte_block
                    .map(|value| value.get())
                    .unwrap_or(0);
                let skip_byte_block = self
                    .tenc_box
                    .default_skip_byte_block
                    .map(|value| value.get())
                    .unwrap_or(0);
                if crypt_byte_block != 0 || skip_byte_block != 0 {
                    return Err(MuxError::EncodeError(Error::invalid_input(
                        "sample KID rotation is not supported for tracks with tenc pattern encryption parameters",
                    )));
                }
            }
            Ok(())
        } else if encryption_info.is_some() {
            Err(MuxError::EncodeError(Error::invalid_input(
                "clear track must not have sample encryption info",
            )))
        } else {
            Ok(())
        }
    }
}

/// fMP4 ファイルを生成するマルチプレックス処理を行うための構造体
///
/// この構造体は、複数のメディアトラック（音声・映像・字幕）からのサンプルを
///  fMP4 形式の初期化セグメントとメディアセグメントに変換する。
///
/// [`crate::mux::Mp4FileMuxer`] と同様に、サンプルごとに `track_kind` / `timescale` /
/// `sample_entry` を受け取り、そこからトラック情報を蓄積する。
/// init segment は、[`init_segment_bytes()`](Self::init_segment_bytes) を呼んだ時点までに
/// 観測した内容を反映して構築される。
///
/// 基本的な使用フロー：
/// 1. [`new()`](Self::new) または [`with_options()`](Self::with_options) でインスタンスを作成
/// 2. [`create_media_segment_metadata()`](Self::create_media_segment_metadata) を繰り返し呼び出してトラック情報と sample entry を蓄積しつつメディアセグメントを生成
/// 3. [`init_segment_bytes()`](Self::init_segment_bytes) で、その時点までに観測した内容を反映した初期化セグメントを取得
/// 4. 必要に応じて [`mfra_bytes()`](Self::mfra_bytes) でランダムアクセスインデックスを取得
#[derive(Debug, Clone)]
pub struct Fmp4SegmentMuxer {
    tracks: Vec<TrackEntry>,
    options: SegmentMuxerOptions,
    sequence_number: u32,
    /// `create_media_segment_metadata*()` で表現したメディアセグメントのバイト数累計
    media_bytes_written: u64,
    /// トラックごとの tfra エントリー（tracks と同じインデックス）
    tfra_entries: Vec<Vec<TfraSegmentEntry>>,
}

impl Fmp4SegmentMuxer {
    /// [`Fmp4SegmentMuxer`] インスタンスを生成する
    pub fn new() -> Result<Self, MuxError> {
        Self::with_options(SegmentMuxerOptions::default())
    }

    /// オプションを指定して [`Fmp4SegmentMuxer`] インスタンスを生成する
    ///
    /// [`SegmentMuxerOptions::cmaf`] と [`SegmentMuxerOptions::styp`] を同時に指定した場合は、
    /// どちらを優先するかを決められないため [`MuxError::EncodeError`] を返す。
    pub fn with_options(options: SegmentMuxerOptions) -> Result<Self, MuxError> {
        // CMAF 指定時は構造ブランドに合わせた `styp` を組み立てて書き出すため、
        // 利用者が指定した `styp` と同時には指定できない。
        // どちらを優先するかを黙って決めないよう、インスタンスの生成時点で拒否する。
        if options.cmaf.is_some() && options.styp.is_some() {
            return Err(MuxError::EncodeError(Error::invalid_input(
                "cmaf and styp cannot both be specified",
            )));
        }
        Ok(Self {
            tracks: Vec::new(),
            options,
            sequence_number: 0,
            media_bytes_written: 0,
            tfra_entries: Vec::new(),
        })
    }

    /// 初期化セグメント（`ftyp` + `moov`）のバイト列を返す
    ///
    /// 返される `moov` には、このメソッドを呼んだ時点までに
    /// [`create_media_segment_metadata()`](Self::create_media_segment_metadata) ないし
    /// [`create_media_segment_metadata_with_sidx()`](Self::create_media_segment_metadata_with_sidx) で
    /// 観測したトラック情報と sample entry が反映される。
    ///
    /// まだどのトラックも観測されていない状態では `EmptyTracks` を返す。
    /// また、後から新しい sample entry を観測した場合は、
    /// このメソッドを再度呼ぶことで更新後の `stsd` を含む init segment を取得できる。
    pub fn init_segment_bytes(&self) -> Result<Vec<u8>, MuxError> {
        if self.tracks.is_empty() {
            return Err(MuxError::EmptyTracks);
        }
        let ftyp = self.build_ftyp();
        let moov = self.build_init_moov()?;

        let mut bytes = ftyp.encode_to_vec()?;
        bytes.extend_from_slice(&moov.encode_to_vec()?);
        Ok(bytes)
    }

    /// メディアセグメント先頭のメタデータ（`styp` + `moof` + `mdat` ヘッダー）のバイト列を生成する
    ///
    /// [`SegmentMuxerOptions::styp`] を指定した場合は、返り値の先頭に `styp` ボックスを含める。
    /// 指定しない場合の出力順は `moof` + `mdat` ヘッダーである。
    /// [`SegmentMuxerOptions::cmaf`] を指定した場合は、この時点で観測済みのトラックから
    /// 組み立てた `styp` を先頭に含める。
    ///
    /// [`SegmentMuxerOptions::cmaf`] を指定した場合、CMAF の構造ブランドを宣言できる
    /// 出力になるかを検査し、複数の [`TrackKind`] を観測する場合、1 つの `moof` に
    /// 複数の `traf` を書く場合、初期化セグメントの `trak` が複数になる場合は
    /// [`MuxError::EncodeError`] を返す。エラー時は muxer の内部状態を変更しない。
    ///
    /// `samples` に含まれるサンプルは `track_kind` でグループ化して扱われる。
    /// 同一セグメント内の同一トラックのサンプルは、`data_offset` の昇順で
    /// `mdat` payload 領域内に連続して配置されている必要がある。
    /// トラック間の payload 配置順は `data_offset` に従って決定される。
    ///
    /// 返り値に含まれるのは `styp`（指定時のみ）と `moof` と `mdat` ヘッダーのみであり、
    /// `mdat` payload そのものは含まれない。
    /// 呼び出し側は、返り値の直後に `samples` が参照する payload 群を
    /// `data_offset` / `data_size` の指定どおりに配置する必要がある。
    ///
    /// `samples[*].data_offset` の基準は、
    /// [`crate::mux::Mp4FileMuxer::append_sample()`] で使う「ファイル全体の絶対位置」ではなく、
    /// 「今回のセグメントに属する `mdat` payload 領域の先頭からの相対位置」である。
    ///
    /// `samples[*].composition_time_offset` は公開 API では demuxer と揃えて `i64` だが、
    /// `trun` に書けるのは ISO/IEC 14496-12 8.8.8 の制約により
    /// version 0 で `0..=u32::MAX`、version 1 で `i32::MIN..=i32::MAX` の範囲に限られる。
    /// また、負値と `> i32::MAX` の値がひとつの `trun` に混在する場合は
    /// どちらの版でも表現できないためエラーになる。
    ///
    /// 現実装は `1 track = 1 traf = 1 trun` を前提としている。
    /// そのため、ひとつのトラックに属する payload を複数の離れた範囲へ分割して
    /// 配置することはサポートしていない。
    ///
    /// このメソッドは、メディアセグメントを生成するだけでなく、
    /// `init_segment_bytes()` の構築に必要なトラック情報と sample entry も内部に蓄積する。
    pub fn create_media_segment_metadata(
        &mut self,
        samples: &[Sample],
    ) -> Result<Vec<u8>, MuxError> {
        let media_segment = self.build_media_segment_bytes(samples)?;
        // セグメントの先頭に `styp` を置く。`styp` を指定しない場合は空なので、
        // 既存の出力バイト列は変わらない
        let mut result = media_segment.styp;
        result.extend_from_slice(&media_segment.metadata);
        Ok(result)
    }

    /// `sidx` ボックスを先頭に付加したメディアセグメント先頭メタデータを生成する
    ///
    /// `sidx` はセグメントインデックスボックスであり、
    /// MPEG-DASH などのアダプティブストリーミングで利用される。
    ///
    /// [`SegmentMuxerOptions::styp`] を指定した場合は、出力順は `styp` + `sidx` + `moof` + `mdat` ヘッダーになる。
    /// `styp` は `sidx` より前にあるため `sidx` の参照範囲には入らず、
    /// `sidx` の `referenced_size` は `moof` と `mdat` の合計、`first_offset` は 0 になる。
    /// [`SegmentMuxerOptions::cmaf`] を指定した場合も、この時点で観測済みのトラックから
    /// 組み立てた `styp` を同じ位置に含める。
    ///
    /// [`SegmentMuxerOptions::cmaf`] を指定した場合の構造の検査は
    /// [`create_media_segment_metadata()`](Self::create_media_segment_metadata) と同じであり、
    /// 検査に反する場合は [`MuxError::EncodeError`] を返して内部状態を変更しない。
    ///
    /// `sidx` の `reference_id` は最初のサンプルのトラック種別に対応する track_id を使用する。
    ///
    /// `sidx` の `earliest_presentation_time` は、参照トラック各サンプルの
    /// PTS（`DTS + composition_time_offset`、`None` は 0）の最小値である。
    /// PTS が負、あるいは PTS または参照トラックの累積 DTS が `u64` の表現範囲を外れた場合は
    /// [`MuxError::Overflow`] を返す。
    /// エラー時は muxer の内部状態を変更しない。
    ///
    /// `sidx` の `starts_with_sap` / `sap_type` は、EPT を採ったサンプル（参照トラックで
    /// PTS が最小のサンプル。同値時は samples[] 内で先に出現したもの）の `keyframe` を基準にする。
    /// `sap_type` はキーフレームを一律 `1`（SAP type 1 相当）、非キーフレームを `0` として扱う近似であり、
    /// ISO/IEC 14496-12 が定義する SAP type 1〜6 の区別（open GoP の I フレームは本来 type 3 相当 等）は
    /// 行わない。
    ///
    /// このメソッドも [`create_media_segment_metadata()`](Self::create_media_segment_metadata) と同様に、
    /// 観測したトラック情報と sample entry を内部に蓄積する。
    ///
    /// このメソッドで生成した sidx 付きセグメントを初期化セグメントの後ろに並べたファイル配置において、
    /// [`mfra_bytes()`](Self::mfra_bytes) が返す `tfra.moof_offset` は、
    /// `styp` を指定した場合はそのサイズも含めた実際の `moof` 位置と整合する。
    pub fn create_media_segment_metadata_with_sidx(
        &mut self,
        samples: &[Sample],
    ) -> Result<Vec<u8>, MuxError> {
        if samples.is_empty() {
            return Err(MuxError::EmptySamples);
        }
        let first_track_kind = samples[0].track_kind;

        let subsegment_duration: u64 = samples
            .iter()
            .filter(|s| s.track_kind == first_track_kind)
            .map(|s| s.duration as u64)
            .sum();
        // セグメントを構築して状態を進める前に検査する。構築後に失敗すると
        // decode_time などが進んだままになり、次のセグメントの時刻がずれる
        let subsegment_duration_u32 = u32::try_from(subsegment_duration).map_err(|_| {
            MuxError::EncodeError(Error::invalid_data(
                "subsegment_duration overflow: duration exceeds u32 max",
            ))
        })?;
        // build_media_segment_bytes 呼び出し前なので、ここでの decode_time は
        // 当該セグメント先頭の累積 DTS（未登録トラックなら 0）である。
        let decode_time = self
            .tracks
            .iter()
            .find(|track| track.track_kind == first_track_kind)
            .map_or(0, |track| track.decode_time);
        // ISO/IEC 14496-12 8.16.3.3 の starts_with_SAP は「参照される subsegment が
        // SAP から始まる」の意味であり、EPT に対応するアクセスユニットが SAP かどうかが問われる。
        let (earliest_presentation_time, sap_at_ept) =
            compute_earliest_presentation_time(samples, first_track_kind, decode_time)?;

        // build_media_segment_bytes を呼ぶ前に、トラックごとの tfra エントリー数を記録しておく。
        // sidx エンコード後にこの記録と比較すれば、今回の呼び出しで新規追加された
        // tfra エントリー（各トラックにつき末尾 1 件）を特定できる。
        let pre_tfra_lens: Vec<usize> = self
            .tfra_entries
            .iter()
            .map(|entries| entries.len())
            .collect();

        let media_segment = self.build_media_segment_bytes(samples)?;
        // `sidx` の参照範囲は `sidx` の直後から始まる。`styp` は `sidx` より前にあるため、
        // `referenced_size` には `moof` と `mdat` の合計だけを含める。
        let media_segment_size = media_segment
            .metadata
            .len()
            .checked_add(
                usize::try_from(media_segment.mdat_payload_size).map_err(|_| MuxError::Overflow)?,
            )
            .ok_or(MuxError::Overflow)?;
        let reference_track = self
            .tracks
            .iter()
            .find(|track| track.track_kind == first_track_kind)
            .expect("bug: first sample track must exist after media segment creation");

        let referenced_size = u32::try_from(media_segment_size).map_err(|_| {
            MuxError::EncodeError(Error::invalid_data(
                "referenced_size overflow: media segment size exceeds u32 max",
            ))
        })?;
        let sidx_box = SidxBox {
            reference_id: reference_track.track_id,
            timescale: reference_track.timescale.get(),
            earliest_presentation_time,
            first_offset: 0,
            references: vec![SidxReference {
                reference_type: false,
                referenced_size,
                subsegment_duration: subsegment_duration_u32,
                starts_with_sap: sap_at_ept,
                sap_type: u8::from(sap_at_ept),
                sap_delta_time: 0,
            }],
        };

        let sidx_bytes = sidx_box.encode_to_vec()?;
        let sidx_size = sidx_bytes.len() as u64;

        // 後続セグメントが moof_relative_offset の起点とする media_bytes_written に対して、
        // 先にオーバーフロー検査を済ませる。ここで検査が通れば、当該セグメントの
        // tfra エントリー（moof_relative_offset は加算前の media_bytes_written 以下）への
        // 加算はオーバーフローしない。
        let new_media_bytes_written = self
            .media_bytes_written
            .checked_add(sidx_size)
            .ok_or(MuxError::Overflow)?;

        // 当該セグメントで新規追加された tfra エントリーの moof_relative_offset に sidx サイズを加算する。
        // build_media_segment_bytes は各 track_kind につき最大 1 件だけ push するため、
        // pre_tfra_lens よりも長くなったトラックは末尾 1 件が今回の新規エントリーである。
        for (track_index, entries) in self.tfra_entries.iter_mut().enumerate() {
            // build_media_segment_bytes が新規トラックを追加した場合、self.tfra_entries は
            // pre_tfra_lens より長くなり、末尾の新規トラックについては pre_tfra_lens に対応する
            // 要素が存在しない。この場合の pre_len は「以前は存在しなかった = 0 件」を表す。
            let pre_len = pre_tfra_lens.get(track_index).copied().unwrap_or(0);
            if entries.len() > pre_len {
                let last = entries
                    .last_mut()
                    .expect("bug: tfra entries grew but the vec is empty");
                last.moof_relative_offset = last
                    .moof_relative_offset
                    .checked_add(sidx_size)
                    .expect(
                        "bug: moof_relative_offset <= media_bytes_written, so sidx_size fits after the media_bytes_written check",
                    );
            }
        }

        self.media_bytes_written = new_media_bytes_written;

        // 出力順は `styp`（指定時のみ）+ `sidx` + `moof` + `mdat` ヘッダーになる
        let mut result = media_segment.styp;
        result.extend_from_slice(&sidx_bytes);
        result.extend_from_slice(&media_segment.metadata);
        Ok(result)
    }

    /// CMAF の構造ブランドを宣言できるメディアセグメントかを検査する
    ///
    /// CMAF (ISO/IEC 23000-19:2024 7.6) の構造ブランドは、1 ファイル 1 メディアトラック、
    /// 1 つの `moof` に 1 つの `traf` という構造を前提とする。
    /// [`SegmentMuxerOptions::cmaf`] を指定していない場合は何もしない。
    ///
    /// 検査に反する場合は [`MuxError::EncodeError`] を返す。
    /// この関数は muxer の内部状態を変更しない。
    /// なお、ここでの条件は ISO/IEC 23000-19:2024 に基づくものであり、
    /// 将来の改訂で変わる可能性がある。
    fn validate_cmaf_track_structure(&self, samples: &[Sample]) -> Result<(), MuxError> {
        if self.options.cmaf.is_none() {
            return Ok(());
        }

        // このメディアセグメントの `moof` に書く `traf` の数。
        // 現実装は 1 TrackKind = 1 traf = 1 trun である
        let mut segment_kinds = Vec::new();
        for sample in samples {
            if !segment_kinds.contains(&sample.track_kind) {
                segment_kinds.push(sample.track_kind);
            }
        }
        if segment_kinds.len() > 1 {
            return Err(MuxError::EncodeError(Error::invalid_input(format!(
                "cmaf requires one traf per moof, but {} trafs would be written",
                segment_kinds.len()
            ))));
        }

        // 観測済みの TrackKind と、この呼び出しで観測する TrackKind を合わせた数。
        // self.tracks は成功した呼び出しでしか増えないため、ここで累積の種別数を検査できる
        let mut observed_kinds: Vec<TrackKind> =
            self.tracks.iter().map(|track| track.track_kind).collect();
        for track_kind in &segment_kinds {
            if !observed_kinds.contains(track_kind) {
                observed_kinds.push(*track_kind);
            }
        }
        if observed_kinds.len() > 1 {
            return Err(MuxError::EncodeError(Error::invalid_input(format!(
                "cmaf requires a single media track kind, but {} kinds would be observed",
                observed_kinds.len()
            ))));
        }

        // 初期化セグメントの `trak` の数。
        // 現実装は 1 TrackKind = 1 トラックなので上の検査に含まれるが、
        // CMAF の制約としては独立しているため、将来 1 つの TrackKind に複数トラックを
        // 持てるようになった場合に検査が漏れないよう明示的に検査する
        let new_track_count = segment_kinds
            .iter()
            .filter(|track_kind| {
                !self
                    .tracks
                    .iter()
                    .any(|track| track.track_kind == **track_kind)
            })
            .count();
        let trak_count = self.tracks.len() + new_track_count;
        if trak_count > 1 {
            return Err(MuxError::EncodeError(Error::invalid_input(format!(
                "cmaf requires one trak in the init segment, but {trak_count} traks would be present"
            ))));
        }

        Ok(())
    }

    fn build_media_segment_bytes(
        &mut self,
        samples: &[Sample],
    ) -> Result<BuiltMediaSegment, MuxError> {
        if samples.is_empty() {
            return Err(MuxError::EmptySamples);
        }
        // CMAF の構造ブランドを宣言できる出力かを検査する。
        // ここから先でトラック情報を書き換えるため、検査はその前に行う
        self.validate_cmaf_track_structure(samples)?;
        // `styp` はセグメントの先頭に置く。エンコードに失敗した場合は
        // 以降で内部状態を変更しないよう、先に済ませておく。
        // CMAF 指定時は、この時点で観測済みのトラックから `StypBox` を組み立てる。
        // 組み立てた `StypBox` は `SegmentMuxerOptions::styp` と同じ書き出し経路に渡し、
        // `styp` の書き出し処理を二重実装しない
        let styp_box = match self.options.cmaf {
            Some(CmafTrackFileBrand::Cmfc) => {
                let segment_sample_entries: Vec<&SampleEntry> = samples
                    .iter()
                    .filter_map(|sample| sample.sample_entry.as_ref())
                    .collect();
                Some(StypBox {
                    major_brand: Brand::CMFS,
                    minor_version: 0,
                    compatible_brands: self.build_styp_compatible_brands(&segment_sample_entries),
                })
            }
            None => self.options.styp.clone(),
        };
        let styp = styp_box
            .as_ref()
            .map(|styp_box| styp_box.encode_to_vec())
            .transpose()?
            .unwrap_or_default();
        let styp_size = styp.len() as u64;
        // `moof` は `styp` の直後に置くため、`styp` を指定した場合は
        // そのサイズの分だけ `moof_relative_offset` が後ろになる
        let moof_relative_offset = self
            .media_bytes_written
            .checked_add(styp_size)
            .ok_or(MuxError::Overflow)?;
        let sequence_number = self.sequence_number.checked_add(1).ok_or_else(|| {
            MuxError::EncodeError(Error::invalid_data("sequence number overflow"))
        })?;
        let mut next_tracks = self.tracks.clone();
        let resolved_tracks = resolve_segment_tracks(&mut next_tracks, samples)?;

        // mdat ヘッダーを先に確定する
        // ペイロードサイズに応じて U32 (8 バイトヘッダー) か U64 (16 バイトヘッダー) を選択する
        let mdat_payload_size = resolved_tracks
            .iter()
            .map(|track| track.payload_end)
            .max()
            .ok_or(MuxError::EmptySamples)?;
        // 現実のメディアで payload が u64::MAX 近傍になることはまずない。
        // それでも病理的な data_size なら公開 API から到達し得るため、防御的に検査する。
        let mdat_box_size_value = (BoxHeader::MIN_SIZE as u64)
            .checked_add(mdat_payload_size)
            .ok_or(MuxError::Overflow)?;
        let (mdat_box_size, mdat_header_size) = if mdat_box_size_value <= u32::MAX as u64 {
            (
                BoxSize::U32(mdat_box_size_value as u32),
                BoxHeader::MIN_SIZE,
            )
        } else {
            // 拡張ヘッダー（largesize 含む 16 バイト）込みの合計を再計算する。
            // MIN_SIZE + payload が成功しても、payload が [u64::MAX - 15, u64::MAX - 8]
            // なら 16 + payload はオーバーフローし得るため、ここでも checked_add が必要。
            let extended_box_size = 16u64
                .checked_add(mdat_payload_size)
                .ok_or(MuxError::Overflow)?;
            (BoxSize::U64(extended_box_size), 16)
        };
        let mdat_header = BoxHeader::new(MdatBox::TYPE, mdat_box_size);
        let mdat_header_bytes = mdat_header.encode_to_vec()?;

        // moof のサイズを確定させるために、仮の data_offset=0 / saio offset=0 で一度エンコードする。
        // data_offset (i32) と saio の offset (version 0 の u32) は固定長フィールドのため、
        // 値が変わっても moof のサイズは変わらない。
        let placeholder_offsets = vec![0i32; next_tracks.len()];
        let placeholder_saio_offsets = vec![0u32; resolved_tracks.len()];
        let placeholder_moof = self.build_moof(
            &next_tracks,
            &resolved_tracks,
            sequence_number,
            &placeholder_offsets,
            &placeholder_saio_offsets,
        )?;
        let moof_size = placeholder_moof.encode_to_vec()?.len();

        // `saio` の offset は、moof 先頭から、その traf の `senc` 内の最初のサンプル SAI 先頭
        // (sample_count フィールドの直後) までのバイト数になる。
        // 仮の moof のレイアウトから求める。実際の offset を入れても各ボックスの長さは変わらない。
        let saio_offsets: Vec<u32> = compute_senc_sai_offsets(&placeholder_moof)?
            .into_iter()
            .map(|offset| offset.unwrap_or(0))
            .collect();

        // 各トラックのサンプルデータの data_offset (moof 先頭からの相対値) を計算する
        let mut track_data_offsets = vec![0i32; next_tracks.len()];
        let data_offset_base = u64::try_from(moof_size + mdat_header_size).map_err(|_| {
            MuxError::EncodeError(Error::invalid_data(
                "data_offset base overflow: moof + mdat header exceeds u64 max",
            ))
        })?;

        for resolved_track in &resolved_tracks {
            let track_data_offset = data_offset_base
                .checked_add(resolved_track.first_data_offset)
                .ok_or(MuxError::Overflow)?;
            track_data_offsets[resolved_track.track_index] = i32::try_from(track_data_offset)
                .map_err(|_| {
                    MuxError::EncodeError(Error::invalid_data(
                        "data_offset overflow: moof + mdat header exceeds i32 max",
                    ))
                })?;
        }

        // 正しい data_offset / saio offset で moof を構築する
        let moof = self.build_moof(
            &next_tracks,
            &resolved_tracks,
            sequence_number,
            &track_data_offsets,
            &saio_offsets,
        )?;
        let moof_bytes = moof.encode_to_vec()?;
        // 2 パスで moof のサイズが変わっていないことを確認する。
        // 変わっていた場合は data_offset の計算が成り立たない
        if moof_bytes.len() != moof_size {
            return Err(MuxError::EncodeError(Error::invalid_input(
                "moof size changed between the two encode passes",
            )));
        }

        // 返り値の metadata には moof + mdat ヘッダーのみを含める（styp は別に返す）。
        let mut segment = moof_bytes;
        segment.extend_from_slice(&mdat_header_bytes);

        // tfra エントリーを記録してから decode_time を更新する
        let mut next_tfra_entries = self.tfra_entries.clone();
        while next_tfra_entries.len() < next_tracks.len() {
            next_tfra_entries.push(Vec::new());
        }
        for (traf_pos, resolved_track) in resolved_tracks.iter().enumerate() {
            let ti = resolved_track.track_index;
            let entry = TfraSegmentEntry {
                time: self.tracks.get(ti).map_or(0, |track| track.decode_time),
                moof_relative_offset,
                traf_number: u32::try_from(traf_pos + 1).expect("traf count exceeds u32::MAX"),
            };
            next_tfra_entries[ti].push(entry);
        }

        for resolved_track in &resolved_tracks {
            let track = &mut next_tracks[resolved_track.track_index];
            track.decode_time = track
                .decode_time
                .checked_add(resolved_track.total_duration)
                .ok_or(MuxError::Overflow)?;
        }

        self.media_bytes_written = self
            .media_bytes_written
            .checked_add(styp_size)
            .and_then(|written| written.checked_add(segment.len() as u64))
            .and_then(|written| written.checked_add(mdat_payload_size))
            .ok_or(MuxError::Overflow)?;
        self.sequence_number = sequence_number;
        self.tracks = next_tracks;
        self.tfra_entries = next_tfra_entries;
        Ok(BuiltMediaSegment {
            metadata: segment,
            styp,
            mdat_payload_size,
        })
    }

    /// ランダムアクセスインデックス（`mfra`）のバイト列を生成する
    ///
    /// `mfra` ボックスはファイルの末尾に付加することで、
    /// ランダムアクセスを高速化するために利用される。
    ///
    /// `mfra` 内の `tfra.moof_offset` は、
    /// このメソッドを呼んだ時点での [`init_segment_bytes()`](Self::init_segment_bytes)
    /// のサイズを先頭オフセットとして計算される。
    /// `styp` や `sidx` を付けたメディアセグメントでは、それらのサイズも加算した実際の `moof` 位置になる。
    ///
    /// したがって、`mfra` を実際に付加するファイルでは、
    /// このメソッドで前提にした init segment を先頭に配置する必要がある。
    /// 途中で観測済みトラックや sample entry が増えた場合は init segment の内容とサイズも
    /// 変わり得るため、最終的に先頭へ配置する init segment を確定させた後で
    /// `mfra_bytes()` を呼ぶこと。
    pub fn mfra_bytes(&self) -> Result<Vec<u8>, MuxError> {
        let init_segment_size =
            u64::try_from(self.init_segment_bytes()?.len()).expect("init segment size exceeds u64");
        let mut tfra_boxes = Vec::new();

        for (ti, entries) in self.tfra_entries.iter().enumerate() {
            if entries.is_empty() {
                continue;
            }
            let track = &self.tracks[ti];

            // time / moof_offset が u32 に収まるか否かで version を決める。
            // moof_offset は init + relative の和なので、加算オーバーフローもここで検出する。
            let mut needs_v1 = false;
            let mut tfra_entries = Vec::new();
            for e in entries {
                let moof_offset = init_segment_size
                    .checked_add(e.moof_relative_offset)
                    .ok_or(MuxError::Overflow)?;
                if e.time > u32::MAX as u64 || moof_offset > u32::MAX as u64 {
                    needs_v1 = true;
                }
                tfra_entries.push(TfraEntry {
                    time: e.time,
                    moof_offset,
                    traf_number: e.traf_number,
                    trun_number: 1,
                    sample_number: 1,
                });
            }
            let version = if needs_v1 { 1 } else { 0 };

            // traf_number の最大値に応じてフィールドサイズを決定する
            // ISO 14496-12: 0=1byte, 1=2bytes, 2=3bytes, 3=4bytes
            let max_traf_num = entries.iter().map(|e| e.traf_number).max().unwrap_or(0);
            let length_size_of_traf_num: u8 = if max_traf_num <= 0xFF {
                0
            } else if max_traf_num <= 0xFFFF {
                1
            } else if max_traf_num <= 0xFF_FFFF {
                2
            } else {
                3
            };

            tfra_boxes.push(TfraBox {
                version,
                track_id: track.track_id,
                length_size_of_traf_num,
                // trun_number / sample_number は常に 1 なので 1 バイトで十分
                length_size_of_trun_num: 0,
                length_size_of_sample_num: 0,
                entries: tfra_entries,
            });
        }

        // mfro.size には MfraBox::encode がエンコード後の mfra 全体のバイト数を書き込むため、
        // ここで入れる値は出力に使われない
        let mfra_box = MfraBox {
            tfra_boxes,
            mfro_box: MfroBox { size: 0 },
        };
        Ok(mfra_box.encode_to_vec()?)
    }

    /// CMAF の構造ブランドを指定している場合に、`ftyp` の `major_brand` に使うブランドを返す
    ///
    /// CMAF の構造ブランドを指定していない場合は `None` を返し、
    /// `ftyp` の従来の `major_brand` を使う。
    fn cmaf_ftyp_brand(&self) -> Option<Brand> {
        self.options.cmaf.map(|cmaf| match cmaf {
            CmafTrackFileBrand::Cmfc => Brand::CMFC,
        })
    }

    /// `ftyp` / `styp` の互換 brand 集合を組み立てる
    ///
    /// `extra_sample_entries` には、まだ `self.tracks` に反映されていないサンプルエントリー
    /// （これから生成するメディアセグメントで観測するもの）を渡す。
    /// `styp` はメディアセグメントの生成時点で組み立てるため、そのセグメントで
    /// 初めて観測するコーデックブランドも含める必要がある。
    fn build_compatible_brands(&self, extra_sample_entries: &[&SampleEntry]) -> Vec<Brand> {
        let mut has_hev1 = false;
        let mut has_hvc1 = false;
        let mut has_av01 = false;

        let sample_entries = self
            .tracks
            .iter()
            .flat_map(|track| track.sample_entries.iter())
            .chain(extra_sample_entries.iter().copied());
        for sample_entry in sample_entries {
            match sample_entry {
                SampleEntry::Hev1(_) => has_hev1 = true,
                SampleEntry::Hvc1(_) => has_hvc1 = true,
                SampleEntry::Av01(_) => has_av01 = true,
                SampleEntry::Encv(b) => match &b.sinf_box.frma_box.data_format {
                    b"hev1" => has_hev1 = true,
                    b"hvc1" => has_hvc1 = true,
                    b"av01" => has_av01 = true,
                    _ => {}
                },
                _ => {}
            }
        }

        // build_moof() はすべての tfhd で default_base_is_moof を true にする。
        // ISO/IEC 14496-12:2022 8.8.7.1 は、このフラグを iso5 より前の brand や
        // 互換 brand を含むファイルで使ってはならないと定めており、NOTE では
        // このフラグが以前の brand と互換でないため、以前の brand が ftyp に
        // 含まれるときは立てられないとしている。
        // 附属書 E の NOTE も、isom (E.2) と avc1 (E.3) を付けたファイルでは
        // このフラグを立てられないとしている。
        // そのため compatible_brands からは、この制約の対象になる isom と avc1 を外す。
        // iso5 / iso6 は制約の対象外であり、mp41 は ISO/IEC 14496-14 の brand で
        // 8.8.7.1 と附属書 E の NOTE が扱う ISO/IEC 14496-12 の brand ではないため、
        // いずれも従来どおり残す。
        // なお、ここでの扱いは ISO/IEC 14496-12:2022 に基づくものであり、
        // 将来の改訂で変わる可能性がある。
        let mut compatible_brands = vec![Brand::ISO5, Brand::ISO6, Brand::MP41];
        // CMAF の構造ブランドを指定した場合は、構造ブランド自身も互換 brand に含める。
        // `styp` の互換 brand は、この集合に `major_brand` の `cmfs` を加えた集合になる
        // （`build_styp_compatible_brands()` を参照）。
        // なお、CMAF のブランドの定義と配置は ISO/IEC 23000-19:2024 に基づくものであり、
        // 将来の改訂で変わる可能性がある。
        if let Some(cmaf_brand) = self.cmaf_ftyp_brand() {
            compatible_brands.push(cmaf_brand);
        }
        if has_hev1 {
            compatible_brands.push(Brand::HEV1);
        }
        if has_hvc1 {
            compatible_brands.push(Brand::HVC1);
        }
        if has_av01 {
            compatible_brands.push(Brand::AV01);
        }
        compatible_brands
    }

    /// `styp` の互換 brand 集合を組み立てる
    ///
    /// [`StypBox::compatible_brands`] は `major_brand` を含めた列挙と定めているため、
    /// [`build_compatible_brands()`](Self::build_compatible_brands) が返す `ftyp` の
    /// 互換 brand 集合の先頭に `major_brand` の `cmfs` を加える。
    /// `cmfs` は `styp` で CMAF Segment を示すブランドであり、`ftyp` では使わないため、
    /// `ftyp` の互換 brand 集合そのものには含めない。
    /// なお、CMAF のブランドの定義と配置は ISO/IEC 23000-19:2024 に基づくものであり、
    /// 将来の改訂で変わる可能性がある。
    fn build_styp_compatible_brands(&self, extra_sample_entries: &[&SampleEntry]) -> Vec<Brand> {
        let mut compatible_brands = self.build_compatible_brands(extra_sample_entries);
        compatible_brands.insert(0, Brand::CMFS);
        compatible_brands
    }

    fn build_ftyp(&self) -> FtypBox {
        FtypBox {
            major_brand: self.cmaf_ftyp_brand().unwrap_or(Brand::ISO5),
            minor_version: 0,
            compatible_brands: self.build_compatible_brands(&[]),
        }
    }

    fn build_init_moov(&self) -> Result<MoovBox, MuxError> {
        if self.tracks.is_empty() {
            return Err(MuxError::EmptyTracks);
        }
        let creation_time = Mp4FileTime::from_unix_time(self.options.creation_timestamp);

        let trak_boxes: Result<Vec<_>, MuxError> = self
            .tracks
            .iter()
            .map(|t| self.build_init_trak(t, creation_time))
            .collect();
        let trak_boxes = trak_boxes?;

        let trex_boxes: Vec<_> = self
            .tracks
            .iter()
            .map(|t| TrexBox {
                track_id: t.track_id,
                default_sample_description_index: 1,
                default_sample_duration: 0,
                default_sample_size: 0,
                default_sample_flags: SampleFlags::new(0),
            })
            .collect();

        let mvex_box = MvexBox {
            mehd_box: Some(MehdBox {
                fragment_duration: 0,
            }),
            trex_boxes,
            unknown_boxes: Vec::new(),
        };

        let mvhd_box = MvhdBox {
            creation_time,
            modification_time: creation_time,
            timescale: NonZeroU32::new(1000).expect("1000 is non-zero"),
            duration: 0,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: u32::try_from(self.tracks.len() + 1)
                .expect("track count exceeds u32::MAX"),
        };

        Ok(MoovBox {
            mvhd_box,
            trak_boxes,
            mvex_box: Some(mvex_box),
            pssh_boxes: self.options.pssh_boxes.clone(),
            unknown_boxes: Vec::new(),
        })
    }

    fn build_init_trak(
        &self,
        entry: &TrackEntry,
        creation_time: Mp4FileTime,
    ) -> Result<TrakBox, MuxError> {
        let sample_entry = entry
            .sample_entries
            .first()
            .ok_or(MuxError::MissingSampleEntry {
                track_kind: entry.track_kind,
            })?;
        // トラック種別依存の tkhd 属性・ハンドラー種別・メディアヘッダーを 1 箇所で決める
        let derived = derive_trak_attributes(entry.track_kind, sample_entry)?;
        let metadata = self.options.track_metadata(entry.track_kind);

        let tkhd_box = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time,
            modification_time: creation_time,
            track_id: entry.track_id,
            duration: 0,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: derived.volume,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: derived.width,
            height: derived.height,
        };

        let hdlr_box = HdlrBox {
            handler_type: derived.handler_type,
            name: metadata.name.clone().into_null_terminated_bytes(),
        };

        let media_header = Some(derived.media_header);

        // fMP4 の初期化セグメントでは stbl は stsd のみ持てばよく、
        // 他のサンプルテーブルは空にする
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: entry.sample_entries.clone(),
            },
            stts_box: SttsBox {
                entries: Vec::new(),
            },
            ctts_box: None,
            cslg_box: None,
            stsc_box: StscBox { entries: vec![] },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![],
            }),
            stss_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let mdhd_box = MdhdBox {
            creation_time,
            modification_time: creation_time,
            timescale: entry.timescale,
            duration: 0,
            language: metadata.language,
        };

        let minf_box = MinfBox {
            media_header,
            dinf_box: DinfBox::LOCAL_FILE,
            stbl_box,
            unknown_boxes: Vec::new(),
        };

        let mdia_box = MdiaBox {
            mdhd_box,
            hdlr_box,
            minf_box,
            unknown_boxes: Vec::new(),
        };

        // fMP4 の初期化セグメントの movie timescale は `build_init_moov()` が常に 1000 を使う
        let movie_timescale = NonZeroU32::new(1000).expect("1000 is non-zero");
        let edts_box = match &metadata.edit_list {
            Some(edit_list) => {
                if edit_list.media_timescale != entry.timescale {
                    return Err(MuxError::TimescaleMismatch {
                        track_kind: entry.track_kind,
                        expected: edit_list.media_timescale,
                        actual: entry.timescale,
                    });
                }
                Some(EdtsBox {
                    elst_box: Some(edit_list.convert(movie_timescale)?),
                    unknown_boxes: Vec::new(),
                })
            }
            None => None,
        };

        Ok(TrakBox {
            tkhd_box,
            edts_box,
            mdia_box,
            unknown_boxes: Vec::new(),
        })
    }

    fn build_moof(
        &self,
        tracks: &[TrackEntry],
        resolved_tracks: &[ResolvedSegmentTrack],
        sequence_number: u32,
        data_offsets: &[i32],
        saio_offsets: &[u32],
    ) -> Result<MoofBox, MuxError> {
        let mfhd_box = MfhdBox { sequence_number };

        let mut traf_boxes = Vec::new();
        for (traf_index, resolved_track) in resolved_tracks.iter().enumerate() {
            let track = &tracks[resolved_track.track_index];
            let has_any_cto = resolved_track
                .samples
                .iter()
                .any(|sample| sample.composition_time_offset.is_some());

            let trun_samples: Vec<TrunSample> = resolved_track
                .samples
                .iter()
                .map(|sample| -> Result<TrunSample, MuxError> {
                    Ok(TrunSample {
                        duration: Some(sample.duration),
                        size: Some(u32::try_from(sample.data_size).map_err(|_| {
                            MuxError::EncodeError(Error::invalid_data(
                                "sample data size exceeds u32::MAX",
                            ))
                        })?),
                        flags: Some(build_sample_flags(sample.keyframe)),
                        composition_time_offset: if has_any_cto {
                            Some(sample.composition_time_offset.unwrap_or(0))
                        } else {
                            None
                        },
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let trun_box = TrunBox {
                data_offset: Some(data_offsets[resolved_track.track_index]),
                first_sample_flags: None,
                samples: TrunSamples::PerSample(trun_samples),
            };

            let tfhd_box = TfhdBox {
                track_id: track.track_id,
                base_data_offset: None,
                sample_description_index: resolved_track.sample_description_index,
                default_sample_duration: None,
                default_sample_size: None,
                default_sample_flags: None,
                duration_is_empty: false,
                default_base_is_moof: true,
            };

            let tfdt_box = TfdtBox {
                version: if track.decode_time > u32::MAX as u64 {
                    1
                } else {
                    0
                },
                base_media_decode_time: track.decode_time,
            };

            // 暗号化されたトラックには `senc` / `saiz` / `saio` を書き出す。
            // `senc` だけでは `saiz` / `saio` を参照する実装が SAI を見つけられないため、
            // 3 つを常に揃えて書く。
            let mut unknown_boxes = Vec::new();
            if let Some(encryption_samples) = &resolved_track.encryption_samples {
                // `senc` は KID を持たないため、`SampleEncryptionInfo` から
                // `SencSample` に変換して書き出す
                let senc_box = SencBox {
                    samples: encryption_samples.iter().map(SencSample::from).collect(),
                };
                let subsample_encryption = senc_box
                    .samples
                    .iter()
                    .any(|sample| !sample.subsamples.is_empty());

                // `saiz` は `default_sample_info_size` を使わず、サンプルごとの SAI 長を並べる。
                // 長さが u8 に収まらない場合は `saiz` で表現できないためエラーにする
                let mut sample_info_sizes = Vec::new();
                for sample in &senc_box.samples {
                    let size = sample
                        .aux_info_size(subsample_encryption)
                        .ok_or(MuxError::Overflow)?;
                    sample_info_sizes.push(u8::try_from(size).map_err(|_| {
                        MuxError::EncodeError(Error::invalid_input(
                            "sample auxiliary information size exceeds u8::MAX",
                        ))
                    })?);
                }
                // SAI を 1 バイトも持たない場合、デコード側は入力サイズからサンプル数を
                // 制約できないため上限を設けている。デコードできない `senc` を書き出さないよう、
                // ここでも同じ上限を検査する
                if sample_info_sizes.iter().all(|size| *size == 0)
                    && senc_box.samples.len() > crate::boxes_cenc::MAX_SAMPLES_WITHOUT_AUX_INFO
                {
                    return Err(MuxError::EncodeError(Error::invalid_input(
                        "sample count exceeds maximum allowed when samples have no auxiliary information",
                    )));
                }
                let sample_count = u32::try_from(senc_box.samples.len()).map_err(|_| {
                    MuxError::EncodeError(Error::invalid_input(
                        "sample auxiliary information sample count exceeds u32::MAX",
                    ))
                })?;
                let saiz_box = SaizBox {
                    // 省略時はトラックの保護スキームの 4CC が既定値になるため、mux では省略する
                    aux_info_type: None,
                    aux_info_type_parameter: 0,
                    default_sample_info_size: 0,
                    sample_count,
                    sample_info_sizes,
                };
                let saio_box = SaioBox {
                    version: 0,
                    aux_info_type: None,
                    aux_info_type_parameter: 0,
                    offsets: vec![u64::from(saio_offsets[traf_index])],
                };
                unknown_boxes.push(encode_box_as_unknown_box(senc_box.encode_to_vec()?)?);
                unknown_boxes.push(encode_box_as_unknown_box(saiz_box.encode_to_vec()?)?);
                unknown_boxes.push(encode_box_as_unknown_box(saio_box.encode_to_vec()?)?);
            }

            // KID が切り替わるトラックには `sgpd` / `sbgp` を書き出す。
            // すべてのサンプルが `tenc.default_kid` と同じ場合は書き出さない
            if let Some(seig_sample_groups) = &resolved_track.seig_sample_groups {
                unknown_boxes.push(encode_box_as_unknown_box(
                    seig_sample_groups.sgpd_box.encode_to_vec()?,
                )?);
                unknown_boxes.push(encode_box_as_unknown_box(
                    seig_sample_groups.sbgp_box.encode_to_vec()?,
                )?);
            }

            traf_boxes.push(TrafBox {
                tfhd_box,
                tfdt_box: Some(tfdt_box),
                trun_boxes: vec![trun_box],
                unknown_boxes,
            });
        }

        Ok(MoofBox {
            mfhd_box,
            traf_boxes,
            unknown_boxes: Vec::new(),
        })
    }
}

/// エンコード済みのボックスを [`UnknownBox`] に変換する
///
/// `TrafBox` は `senc` / `saiz` / `saio` の型付きフィールドを持たず、`unknown_boxes` に
/// 未解釈のボックスとして保持するため、エンコードしたバイト列から [`UnknownBox`] を作る。
fn encode_box_as_unknown_box(box_bytes: Vec<u8>) -> Result<UnknownBox, MuxError> {
    let (unknown_box, size) = UnknownBox::decode(&box_bytes)?;
    if size != box_bytes.len() {
        return Err(MuxError::EncodeError(Error::invalid_input(
            "encoded box bytes contain more than one box",
        )));
    }
    Ok(unknown_box)
}

/// moof 内の各 traf について、`senc` 内の最初のサンプル SAI の先頭位置を返す
///
/// `senc` は `sample_count` フィールドの直後からサンプルごとの SAI が並ぶ
/// (ISO/IEC 23001-7:2023 7.2.1)。そのため最初の SAI の先頭は、
/// `senc` のボックスヘッダー (8 バイト) + FullBox ヘッダー (4 バイト) + `sample_count` (4 バイト)
/// の直後になる。
///
/// 返り値は `resolved_tracks` と同じ並び (moof の `traf` の並び) であり、
/// 暗号化されていないトラックは `None` になる。返り値の値は moof 先頭からのバイト数である。
fn compute_senc_sai_offsets(moof: &MoofBox) -> Result<Vec<Option<u32>>, MuxError> {
    // moof 自身のヘッダーは 8 バイト (U32) で、続けて mfhd が置かれる。
    // moof のサイズが U32 に収まらない場合はエンコード時にエラーになるため、
    // ここでは U32 ヘッダーだけを前提にする
    let mut traf_start = BoxHeader::MIN_SIZE + moof.mfhd_box.encode_to_vec()?.len();
    let mut offsets = Vec::new();
    for traf_box in &moof.traf_boxes {
        let mut child_start = traf_start + BoxHeader::MIN_SIZE;
        child_start += traf_box.tfhd_box.encode_to_vec()?.len();
        if let Some(tfdt_box) = &traf_box.tfdt_box {
            child_start += tfdt_box.encode_to_vec()?.len();
        }
        for trun_box in &traf_box.trun_boxes {
            child_start += trun_box.encode_to_vec()?.len();
        }

        let mut senc_sai_offset = None;
        for unknown_box in &traf_box.unknown_boxes {
            if unknown_box.box_type == SencBox::TYPE && senc_sai_offset.is_none() {
                let offset = child_start.checked_add(16).ok_or(MuxError::Overflow)?;
                senc_sai_offset = Some(u32::try_from(offset).map_err(|_| {
                    MuxError::EncodeError(Error::invalid_input(
                        "senc SAI offset exceeds u32::MAX; version 0 saio cannot represent it",
                    ))
                })?);
            }
            child_start += unknown_box.encode_to_vec()?.len();
        }
        offsets.push(senc_sai_offset);
        traf_start = child_start;
    }
    Ok(offsets)
}

/// 参照トラックの各サンプルについて PTS を求め、その最小値と当該サンプルの `keyframe` を返す
///
/// `DTS_i = decode_time + Σ_{k < i} duration_k`（当該トラックのサンプルのみ）、
/// `PTS_i = DTS_i + composition_time_offset.unwrap_or(0)`。
/// 戻り値は `(min_pts, sap_at_ept)` で、第 2 要素は `min_pts` を採ったサンプルの `keyframe`。
/// PTS が同値のときは samples[] 内で先に出現したサンプルの `keyframe` を保持する
/// （PTS が厳密に減少するときだけ第 2 要素を更新する）。
/// PTS が負、あるいは PTS または累積 DTS が `u64` の表現範囲を外れた場合は
/// [`MuxError::Overflow`] を返す。
///
/// 呼び出し側は `samples` に `track_kind` を持つサンプルを最低 1 件含めること
/// （逸脱した場合は末尾の `expect` で panic する）。
fn compute_earliest_presentation_time(
    samples: &[Sample],
    track_kind: TrackKind,
    decode_time: u64,
) -> Result<(u64, bool), MuxError> {
    let mut dts = decode_time;
    // (min_pts, EPT サンプルの keyframe) を追跡する
    let mut min_pts: Option<(i128, bool)> = None;

    for sample in samples
        .iter()
        .filter(|sample| sample.track_kind == track_kind)
    {
        let cto = i128::from(sample.composition_time_offset.unwrap_or(0));
        // dts (u64) + cto (i64) は i128 の表現範囲を超え得ないため、単純加算でよい
        let pts = i128::from(dts) + cto;
        match min_pts {
            Some((current, _)) if pts >= current => {}
            _ => {
                min_pts = Some((pts, sample.keyframe));
            }
        }
        dts = dts
            .checked_add(u64::from(sample.duration))
            .ok_or(MuxError::Overflow)?;
    }

    // 呼び出し側は samples[0].track_kind を渡すため、当該 kind のサンプルは少なくとも 1 件ある。
    let (min_pts, sap_at_ept) =
        min_pts.expect("bug: reference track must have at least one sample");
    let min_pts = u64::try_from(min_pts).map_err(|_| MuxError::Overflow)?;
    Ok((min_pts, sap_at_ept))
}

/// `seig` サンプルグループで KID を切り替える `sgpd` / `sbgp` を組み立てる
///
/// KID ごとに `sgpd` の `seig` エントリーを 1 つ作り、連続する同一 KID ごとに
/// `sbgp` のエントリーを作る。`sbgp` の `group_description_index` は
/// `0x10000 + (1 始まりのエントリー番号)` であり、1 番目のエントリーが `0x10001` になる。
/// 既定 KID を使うサンプルは index `0` にする。
///
/// すべてのサンプルの実効 KID が `tenc.default_kid` と同じ場合は [`None`] を返し、
/// `sgpd` / `sbgp` を書き出さない。なお、ここでの扱いは
/// ISO/IEC 23001-7:2023 6 と ISO/IEC 14496-12:2022 8.9.4 に基づくものであり、
/// 将来の改訂で変わる可能性がある。
fn build_seig_sample_groups(
    tenc_box: &TencBox,
    samples: &[SampleEncryptionInfo],
) -> Result<Option<SeigSampleGroups>, MuxError> {
    // KID ごとのエントリー番号 (1 始まり) を、最初に現れた順に割り当てる
    let mut kids: Vec<[u8; 16]> = Vec::new();
    let mut group_description_indices = Vec::new();
    for sample in samples {
        let group_description_index = match sample.kid {
            None => 0,
            Some(kid) => {
                let entry_index = match kids.iter().position(|known| *known == kid) {
                    Some(index) => index,
                    None => {
                        kids.push(kid);
                        kids.len() - 1
                    }
                };
                // fragment-local の `sgpd` を参照する index は
                // 0x10000 + (1 始まりのエントリー番号) になる
                let entry_number = u32::try_from(entry_index + 1).map_err(|_| {
                    MuxError::EncodeError(Error::invalid_input("seig entry count exceeds u32::MAX"))
                })?;
                0x1_0000u32.checked_add(entry_number).ok_or_else(|| {
                    MuxError::EncodeError(Error::invalid_input(
                        "seig group_description_index overflow",
                    ))
                })?
            }
        };
        group_description_indices.push(group_description_index);
    }
    if kids.is_empty() {
        return Ok(None);
    }

    // 連続する同一 index ごとに 1 エントリーへまとめる
    let mut sbgp_entries: Vec<SbgpEntry> = Vec::new();
    for group_description_index in group_description_indices {
        match sbgp_entries.last_mut() {
            Some(last) if last.group_description_index == group_description_index => {
                last.sample_count = last.sample_count.checked_add(1).ok_or_else(|| {
                    MuxError::EncodeError(Error::invalid_input(
                        "sbgp sample_count exceeds u32::MAX",
                    ))
                })?;
            }
            _ => sbgp_entries.push(SbgpEntry {
                sample_count: 1,
                group_description_index,
            }),
        }
    }

    let sgpd_box = SgpdBox {
        version: 1,
        grouping_type: SEIG_GROUPING_TYPE,
        default_length: Some(
            u32::try_from(SeigEntry::FIXED_SIZE).expect("SeigEntry::FIXED_SIZE fits in u32"),
        ),
        entries: kids
            .into_iter()
            .map(|kid| {
                SgpdEntry::Seig(SeigEntry {
                    is_protected: 1,
                    per_sample_iv_size: tenc_box.default_per_sample_iv_size,
                    kid,
                    constant_iv: None,
                })
            })
            .collect(),
    };
    let sbgp_box = SbgpBox {
        version: 0,
        grouping_type: SEIG_GROUPING_TYPE,
        grouping_type_parameter: None,
        entries: sbgp_entries,
    };
    Ok(Some(SeigSampleGroups { sgpd_box, sbgp_box }))
}

fn resolve_segment_tracks(
    tracks: &mut Vec<TrackEntry>,
    samples: &[Sample],
) -> Result<Vec<ResolvedSegmentTrack>, MuxError> {
    let mut ordered_kinds = Vec::new();
    for sample in samples {
        if !ordered_kinds.contains(&sample.track_kind) {
            ordered_kinds.push(sample.track_kind);
        }
    }

    let mut resolved_tracks = Vec::new();
    for track_kind in ordered_kinds {
        let track_samples: Vec<&Sample> = samples
            .iter()
            .filter(|sample| sample.track_kind == track_kind)
            .collect();
        let first_sample = track_samples
            .first()
            .expect("bug: ordered track kind must have at least one sample");
        let track_index = ensure_track_entry(tracks, track_kind, first_sample.timescale)?;
        let track = &mut tracks[track_index];

        let mut current_sample_entry_index = track.current_sample_entry_index;
        let mut segment_sample_entry_index = None;
        // このセグメントで使うサンプルエントリーは 1 つに固定されるため、
        // CENC のトラック暗号化情報も最初のサンプルで確定できる
        let mut track_encryption: Option<TrackEncryption> = None;
        let mut resolved_samples = Vec::new();
        let mut total_duration = 0u64;
        let mut expected_next_data_offset: Option<u64> = None;
        let mut first_data_offset = None;
        let mut payload_end = 0u64;

        for sample in track_samples {
            if sample.timescale != track.timescale {
                return Err(MuxError::TimescaleMismatch {
                    track_kind,
                    expected: track.timescale,
                    actual: sample.timescale,
                });
            }

            let sample_entry_index = if let Some(sample_entry) = &sample.sample_entry {
                match track
                    .sample_entries
                    .iter()
                    .position(|known_entry| known_entry == sample_entry)
                {
                    Some(index) => index,
                    None => {
                        track.sample_entries.push(sample_entry.clone());
                        track.sample_entries.len() - 1
                    }
                }
            } else {
                current_sample_entry_index.ok_or(MuxError::MissingSampleEntry { track_kind })?
            };

            if let Some(expected_index) = segment_sample_entry_index {
                if expected_index != sample_entry_index {
                    return Err(MuxError::MixedSampleEntries { track_kind });
                }
            } else {
                segment_sample_entry_index = Some(sample_entry_index);
                let segment_sample_entry = track
                    .sample_entries
                    .get(sample_entry_index)
                    .ok_or(MuxError::MissingSampleEntry { track_kind })?;
                track_encryption = TrackEncryption::from_sample_entry(segment_sample_entry);
            }

            // 実効 KID が `tenc` の既定 KID と同じ場合は `None` に正規化する。
            // `Some(tenc.default_kid)` は「既定 KID を使う」と同じ意味であり、
            // `sgpd` / `sbgp` を書き出す必要はない
            let mut encryption_info = sample.encryption_info.clone();
            match &track_encryption {
                Some(track_encryption) => {
                    if let Some(info) = &mut encryption_info
                        && info.kid == Some(track_encryption.tenc_box.default_kid)
                    {
                        info.kid = None;
                    }
                    // 選択的暗号化は対象外なので、`tenc` の既定値とサンプルの暗号化情報の
                    // 有無が一致していること、IV 長と KID の切り替えが対象の構成かを検査する
                    track_encryption.validate_sample_encryption(encryption_info.as_ref())?;
                }
                None => {
                    if encryption_info.is_some() {
                        return Err(MuxError::EncodeError(Error::invalid_input(
                            "sample encryption info requires a CENC protected sample entry",
                        )));
                    }
                }
            }

            current_sample_entry_index = Some(sample_entry_index);
            total_duration = total_duration
                .checked_add(sample.duration as u64)
                .ok_or(MuxError::Overflow)?;
            if first_data_offset.is_none() {
                first_data_offset = Some(sample.data_offset);
            }
            if let Some(expected_offset) = expected_next_data_offset
                && expected_offset != sample.data_offset
            {
                return Err(MuxError::EncodeError(Error::invalid_input(
                    "sample data for the same track must be contiguous in the segment payload",
                )));
            }
            expected_next_data_offset = Some(
                sample
                    .data_offset
                    .checked_add(sample.data_size as u64)
                    .ok_or(MuxError::Overflow)?,
            );
            payload_end = expected_next_data_offset.expect("offset must be set");

            resolved_samples.push(ResolvedSegmentSample {
                duration: sample.duration,
                keyframe: sample.keyframe,
                composition_time_offset: sample.composition_time_offset,
                data_size: sample.data_size,
                encryption_info,
            });
        }

        // `default_is_protected = 1` のトラックは、`validate_sample_encryption()` により
        // 全サンプルが暗号化情報を持つことが保証されている
        let encryption_samples = match &track_encryption {
            Some(track_encryption) if track_encryption.tenc_box.default_is_protected == 1 => {
                let mut encryption_samples = Vec::new();
                for resolved_sample in &resolved_samples {
                    let encryption_info =
                        resolved_sample.encryption_info.as_ref().ok_or_else(|| {
                            MuxError::EncodeError(Error::invalid_input(
                                "encrypted track requires sample encryption info for every sample",
                            ))
                        })?;
                    encryption_samples.push(encryption_info.clone());
                }
                Some(encryption_samples)
            }
            _ => None,
        };

        // KID が切り替わるサンプルがあれば、`sgpd` / `sbgp` に書き出す内容を組み立てる
        let seig_sample_groups = match (&track_encryption, &encryption_samples) {
            (Some(track_encryption), Some(encryption_samples))
                if track_encryption.tenc_box.default_is_protected == 1 =>
            {
                build_seig_sample_groups(&track_encryption.tenc_box, encryption_samples)?
            }
            _ => None,
        };

        track.current_sample_entry_index = current_sample_entry_index;
        let sample_description_index = match segment_sample_entry_index {
            // ISO 14496-12 では tfhd.sample_description_index を省略した場合、
            // trex.default_sample_description_index が適用される。
            // build_init_moov() では各トラックの default_sample_description_index に 1 を
            // 設定しているため、0-based index=0 のときは tfhd 側を省略してよい。
            Some(0) => None,
            Some(index) => Some(u32::try_from(index + 1).map_err(|_| {
                MuxError::EncodeError(Error::invalid_data(
                    "sample_description_index exceeds u32::MAX",
                ))
            })?),
            None => None,
        };
        resolved_tracks.push(ResolvedSegmentTrack {
            track_index,
            samples: resolved_samples,
            total_duration,
            sample_description_index,
            first_data_offset: first_data_offset.expect("track must contain at least one sample"),
            payload_end,
            encryption_samples,
            seig_sample_groups,
        });
    }

    resolved_tracks.sort_by_key(|track| track.first_data_offset);
    let mut expected_track_offset = 0u64;
    for track in &resolved_tracks {
        if track.first_data_offset != expected_track_offset {
            return Err(MuxError::EncodeError(Error::invalid_input(
                "track payload ranges must be contiguous and ordered by data_offset",
            )));
        }
        expected_track_offset = track.payload_end;
    }

    Ok(resolved_tracks)
}

fn ensure_track_entry(
    tracks: &mut Vec<TrackEntry>,
    track_kind: TrackKind,
    timescale: NonZeroU32,
) -> Result<usize, MuxError> {
    if let Some(track_index) = tracks
        .iter()
        .position(|track| track.track_kind == track_kind)
    {
        let track = &tracks[track_index];
        if track.timescale != timescale {
            return Err(MuxError::TimescaleMismatch {
                track_kind,
                expected: track.timescale,
                actual: timescale,
            });
        }
        return Ok(track_index);
    }

    let track_id = u32::try_from(tracks.len() + 1).expect("track count exceeds u32::MAX");
    tracks.push(TrackEntry {
        track_kind,
        timescale,
        sample_entries: Vec::new(),
        track_id,
        decode_time: 0,
        current_sample_entry_index: None,
    });
    Ok(tracks.len() - 1)
}

/// tkhd の `width` / `height` を表す固定小数点数のペア型エイリアス
type TkhdDimensions = (FixedPointNumber<i16, u16>, FixedPointNumber<i16, u16>);

/// `track_kind` から派生する `trak` の属性群
///
/// tkhd の volume / width / height、ハンドラー種別、メディアヘッダーはすべて
/// トラック種別ごとに決まる。使用箇所ごとに個別に match するのを避け、
/// 決定表として 1 つの構造体に集約する。
/// [`Fmp4SegmentMuxer`] と [`crate::mux::Mp4FileMuxer`] の両方から利用する
pub(crate) struct TrakDerivation {
    pub(crate) volume: FixedPointNumber<i8, u8>,
    pub(crate) width: FixedPointNumber<i16, u16>,
    pub(crate) height: FixedPointNumber<i16, u16>,
    pub(crate) handler_type: [u8; 4],
    pub(crate) media_header: MediaHeader,
}

/// `track_kind` と `sample_entry` から tkhd / hdlr / media_header 用の属性を導出する
///
/// [`TrackKind::Subtitle`] 側の (handler_type, media_header) 対応表は
/// [`subtitle_trak_attributes`] を参照
pub(crate) fn derive_trak_attributes(
    track_kind: TrackKind,
    sample_entry: &SampleEntry,
) -> Result<TrakDerivation, MuxError> {
    match track_kind {
        TrackKind::Video => {
            let (width, height): TkhdDimensions = extract_video_dimensions(sample_entry)?;
            Ok(TrakDerivation {
                volume: TkhdBox::DEFAULT_VIDEO_VOLUME,
                width,
                height,
                handler_type: HdlrBox::HANDLER_TYPE_VIDE,
                media_header: MediaHeader::Vmhd(VmhdBox::default()),
            })
        }
        TrackKind::Audio => Ok(TrakDerivation {
            volume: TkhdBox::DEFAULT_AUDIO_VOLUME,
            width: FixedPointNumber::default(),
            height: FixedPointNumber::default(),
            handler_type: HdlrBox::HANDLER_TYPE_SOUN,
            media_header: MediaHeader::Smhd(SmhdBox::default()),
        }),
        // 字幕トラックの tkhd volume は 0 が慣習（DEFAULT_VIDEO_VOLUME と同じ値）。
        // width / height は 0（表示領域を指定する必要が生じたら方式固有の実装で拡張する）
        TrackKind::Subtitle => {
            let (handler_type, media_header) = subtitle_trak_attributes(sample_entry);
            Ok(TrakDerivation {
                volume: TkhdBox::DEFAULT_VIDEO_VOLUME,
                width: FixedPointNumber::default(),
                height: FixedPointNumber::default(),
                handler_type,
                media_header,
            })
        }
    }
}

/// 字幕サンプルエントリーからハンドラー種別とメディアヘッダーを決める
///
/// 対応表:
///   stpp → subt + sthd (ISO/IEC 14496-30)
///   wvtt → text + sthd (ISO/IEC 14496-30)
///   tx3g → text + nmhd (3GPP TS 26.245)
///
/// `hdlr` と `minf.media_header` はトラック単位で 1 つしか持てないため、
/// 1 つのトラック内でこの組が異なるサンプルエントリーが混在すると、
/// `stsd` には両方が並ぶ一方でトラック側の属性は片方に固定され、規格上整合しなくなる。
/// 呼び出し側はこの戻り値同士を突き合わせて混在を検出する
pub(crate) fn subtitle_trak_attributes(sample_entry: &SampleEntry) -> ([u8; 4], MediaHeader) {
    match sample_entry {
        SampleEntry::Stpp(_) => (HdlrBox::HANDLER_TYPE_SUBT, MediaHeader::Sthd(SthdBox)),
        SampleEntry::Wvtt(_) => (HdlrBox::HANDLER_TYPE_TEXT, MediaHeader::Sthd(SthdBox)),
        SampleEntry::Tx3g(_) => (HdlrBox::HANDLER_TYPE_TEXT, MediaHeader::Nmhd(NmhdBox)),
        // 対応表に載っていないバリアントは防御的に subt + sthd に丸める。
        // 字幕トラックに映像系・音声系のサンプルエントリーが紐付く運用は無く、
        // 実際には未知の字幕系サンプルエントリー（`SampleEntry::decode` が
        // 型付きに落とせずに `SampleEntry::Unknown` に落としたもの）だけがこの arm に到達する
        _ => (HdlrBox::HANDLER_TYPE_SUBT, MediaHeader::Sthd(SthdBox)),
    }
}

/// 映像系サンプルエントリーから幅・高さを取り出して tkhd 用の [`FixedPointNumber`] に変換する
///
/// 非映像系 SampleEntry（`SampleEntry::Unknown` 等）が渡された場合は `(0, 0)` を返す
/// （Video トラックに変則的に非映像系エントリーが渡ったケースへの防御）
fn extract_video_dimensions(sample_entry: &SampleEntry) -> Result<TkhdDimensions, MuxError> {
    let visual = match sample_entry {
        SampleEntry::Avc1(b) => Some(&b.visual),
        SampleEntry::Hev1(b) => Some(&b.visual),
        SampleEntry::Hvc1(b) => Some(&b.visual),
        SampleEntry::Vp08(b) => Some(&b.visual),
        SampleEntry::Vp09(b) => Some(&b.visual),
        SampleEntry::Av01(b) => Some(&b.visual),
        SampleEntry::Encv(b) => Some(&b.visual),
        _ => None,
    };
    match visual {
        Some(v) => {
            let w = i16::try_from(v.width).map_err(|_| {
                MuxError::EncodeError(crate::Error::invalid_data("video width exceeds i16::MAX"))
            })?;
            let h = i16::try_from(v.height).map_err(|_| {
                MuxError::EncodeError(crate::Error::invalid_data("video height exceeds i16::MAX"))
            })?;
            Ok((FixedPointNumber::new(w, 0), FixedPointNumber::new(h, 0)))
        }
        None => Ok((FixedPointNumber::default(), FixedPointNumber::default())),
    }
}

/// SampleFlags を生成する
///
/// キーフレーム（同期サンプル）かどうかに応じて適切なフラグを設定する。
fn build_sample_flags(keyframe: bool) -> SampleFlags {
    if keyframe {
        // sample_depends_on=2 (独立している), sample_is_non_sync_sample=false
        SampleFlags::from_fields(0, 2, 0, 0, 0, false, 0)
    } else {
        // sample_depends_on=1 (他に依存している), sample_is_non_sync_sample=true
        SampleFlags::from_fields(0, 1, 0, 0, 0, true, 0)
    }
}
