//! 完全な fMP4 ファイルのデマルチプレックス機能を提供するモジュール
//!
//! このモジュールは、`moov` + `moof`/`mdat` で構成される完全な fMP4 ファイルを
//! 入力として受け取り、サンプルを順番に取り出すための機能を提供する。
//!
//! ストリーミング向けの [`crate::demux_fmp4::Fmp4Demuxer`] とは異なり、
//! ファイル全体のバイト列を一度に受け取って処理する。
//!
//! # 制限事項
//!
//! `tfhd` の `base_data_offset` フィールドにファイル先頭からの絶対オフセットが
//! 記録されている形式には対応していない。
//! この形式が検出された場合は [`Fmp4DemuxError`] を返す。
//!
//! # Examples
//!
//! ```no_run
//! use shiguredo_mp4::demux_file_fmp4::Fmp4FileDemuxer;
//!
//! # fn main() -> Result<(), Box<dyn 'static + std::error::Error>> {
//! let file_data: Vec<u8> = todo!("fMP4 ファイルのバイト列");
//! let mut demuxer = Fmp4FileDemuxer::new(file_data)?;
//!
//! let tracks = demuxer.tracks()?;
//! println!("{}個のトラックが見つかりました", tracks.len());
//!
//! while let Some(sample) = demuxer.next_sample()? {
//!     println!(
//!         "track_id={}, decode_time={}, size={}",
//!         sample.track_id,
//!         sample.base_media_decode_time,
//!         sample.data.len(),
//!     );
//! }
//! # Ok(())
//! # }
//! ```
use alloc::{collections::VecDeque, vec::Vec};
use core::ops::Range;

use crate::{
    BoxHeader, Decode,
    boxes::{MdatBox, MoofBox, MoovBox},
    demux_fmp4::{Fmp4DemuxError, Fmp4DemuxSample, Fmp4Demuxer, Fmp4TrackInfo},
};

/// [`Fmp4FileDemuxer::next_sample()`] が返すサンプル
///
/// [`crate::demux_fmp4::Fmp4DemuxSample`] とは異なり、サンプルデータ本体 (`data`) を
/// 内包しているため、ファイルバイト列への参照を別途管理する必要がない。
#[derive(Debug, Clone)]
pub struct Fmp4FileSample {
    /// サンプルが属するトラックの ID
    pub track_id: u32,

    /// ベースデコード時間（タイムスケール単位）
    pub base_media_decode_time: u64,

    /// サンプルの尺（タイムスケール単位）
    pub duration: u32,

    /// キーフレームかどうか
    pub keyframe: bool,

    /// コンポジション時間オフセット（B フレーム向け）
    ///
    /// B フレームを含まない場合は `None`。
    pub composition_time_offset: Option<i32>,

    /// サンプルのバイト列
    pub data: Vec<u8>,
}

/// 完全な fMP4 ファイルをデマルチプレックス処理するための構造体
///
/// ファイル全体のバイト列を保持し、`moov` を解析してトラック情報を初期化した後、
/// `moof`/`mdat` を順番に処理してサンプルを返す。
///
/// # 注意
///
/// ファイル全体を `Vec<u8>` として内部に保持するため、大きなファイルでは
/// メモリ消費量が増える点に注意。
#[derive(Debug, Clone)]
pub struct Fmp4FileDemuxer {
    /// ファイル全体のバイト列
    data: Vec<u8>,

    /// 内部で利用するストリーミング用デマルチプレクサ
    inner: Fmp4Demuxer,

    /// ファイル内の各 moof+mdat セグメントの範囲（data 先頭からのバイトオフセット）
    segment_ranges: Vec<Range<usize>>,

    /// 次に処理するセグメントのインデックス
    next_segment_idx: usize,

    /// 現在処理中のセグメントから未返却のサンプル
    ///
    /// タプルの第 2 要素は `self.data` 先頭からのセグメント開始オフセット。
    /// `Fmp4DemuxSample::data_offset` はセグメントスライス先頭からの相対値なので、
    /// 絶対ファイルオフセットへの変換に使用する。
    pending_samples: VecDeque<(Fmp4DemuxSample, usize)>,
}

impl Fmp4FileDemuxer {
    /// fMP4 ファイルのバイト列からインスタンスを生成する
    ///
    /// `data` は `ftyp` + `moov` で始まり、その後に `moof`/`mdat` のペアが続く
    /// 完全な fMP4 ファイルのバイト列でなければならない。
    pub fn new(data: impl Into<Vec<u8>>) -> Result<Self, Fmp4DemuxError> {
        let data = data.into();

        let (init_end, segment_ranges) = scan_boxes(&data)?;

        let mut inner = Fmp4Demuxer::new();
        inner.handle_init_segment(&data[..init_end])?;

        Ok(Self {
            data,
            inner,
            segment_ranges,
            next_segment_idx: 0,
            pending_samples: VecDeque::new(),
        })
    }

    /// トラック情報を返す
    pub fn tracks(&self) -> Result<Vec<Fmp4TrackInfo>, Fmp4DemuxError> {
        self.inner.tracks()
    }

    /// 次のサンプルを返す
    ///
    /// 全サンプルを返し終えた場合は `Ok(None)` を返す。
    pub fn next_sample(&mut self) -> Result<Option<Fmp4FileSample>, Fmp4DemuxError> {
        loop {
            if let Some((raw, segment_start)) = self.pending_samples.pop_front() {
                let abs_start = segment_start + raw.data_offset;
                let abs_end = abs_start + raw.data_size;
                if abs_end > self.data.len() {
                    return Err(Fmp4DemuxError::DecodeError(crate::Error::invalid_data(
                        "sample data offset out of file bounds",
                    )));
                }
                return Ok(Some(Fmp4FileSample {
                    track_id: raw.track_id,
                    base_media_decode_time: raw.base_media_decode_time,
                    duration: raw.duration,
                    keyframe: raw.keyframe,
                    composition_time_offset: raw.composition_time_offset,
                    data: self.data[abs_start..abs_end].to_vec(),
                }));
            }

            if self.next_segment_idx >= self.segment_ranges.len() {
                return Ok(None);
            }

            let range = self.segment_ranges[self.next_segment_idx].clone();
            self.next_segment_idx += 1;

            let segment_start = range.start;
            let segment_data = &self.data[range];
            let raw_samples = self.inner.handle_media_segment(segment_data)?;
            for raw in raw_samples {
                self.pending_samples.push_back((raw, segment_start));
            }
        }
    }
}

/// ファイル全体を走査して moov の終端位置と moof+mdat セグメント範囲を返す
///
/// - `ftyp` / `free` / `sidx` / `mfra` 等の認識外ボックスは読み飛ばす
/// - `moof` を検出したら直後の `mdat` を確認してセグメント範囲を記録する
/// - `tfhd` に絶対オフセット形式の `base_data_offset` が含まれる場合はエラーを返す
fn scan_boxes(data: &[u8]) -> Result<(usize, Vec<Range<usize>>), Fmp4DemuxError> {
    let mut offset = 0;
    let mut moov_end: Option<usize> = None;
    let mut segment_ranges = Vec::new();

    while offset < data.len() {
        if data.len() - offset < BoxHeader::MIN_SIZE {
            break;
        }

        let (header, _) = BoxHeader::decode(&data[offset..])?;
        let box_size = header.box_size.get() as usize;
        if box_size == 0 {
            break;
        }
        if offset + box_size > data.len() {
            return Err(Fmp4DemuxError::DecodeError(crate::Error::invalid_data(
                "box size exceeds file length",
            )));
        }

        if header.box_type == MoovBox::TYPE {
            moov_end = Some(offset + box_size);
        } else if header.box_type == MoofBox::TYPE {
            // moof を一度デコードして tfhd の base_data_offset を確認する
            let (moof, _) = MoofBox::decode(&data[offset..offset + box_size])?;
            for traf in &moof.traf_boxes {
                if traf.tfhd_box.base_data_offset.is_some() {
                    return Err(Fmp4DemuxError::DecodeError(crate::Error::invalid_data(
                        "tfhd with absolute base_data_offset is not supported in Fmp4FileDemuxer",
                    )));
                }
            }

            let moof_start = offset;
            let mdat_start = offset + box_size;

            if mdat_start >= data.len() || data.len() - mdat_start < BoxHeader::MIN_SIZE {
                return Err(Fmp4DemuxError::DecodeError(crate::Error::invalid_data(
                    "mdat box not found after moof",
                )));
            }

            let (mdat_header, _) = BoxHeader::decode(&data[mdat_start..])?;
            if mdat_header.box_type != MdatBox::TYPE {
                return Err(Fmp4DemuxError::DecodeError(crate::Error::invalid_data(
                    "expected mdat box after moof",
                )));
            }

            let mdat_size = mdat_header.box_size.get() as usize;
            segment_ranges.push(moof_start..mdat_start + mdat_size);
        }

        offset += box_size;
    }

    let init_end = moov_end.ok_or_else(|| {
        Fmp4DemuxError::DecodeError(crate::Error::invalid_data("moov box not found in file"))
    })?;

    Ok((init_end, segment_ranges))
}
