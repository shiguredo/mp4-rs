//! NAL ユニットのフレーミング処理 (Annex B / length-prefixed)
//!
//! H.264 / H.265 共通の、コーデックに依存しない NAL ユニット境界の走査と
//! 相互変換を提供する。NAL ヘッダーの解釈は行わず、NAL 本体をバイト列として
//! 呼び出し側へ渡す。
//!
//! crate 内部専用の非公開モジュールであり、公開 API にはしない。

use alloc::vec::Vec;

use crate::{Error, Result};

/// 4 バイト開始コード (`start_code_prefix_one_4bytes`、ITU-T H.264 Annex B)
pub(crate) const START_CODE_PREFIX_ONE_4BYTES: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

/// 3 バイト開始コード (`start_code_prefix_one_3bytes`、ITU-T H.264 Annex B)
pub(crate) const START_CODE_PREFIX_ONE_3BYTES: [u8; 3] = [0x00, 0x00, 0x01];

/// NAL 長フィールド幅 (ISO/IEC 14496-15 の `lengthSizeMinusOne`)
///
/// `lengthSizeMinusOne` は 0 / 1 / 3 が正当で、2 (幅 3) は reserved のため
/// この型では表現できない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LengthSize {
    /// 幅 1 (`lengthSizeMinusOne == 0`)
    OneByte,
    /// 幅 2 (`lengthSizeMinusOne == 1`)
    TwoBytes,
    /// 幅 4 (`lengthSizeMinusOne == 3`)
    FourBytes,
}

impl LengthSize {
    /// バイト幅 (1 / 2 / 4)
    pub fn bytes(self) -> usize {
        match self {
            Self::OneByte => 1,
            Self::TwoBytes => 2,
            Self::FourBytes => 4,
        }
    }

    /// `lengthSizeMinusOne` の値 (0 / 1 / 3)
    pub fn length_size_minus_one(self) -> u8 {
        match self {
            Self::OneByte => 0,
            Self::TwoBytes => 1,
            Self::FourBytes => 3,
        }
    }

    /// `lengthSizeMinusOne` の値 (0 / 1 / 3) から [`LengthSize`] へ変換する
    ///
    /// ISO/IEC 14496-15 で 0 / 1 / 3 が正当で、2 (幅 3) は reserved、4 以上は
    /// 2 ビット欄の範囲外となる。正当な値は対応する variant へ変換し、
    /// それ以外は [`crate::ErrorKind::InvalidInput`] を返す。
    pub fn from_length_size_minus_one(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::OneByte),
            1 => Ok(Self::TwoBytes),
            3 => Ok(Self::FourBytes),
            // 2 は ISO/IEC 14496-15 の lengthSizeMinusOne で reserved のため拒否する
            2 => Err(Error::invalid_input("lengthSizeMinusOne 2 is reserved")),
            // 4 以上は 2 ビット欄の範囲外のため拒否する
            _ => Err(Error::invalid_input(
                "lengthSizeMinusOne must be 0, 1, or 3",
            )),
        }
    }
}

/// `pos` に開始コードがある場合にその長さ (3 または 4) を返す
///
/// 4 バイト開始コードを 3 バイト開始コード + 先行ゼロに誤分割しないため、
/// 4 バイトを先に検査する
fn start_code_len_at(input: &[u8], pos: usize) -> Option<usize> {
    if input.len() >= pos + START_CODE_PREFIX_ONE_4BYTES.len()
        && input[pos..pos + START_CODE_PREFIX_ONE_4BYTES.len()] == START_CODE_PREFIX_ONE_4BYTES
    {
        Some(START_CODE_PREFIX_ONE_4BYTES.len())
    } else if input.len() >= pos + START_CODE_PREFIX_ONE_3BYTES.len()
        && input[pos..pos + START_CODE_PREFIX_ONE_3BYTES.len()] == START_CODE_PREFIX_ONE_3BYTES
    {
        Some(START_CODE_PREFIX_ONE_3BYTES.len())
    } else {
        None
    }
}

/// `pos` 以降で最初に現れる開始コードの位置と長さを返す
fn find_start_code(input: &[u8], pos: usize) -> Option<(usize, usize)> {
    let mut i = pos;
    while i + START_CODE_PREFIX_ONE_3BYTES.len() <= input.len() {
        if let Some(len) = start_code_len_at(input, i) {
            return Some((i, len));
        }
        i += 1;
    }
    None
}

/// 大端序の長さフィールドを整数値として読む
fn read_length_field(bytes: &[u8], length_size: LengthSize) -> u32 {
    let mut len: u32 = 0;
    for byte in &bytes[..length_size.bytes()] {
        len = (len << 8) | u32::from(*byte);
    }
    len
}

/// `len` が `length_size` バイトの長さフィールドに収まるかを返す
fn length_fits(len: usize, length_size: LengthSize) -> bool {
    match length_size {
        LengthSize::OneByte => len <= u8::MAX as usize,
        LengthSize::TwoBytes => len <= u16::MAX as usize,
        LengthSize::FourBytes => len <= u32::MAX as usize,
    }
}

/// 大端序の長さフィールドを書き出す
///
/// `length_fits` で収まることを検証済みの `len` だけを渡すこと
fn write_length_field(out: &mut Vec<u8>, len: usize, length_size: LengthSize) {
    match length_size {
        LengthSize::OneByte => out.push(len as u8),
        LengthSize::TwoBytes => out.extend_from_slice(&(len as u16).to_be_bytes()),
        LengthSize::FourBytes => out.extend_from_slice(&(len as u32).to_be_bytes()),
    }
}

/// Annex B (ITU-T H.264 Annex B) の NAL ユニット境界を走査する
///
/// 返す NAL 本体は開始コードを含まないバイト列の借用。
///
/// # 契約
///
/// - 空入力は NAL ユニット 0 個の成功 (開始コード欠落とは区別する)
/// - 非空入力に開始コードが 1 つも無い場合は [`crate::Error`]
/// - 最初の開始コードより前の `leading_zero_8bits`、NAL 間のゼロ詰め
///   (`trailing_zero_8bits` / 次の開始コードの `leading_zero_8bits`)、および
///   最後の NAL より後の `trailing_zero_8bits` は境界の詰め物として捨て、
///   NAL 本体に含めない (NAL 本体は ITU-T H.264 Annex B B.2 どおり後続の
///   バイトアラインされた `0x000000` / `0x000001` の直前まで)
/// - 最初の開始コードより前に非ゼロバイトがある場合は [`crate::Error`]
///   (詰め物でも NAL 本体でもないデータを黙って捨てない)
/// - 開始コードの直後に次の開始コードまたは入力終端が来る空 NAL は [`crate::Error`]
/// - 3 バイトと 4 バイトの開始コードの混在を受理する
pub(crate) fn scan_annexb_nals(input: &[u8]) -> Result<Vec<&[u8]>> {
    let mut nals = Vec::new();
    if input.is_empty() {
        return Ok(nals);
    }

    // 最初の開始コードを探す。見つからない場合は入力全体がどの NAL にも属さない
    let Some((first_start, first_len)) = find_start_code(input, 0) else {
        return Err(Error::invalid_input("Annex B input has no start code"));
    };

    // 最初の開始コードより前は leading_zero_8bits の詰め物にできるゼロだけを許す
    if input[..first_start].iter().any(|b| *b != 0) {
        return Err(Error::invalid_input(
            "Annex B input has non-zero bytes before the first start code",
        ));
    }

    let mut cursor = first_start + first_len;
    loop {
        match find_start_code(input, cursor) {
            Some((next_start, next_len)) => {
                // 直前の NAL 本体は次の開始コードの直前まで
                if next_start == cursor {
                    return Err(Error::invalid_input("Annex B input has an empty NAL unit"));
                }
                // NAL 本体の末尾ゼロは次の開始コードのパディング
                // (trailing_zero_8bits / leading_zero_8bits) として含めない。
                // 仕様 (ITU-T H.264 Annex B B.2) では NAL 本体は後続の
                // バイトアラインされた 0x000000 / 0x000001 の直前までであり、
                // 正しい EBSP は内部に 0x000000 / 0x000001 を含まないため
                // 末尾ゼロの除去で仕様と一致する
                let mut end = next_start;
                while end > cursor && input[end - 1] == 0 {
                    end -= 1;
                }
                if end == cursor {
                    return Err(Error::invalid_input("Annex B input has an empty NAL unit"));
                }
                nals.push(&input[cursor..end]);
                cursor = next_start + next_len;
            }
            None => {
                // 最後の NAL の後は trailing_zero_8bits の詰め物を除く。
                // 正しい EBSP の NAL は rbsp_stop_one_bit で終わるため
                // 本体が末尾ゼロで終わることはない (ITU-T H.264 7.4.1)
                let mut end = input.len();
                while end > cursor && input[end - 1] == 0 {
                    end -= 1;
                }
                if end == cursor {
                    return Err(Error::invalid_input("Annex B input has an empty NAL unit"));
                }
                nals.push(&input[cursor..end]);
                break;
            }
        }
    }
    Ok(nals)
}

/// length-prefixed 形式 (ISO/IEC 14496-15) の NAL ユニット列を走査する
///
/// 返す NAL 本体は長さプレフィックスを含まないバイト列の借用。
///
/// # 契約
///
/// - 空入力は NAL ユニット 0 個の成功
/// - 長さフィールドが入力末尾を超える、宣言長が残バイトを超える、
///   宣言長が 0 の NAL は [`crate::Error`]
pub(crate) fn scan_length_prefixed_nals(
    input: &[u8],
    length_size: LengthSize,
) -> Result<Vec<&[u8]>> {
    let length_size_bytes = length_size.bytes();

    let mut nals = Vec::new();
    let mut cursor = 0;
    while cursor < input.len() {
        // 長さフィールドが入力末尾を超える場合は切り詰めとして Error
        if input.len() - cursor < length_size_bytes {
            return Err(Error::invalid_input(
                "length field exceeds the end of the input",
            ));
        }
        let len = read_length_field(&input[cursor..], length_size) as usize;
        cursor += length_size_bytes;
        // 宣言長 0 の NAL は黙って読み飛ばさず Error
        if len == 0 {
            return Err(Error::invalid_input("NAL unit with length 0"));
        }
        // 宣言長が残バイトを超える場合は切り詰めとして Error
        if len > input.len() - cursor {
            return Err(Error::invalid_input(
                "declared NAL length exceeds the remaining input",
            ));
        }
        nals.push(&input[cursor..cursor + len]);
        cursor += len;
    }
    Ok(nals)
}

/// Annex B の NAL ユニット列を length-prefixed 形式へ変換する
///
/// 各 NAL 本体の前に指定幅の長さフィールドを付ける。フレーミングのみを行い、
/// NAL ヘッダーの解釈はしない。
///
/// # 契約
///
/// - 入力の走査は [`scan_annexb_nals`] と同じ契約に従う
/// - NAL 本体が長さフィールド幅に収まらない場合は [`crate::Error`]
///   (黙った切り詰めはしない)
pub(crate) fn annexb_to_length_prefixed(input: &[u8], length_size: LengthSize) -> Result<Vec<u8>> {
    let nals = scan_annexb_nals(input)?;

    let mut out = Vec::new();
    for nal in nals {
        if !length_fits(nal.len(), length_size) {
            return Err(Error::invalid_input(
                "NAL unit is too long for the length field",
            ));
        }
        write_length_field(&mut out, nal.len(), length_size);
        out.extend_from_slice(nal);
    }
    Ok(out)
}

/// length-prefixed 形式の NAL ユニット列を Annex B へ変換する
///
/// 各 NAL 本体の前に 4 バイト開始コード (`0x00000001`) を付ける。
/// フレーミングのみを行い、NAL ヘッダーの解釈はしない。
///
/// # 契約
///
/// - 入力の走査は [`scan_length_prefixed_nals`] と同じ契約に従う
pub(crate) fn length_prefixed_to_annexb(input: &[u8], length_size: LengthSize) -> Result<Vec<u8>> {
    let nals = scan_length_prefixed_nals(input, length_size)?;

    let mut out = Vec::new();
    for nal in nals {
        out.extend_from_slice(&START_CODE_PREFIX_ONE_4BYTES);
        out.extend_from_slice(nal);
    }
    Ok(out)
}
