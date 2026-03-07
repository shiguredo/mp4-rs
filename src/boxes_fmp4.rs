//! Fragmented MP4 (fMP4) 関連のボックス定義
//!
//! このモジュールは内部的なもので、構造体などの外部への提供は boxes モジュールを通して行う
use alloc::{boxed::Box, vec::Vec};

use crate::{
    BaseBox, BoxHeader, BoxType, Decode, Encode, Error, FullBox, FullBoxFlags, FullBoxHeader,
    Result, SampleFlags,
    basic_types::as_box_object,
    boxes::{UnknownBox, with_box_type},
};

/// [ISO/IEC 14496-12] MovieFragmentBox class
///
/// ムービーフラグメントのコンテナボックス。
/// fMP4 のメディアセグメントはこのボックスと mdat ボックスで構成される。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct MoofBox {
    pub mfhd_box: MfhdBox,
    pub traf_boxes: Vec<TrafBox>,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl MoofBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"moof");
}

impl Encode for MoofBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.mfhd_box.encode(&mut buf[offset..])?;
        for b in &self.traf_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for MoofBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut mfhd_box = None;
            let mut traf_boxes = Vec::new();
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    MfhdBox::TYPE if mfhd_box.is_none() => {
                        mfhd_box = Some(MfhdBox::decode_at(payload, &mut offset)?);
                    }
                    TrafBox::TYPE => {
                        traf_boxes.push(TrafBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    mfhd_box: crate::boxes::check_mandatory_box(mfhd_box, "mfhd", "moof")?,
                    traf_boxes,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for MoofBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.mfhd_box).map(as_box_object))
                .chain(self.traf_boxes.iter().map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-12] MovieFragmentHeaderBox class (親: [`MoofBox`])
///
/// フラグメントのシーケンス番号を格納する。
/// シーケンス番号は 1 から始まり、フラグメントごとに 1 ずつ増加する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct MfhdBox {
    pub sequence_number: u32,
}

impl MfhdBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"mfhd");
}

impl Encode for MfhdBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += self.sequence_number.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for MfhdBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_header = FullBoxHeader::decode_at(payload, &mut offset)?;

            let sequence_number = u32::decode_at(payload, &mut offset)?;

            Ok((
                Self { sequence_number },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for MfhdBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for MfhdBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] TrackFragmentBox class (親: [`MoofBox`])
///
/// トラックフラグメントのコンテナボックス。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct TrafBox {
    pub tfhd_box: TfhdBox,
    pub tfdt_box: Option<TfdtBox>,
    pub trun_boxes: Vec<TrunBox>,
    pub unknown_boxes: Vec<UnknownBox>,
}

impl TrafBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"traf");
}

impl Encode for TrafBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += self.tfhd_box.encode(&mut buf[offset..])?;
        if let Some(b) = &self.tfdt_box {
            offset += b.encode(&mut buf[offset..])?;
        }
        for b in &self.trun_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        for b in &self.unknown_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for TrafBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut tfhd_box = None;
            let mut tfdt_box = None;
            let mut trun_boxes = Vec::new();
            let mut unknown_boxes = Vec::new();

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    TfhdBox::TYPE if tfhd_box.is_none() => {
                        tfhd_box = Some(TfhdBox::decode_at(payload, &mut offset)?);
                    }
                    TfdtBox::TYPE if tfdt_box.is_none() => {
                        tfdt_box = Some(TfdtBox::decode_at(payload, &mut offset)?);
                    }
                    TrunBox::TYPE => {
                        trun_boxes.push(TrunBox::decode_at(payload, &mut offset)?);
                    }
                    _ => {
                        unknown_boxes.push(UnknownBox::decode_at(payload, &mut offset)?);
                    }
                }
            }

            Ok((
                Self {
                    tfhd_box: crate::boxes::check_mandatory_box(tfhd_box, "tfhd", "traf")?,
                    tfdt_box,
                    trun_boxes,
                    unknown_boxes,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for TrafBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            core::iter::empty()
                .chain(core::iter::once(&self.tfhd_box).map(as_box_object))
                .chain(self.tfdt_box.iter().map(as_box_object))
                .chain(self.trun_boxes.iter().map(as_box_object))
                .chain(self.unknown_boxes.iter().map(as_box_object)),
        )
    }
}

/// [ISO/IEC 14496-12] TrackFragmentHeaderBox class (親: [`TrafBox`])
///
/// トラックフラグメントのヘッダー情報を格納する。
/// フラグによって存在するフィールドが異なる。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct TfhdBox {
    pub track_id: u32,
    pub base_data_offset: Option<u64>,
    pub sample_description_index: Option<u32>,
    pub default_sample_duration: Option<u32>,
    pub default_sample_size: Option<u32>,
    pub default_sample_flags: Option<SampleFlags>,
    pub duration_is_empty: bool,
    pub default_base_is_moof: bool,
}

impl TfhdBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"tfhd");

    /// base_data_offset が存在することを示すフラグ
    pub const FLAG_BASE_DATA_OFFSET_PRESENT: u32 = 0x000001;

    /// sample_description_index が存在することを示すフラグ
    pub const FLAG_SAMPLE_DESCRIPTION_INDEX_PRESENT: u32 = 0x000002;

    /// default_sample_duration が存在することを示すフラグ
    pub const FLAG_DEFAULT_SAMPLE_DURATION_PRESENT: u32 = 0x000008;

    /// default_sample_size が存在することを示すフラグ
    pub const FLAG_DEFAULT_SAMPLE_SIZE_PRESENT: u32 = 0x000010;

    /// default_sample_flags が存在することを示すフラグ
    pub const FLAG_DEFAULT_SAMPLE_FLAGS_PRESENT: u32 = 0x000020;

    /// 継続時間が空であることを示すフラグ
    pub const FLAG_DURATION_IS_EMPTY: u32 = 0x010000;

    /// デフォルトの base_data_offset が moof の先頭であることを示すフラグ
    pub const FLAG_DEFAULT_BASE_IS_MOOF: u32 = 0x020000;
}

impl Encode for TfhdBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += self.track_id.encode(&mut buf[offset..])?;

        if let Some(v) = self.base_data_offset {
            offset += v.encode(&mut buf[offset..])?;
        }
        if let Some(v) = self.sample_description_index {
            offset += v.encode(&mut buf[offset..])?;
        }
        if let Some(v) = self.default_sample_duration {
            offset += v.encode(&mut buf[offset..])?;
        }
        if let Some(v) = self.default_sample_size {
            offset += v.encode(&mut buf[offset..])?;
        }
        if let Some(v) = self.default_sample_flags {
            offset += v.encode(&mut buf[offset..])?;
        }

        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for TfhdBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let flags = full_header.flags.get();

            let track_id = u32::decode_at(payload, &mut offset)?;

            let base_data_offset = if flags & Self::FLAG_BASE_DATA_OFFSET_PRESENT != 0 {
                Some(u64::decode_at(payload, &mut offset)?)
            } else {
                None
            };

            let sample_description_index =
                if flags & Self::FLAG_SAMPLE_DESCRIPTION_INDEX_PRESENT != 0 {
                    Some(u32::decode_at(payload, &mut offset)?)
                } else {
                    None
                };

            let default_sample_duration = if flags & Self::FLAG_DEFAULT_SAMPLE_DURATION_PRESENT != 0
            {
                Some(u32::decode_at(payload, &mut offset)?)
            } else {
                None
            };

            let default_sample_size = if flags & Self::FLAG_DEFAULT_SAMPLE_SIZE_PRESENT != 0 {
                Some(u32::decode_at(payload, &mut offset)?)
            } else {
                None
            };

            let default_sample_flags = if flags & Self::FLAG_DEFAULT_SAMPLE_FLAGS_PRESENT != 0 {
                Some(SampleFlags::decode_at(payload, &mut offset)?)
            } else {
                None
            };

            let duration_is_empty = flags & Self::FLAG_DURATION_IS_EMPTY != 0;
            let default_base_is_moof = flags & Self::FLAG_DEFAULT_BASE_IS_MOOF != 0;

            Ok((
                Self {
                    track_id,
                    base_data_offset,
                    sample_description_index,
                    default_sample_duration,
                    default_sample_size,
                    default_sample_flags,
                    duration_is_empty,
                    default_base_is_moof,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for TfhdBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for TfhdBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        let mut flags = 0u32;
        if self.base_data_offset.is_some() {
            flags |= Self::FLAG_BASE_DATA_OFFSET_PRESENT;
        }
        if self.sample_description_index.is_some() {
            flags |= Self::FLAG_SAMPLE_DESCRIPTION_INDEX_PRESENT;
        }
        if self.default_sample_duration.is_some() {
            flags |= Self::FLAG_DEFAULT_SAMPLE_DURATION_PRESENT;
        }
        if self.default_sample_size.is_some() {
            flags |= Self::FLAG_DEFAULT_SAMPLE_SIZE_PRESENT;
        }
        if self.default_sample_flags.is_some() {
            flags |= Self::FLAG_DEFAULT_SAMPLE_FLAGS_PRESENT;
        }
        if self.duration_is_empty {
            flags |= Self::FLAG_DURATION_IS_EMPTY;
        }
        if self.default_base_is_moof {
            flags |= Self::FLAG_DEFAULT_BASE_IS_MOOF;
        }
        FullBoxFlags::new(flags)
    }
}

/// [ISO/IEC 14496-12] TrackFragmentBaseMediaDecodeTimeBox class (親: [`TrafBox`])
///
/// トラックフラグメントのベースデコード時間を格納する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct TfdtBox {
    /// FullBox バージョン (0 または 1)
    ///
    /// version=1 の場合は base_media_decode_time が 64-bit、
    /// version=0 の場合は 32-bit でエンコードされる。
    /// ラウンドトリップ時に元のバージョンを保持するために使用される。
    pub version: u8,
    pub base_media_decode_time: u64,
}

impl TfdtBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"tfdt");
}

impl Encode for TfdtBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        if self.full_box_version() == 1 {
            offset += self.base_media_decode_time.encode(&mut buf[offset..])?;
        } else {
            offset += (self.base_media_decode_time as u32).encode(&mut buf[offset..])?;
        }
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for TfdtBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;

            let base_media_decode_time = if full_header.version == 1 {
                u64::decode_at(payload, &mut offset)?
            } else {
                u32::decode_at(payload, &mut offset)? as u64
            };

            Ok((
                Self {
                    version: full_header.version,
                    base_media_decode_time,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for TfdtBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for TfdtBox {
    fn full_box_version(&self) -> u8 {
        // 値が 32-bit に収まらない場合は version=1 が必須
        if self.base_media_decode_time > u32::MAX as u64 {
            1
        } else {
            // それ以外はデコード時に保存されたバージョンを使用
            self.version
        }
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [ISO/IEC 14496-12] TrackRunBox class (親: [`TrafBox`])
///
/// サンプルのリストを格納する。フラグによって存在するフィールドが異なる。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct TrunBox {
    pub data_offset: Option<i32>,
    pub first_sample_flags: Option<SampleFlags>,
    pub samples: Vec<TrunSample>,
}

impl TrunBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"trun");

    /// data_offset が存在することを示すフラグ
    pub const FLAG_DATA_OFFSET_PRESENT: u32 = 0x000001;

    /// first_sample_flags が存在することを示すフラグ
    pub const FLAG_FIRST_SAMPLE_FLAGS_PRESENT: u32 = 0x000004;

    /// sample_duration が存在することを示すフラグ
    pub const FLAG_SAMPLE_DURATION_PRESENT: u32 = 0x000100;

    /// sample_size が存在することを示すフラグ
    pub const FLAG_SAMPLE_SIZE_PRESENT: u32 = 0x000200;

    /// sample_flags が存在することを示すフラグ
    pub const FLAG_SAMPLE_FLAGS_PRESENT: u32 = 0x000400;

    /// sample_composition_time_offset が存在することを示すフラグ
    pub const FLAG_SAMPLE_COMPOSITION_TIME_OFFSETS_PRESENT: u32 = 0x000800;

    fn compute_flags(&self) -> u32 {
        let mut flags = 0u32;
        if self.data_offset.is_some() {
            flags |= Self::FLAG_DATA_OFFSET_PRESENT;
        }
        if self.first_sample_flags.is_some() {
            flags |= Self::FLAG_FIRST_SAMPLE_FLAGS_PRESENT;
        }
        if let Some(sample) = self.samples.first() {
            if sample.duration.is_some() {
                flags |= Self::FLAG_SAMPLE_DURATION_PRESENT;
            }
            if sample.size.is_some() {
                flags |= Self::FLAG_SAMPLE_SIZE_PRESENT;
            }
            if sample.flags.is_some() {
                flags |= Self::FLAG_SAMPLE_FLAGS_PRESENT;
            }
            if sample.composition_time_offset.is_some() {
                flags |= Self::FLAG_SAMPLE_COMPOSITION_TIME_OFFSETS_PRESENT;
            }
        }
        flags
    }

    fn uses_version_1(&self) -> bool {
        self.samples.iter().any(|s| {
            if let Some(offset) = s.composition_time_offset {
                offset < 0
            } else {
                false
            }
        })
    }
}

impl Encode for TrunBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;

        let flags = self.compute_flags();

        // sample_count
        let sample_count = u32::try_from(self.samples.len())
            .map_err(|_| crate::Error::invalid_data("TrunBox sample count exceeds u32::MAX"))?;
        offset += sample_count.encode(&mut buf[offset..])?;

        if let Some(v) = self.data_offset {
            offset += v.encode(&mut buf[offset..])?;
        }
        if let Some(v) = self.first_sample_flags {
            offset += v.encode(&mut buf[offset..])?;
        }

        let version = self.full_box_version();

        for sample in &self.samples {
            if flags & Self::FLAG_SAMPLE_DURATION_PRESENT != 0 {
                offset += sample.duration.unwrap_or(0).encode(&mut buf[offset..])?;
            }
            if flags & Self::FLAG_SAMPLE_SIZE_PRESENT != 0 {
                offset += sample.size.unwrap_or(0).encode(&mut buf[offset..])?;
            }
            if flags & Self::FLAG_SAMPLE_FLAGS_PRESENT != 0 {
                offset += sample
                    .flags
                    .unwrap_or(SampleFlags::empty())
                    .encode(&mut buf[offset..])?;
            }
            if flags & Self::FLAG_SAMPLE_COMPOSITION_TIME_OFFSETS_PRESENT != 0 {
                if version == 1 {
                    offset += sample
                        .composition_time_offset
                        .unwrap_or(0)
                        .encode(&mut buf[offset..])?;
                } else {
                    offset += (sample.composition_time_offset.unwrap_or(0) as u32)
                        .encode(&mut buf[offset..])?;
                }
            }
        }

        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for TrunBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let flags = full_header.flags.get();
            let version = full_header.version;

            let sample_count = u32::decode_at(payload, &mut offset)?;

            let data_offset = if flags & Self::FLAG_DATA_OFFSET_PRESENT != 0 {
                Some(i32::decode_at(payload, &mut offset)?)
            } else {
                None
            };

            let first_sample_flags = if flags & Self::FLAG_FIRST_SAMPLE_FLAGS_PRESENT != 0 {
                Some(SampleFlags::decode_at(payload, &mut offset)?)
            } else {
                None
            };

            // サンプルあたりのバイト数を計算してオーバーフローチェック
            let bytes_per_sample = {
                let mut size = 0usize;
                if flags & Self::FLAG_SAMPLE_DURATION_PRESENT != 0 {
                    size += 4;
                }
                if flags & Self::FLAG_SAMPLE_SIZE_PRESENT != 0 {
                    size += 4;
                }
                if flags & Self::FLAG_SAMPLE_FLAGS_PRESENT != 0 {
                    size += 4;
                }
                if flags & Self::FLAG_SAMPLE_COMPOSITION_TIME_OFFSETS_PRESENT != 0 {
                    size += 4;
                }
                size
            };

            let remaining = payload.len().saturating_sub(offset);
            if bytes_per_sample > 0 {
                let max_samples = remaining / bytes_per_sample;
                if sample_count as usize > max_samples {
                    return Err(Error::invalid_data(
                        "TrunBox sample_count exceeds available payload",
                    ));
                }
            } else {
                // bytes_per_sample = 0 の場合、サンプルデータなしで sample_count 個のサンプルを許容
                // ただし、極端に大きな sample_count による OOM を防ぐ
                // TrunSample 構造体のサイズが約 32 バイトとして、512MB (= 16M サンプル) を上限とする
                const MAX_SAMPLES_WITHOUT_DATA: usize = 16_777_216;
                if sample_count as usize > MAX_SAMPLES_WITHOUT_DATA {
                    return Err(Error::invalid_data(
                        "TrunBox sample_count exceeds maximum allowed when no per-sample data is present",
                    ));
                }
            }

            let mut samples = Vec::new();
            for _ in 0..sample_count {
                let duration = if flags & Self::FLAG_SAMPLE_DURATION_PRESENT != 0 {
                    Some(u32::decode_at(payload, &mut offset)?)
                } else {
                    None
                };

                let size = if flags & Self::FLAG_SAMPLE_SIZE_PRESENT != 0 {
                    Some(u32::decode_at(payload, &mut offset)?)
                } else {
                    None
                };

                let sample_flags = if flags & Self::FLAG_SAMPLE_FLAGS_PRESENT != 0 {
                    Some(SampleFlags::decode_at(payload, &mut offset)?)
                } else {
                    None
                };

                let composition_time_offset =
                    if flags & Self::FLAG_SAMPLE_COMPOSITION_TIME_OFFSETS_PRESENT != 0 {
                        if version == 1 {
                            Some(i32::decode_at(payload, &mut offset)?)
                        } else {
                            Some(u32::decode_at(payload, &mut offset)? as i32)
                        }
                    } else {
                        None
                    };

                samples.push(TrunSample {
                    duration,
                    size,
                    flags: sample_flags,
                    composition_time_offset,
                });
            }

            Ok((
                Self {
                    data_offset,
                    first_sample_flags,
                    samples,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for TrunBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for TrunBox {
    fn full_box_version(&self) -> u8 {
        if self.uses_version_1() { 1 } else { 0 }
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(self.compute_flags())
    }
}

/// [`TrunBox`] のサンプル情報
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct TrunSample {
    pub duration: Option<u32>,
    pub size: Option<u32>,
    pub flags: Option<SampleFlags>,
    pub composition_time_offset: Option<i32>,
}

/// [ISO/IEC 14496-12] SegmentIndexBox class
///
/// セグメントインデックスボックス。DASH などで使用される。
/// メディアセグメントへの参照情報を格納する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct SidxBox {
    pub reference_id: u32,
    pub timescale: u32,
    pub earliest_presentation_time: u64,
    pub first_offset: u64,
    pub references: Vec<SidxReference>,
}

impl SidxBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"sidx");
}

impl Encode for SidxBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;

        offset += self.reference_id.encode(&mut buf[offset..])?;
        offset += self.timescale.encode(&mut buf[offset..])?;

        if self.full_box_version() == 1 {
            offset += self.earliest_presentation_time.encode(&mut buf[offset..])?;
            offset += self.first_offset.encode(&mut buf[offset..])?;
        } else {
            offset += (self.earliest_presentation_time as u32).encode(&mut buf[offset..])?;
            offset += (self.first_offset as u32).encode(&mut buf[offset..])?;
        }

        // reserved (16 bits)
        offset += 0u16.encode(&mut buf[offset..])?;

        // reference_count
        let reference_count = u16::try_from(self.references.len())
            .map_err(|_| crate::Error::invalid_data("SidxBox reference count exceeds u16::MAX"))?;
        offset += reference_count.encode(&mut buf[offset..])?;

        for reference in &self.references {
            // reference_type (1 bit) | referenced_size (31 bits)
            let first_word = ((reference.reference_type as u32) << 31)
                | (reference.referenced_size & 0x7FFFFFFF);
            offset += first_word.encode(&mut buf[offset..])?;

            offset += reference.subsegment_duration.encode(&mut buf[offset..])?;

            // starts_with_sap (1 bit) | sap_type (3 bits) | sap_delta_time (28 bits)
            let third_word = ((reference.starts_with_sap as u32) << 31)
                | ((reference.sap_type as u32 & 0x7) << 28)
                | (reference.sap_delta_time & 0x0FFFFFFF);
            offset += third_word.encode(&mut buf[offset..])?;
        }

        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for SidxBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let version = full_header.version;

            let reference_id = u32::decode_at(payload, &mut offset)?;
            let timescale = u32::decode_at(payload, &mut offset)?;

            let (earliest_presentation_time, first_offset) = if version == 1 {
                let ept = u64::decode_at(payload, &mut offset)?;
                let fo = u64::decode_at(payload, &mut offset)?;
                (ept, fo)
            } else {
                let ept = u32::decode_at(payload, &mut offset)? as u64;
                let fo = u32::decode_at(payload, &mut offset)? as u64;
                (ept, fo)
            };

            // reserved (16 bits)
            let _reserved = u16::decode_at(payload, &mut offset)?;

            let reference_count = u16::decode_at(payload, &mut offset)?;

            // 各 reference は 12 バイト (3 x u32)
            let required_bytes = usize::from(reference_count) * 12;
            if offset + required_bytes > payload.len() {
                return Err(crate::Error::invalid_data(
                    "SidxBox reference_count exceeds remaining payload",
                ));
            }

            let mut references = Vec::new();
            for _ in 0..reference_count {
                let first_word = u32::decode_at(payload, &mut offset)?;
                let reference_type = (first_word >> 31) != 0;
                let referenced_size = first_word & 0x7FFFFFFF;

                let subsegment_duration = u32::decode_at(payload, &mut offset)?;

                let third_word = u32::decode_at(payload, &mut offset)?;
                let starts_with_sap = (third_word >> 31) != 0;
                let sap_type = ((third_word >> 28) & 0x7) as u8;
                let sap_delta_time = third_word & 0x0FFFFFFF;

                references.push(SidxReference {
                    reference_type,
                    referenced_size,
                    subsegment_duration,
                    starts_with_sap,
                    sap_type,
                    sap_delta_time,
                });
            }

            Ok((
                Self {
                    reference_id,
                    timescale,
                    earliest_presentation_time,
                    first_offset,
                    references,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for SidxBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for SidxBox {
    fn full_box_version(&self) -> u8 {
        if self.earliest_presentation_time > u32::MAX as u64 || self.first_offset > u32::MAX as u64
        {
            1
        } else {
            0
        }
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [`SidxBox`] の参照情報
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SidxReference {
    /// true の場合は sidx への参照、false の場合はメディアセグメントへの参照
    pub reference_type: bool,
    /// 参照先のサイズ（バイト）
    pub referenced_size: u32,
    /// サブセグメントの継続時間
    pub subsegment_duration: u32,
    /// SAP で始まるかどうか
    pub starts_with_sap: bool,
    /// SAP の種類 (0-7)
    pub sap_type: u8,
    /// SAP までのデルタ時間
    pub sap_delta_time: u32,
}

/// [ISO/IEC 14496-12] MovieFragmentRandomAccessBox class
///
/// ムービーフラグメントのランダムアクセス情報を格納するボックス。
/// ファイルの末尾に配置される。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct MfraBox {
    pub tfra_boxes: Vec<TfraBox>,
    pub mfro_box: MfroBox,
}

impl MfraBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"mfra");
}

impl Encode for MfraBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        for b in &self.tfra_boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        offset += self.mfro_box.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for MfraBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let mut tfra_boxes = Vec::new();
            let mut mfro_box = None;

            while offset < payload.len() {
                let (child_header, _) = BoxHeader::decode(&payload[offset..])?;
                match child_header.box_type {
                    TfraBox::TYPE => {
                        let (b, n) = TfraBox::decode(&payload[offset..])?;
                        tfra_boxes.push(b);
                        offset += n;
                    }
                    MfroBox::TYPE => {
                        let (b, n) = MfroBox::decode(&payload[offset..])?;
                        mfro_box = Some(b);
                        offset += n;
                    }
                    _ => {
                        // 未知のボックスはスキップ
                        offset += usize::try_from(child_header.box_size.get()).map_err(|_| {
                            crate::Error::invalid_data("MfraBox child box size exceeds usize::MAX")
                        })?;
                    }
                }
            }

            let mfro_box = mfro_box.ok_or_else(|| {
                crate::Error::invalid_data("Missing mandatory 'mfro' box in 'mfra' box")
            })?;

            Ok((
                Self {
                    tfra_boxes,
                    mfro_box,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for MfraBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(
            self.tfra_boxes
                .iter()
                .map(as_box_object)
                .chain(core::iter::once(as_box_object(&self.mfro_box))),
        )
    }
}

/// [ISO/IEC 14496-12] TrackFragmentRandomAccessBox class
///
/// トラックフラグメントのランダムアクセス情報を格納するボックス。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct TfraBox {
    /// FullBox バージョン (0 または 1)
    ///
    /// version=1 の場合は time と moof_offset が 64-bit、
    /// version=0 の場合は 32-bit でエンコードされる。
    /// ラウンドトリップ時に元のバージョンを保持するために使用される。
    pub version: u8,
    pub track_id: u32,
    /// traf_number のバイト数 - 1 (0-3)
    pub length_size_of_traf_num: u8,
    /// trun_number のバイト数 - 1 (0-3)
    pub length_size_of_trun_num: u8,
    /// sample_number のバイト数 - 1 (0-3)
    pub length_size_of_sample_num: u8,
    pub entries: Vec<TfraEntry>,
}

impl TfraBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"tfra");
}

impl Encode for TfraBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;

        offset += self.track_id.encode(&mut buf[offset..])?;

        // reserved (26 bits) + length_size_of_traf_num (2 bits) + length_size_of_trun_num (2 bits) + length_size_of_sample_num (2 bits)
        let lengths: u32 = ((self.length_size_of_traf_num as u32 & 0x3) << 4)
            | ((self.length_size_of_trun_num as u32 & 0x3) << 2)
            | (self.length_size_of_sample_num as u32 & 0x3);
        offset += lengths.encode(&mut buf[offset..])?;

        let number_of_entry = u32::try_from(self.entries.len())
            .map_err(|_| crate::Error::invalid_data("TfraBox entry count exceeds u32::MAX"))?;
        offset += number_of_entry.encode(&mut buf[offset..])?;

        let version = self.full_box_version();
        for entry in &self.entries {
            if version == 1 {
                offset += entry.time.encode(&mut buf[offset..])?;
                offset += entry.moof_offset.encode(&mut buf[offset..])?;
            } else {
                offset += (entry.time as u32).encode(&mut buf[offset..])?;
                offset += (entry.moof_offset as u32).encode(&mut buf[offset..])?;
            }

            // traf_number, trun_number, sample_number は可変長
            offset += encode_variable_uint(
                entry.traf_number,
                self.length_size_of_traf_num + 1,
                &mut buf[offset..],
            )?;
            offset += encode_variable_uint(
                entry.trun_number,
                self.length_size_of_trun_num + 1,
                &mut buf[offset..],
            )?;
            offset += encode_variable_uint(
                entry.sample_number,
                self.length_size_of_sample_num + 1,
                &mut buf[offset..],
            )?;
        }

        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for TfraBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let full_box_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let version = full_box_header.version;

            let track_id = u32::decode_at(payload, &mut offset)?;
            let lengths = u32::decode_at(payload, &mut offset)?;
            let length_size_of_traf_num = ((lengths >> 4) & 0x3) as u8;
            let length_size_of_trun_num = ((lengths >> 2) & 0x3) as u8;
            let length_size_of_sample_num = (lengths & 0x3) as u8;

            let number_of_entry = u32::decode_at(payload, &mut offset)?;

            // エントリごとの最小バイト数を計算してペイロード残量と照合する
            let time_moof_size: usize = if version == 1 { 16 } else { 8 };
            let variable_size: usize = (length_size_of_traf_num as usize + 1)
                + (length_size_of_trun_num as usize + 1)
                + (length_size_of_sample_num as usize + 1);
            let entry_size = time_moof_size + variable_size;
            let remaining = payload.len().saturating_sub(offset);
            if let Some(required) = (number_of_entry as usize).checked_mul(entry_size) {
                if required > remaining {
                    return Err(crate::Error::invalid_data(
                        "TfraBox number_of_entry exceeds remaining payload",
                    ));
                }
            } else {
                return Err(crate::Error::invalid_data(
                    "TfraBox number_of_entry causes overflow",
                ));
            }

            let mut entries = Vec::new();
            for _ in 0..number_of_entry {
                let (time, moof_offset) = if version == 1 {
                    let time = u64::decode_at(payload, &mut offset)?;
                    let moof_offset = u64::decode_at(payload, &mut offset)?;
                    (time, moof_offset)
                } else {
                    let time = u32::decode_at(payload, &mut offset)? as u64;
                    let moof_offset = u32::decode_at(payload, &mut offset)? as u64;
                    (time, moof_offset)
                };

                let traf_number =
                    decode_variable_uint(payload, &mut offset, length_size_of_traf_num + 1)?;
                let trun_number =
                    decode_variable_uint(payload, &mut offset, length_size_of_trun_num + 1)?;
                let sample_number =
                    decode_variable_uint(payload, &mut offset, length_size_of_sample_num + 1)?;

                entries.push(TfraEntry {
                    time,
                    moof_offset,
                    traf_number,
                    trun_number,
                    sample_number,
                });
            }

            Ok((
                Self {
                    version,
                    track_id,
                    length_size_of_traf_num,
                    length_size_of_trun_num,
                    length_size_of_sample_num,
                    entries,
                },
                header.external_size() + payload.len(),
            ))
        })
    }
}

impl BaseBox for TfraBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for TfraBox {
    fn full_box_version(&self) -> u8 {
        // 値が 32-bit に収まらない場合は version=1 が必須
        let needs_64bit = self
            .entries
            .iter()
            .any(|e| e.time > u32::MAX as u64 || e.moof_offset > u32::MAX as u64);
        if needs_64bit {
            1
        } else {
            // それ以外はデコード時に保存されたバージョンを使用
            self.version
        }
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// [`TfraBox`] のエントリ
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[expect(missing_docs)]
pub struct TfraEntry {
    pub time: u64,
    pub moof_offset: u64,
    pub traf_number: u32,
    pub trun_number: u32,
    pub sample_number: u32,
}

/// [ISO/IEC 14496-12] MovieFragmentRandomAccessOffsetBox class
///
/// mfra ボックスのサイズを格納するボックス。
/// ファイル末尾から逆方向にシークするために使用される。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MfroBox {
    /// mfra ボックスのサイズ
    pub size: u32,
}

impl MfroBox {
    /// ボックス種別
    pub const TYPE: BoxType = BoxType::Normal(*b"mfro");
}

impl Encode for MfroBox {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let header = BoxHeader::new_variable_size(Self::TYPE);
        let mut offset = header.encode(buf)?;
        offset += FullBoxHeader::from_box(self).encode(&mut buf[offset..])?;
        offset += self.size.encode(&mut buf[offset..])?;
        header.finalize_box_size(&mut buf[..offset])?;
        Ok(offset)
    }
}

impl Decode for MfroBox {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        with_box_type(Self::TYPE, || {
            let (header, payload) = BoxHeader::decode_header_and_payload(buf)?;
            header.box_type.expect(Self::TYPE)?;

            let mut offset = 0;
            let _full_box_header = FullBoxHeader::decode_at(payload, &mut offset)?;
            let size = u32::decode_at(payload, &mut offset)?;

            Ok((Self { size }, header.external_size() + payload.len()))
        })
    }
}

impl BaseBox for MfroBox {
    fn box_type(&self) -> BoxType {
        Self::TYPE
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        Box::new(core::iter::empty())
    }
}

impl FullBox for MfroBox {
    fn full_box_version(&self) -> u8 {
        0
    }

    fn full_box_flags(&self) -> FullBoxFlags {
        FullBoxFlags::new(0)
    }
}

/// 可変長の符号なし整数をエンコード
fn encode_variable_uint(value: u32, byte_count: u8, buf: &mut [u8]) -> Result<usize> {
    match byte_count {
        1 => {
            buf[0] = value as u8;
            Ok(1)
        }
        2 => {
            buf[0] = (value >> 8) as u8;
            buf[1] = value as u8;
            Ok(2)
        }
        3 => {
            buf[0] = (value >> 16) as u8;
            buf[1] = (value >> 8) as u8;
            buf[2] = value as u8;
            Ok(3)
        }
        4 => value.encode(buf),
        _ => Err(crate::Error::invalid_data(
            "Invalid byte count for variable uint",
        )),
    }
}

/// 可変長の符号なし整数をデコード
fn decode_variable_uint(buf: &[u8], offset: &mut usize, byte_count: u8) -> Result<u32> {
    if *offset + byte_count as usize > buf.len() {
        return Err(crate::Error::invalid_data("Unexpected end of data"));
    }

    let value = match byte_count {
        1 => {
            let v = buf[*offset] as u32;
            *offset += 1;
            v
        }
        2 => {
            let v = ((buf[*offset] as u32) << 8) | (buf[*offset + 1] as u32);
            *offset += 2;
            v
        }
        3 => {
            let v = ((buf[*offset] as u32) << 16)
                | ((buf[*offset + 1] as u32) << 8)
                | (buf[*offset + 2] as u32);
            *offset += 3;
            v
        }
        4 => u32::decode_at(buf, offset)?,
        _ => {
            return Err(crate::Error::invalid_data(
                "Invalid byte count for variable uint",
            ));
        }
    };

    Ok(value)
}
