use alloc::{borrow::ToOwned, boxed::Box, format, string::String, vec::Vec};
use core::{
    ops::{BitAnd, Shl, Shr, Sub},
    time::Duration,
};

use crate::{
    Decode, Encode, Error, Result,
    boxes::{FtypBox, RootBox},
};

/// 全てのボックスが実装するトレイト
///
/// 本来なら `Box` という名前が適切だが、それだと標準ライブラリの [`alloc::boxed::Box`] と名前が
/// 衝突してしまうので、それを避けるために `BaseBox` としている
pub trait BaseBox {
    /// ボックスの種別
    fn box_type(&self) -> BoxType;

    /// 未知のボックスかどうか
    ///
    /// 基本的には `false` を返すデフォルト実装のままで問題ないが、
    /// [`UnknownBox`][crate::boxes::UnknownBox] を含む `enum` を定義する場合には、独自の実装が必要となる
    fn is_unknown_box(&self) -> bool {
        false
    }

    /// 子ボックスを走査するイテレーターを返す
    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>>;
}

pub(crate) fn as_box_object<T: BaseBox>(t: &T) -> &dyn BaseBox {
    t
}

/// フルボックスを表すトレイト
pub trait FullBox: BaseBox {
    /// フルボックスのバージョンを返す
    fn full_box_version(&self) -> u8;

    /// フルボックスのフラグを返す
    fn full_box_flags(&self) -> FullBoxFlags;
}

/// MP4 ファイルを表す構造体
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mp4File<B = RootBox> {
    /// MP4 ファイルの先頭に位置する `ftyp` ボックス
    pub ftyp_box: FtypBox,

    /// `ftyp` に続くボックス群
    pub boxes: Vec<B>,
}

impl<B: BaseBox> Mp4File<B> {
    /// ファイル内のトップレベルのボックス群を走査するイテレーターを返す
    pub fn iter(&self) -> impl Iterator<Item = &dyn BaseBox> {
        core::iter::empty()
            .chain(core::iter::once(&self.ftyp_box).map(as_box_object))
            .chain(self.boxes.iter().map(as_box_object))
    }
}

impl<B: BaseBox + Decode> Decode for Mp4File<B> {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let mut offset = 0;

        let ftyp_box = FtypBox::decode_at(buf, &mut offset)?;

        let mut boxes = Vec::new();
        while offset < buf.len() {
            boxes.push(B::decode_at(buf, &mut offset)?);
        }

        Ok((Self { ftyp_box, boxes }, offset))
    }
}

impl<B: BaseBox + Encode> Encode for Mp4File<B> {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = 0;
        offset += self.ftyp_box.encode(&mut buf[offset..])?;
        for b in &self.boxes {
            offset += b.encode(&mut buf[offset..])?;
        }
        Ok(offset)
    }
}

/// [`BaseBox`] に共通のヘッダー
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoxHeader {
    /// ボックスの種別
    pub box_type: BoxType,

    /// ボックスのサイズ
    pub box_size: BoxSize,
}

impl BoxHeader {
    /// ボックスヘッダーの最小サイズ（バイト数）
    ///
    /// サイズフィールド（4バイト）とボックス種別フィールド（4バイト）で構成される
    pub const MIN_SIZE: usize = 8;

    /// ボックスヘッダーの最大サイズ（バイト数）
    ///
    /// サイズフィールド（4バイト）+ 拡張サイズ（8バイト）+ ボックス種別（4バイト）+ UUID（16バイト）で構成される
    pub const MAX_SIZE: usize = 4 + 8 + 4 + 16;

    pub(crate) const fn new(box_type: BoxType, box_size: BoxSize) -> Self {
        Self { box_type, box_size }
    }

    pub(crate) const fn new_variable_size(box_type: BoxType) -> Self {
        Self::new(box_type, BoxSize::VARIABLE_SIZE)
    }

    pub(crate) fn finalize_box_size(mut self, box_bytes: &mut [u8]) -> Result<()> {
        if self.box_size != BoxSize::VARIABLE_SIZE {
            return Err(Error::invalid_input(
                "box_size must be VARIABLE_SIZE before finalization",
            ));
        }

        // NOTE: もし `box_bytes.len() < self.external_size()` の場合は後続の `self.encode()` でエラーになる
        let payload_size = box_bytes.len().saturating_sub(self.external_size());
        self.box_size = BoxSize::with_payload_size(self.box_type, payload_size as u64);
        if !matches!(self.box_size, BoxSize::U32(_)) {
            // ヘッダーのサイズに変更があると box_bytes 全体のレイアウトが変わってしまうのでエラーにする
            return Err(Error::invalid_input(
                "box payload too large: resulting box size exceeds U32 boundary (4GB), cannot update in-place",
            ));
        }

        // 正しいサイズでヘッダー部分を上書きする
        self.encode(box_bytes)?;

        Ok(())
    }

    /// ヘッダーをエンコードした際のバイト数を返す
    pub fn external_size(self) -> usize {
        self.box_type.external_size() + self.box_size.external_size()
    }

    /// ボックスヘッダーをデコードし、ヘッダーとペイロードスライスを返す
    ///
    /// バッファからボックスヘッダーを読み込み、対応するペイロード部分を抽出する。
    ///
    /// # ボックスサイズ 0 の扱いについて
    ///
    /// MP4 の仕様では、ボックスサイズが 0 の場合は「ファイルの最後まで」を意味するが、
    /// 本実装では、渡されたバッファ全体をペイロードとして扱う。
    ///
    /// この挙動には以下の条件を前提がある：
    /// - `buf` がファイル末尾を含む完全なデータである
    /// - ストリーミングやチャンク読み込みには非対応
    ///
    /// ストリーミング対応が必要な場合は、呼び出し側で別途サイズ管理が必要となる。
    pub fn decode_header_and_payload(buf: &[u8]) -> Result<(Self, &[u8])> {
        let (header, header_size) = Self::decode(buf)?;

        let mut box_size = usize::try_from(header.box_size.get())
            .map_err(|_| Error::invalid_data("too large box size"))?;
        if box_size < header_size {
            return Err(Error::invalid_data("box size is smaller than header size"));
        }
        Error::check_buffer_size(box_size, buf)?;

        // サイズが0の場合は、バッファ全体を使用する（ファイル末尾の可変長ボックスと想定）
        if box_size == 0 {
            box_size = buf.len();
        }

        Ok((header, &buf[header_size..box_size]))
    }
}

impl Encode for BoxHeader {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = 0;

        let large_size = match self.box_size {
            BoxSize::U32(size) => {
                offset += size.encode(&mut buf[offset..])?;
                None
            }
            BoxSize::U64(size) => {
                offset += 1u32.encode(&mut buf[offset..])?;
                Some(size)
            }
        };

        match self.box_type {
            BoxType::Normal(ty) => {
                offset += ty.encode(&mut buf[offset..])?;
            }
            BoxType::Uuid(ty) => {
                offset += b"uuid".encode(&mut buf[offset..])?;
                offset += ty.encode(&mut buf[offset..])?;
            }
        }

        if let Some(large_size) = large_size {
            offset += large_size.encode(&mut buf[offset..])?;
        }

        Ok(offset)
    }
}

impl Decode for BoxHeader {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let mut offset = 0;

        let box_size = u32::decode_at(buf, &mut offset)?;
        Error::check_buffer_size(offset + 4, buf)?;

        let mut box_type = [0; 4];
        box_type.copy_from_slice(&buf[offset..offset + 4]);
        offset += 4;

        let box_type = if box_type == [b'u', b'u', b'i', b'd'] {
            Error::check_buffer_size(offset + 16, buf)?;

            let mut uuid = [0; 16];
            uuid.copy_from_slice(&buf[offset..offset + 16]);
            offset += 16;
            BoxType::Uuid(uuid)
        } else {
            BoxType::Normal(box_type)
        };

        let box_size = if box_size == 1 {
            let size = u64::decode_at(buf, &mut offset)?;
            BoxSize::U64(size)
        } else {
            BoxSize::U32(box_size)
        };

        if box_size.get() != 0
            && box_size.get() < (box_size.external_size() + box_type.external_size()) as u64
        {
            return Err(Error::invalid_input(format!(
                "Too small box size: actual={}, expected={} or more",
                box_size.get(),
                box_size.external_size() + box_type.external_size()
            )));
        }

        Ok((Self { box_type, box_size }, offset))
    }
}

/// [`FullBox`] に共通のヘッダー
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FullBoxHeader {
    /// バージョン
    pub version: u8,

    /// フラグ
    pub flags: FullBoxFlags,
}

impl FullBoxHeader {
    /// フルボックスへの参照を受け取って、対応するヘッダーを作成する
    pub fn from_box<B: FullBox>(b: &B) -> Self {
        Self {
            version: b.full_box_version(),
            flags: b.full_box_flags(),
        }
    }
}

impl Encode for FullBoxHeader {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = 0;
        offset += self.version.encode(&mut buf[offset..])?;
        offset += self.flags.encode(&mut buf[offset..])?;
        Ok(offset)
    }
}

impl Decode for FullBoxHeader {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let mut offset = 0;
        let version = u8::decode_at(buf, &mut offset)?;
        let flags = FullBoxFlags::decode_at(buf, &mut offset)?;
        Ok((Self { version, flags }, offset))
    }
}

/// [`FullBox`] のヘッダー部分に含まれるビットフラグ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FullBoxFlags(u32);

impl FullBoxFlags {
    /// 空のビットフラグを作成する
    pub const fn empty() -> Self {
        Self(0)
    }

    /// [`u32`] を受け取って、対応するビットフラグを作成する
    pub const fn new(flags: u32) -> Self {
        Self(flags)
    }

    /// `(ビット位置、フラグがセットされているかどうか)` のイテレーターを受け取って、対応するビットフラグを作成する
    pub fn from_flags<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (usize, bool)>,
    {
        let flags = iter.into_iter().filter(|x| x.1).map(|x| 1 << x.0).sum();
        Self(flags)
    }

    /// このビットフラグに対応する [`u32`] 値を返す
    pub const fn get(self) -> u32 {
        self.0
    }

    /// 指定されたビット位置のフラグがセットされているかどうかを判定する
    pub const fn is_set(self, i: usize) -> bool {
        (self.0 & (1 << i)) != 0
    }
}

impl Encode for FullBoxFlags {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        self.0.to_be_bytes()[1..].encode(buf)
    }
}

impl Decode for FullBoxFlags {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        Error::check_buffer_size(3, buf)?;
        let mut full_buf = [0; 4];
        full_buf[1..].copy_from_slice(&buf[..3]);
        Ok((Self(u32::from_be_bytes(full_buf)), 3))
    }
}

/// [`BaseBox`] のサイズ
///
/// ボックスのサイズは原則として、ヘッダー部分とペイロード部分のサイズを足した値となる。
/// ただし、MP4 ファイルの末尾にあるボックスについてはサイズを 0 とすることで、ペイロードが可変長（追記可能）なボックスとして扱うことが可能となっている。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum BoxSize {
    U32(u32),
    U64(u64),
}

impl BoxSize {
    /// ファイル末尾に位置する可変長のボックスを表すための特別な値
    pub const VARIABLE_SIZE: Self = Self::U32(0);

    /// ファイル末尾に位置する可変長のボックスを表すための特別な値
    ///
    /// 基本的には [`BoxSize::VARIABLE_SIZE`] と同じだが、4GB を超えるボックスに備えて、こちらは 64 ビットサイズエンコーディングを使用する
    pub const LARGE_VARIABLE_SIZE: Self = Self::U64(0);

    /// ボックス種別とペイロードサイズを受け取って、対応する [`BoxSize`] インスタンスを作成する
    pub fn with_payload_size(box_type: BoxType, payload_size: u64) -> Self {
        let mut size = 4 + box_type.external_size() as u64 + payload_size;
        if let Ok(size) = u32::try_from(size) {
            Self::U32(size)
        } else {
            size += 8; // もともとのサイズフィールドには 1 が設定されて、ヘッダーの末尾に 8 バイトのサイズが格納される
            Self::U64(size)
        }
    }

    /// ボックスのサイズの値を取得する
    pub const fn get(self) -> u64 {
        match self {
            BoxSize::U32(v) => v as u64,
            BoxSize::U64(v) => v,
        }
    }

    /// [`BoxHeader`] 内のサイズフィールドをエンコードする際に必要となるバイト数を返す
    pub const fn external_size(self) -> usize {
        match self {
            BoxSize::U32(_) => 4,
            BoxSize::U64(_) => 4 + 8,
        }
    }
}

/// [`BaseBox`] の種別
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BoxType {
    /// 四文字で表現される通常のボックス種別
    Normal([u8; 4]),

    /// UUID 形式のボックス種別
    Uuid([u8; 16]),
}

impl BoxType {
    /// 種別を表すバイト列を返す
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            BoxType::Normal(ty) => &ty[..],
            BoxType::Uuid(ty) => &ty[..],
        }
    }

    /// [`BoxHeader`] 内のボックス種別フィールドをエンコードする際に必要となるバイト数を返す
    pub const fn external_size(self) -> usize {
        if matches!(self, Self::Normal(_)) {
            4
        } else {
            4 + 16
        }
    }

    /// 自分が `expected` と同じ種別であるかをチェックする
    pub fn expect(self, expected: Self) -> Result<()> {
        if self == expected {
            Ok(())
        } else {
            Err(Error::invalid_data(format!(
                "Expected box type `{expected}`, but got `{self}`"
            )))
        }
    }
}

impl core::fmt::Debug for BoxType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BoxType::Normal(ty) => {
                if let Ok(ty) = core::str::from_utf8(ty) {
                    f.debug_tuple("BoxType").field(&ty).finish()
                } else {
                    f.debug_tuple("BoxType").field(ty).finish()
                }
            }
            BoxType::Uuid(ty) => f.debug_tuple("BoxType").field(ty).finish(),
        }
    }
}

impl core::fmt::Display for BoxType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let BoxType::Normal(ty) = self
            && let Ok(ty) = core::str::from_utf8(&ty[..])
        {
            return write!(f, "{ty}");
        }
        write!(f, "{:?}", self.as_bytes())
    }
}

/// MP4 ファイル内で使われる時刻形式（1904/1/1 からの経過秒数）
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mp4FileTime(u64);

impl Mp4FileTime {
    /// 1904/1/1 からの経過秒数を引数にとって [`Mp4FileTime`] インスタンスを作成する
    pub const fn from_secs(secs: u64) -> Self {
        Self(secs)
    }

    /// 1904/1/1 からの経過秒数を返す
    pub const fn as_secs(self) -> u64 {
        self.0
    }

    /// UNIX EPOCH (1970-01-01 00:00:00 UTC) を起点とした経過時間を受け取って、対応する [`Mp4FileTime`] インスタンスを作成する
    pub const fn from_unix_time(unix_time: Duration) -> Self {
        let delta = 2082844800; // 1904/1/1 から 1970/1/1 までの経過秒数
        let unix_time_secs = unix_time.as_secs();
        Self::from_secs(unix_time_secs + delta)
    }
}

/// 固定小数点数
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FixedPointNumber<I, F = I> {
    /// 整数部
    pub integer: I,

    /// 小数部
    pub fraction: F,
}

impl<I, F> FixedPointNumber<I, F> {
    /// 整数部と小数部を受け取って固定小数点数を返す
    pub const fn new(integer: I, fraction: F) -> Self {
        Self { integer, fraction }
    }
}

impl<I: Encode, F: Encode> Encode for FixedPointNumber<I, F> {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = 0;
        offset += self.integer.encode(&mut buf[offset..])?;
        offset += self.fraction.encode(&mut buf[offset..])?;
        Ok(offset)
    }
}

impl<I: Decode, F: Decode> Decode for FixedPointNumber<I, F> {
    #[track_caller]
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let mut offset = 0;
        let integer = I::decode_at(buf, &mut offset)?;
        let fraction = F::decode_at(buf, &mut offset)?;
        Ok((Self { integer, fraction }, offset))
    }
}

/// null 終端の UTF-8 文字列
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Utf8String(String);

impl Utf8String {
    /// 空文字列
    pub const EMPTY: Self = Utf8String(String::new());

    /// 終端の null を含まない文字列を受け取って [`Utf8String`] インスタンスを作成する
    ///
    /// 引数の文字列内の null 文字が含まれている場合には [`None`] が返される
    pub fn new(s: &str) -> Option<Self> {
        if s.as_bytes().contains(&0) {
            return None;
        }
        Some(Self(s.to_owned()))
    }

    /// このインスタンスが保持する、null 終端部分を含まない文字列を返す
    pub fn get(&self) -> &str {
        &self.0
    }

    /// このインスタンスを、null 終端部分を含むバイト列へと変換する
    pub fn into_null_terminated_bytes(self) -> Vec<u8> {
        let mut v = self.0.into_bytes();
        v.push(0);
        v
    }
}

impl Encode for Utf8String {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut offset = 0;
        offset += self.0.as_bytes().encode(&mut buf[offset..])?;
        offset += 0u8.encode(&mut buf[offset..])?;
        Ok(offset)
    }
}

impl Decode for Utf8String {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let mut offset = 0;
        let mut bytes = Vec::new();

        while offset < buf.len() {
            if buf[offset] == 0 {
                offset += 1;
                break;
            }
            bytes.push(buf[offset]);
            offset += 1;
        }

        if offset == 0 || (offset > 0 && buf[offset - 1] != 0) {
            return Err(Error::invalid_input("Null-terminated string not found"));
        }

        let s = String::from_utf8(bytes).map_err(|e| {
            Error::invalid_input(format!("Invalid UTF-8 string: {:?}", e.as_bytes()))
        })?;

        Ok((Self(s), offset))
    }
}

/// `A` か `B` のどちらかの値を保持する列挙型
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum Either<A, B> {
    A(A),
    B(B),
}

impl<A: BaseBox, B: BaseBox> Either<A, B> {
    fn inner_box(&self) -> &dyn BaseBox {
        match self {
            Self::A(x) => x,
            Self::B(x) => x,
        }
    }
}

impl<A: BaseBox, B: BaseBox> BaseBox for Either<A, B> {
    fn box_type(&self) -> BoxType {
        self.inner_box().box_type()
    }

    fn is_unknown_box(&self) -> bool {
        self.inner_box().is_unknown_box()
    }

    fn children<'a>(&'a self) -> Box<dyn 'a + Iterator<Item = &'a dyn BaseBox>> {
        self.inner_box().children()
    }
}

/// 任意のビット数の非負の整数を表現するための型
///
/// - `T`: 数値の内部的な型。 最低限 `BITS` 分の数値を表現可能な型である必要がある。
/// - `BITS`: 数値のビット数
/// - `OFFSET`: 一つの `T` に複数の [`Uint`] 値がパックされる場合の、この数値のオフセット位置（ビット数）
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uint<T, const BITS: u32, const OFFSET: u32 = 0>(T);

impl<T, const BITS: u32, const OFFSET: u32> Uint<T, BITS, OFFSET> {
    /// 指定された数値を受け取ってインスタンスを作成する
    pub const fn new(v: T) -> Self {
        Self(v)
    }

    /// このインスタンスが表現する整数値を返す
    pub fn get(self) -> T {
        self.0
    }
}

impl<T, const BITS: u32, const OFFSET: u32> Uint<T, BITS, OFFSET>
where
    T: Shr<u32, Output = T>
        + Shl<u32, Output = T>
        + BitAnd<Output = T>
        + Sub<Output = T>
        + From<u8>,
{
    /// `T` が保持するビット列の `OFFSET` 位置から `BITS` 分のビット列に対応する整数値を返す
    pub fn from_bits(v: T) -> Self {
        Self((v >> OFFSET) & ((T::from(1) << BITS) - T::from(1)))
    }

    /// このインスタンスに対応する `T` 内のビット列を返す
    ///
    /// なお `OFFSET` が `0` の場合には、このメソッドは [`Uint::get()`] と等価である
    pub fn to_bits(self) -> T {
        self.0 << OFFSET
    }
}

impl<T, const OFFSET: u32> Uint<T, 1, OFFSET>
where
    T: From<u8> + Eq,
{
    /// このインスタンスの値を対応する boolean 値に変換する
    pub fn as_bool(self) -> bool {
        self.get() != T::from(0)
    }
}

impl<T, const OFFSET: u32> From<bool> for Uint<T, 1, OFFSET>
where
    T: From<bool>,
{
    fn from(value: bool) -> Self {
        Self::new(T::from(value))
    }
}

/// トラックの種類を表す列挙型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackKind {
    /// 音声トラック
    Audio,

    /// 映像トラック
    Video,
}

/// [ISO/IEC 14496-12] Sample Flags
///
/// fMP4 で使用されるサンプルフラグ（32 ビット）。
/// TrexBox, TfhdBox, TrunBox で使用される。
///
/// ビットレイアウト:
/// - ビット 31-28: reserved (4 bits)
/// - ビット 27-26: is_leading (2 bits)
/// - ビット 25-24: sample_depends_on (2 bits)
/// - ビット 23-22: sample_is_depended_on (2 bits)
/// - ビット 21-20: sample_has_redundancy (2 bits)
/// - ビット 19-17: sample_padding_value (3 bits)
/// - ビット 16: sample_is_non_sync_sample (1 bit)
/// - ビット 15-0: sample_degradation_priority (16 bits)
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleFlags(u32);

impl SampleFlags {
    /// 空のサンプルフラグを作成する
    pub const fn empty() -> Self {
        Self(0)
    }

    /// [`u32`] を受け取って、対応するサンプルフラグを作成する
    pub const fn new(flags: u32) -> Self {
        Self(flags)
    }

    /// このサンプルフラグに対応する [`u32`] 値を返す
    pub const fn get(self) -> u32 {
        self.0
    }

    /// is_leading フィールドを取得する（2 bits、ビット 27-26）
    pub const fn is_leading(self) -> u8 {
        ((self.0 >> 26) & 0b11) as u8
    }

    /// sample_depends_on フィールドを取得する（2 bits、ビット 25-24）
    pub const fn sample_depends_on(self) -> u8 {
        ((self.0 >> 24) & 0b11) as u8
    }

    /// sample_is_depended_on フィールドを取得する（2 bits、ビット 23-22）
    pub const fn sample_is_depended_on(self) -> u8 {
        ((self.0 >> 22) & 0b11) as u8
    }

    /// sample_has_redundancy フィールドを取得する（2 bits、ビット 21-20）
    pub const fn sample_has_redundancy(self) -> u8 {
        ((self.0 >> 20) & 0b11) as u8
    }

    /// sample_padding_value フィールドを取得する（3 bits、ビット 19-17）
    pub const fn sample_padding_value(self) -> u8 {
        ((self.0 >> 17) & 0b111) as u8
    }

    /// sample_is_non_sync_sample フィールドを取得する（1 bit、ビット 16）
    pub const fn sample_is_non_sync_sample(self) -> bool {
        ((self.0 >> 16) & 1) != 0
    }

    /// sample_degradation_priority フィールドを取得する（16 bits、ビット 15-0）
    pub const fn sample_degradation_priority(self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    /// 各フィールドからサンプルフラグを構築する
    pub const fn from_fields(
        is_leading: u8,
        sample_depends_on: u8,
        sample_is_depended_on: u8,
        sample_has_redundancy: u8,
        sample_padding_value: u8,
        sample_is_non_sync_sample: bool,
        sample_degradation_priority: u16,
    ) -> Self {
        let flags = ((is_leading as u32 & 0b11) << 26)
            | ((sample_depends_on as u32 & 0b11) << 24)
            | ((sample_is_depended_on as u32 & 0b11) << 22)
            | ((sample_has_redundancy as u32 & 0b11) << 20)
            | ((sample_padding_value as u32 & 0b111) << 17)
            | ((sample_is_non_sync_sample as u32) << 16)
            | (sample_degradation_priority as u32);
        Self(flags)
    }
}

impl Encode for SampleFlags {
    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        self.0.encode(buf)
    }
}

impl Decode for SampleFlags {
    fn decode(buf: &[u8]) -> Result<(Self, usize)> {
        let (flags, size) = u32::decode(buf)?;
        Ok((Self(flags), size))
    }
}
