//! ISO_IEC_14496-1 で定義されているディスクリプター群
use alloc::{format, string::String, vec::Vec};

use crate::{Decode, Encode, Error, Result, Uint};

/// [ISO_IEC_14496-1] ES_Descriptor class
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct EsDescriptor {
    pub es_id: u16,
    pub stream_priority: Uint<u8, 5>,
    pub depends_on_es_id: Option<u16>,
    pub url_string: Option<String>,
    pub ocr_es_id: Option<u16>,
    pub dec_config_descr: DecoderConfigDescriptor,
    pub sl_config_descr: SlConfigDescriptor,
}

impl EsDescriptor {
    const TAG: u8 = 3; // ES_DescrTag

    /// [`EsDescriptor::es_id`] の実質的な最小値 (0 は予約されている）
    pub const MIN_ES_ID: u16 = 1;

    /// [`EsDescriptor::stream_priority`] で一番優先度が低くなる値
    pub const LOWEST_STREAM_PRIORITY: Uint<u8, 5> = Uint::new(0);
}

impl Decode for EsDescriptor {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (tag, _size, mut offset) = decode_tag_and_size(buf)?;
        if tag != Self::TAG {
            return Err(Error::invalid_data(format!(
                "Unexpected descriptor tag: expected={}, actual={tag}",
                Self::TAG
            )));
        }

        let es_id = u16::decode_at(buf, &mut offset)?;

        let b = u8::decode_at(buf, &mut offset)?;
        let stream_dependence_flag: Uint<u8, 1, 7> = Uint::from_bits(b);
        let url_flag: Uint<u8, 1, 6> = Uint::from_bits(b);
        let ocr_stream_flag: Uint<u8, 1, 5> = Uint::from_bits(b);
        let stream_priority = Uint::from_bits(b);

        let depends_on_es_id = if stream_dependence_flag.get() == 1 {
            Some(u16::decode_at(buf, &mut offset)?)
        } else {
            None
        };

        let url_string = if url_flag.get() == 1 {
            let len = u8::decode_at(buf, &mut offset)? as usize;
            if len > buf[offset..].len() {
                return Err(Error::invalid_data("URL string exceeds buffer boundary"));
            }
            let s = String::from_utf8(buf[offset..][..len].to_vec())
                .map_err(|_| Error::invalid_data("Invalid UTF-8 in URL string"))?;
            offset += len;
            Some(s)
        } else {
            None
        };

        let ocr_es_id = if ocr_stream_flag.get() == 1 {
            Some(u16::decode_at(buf, &mut offset)?)
        } else {
            None
        };

        let dec_config_descr = DecoderConfigDescriptor::decode_at(buf, &mut offset)?;
        let sl_config_descr = SlConfigDescriptor::decode_at(buf, &mut offset)?;

        Ok((
            Self {
                es_id,
                stream_priority,
                depends_on_es_id,
                url_string,
                ocr_es_id,
                dec_config_descr,
                sl_config_descr,
            },
            offset,
        ))
    }
}

impl Encode for EsDescriptor {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = 0;
        offset += self.es_id.encode(&mut buf[offset..])?;
        offset += (Uint::<u8, 1, 7>::new(self.depends_on_es_id.is_some() as u8).to_bits()
            | Uint::<u8, 1, 6>::new(self.url_string.is_some() as u8).to_bits()
            | Uint::<u8, 1, 5>::new(self.ocr_es_id.is_some() as u8).to_bits()
            | self.stream_priority.to_bits())
        .encode(&mut buf[offset..])?;

        if let Some(v) = self.depends_on_es_id {
            offset += v.encode(&mut buf[offset..])?;
        }
        if let Some(v) = &self.url_string {
            if v.len() > 255 {
                return Err(Error::invalid_input("URL string too long (max 255 bytes)"));
            }
            offset += (v.len() as u8).encode(&mut buf[offset..])?;
            offset += v.as_bytes().encode(&mut buf[offset..])?;
        }
        if let Some(v) = self.ocr_es_id {
            offset += v.encode(&mut buf[offset..])?;
        }

        offset += self.dec_config_descr.encode(&mut buf[offset..])?;
        offset += self.sl_config_descr.encode(&mut buf[offset..])?;

        encode_tag_and_payload(buf, Self::TAG, offset)
    }
}

/// [ISO_IEC_14496-1] DecoderConfigDescriptor class
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct DecoderConfigDescriptor {
    pub object_type_indication: u8,
    pub stream_type: Uint<u8, 6, 2>,
    pub up_stream: Uint<u8, 1, 1>,
    pub buffer_size_db: Uint<u32, 24>,
    pub max_bitrate: u32,
    pub avg_bitrate: u32,
    pub dec_specific_info: Option<DecoderSpecificInfo>,
}

impl DecoderConfigDescriptor {
    const TAG: u8 = 4; // DecoderConfigDescrTag

    /// AAC 用の [`DecoderConfigDescriptor::object_type_indication`] の値
    pub const OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3: u8 = 0x40;

    /// 音声用の [`DecoderConfigDescriptor::stream_type`] の値
    pub const STREAM_TYPE_AUDIO: Uint<u8, 6, 2> = Uint::new(0x05);

    /// 通常の再生用メディアファイル向けの [`DecoderConfigDescriptor::up_stream`] の値
    pub const UP_STREAM_FALSE: Uint<u8, 1, 1> = Uint::new(0);
}

impl Decode for DecoderConfigDescriptor {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (tag, _size, mut offset) = decode_tag_and_size(buf)?;
        if tag != Self::TAG {
            return Err(Error::invalid_data(format!(
                "Unexpected descriptor tag: expected={}, actual={tag}",
                Self::TAG
            )));
        }

        let object_type_indication = u8::decode_at(buf, &mut offset)?;

        let b = u8::decode_at(buf, &mut offset)?;
        let stream_type = Uint::from_bits(b);
        let up_stream = Uint::from_bits(b);

        let buffer_size_db = {
            let mut temp = [0; 4];
            if 3 > buf[offset..].len() {
                return Err(Error::invalid_data(
                    "buffer_size_db exceeds buffer boundary",
                ));
            }
            temp[1..].copy_from_slice(&buf[offset..][..3]);
            offset += 3;
            Uint::from_bits(u32::from_be_bytes(temp))
        };

        let max_bitrate = u32::decode_at(buf, &mut offset)?;
        let avg_bitrate = u32::decode_at(buf, &mut offset)?;

        let dec_specific_info = if starts_with_tag(&buf[offset..], DecoderSpecificInfo::TAG) {
            Some(DecoderSpecificInfo::decode_at(buf, &mut offset)?)
        } else {
            None
        };

        // [NOTE]
        // 仕様的には、ここに複数個の profileLevelIndicationIndexDescriptor が存在する可能性がある。
        // ただし、実際に使われることがあるかどうかは不明なのと、
        // 実例データがないと実装が適切かどうかの判断が難しいため、いったんは未実装としておいて、
        // 本当に必要になったタイミングで実装することにする。

        Ok((
            Self {
                object_type_indication,
                stream_type,
                up_stream,
                buffer_size_db,
                max_bitrate,
                avg_bitrate,
                dec_specific_info,
            },
            offset,
        ))
    }
}

impl Encode for DecoderConfigDescriptor {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = 0;

        offset += self.object_type_indication.encode(&mut buf[offset..])?;
        offset += (self.stream_type.to_bits()
            | self.up_stream.to_bits()
            | Uint::<u8, 1>::new(1).to_bits())
        .encode(&mut buf[offset..])?;
        offset += self.buffer_size_db.to_bits().to_be_bytes()[1..].encode(&mut buf[offset..])?;
        offset += self.max_bitrate.encode(&mut buf[offset..])?;
        offset += self.avg_bitrate.encode(&mut buf[offset..])?;
        if let Some(dec_specific_info) = &self.dec_specific_info {
            offset += dec_specific_info.encode(&mut buf[offset..])?;
        }

        encode_tag_and_payload(buf, Self::TAG, offset)
    }
}

/// [ISO_IEC_14496-1] DecoderSpecificInfo class
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub struct DecoderSpecificInfo {
    pub payload: Vec<u8>,
}

impl DecoderSpecificInfo {
    const TAG: u8 = 5; // DecSpecificInfoTag
}

impl Decode for DecoderSpecificInfo {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (tag, size, mut offset) = decode_tag_and_size(buf)?;

        if tag != Self::TAG {
            return Err(Error::invalid_data(format!(
                "Unexpected descriptor tag: expected={}, actual={tag}",
                Self::TAG
            )));
        }

        if size > buf[offset..].len() {
            return Err(Error::invalid_data(
                "DecoderSpecificInfo payload exceeds buffer boundary",
            ));
        }
        let payload = buf[offset..][..size].to_vec();
        offset += size;

        Ok((Self { payload }, offset))
    }
}

impl Encode for DecoderSpecificInfo {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let offset = self.payload.encode(buf)?;
        encode_tag_and_payload(buf, Self::TAG, offset)
    }
}

/// [ISO_IEC_14496-1] SLConfigDescriptor class
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SlConfigDescriptor;

impl SlConfigDescriptor {
    const TAG: u8 = 6; // SLConfigDescrTag
}

impl Decode for SlConfigDescriptor {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (tag, _size, mut offset) = decode_tag_and_size(buf)?;

        if tag != Self::TAG {
            return Err(Error::invalid_data(format!(
                "Unexpected descriptor tag: expected={}, actual={tag}",
                Self::TAG
            )));
        }

        let predefined = u8::decode_at(buf, &mut offset)?;
        if predefined != 2 {
            return Err(Error::unsupported(format!(
                "Unsupported `SLConfigDescriptor.predefined` value: {predefined}"
            )));
        }

        Ok((Self, offset))
    }
}

impl Encode for SlConfigDescriptor {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let predefined = 2u8;
        let offset = predefined.encode(buf)?;
        encode_tag_and_payload(buf, Self::TAG, offset)
    }
}

/// バッファが指定したタグから始まるかどうかをチェックする
fn starts_with_tag(buf: &[u8], expected_tag: u8) -> bool {
    buf.first().is_some_and(|&tag| tag == expected_tag)
}

fn decode_tag_and_size(buf: &[u8]) -> Result<(u8, usize, usize)> {
    let mut offset = 0;
    let tag = u8::decode_at(buf, &mut offset)?;

    let mut size: usize = 0;
    let mut has_next_byte = true;
    while has_next_byte {
        let b = u8::decode_at(buf, &mut offset)?;
        has_next_byte = Uint::<u8, 1, 7>::from_bits(b).get() == 1;

        let new_size_base = size
            .checked_shl(7)
            .ok_or_else(|| Error::invalid_data("Descriptor size overflow"))?;
        size = new_size_base | Uint::<u8, 7>::from_bits(b).get() as usize
    }

    Ok((tag, size, offset))
}

// buf の先頭にペイロードが格納されている前提
fn encode_tag_and_payload(buf: &mut [u8], tag: u8, payload_size: usize) -> Result<usize> {
    let mut header_buf = [0; 64];
    let header_size = encode_tag_and_size(&mut header_buf, tag, payload_size)?;
    Error::check_buffer_size(header_size + payload_size, buf)?;
    buf.copy_within(..payload_size, header_size);
    buf[..header_size].copy_from_slice(&header_buf[..header_size]);
    Ok(header_size + payload_size)
}

fn encode_tag_and_size(buf: &mut [u8], tag: u8, mut size: usize) -> Result<usize> {
    let mut offset = 0;
    offset += tag.encode(&mut buf[offset..])?;

    let mut size_bytes = Vec::new();
    for i in 0.. {
        let mut b = (size & 0b0111_1111) as u8;
        size >>= 7;

        if i > 0 {
            b |= 0b1000_0000;
        }
        size_bytes.push(b);

        if size == 0 {
            break;
        }
    }
    size_bytes.reverse(); // リトルエンディアンからビッグエンディアンにする

    offset += size_bytes.encode(&mut buf[offset..])?;
    Ok(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::boxes::EsdsBox;

    #[test]
    fn tag_and_size() {
        let mut buf = [0; 32];
        let encoded_size = encode_tag_and_size(&mut buf, 12, 123456).unwrap();

        let (tag, size, consumed) = decode_tag_and_size(&buf).unwrap();
        assert_eq!(tag, 12);
        assert_eq!(size, 123456);
        assert_eq!(consumed, encoded_size);
    }

    // 過去に見つかったバグのリグレッションテスト
    #[test]
    fn decoder_specific_info_size_overflow() {
        let crash_input: &[u8] = &[
            0, 0, 0, 171, 101, 115, 100, 115, 224, 206, 255, 64, 3, 93, 47, 115, 224, 202, 191, 0,
            0, 1, 4, 10, 0, 254, 255, 0, 0, 0, 0, 0, 0, 0, 27, 0, 0, 5, 255, 255, 255, 255, 255,
            145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145,
            145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145,
            145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145,
            145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 111, 255, 255, 255, 255, 255, 255,
            255, 255, 145, 145, 145, 0, 0, 0, 0, 0, 0, 0, 19, 145, 145, 145, 145, 145, 145, 145,
            145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145,
            145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145, 145,
            145, 145, 145, 145, 145, 145, 145, 145, 145, 145,
        ];

        // オーバーフローでパニックせずにエラーを返すべき
        assert!(EsdsBox::decode(crash_input).is_err());
    }
}
