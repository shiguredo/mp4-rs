use alloc::{string::String, vec, vec::Vec};
use core::{
    num::{NonZeroU16, NonZeroU32},
    panic::Location,
};

use crate::BoxType;

/// このライブラリ用の Result 型
pub type Result<T> = core::result::Result<T, Error>;

/// エンコード/デコード操作のエラーの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// 入力データの形式または構造が無効である
    InvalidInput,

    /// データコンテンツが無効または破損している
    InvalidData,

    /// 提供されたバッファがエンコード/デコード結果を保持するのに小さすぎる
    InsufficientBuffer,

    /// 操作またはデータ形式がサポートされていない
    Unsupported,
}

/// エラー型
#[derive(Clone)]
pub struct Error {
    /// 発生したエラーの種類
    pub kind: ErrorKind,

    /// エラーが発生した理由
    pub reason: String,

    /// エラーが作成されたソースコードの場所
    pub location: &'static Location<'static>,

    /// エラーが発生した MP4 ボックスの種類
    pub box_type: Option<BoxType>,
}

impl Error {
    /// [`Error`] インスタンスを生成する
    #[track_caller]
    pub fn new(kind: ErrorKind) -> Self {
        Self::with_reason(kind, String::new())
    }

    /// エラー理由つきで [`Error`] インスタンスを生成する
    #[track_caller]
    pub fn with_reason<T: Into<String>>(kind: ErrorKind, reason: T) -> Self {
        Self {
            kind,
            reason: reason.into(),
            location: Location::caller(),
            box_type: None,
        }
    }

    #[track_caller]
    pub(crate) fn unsupported<T: Into<String>>(reason: T) -> Self {
        Self::with_reason(ErrorKind::Unsupported, reason)
    }

    #[track_caller]
    pub(crate) fn invalid_input<T: Into<String>>(reason: T) -> Self {
        Self::with_reason(ErrorKind::InvalidInput, reason)
    }

    #[track_caller]
    pub(crate) fn invalid_data<T: Into<String>>(reason: T) -> Self {
        Self::with_reason(ErrorKind::InvalidData, reason)
    }

    #[track_caller]
    pub(crate) fn insufficient_buffer() -> Self {
        Self::new(ErrorKind::InsufficientBuffer)
    }

    #[track_caller]
    pub(crate) fn check_buffer_size(required_size: usize, buf: &[u8]) -> Result<()> {
        if buf.len() < required_size {
            Err(Self::insufficient_buffer())
        } else {
            Ok(())
        }
    }
}

impl core::fmt::Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self}")
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(ty) = self.box_type {
            write!(f, "[{ty}] ")?;
        }

        if self.reason.is_empty() {
            write!(f, "{:?}", self.kind)?;
        } else {
            write!(f, "{:?}: {}", self.kind, self.reason)?;
        }

        write!(f, " (at {}:{})", self.location.file(), self.location.line())?;

        Ok(())
    }
}

impl core::error::Error for Error {}

/// バイト列に変換可能な型を表現するためのトレイト
pub trait Encode {
    /// `self` をバイト列に変換して `buf` に書きこむ
    ///
    /// 返り値は、変換後のバイト列のサイズで、
    /// もし `buf` のサイズが不足している場合には [`ErrorKind::InsufficientBuffer`] エラーが返される
    fn encode(&self, buf: &mut [u8]) -> Result<usize>;

    /// `self` をバイト列に変換して、変換後のバイト列を返す
    fn encode_to_vec(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0; 256];
        loop {
            match self.encode(&mut buf) {
                Ok(size) => {
                    buf.truncate(size);
                    return Ok(buf);
                }
                Err(e) if e.kind == ErrorKind::InsufficientBuffer => {
                    let new_size = buf.len().checked_mul(2).ok_or(e)?;
                    buf.resize(new_size, 0);
                }
                Err(e) => return Err(e),
            }
        }
    }
}

impl Encode for u8 {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        Error::check_buffer_size(1, buf)?;
        buf[0] = *self;
        Ok(1)
    }
}

impl Encode for u16 {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        Error::check_buffer_size(2, buf)?;
        buf[..2].copy_from_slice(&self.to_be_bytes());
        Ok(2)
    }
}

impl Encode for u32 {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        Error::check_buffer_size(4, buf)?;
        buf[..4].copy_from_slice(&self.to_be_bytes());
        Ok(4)
    }
}

impl Encode for u64 {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        Error::check_buffer_size(8, buf)?;
        buf[..8].copy_from_slice(&self.to_be_bytes());
        Ok(8)
    }
}

impl Encode for i8 {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        Error::check_buffer_size(1, buf)?;
        buf[0] = *self as u8;
        Ok(1)
    }
}

impl Encode for i16 {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        Error::check_buffer_size(2, buf)?;
        buf[..2].copy_from_slice(&self.to_be_bytes());
        Ok(2)
    }
}

impl Encode for i32 {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        Error::check_buffer_size(4, buf)?;
        buf[..4].copy_from_slice(&self.to_be_bytes());
        Ok(4)
    }
}

impl Encode for i64 {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        Error::check_buffer_size(8, buf)?;
        buf[..8].copy_from_slice(&self.to_be_bytes());
        Ok(8)
    }
}

impl Encode for NonZeroU16 {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        self.get().encode(buf)
    }
}

impl Encode for NonZeroU32 {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        self.get().encode(buf)
    }
}

impl<T: Encode, const N: usize> Encode for [T; N] {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = 0;
        for item in self {
            offset += item.encode(&mut buf[offset..])?;
        }
        Ok(offset)
    }
}

impl Encode for [u8] {
    #[track_caller]
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        Error::check_buffer_size(self.len(), buf)?;
        buf[..self.len()].copy_from_slice(self);
        Ok(self.len())
    }
}

/// バイト列から `Self` に変換するためのトレイト
pub trait Decode: Sized {
    /// バイト列からこの型の値をデコードする
    ///
    /// 成功時には、デコードされた値とデコードに消費されたバイト数のタプルが、
    /// 失敗時には [`Error`] が返される
    fn decode(buf: &[u8]) -> Result<(Self, usize)>;

    /// オフセット位置からバイト列をデコードし、オフセットを自動で進める
    ///
    /// なお、デコードが失敗した場合はオフセットの更新は行われない
    #[track_caller]
    fn decode_at(buf: &[u8], offset: &mut usize) -> Result<Self> {
        let (decoded, size) = Self::decode(&buf[*offset..])?;
        *offset += size;
        Ok(decoded)
    }
}

impl Decode for u8 {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        Error::check_buffer_size(1, buf)?;
        Ok((buf[0], 1))
    }
}

impl Decode for u16 {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        Error::check_buffer_size(2, buf)?;
        Ok((Self::from_be_bytes([buf[0], buf[1]]), 2))
    }
}

impl Decode for u32 {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        Error::check_buffer_size(4, buf)?;
        Ok((Self::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]), 4))
    }
}

impl Decode for u64 {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        Error::check_buffer_size(8, buf)?;
        let bytes = [
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ];
        Ok((Self::from_be_bytes(bytes), 8))
    }
}

impl Decode for i8 {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        Error::check_buffer_size(1, buf)?;
        Ok((buf[0] as i8, 1))
    }
}

impl Decode for i16 {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        Error::check_buffer_size(2, buf)?;
        Ok((Self::from_be_bytes([buf[0], buf[1]]), 2))
    }
}

impl Decode for i32 {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        Error::check_buffer_size(4, buf)?;
        Ok((Self::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]), 4))
    }
}

impl Decode for i64 {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        Error::check_buffer_size(8, buf)?;
        let bytes = [
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ];
        Ok((Self::from_be_bytes(bytes), 8))
    }
}

impl Decode for NonZeroU16 {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (v, size) = u16::decode(buf)?;
        NonZeroU16::new(v)
            .map(|nz| (nz, size))
            .ok_or_else(|| Error::invalid_input("Expected a non-zero integer, but got 0"))
    }
}

impl Decode for NonZeroU32 {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (v, size) = u32::decode(buf)?;
        NonZeroU32::new(v)
            .map(|nz| (nz, size))
            .ok_or_else(|| Error::invalid_input("Expected a non-zero integer, but got 0"))
    }
}

impl<T: Decode + Default + Copy, const N: usize> Decode for [T; N] {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let mut items = [T::default(); N];
        let mut offset = 0;

        for item in &mut items {
            *item = T::decode_at(buf, &mut offset)?;
        }

        Ok((items, offset))
    }
}
