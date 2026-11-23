//! ISO_IEC_14496-1 で定義されているディスクリプター群
use alloc::{format, string::String, vec::Vec};

use crate::{Decode, Encode, Error, Result, Uint};

/// [ISO_IEC_14496-1] ES_Descriptor class
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EsDescriptor {
    /// ElementaryStream の識別子（0 は予約、実質的な最小値は [`EsDescriptor::MIN_ES_ID`]）
    pub es_id: u16,

    /// ストリーム優先度（5 ビット。値が大きいほど優先度が高い）
    pub stream_priority: Uint<u8, 5>,

    /// このストリームが依存する別 ES の `es_id`。存在しない場合は [`None`]
    pub depends_on_es_id: Option<u16>,

    /// このストリームの外部データを指す URL（最大 255 バイト）。存在しない場合は [`None`]
    pub url_string: Option<String>,

    /// object clock reference を共有する別 ES の `es_id`。存在しない場合は [`None`]
    pub ocr_es_id: Option<u16>,

    /// このストリームのデコーダー設定
    pub dec_config_descr: DecoderConfigDescriptor,

    /// このストリームの Sync Layer 設定
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
pub struct DecoderConfigDescriptor {
    /// オブジェクトタイプ識別子（例: AAC は
    /// [`DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3`]）
    pub object_type_indication: u8,

    /// ストリーム種別（6 ビット。例: 音声は [`DecoderConfigDescriptor::STREAM_TYPE_AUDIO`]）
    pub stream_type: Uint<u8, 6, 2>,

    /// 上り方向ストリームか（1 ビット。再生用ファイルは常に
    /// [`DecoderConfigDescriptor::UP_STREAM_FALSE`]）
    pub up_stream: Uint<u8, 1, 1>,

    /// デコーダーバッファサイズ（24 ビット、バイト単位）
    pub buffer_size_db: Uint<u32, 24>,

    /// 最大ビットレート（bps）
    pub max_bitrate: u32,

    /// 平均ビットレート（bps。0 は「不明」を意味する）
    pub avg_bitrate: u32,

    /// コーデック固有のデコーダー情報（AAC の AudioSpecificConfig 等）
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
pub struct DecoderSpecificInfo {
    /// コーデック固有のバイト列（AAC なら AudioSpecificConfig など）
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
        size = shift_in_size_bits(size, Uint::<u8, 7>::from_bits(b).get())?;
    }

    Ok((tag, size, offset))
}

/// それまでにデコードしたサイズを 7 ビット上位へずらし、サイズ欄 1 バイト分の下位 7 ビットを加えた値を返す
///
/// `bits` は `0..=0x7F` に収まっていること。
/// 結果が `usize` に収まらない場合は [`crate::ErrorKind::InvalidData`] を返す
fn shift_in_size_bits(size: usize, bits: u8) -> Result<usize> {
    // ディスクリプターのサイズ（sizeOfInstance）は、ISO/IEC 14496-34:2025 の 7.5 Expandable classes の規則に従い、
    // 上位から 7 ビットずつ並べて表される。
    // この規則は、ISO/IEC 14496-1:2010 では 8 章 Syntactic Description Language で定義されていた。
    // なお、ここでの扱いは ISO/IEC 14496-34:2025 に基づくものであり、将来の改訂で変わる可能性がある。
    //
    // `usize::checked_shl` はシフト量しか検査せず、上位ビットが失われても失敗しないため、
    // チェック付きの乗算で 7 ビット上位へずらす。
    // 乗算が成功すれば結果は `usize::MAX - 0x7F` 以下なので、7 ビットに収まる `bits` を足してもオーバーフローしない。
    // それでも加算をチェック付きにするのは、`bits` の前提が破られた場合にも、
    // パニックや値の回り込みを起こさず同じエラーにするためである
    size.checked_mul(1 << 7)
        .and_then(|shifted| shifted.checked_add(usize::from(bits)))
        .ok_or_else(|| Error::invalid_data("Descriptor size exceeds usize::MAX"))
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

    use crate::{ErrorKind, boxes::EsdsBox};

    #[test]
    fn tag_and_size() {
        let mut buf = [0; 32];
        let encoded_size = encode_tag_and_size(&mut buf, 12, 123456)
            .expect("32 バイトのバッファは 5 バイト以内のエンコード結果を必ず収められる");

        let (tag, size, consumed) = decode_tag_and_size(&buf)
            .expect("直前にエンコードした有効なタグとサイズが必ずデコードできる");
        assert_eq!(tag, 12);
        assert_eq!(size, 123456);
        assert_eq!(consumed, encoded_size);
    }

    /// サイズ欄の 1 バイト分の更新で、通常の値が正しく計算されること
    ///
    /// 0x12 を 7 ビット上位へずらした 0x900 に 0x34 を加えて 0x934 になる。
    /// シフトが抜ければ 0x46、加算が抜ければ 0x900 になるため、どちらの欠落も検出できる
    #[test]
    fn shift_in_size_bits_computes_size() {
        assert_eq!(
            shift_in_size_bits(0x12, 0x34).expect("小さい値の更新はオーバーフローしない"),
            0x934
        );
    }

    /// サイズ欄の 1 バイト分の更新で、結果が `usize::MAX` ちょうどなら成功し、`usize` に収まらない場合に
    /// `InvalidData` になること
    ///
    /// 値は `usize::MAX` から求めるため、`usize` の幅によらず同じ境界を確認できる
    #[test]
    fn shift_in_size_bits_overflow_boundary() {
        // `usize::MAX >> 7` を 7 ビット上位へずらして 7 ビットの最大値を加えると `usize::MAX` ちょうどになる
        assert_eq!(
            shift_in_size_bits(usize::MAX >> 7, 0x7F)
                .expect("結果が usize::MAX ちょうどになる更新はオーバーフローしない"),
            usize::MAX
        );

        // `(usize::MAX >> 7) + 1` はオーバーフローする最小の値で、上位ビットを切り捨てると 0 になる。
        // `usize::MAX` は最大の値で、上位 7 ビットが失われる
        for size in [(usize::MAX >> 7) + 1, usize::MAX] {
            let err = shift_in_size_bits(size, 0)
                .expect_err("7 ビット上位へずらすと usize に収まらない更新はエラーになる");
            assert_eq!(
                err.kind,
                ErrorKind::InvalidData,
                "size={size:#X} からの更新は InvalidData になる"
            );
        }
    }

    /// 宣言サイズが `usize::MAX` の DecoderSpecificInfo でも、境界検査がオーバーフローせずに `InvalidData` を返すこと
    ///
    /// 境界検査が加算（`offset + size`）の形に戻った場合のオーバーフローは、このテストで検出する。
    /// サイズ欄を `usize` の幅に依存せずに作るため、private の `encode_tag_and_size` を使う
    #[test]
    fn decoder_specific_info_payload_exceeds_boundary_with_usize_max_size() {
        let mut buf = [0; 32];
        let header_size = encode_tag_and_size(&mut buf, DecoderSpecificInfo::TAG, usize::MAX)
            .expect("32 バイトのバッファは usize::MAX のサイズ欄を必ず収められる");

        // ペイロードは 2 バイトしかないため、宣言サイズを満たせない
        let mut input = buf[..header_size].to_vec();
        input.extend_from_slice(&[0x11, 0x90]);

        let err = DecoderSpecificInfo::decode(&input)
            .expect_err("宣言サイズが入力の残りより大きい DecoderSpecificInfo はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert!(
            err.reason
                .contains("DecoderSpecificInfo payload exceeds buffer boundary"),
            "理由文字列が期待と違う: {}",
            err.reason
        );
    }

    /// fuzzing で見つかった入力で、パニックせずに `InvalidData` を返すこと
    ///
    /// DecoderSpecificInfo の境界検査の加算がオーバーフローしてパニックした入力である。
    /// DecoderSpecificInfo のサイズ欄の値が `usize` に収まらない時点で `InvalidData` になる
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

        let err =
            EsdsBox::decode(crash_input).expect_err("ディスクリプターが壊れた esds はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
    }
}
