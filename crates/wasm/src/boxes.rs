//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール

use std::alloc::Layout;

use c_api::boxes::{Mp4SampleEntry, Mp4SampleEntryKind};

/// wasm ABI 越しに扱う POD 型（`u16` / `u32` / 生ポインタ等）の配列を、
/// 要素型 `T` のアライメントで領域を確保して `data` をコピーする
///
/// `mp4_alloc`（align 1）経由では `u16` / `u32` / ポインタ配列として読めないため、
/// typed 配列の確保はこの関数（またはこれを使う公開ヘルパ）経由に限定する。
/// 解放は必ず同じ型引数 `T` の [`free_aligned`] を通すこと（layout 不整合で UB）
fn allocate_and_copy_aligned<T: Copy>(data: &[T]) -> (*const T, u32) {
    if data.is_empty() {
        return (std::ptr::null(), 0);
    }

    let byte_size = std::mem::size_of_val(data);
    // Rust の `align_of::<T>()` は必ず power-of-two、`&[T]` の byte 長は必ず `isize::MAX` 以下
    // なので Layout の構築は失敗しない
    let layout = Layout::from_size_align(byte_size, std::mem::align_of::<T>())
        .expect("Rust align is power-of-two and slice byte length fits isize::MAX");
    let allocated = unsafe { std::alloc::alloc(layout) };
    if allocated.is_null() {
        return (std::ptr::null(), 0);
    }
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr().cast::<u8>(), allocated, byte_size);
    }
    (allocated.cast::<T>(), data.len() as u32)
}

/// [`allocate_and_copy_aligned`] で確保した領域を、同じ align で解放する
///
/// # Safety
///
/// - `ptr` は [`allocate_and_copy_aligned`] を **同じ型引数 `T`** で呼んで得たポインタでなければならない
/// - `count` は確保時の要素数（[`allocate_and_copy_aligned`] の第 2 返り値）と一致していなければならない
/// - 同じ `ptr` に対して二重に呼んではならない
///
/// これらを満たさない場合、layout 不整合で UB になる
unsafe fn free_aligned<T>(ptr: *mut T, count: u32) {
    if ptr.is_null() || count == 0 {
        return;
    }
    let byte_size = count as usize * std::mem::size_of::<T>();
    // Rust の `align_of::<T>()` は必ず power-of-two、`byte_size` は確保時と同じ計算式なので
    // Layout の構築は失敗しない
    let layout = Layout::from_size_align(byte_size, std::mem::align_of::<T>())
        .expect("Rust align is power-of-two and byte_size matches allocation-time value");
    unsafe {
        std::alloc::dealloc(ptr.cast::<u8>(), layout);
    }
}

pub(crate) fn fmt_json_mp4_sample_entry(
    f: &mut nojson::JsonFormatter<'_, '_>,
    sample_entry: &Mp4SampleEntry,
) -> std::fmt::Result {
    match sample_entry.kind {
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_AVC1 => {
            let data = unsafe { &sample_entry.data.avc1 };
            crate::boxes_avc1::fmt_json_mp4_sample_entry_avc1(f, data)?;
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HEV1 => {
            let data = unsafe { &sample_entry.data.hev1 };
            crate::boxes_hev1::fmt_json_mp4_sample_entry_hev1(f, data)?;
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HVC1 => {
            let data = unsafe { &sample_entry.data.hvc1 };
            crate::boxes_hvc1::fmt_json_mp4_sample_entry_hvc1(f, data)?;
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_VP08 => {
            let data = unsafe { &sample_entry.data.vp08 };
            crate::boxes_vp08::fmt_json_mp4_sample_entry_vp08(f, data)?;
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_VP09 => {
            let data = unsafe { &sample_entry.data.vp09 };
            crate::boxes_vp09::fmt_json_mp4_sample_entry_vp09(f, data)?;
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_AV01 => {
            let data = unsafe { &sample_entry.data.av01 };
            crate::boxes_av01::fmt_json_mp4_sample_entry_av01(f, data)?;
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_OPUS => {
            let data = unsafe { &sample_entry.data.opus };
            crate::boxes_opus::fmt_json_mp4_sample_entry_opus(f, data)?;
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_MP4A => {
            let data = unsafe { &sample_entry.data.mp4a };
            crate::boxes_mp4a::fmt_json_mp4_sample_entry_mp4a(f, data)?;
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_FLAC => {
            let data = unsafe { &sample_entry.data.flac };
            crate::boxes_flac::fmt_json_mp4_sample_entry_flac(f, data)?;
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_STPP => {
            let data = unsafe { &sample_entry.data.stpp };
            crate::boxes_stpp::fmt_json_mp4_sample_entry_stpp(f, data)?;
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_WVTT => {
            let data = unsafe { &sample_entry.data.wvtt };
            crate::boxes_wvtt::fmt_json_mp4_sample_entry_wvtt(f, data)?;
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_TX3G => {
            let data = unsafe { &sample_entry.data.tx3g };
            crate::boxes_tx3g::fmt_json_mp4_sample_entry_tx3g(f, data)?;
        }
    }
    Ok(())
}

/// JSON から Mp4SampleEntry に変換する
pub fn parse_json_mp4_sample_entry(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntry, nojson::JsonParseError> {
    let kind_value = value.to_member("kind")?.required()?;
    let kind_str = kind_value.to_unquoted_string_str()?;

    match kind_str.as_ref() {
        "avc1" => {
            let avc1 = crate::boxes_avc1::parse_json_mp4_sample_entry_avc1(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_AVC1,
                data: c_api::boxes::Mp4SampleEntryData { avc1 },
            })
        }
        "hev1" => {
            let hev1 = crate::boxes_hev1::parse_json_mp4_sample_entry_hev1(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HEV1,
                data: c_api::boxes::Mp4SampleEntryData { hev1 },
            })
        }
        "hvc1" => {
            let hvc1 = crate::boxes_hvc1::parse_json_mp4_sample_entry_hvc1(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HVC1,
                data: c_api::boxes::Mp4SampleEntryData { hvc1 },
            })
        }
        "vp08" => {
            let vp08 = crate::boxes_vp08::parse_json_mp4_sample_entry_vp08(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_VP08,
                data: c_api::boxes::Mp4SampleEntryData { vp08 },
            })
        }
        "vp09" => {
            let vp09 = crate::boxes_vp09::parse_json_mp4_sample_entry_vp09(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_VP09,
                data: c_api::boxes::Mp4SampleEntryData { vp09 },
            })
        }
        "av01" => {
            let av01 = crate::boxes_av01::parse_json_mp4_sample_entry_av01(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_AV01,
                data: c_api::boxes::Mp4SampleEntryData { av01 },
            })
        }
        "opus" => {
            let opus = crate::boxes_opus::parse_json_mp4_sample_entry_opus(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_OPUS,
                data: c_api::boxes::Mp4SampleEntryData { opus },
            })
        }
        "mp4a" => {
            let mp4a = crate::boxes_mp4a::parse_json_mp4_sample_entry_mp4a(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_MP4A,
                data: c_api::boxes::Mp4SampleEntryData { mp4a },
            })
        }
        "flac" => {
            let flac = crate::boxes_flac::parse_json_mp4_sample_entry_flac(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_FLAC,
                data: c_api::boxes::Mp4SampleEntryData { flac },
            })
        }
        "stpp" => {
            let stpp = crate::boxes_stpp::parse_json_mp4_sample_entry_stpp(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_STPP,
                data: c_api::boxes::Mp4SampleEntryData { stpp },
            })
        }
        "wvtt" => {
            let wvtt = crate::boxes_wvtt::parse_json_mp4_sample_entry_wvtt(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_WVTT,
                data: c_api::boxes::Mp4SampleEntryData { wvtt },
            })
        }
        "tx3g" => {
            let tx3g = crate::boxes_tx3g::parse_json_mp4_sample_entry_tx3g(value)?;
            Ok(Mp4SampleEntry {
                kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_TX3G,
                data: c_api::boxes::Mp4SampleEntryData { tx3g },
            })
        }
        _ => Err(kind_value.invalid("unknown sample entry kind")),
    }
}

/// Mp4SampleEntry のメモリを解放する
pub unsafe fn mp4_sample_entry_free(sample_entry: *mut Mp4SampleEntry) {
    if sample_entry.is_null() {
        return;
    }

    let sample_entry = unsafe { &mut *sample_entry };

    match sample_entry.kind {
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_AVC1 => {
            let data = unsafe { &mut sample_entry.data.avc1 };
            crate::boxes_avc1::mp4_sample_entry_avc1_free(data);
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HEV1 => {
            let data = unsafe { &mut sample_entry.data.hev1 };
            crate::boxes_hev1::mp4_sample_entry_hev1_free(data);
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HVC1 => {
            let data = unsafe { &mut sample_entry.data.hvc1 };
            crate::boxes_hvc1::mp4_sample_entry_hvc1_free(data);
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_VP08 => {
            // VP08 はポインタフィールドがないため解放処理なし
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_VP09 => {
            // VP09 はポインタフィールドがないため解放処理なし
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_AV01 => {
            let data = unsafe { &mut sample_entry.data.av01 };
            crate::boxes_av01::mp4_sample_entry_av01_free(data);
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_OPUS => {
            // Opus はポインタフィールドがないため解放処理なし
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_MP4A => {
            let data = unsafe { &mut sample_entry.data.mp4a };
            crate::boxes_mp4a::mp4_sample_entry_mp4a_free(data);
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_FLAC => {
            let data = unsafe { &mut sample_entry.data.flac };
            crate::boxes_flac::mp4_sample_entry_flac_free(data);
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_STPP => {
            let data = unsafe { &mut sample_entry.data.stpp };
            crate::boxes_stpp::mp4_sample_entry_stpp_free(data);
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_WVTT => {
            let data = unsafe { &mut sample_entry.data.wvtt };
            crate::boxes_wvtt::mp4_sample_entry_wvtt_free(data);
        }
        Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_TX3G => {
            let data = unsafe { &mut sample_entry.data.tx3g };
            crate::boxes_tx3g::mp4_sample_entry_tx3g_free(data);
        }
    }

    // 構造体自体を解放
    let _ = unsafe { Box::from_raw(sample_entry) };
}

/// Mp4SampleEntry* の `*const u8 + u32` フィールドを `&str` に復元する共通ヘルパ
///
/// バイト列は必ず有効な UTF-8 でなければならない（invariant は書き出し側で保証される）。
/// invariant が壊れて UTF-8 不正なバイト列が渡された場合は実装バグとして panic する。
///
/// 第 1 引数 `_bound` は返り値 `&str` のライフタイムを借用に紐付けるためだけに存在し、
/// 関数本体では未使用。呼び出し側はバッファを所有する struct への借用を渡す
pub(crate) fn raw_bytes_as_str<T>(_bound: &T, data: *const u8, size: u32) -> &str {
    if size == 0 || data.is_null() {
        return "";
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, size as usize) };
    std::str::from_utf8(bytes).expect("Mp4SampleEntry field bytes must be valid UTF-8")
}

/// バイト配列を mp4_alloc で確保してコピーするユーティリティ関数
pub fn allocate_and_copy_bytes(data: &[u8]) -> (*const u8, u32) {
    if data.is_empty() {
        return (std::ptr::null(), 0);
    }

    let size = data.len() as u32;
    let ptr = unsafe {
        let allocated = crate::mp4_alloc(size);
        if allocated.is_null() {
            return (std::ptr::null(), 0);
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), allocated, data.len());
        allocated as *const u8
    };
    (ptr, size)
}

/// 複数のバイト列をメモリに割り当ててコピーする
///
/// JSON から複数の配列（SPS/PPS や NALU リストなど）を割り当てる際に使用する。
/// 各要素バイト列は `mp4_alloc`（u8 用途）で確保し、ポインタ配列とサイズ配列は
/// それぞれ `*const u8` / `u32` のアライメントで確保する
pub fn allocate_and_copy_array_list(arrays: &[Vec<u8>]) -> (*const *const u8, *const u32, u32) {
    let count = arrays.len() as u32;

    if count == 0 {
        return (std::ptr::null(), std::ptr::null(), 0);
    }

    // 各要素バイト列を確保し、そのポインタ列を align 付きで確保する
    let data_ptrs: Vec<*const u8> = arrays
        .iter()
        .map(|array| allocate_and_copy_bytes(array).0)
        .collect();
    let (data_ptr, _) = allocate_and_copy_aligned(&data_ptrs);

    // サイズ配列を u32 アライメントで確保する
    let sizes: Vec<u32> = arrays.iter().map(|array| array.len() as u32).collect();
    let (sizes_ptr, _) = allocate_and_copy_aligned(&sizes);

    (data_ptr, sizes_ptr, count)
}

/// u16 の 1 本の連続バッファを、u16 アライメントで確保してコピーする
///
/// 返り値は「バッファ先頭ポインタ」と「要素数」。
/// 解放は必ず `free_u16_array` を使う（`mp4_free` では align が合わない）
pub fn allocate_and_copy_u16_array(data: &[u16]) -> (*const u16, u32) {
    allocate_and_copy_aligned(data)
}

/// `allocate_and_copy_u16_array()` で割り当てられたメモリを解放する
///
/// # Safety
///
/// - `ptr` は [`allocate_and_copy_u16_array`] で得たポインタでなければならない（`mp4_free` などで解放しないこと）
/// - `count` は確保時の要素数と一致していなければならない
/// - 同じ `ptr` に対して二重に呼んではならない
pub unsafe fn free_u16_array(ptr: *mut u16, count: u32) {
    unsafe {
        free_aligned(ptr, count);
    }
}

/// u32 の 1 本の連続バッファを、u32 アライメントで確保してコピーする
///
/// 返り値は「バッファ先頭ポインタ」と「要素数」。
/// 解放は必ず `free_u32_array` を使う（`mp4_free` では align が合わない）
pub fn allocate_and_copy_u32_array(data: &[u32]) -> (*const u32, u32) {
    allocate_and_copy_aligned(data)
}

/// `allocate_and_copy_u32_array()` で割り当てられたメモリを解放する
///
/// # Safety
///
/// - `ptr` は [`allocate_and_copy_u32_array`] で得たポインタでなければならない（`mp4_free` などで解放しないこと）
/// - `count` は確保時の要素数と一致していなければならない
/// - 同じ `ptr` に対して二重に呼んではならない
pub unsafe fn free_u32_array(ptr: *mut u32, count: u32) {
    unsafe {
        free_aligned(ptr, count);
    }
}

/// `allocate_and_copy_array_list()` で割り当てられたメモリを解放する
///
/// 各要素バイト列は `mp4_free`、ポインタ配列とサイズ配列は確保時と同じ align で `dealloc` する。
///
/// # Safety
///
/// - `data_ptrs` / `sizes` は [`allocate_and_copy_array_list`] の返り値（同じ組）でなければならない
/// - `element_count` は「ポインタ配列と size 配列に共通するスロット数」（＝解放対象のバイト列本数）で、
///   [`allocate_and_copy_array_list`] の第 3 返り値と一致していなければならない。
///   NALU 配列の「外側の配列個数」ではなく「全バイト列の総数」であることに注意
/// - 同じ `data_ptrs` / `sizes` に対して二重に呼んではならない
pub unsafe fn free_array_list(data_ptrs: *mut *mut u8, sizes: *mut u32, element_count: u32) {
    if element_count == 0 {
        return;
    }

    // 各バイト列のメモリを解放
    if !data_ptrs.is_null() && !sizes.is_null() {
        let ptrs = unsafe { std::slice::from_raw_parts(data_ptrs, element_count as usize) };
        let size_list = unsafe { std::slice::from_raw_parts(sizes, element_count as usize) };

        // 各バイト列を実際のサイズで解放（u8 用途なので mp4_free）
        for (ptr, size) in ptrs.iter().zip(size_list.iter()) {
            if !ptr.is_null() {
                unsafe {
                    crate::mp4_free(*ptr, *size);
                }
            }
        }

        // ポインタ配列自体を解放（確保時の *const u8 アライメントと対にする）
        unsafe {
            free_aligned(data_ptrs.cast::<*const u8>(), element_count);
        }
    }

    // サイズ配列を解放（確保時の u32 アライメントと対にする）
    if !sizes.is_null() {
        unsafe {
            free_aligned(sizes, element_count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ポインタが要素型 `T` の align 境界に載っていることを検証する
    fn assert_aligned<T>(ptr: *const T) {
        assert_eq!(
            ptr as usize % std::mem::align_of::<T>(),
            0,
            "確保されたポインタが align_of::<{}>() = {} の境界に載っていない",
            std::any::type_name::<T>(),
            std::mem::align_of::<T>(),
        );
    }

    #[test]
    fn test_allocate_and_copy_u16_array_empty_returns_null() {
        // 空スライスは (null, 0) を返し、以後 free 側の early return と噛み合う
        let (ptr, count) = allocate_and_copy_u16_array(&[]);
        assert!(ptr.is_null());
        assert_eq!(count, 0);
    }

    #[test]
    fn test_allocate_and_copy_u16_array_roundtrip() {
        // 非空スライスの alloc → 中身読み出し → align 検証 → free
        let src: [u16; 4] = [0x0102, 0x0304, 0x0506, 0x0708];
        let (ptr, count) = allocate_and_copy_u16_array(&src);
        assert!(!ptr.is_null());
        assert_eq!(count, 4);
        assert_aligned(ptr);
        // 各要素が元の値と一致することを read で確認する
        for (i, expected) in src.iter().enumerate() {
            assert_eq!(unsafe { *ptr.add(i) }, *expected);
        }
        unsafe { free_u16_array(ptr.cast_mut(), count) };
    }

    #[test]
    fn test_allocate_and_copy_u32_array_empty_returns_null() {
        // 空スライスは (null, 0) を返す
        let (ptr, count) = allocate_and_copy_u32_array(&[]);
        assert!(ptr.is_null());
        assert_eq!(count, 0);
    }

    #[test]
    fn test_allocate_and_copy_u32_array_roundtrip() {
        // 非空スライスの alloc → 中身読み出し → align 検証 → free
        let src: [u32; 3] = [0x0102_0304, 0x0506_0708, 0x090a_0b0c];
        let (ptr, count) = allocate_and_copy_u32_array(&src);
        assert!(!ptr.is_null());
        assert_eq!(count, 3);
        assert_aligned(ptr);
        for (i, expected) in src.iter().enumerate() {
            assert_eq!(unsafe { *ptr.add(i) }, *expected);
        }
        unsafe { free_u32_array(ptr.cast_mut(), count) };
    }

    #[test]
    fn test_allocate_and_copy_array_list_empty_returns_all_null() {
        // 空 arrays は (null, null, 0) を返し、free 側の分岐がすべて素通りする
        let (data_ptr, sizes_ptr, count) = allocate_and_copy_array_list(&[]);
        assert!(data_ptr.is_null());
        assert!(sizes_ptr.is_null());
        assert_eq!(count, 0);
    }

    #[test]
    fn test_allocate_and_copy_array_list_roundtrip() {
        // 複数バイト列の alloc → 各要素バイト列とサイズ配列の中身確認 →
        // ポインタ配列とサイズ配列の align 検証 → free
        let src = vec![vec![0x01_u8, 0x02], vec![0x03, 0x04, 0x05], vec![0x06]];
        let (data_ptr, sizes_ptr, count) = allocate_and_copy_array_list(&src);
        assert!(!data_ptr.is_null());
        assert!(!sizes_ptr.is_null());
        assert_eq!(count, 3);
        // ポインタ配列とサイズ配列自体が要素型の align に載っていることが本 diff の主眼
        assert_aligned(data_ptr);
        assert_aligned(sizes_ptr);

        // 各バイト列の内容とサイズが元の Vec と一致することを read で確認する
        for (i, expected) in src.iter().enumerate() {
            let element_ptr = unsafe { *data_ptr.add(i) };
            let element_size = unsafe { *sizes_ptr.add(i) };
            assert_eq!(element_size as usize, expected.len());
            let element = unsafe { std::slice::from_raw_parts(element_ptr, element_size as usize) };
            assert_eq!(element, expected.as_slice());
        }

        // 実際の呼び出し元（boxes_avc1 など）と同じキャストを通して free する
        unsafe {
            free_array_list(data_ptr as *mut *mut u8, sizes_ptr as *mut u32, count);
        }
    }

    #[test]
    fn test_free_u16_array_null_or_zero_is_noop() {
        // null / count 0 の組み合わせがどれも panic せず素通りすることを確認する
        unsafe {
            free_u16_array(std::ptr::null_mut(), 0);
            free_u16_array(std::ptr::null_mut(), 4);
            let dummy_but_zero_count: [u16; 1] = [0];
            free_u16_array(dummy_but_zero_count.as_ptr().cast_mut(), 0);
        }
    }

    #[test]
    fn test_free_u32_array_null_or_zero_is_noop() {
        // null / count 0 の組み合わせがどれも panic せず素通りすることを確認する
        unsafe {
            free_u32_array(std::ptr::null_mut(), 0);
            free_u32_array(std::ptr::null_mut(), 4);
            let dummy_but_zero_count: [u32; 1] = [0];
            free_u32_array(dummy_but_zero_count.as_ptr().cast_mut(), 0);
        }
    }

    #[test]
    fn test_free_array_list_zero_element_count_is_noop() {
        // element_count = 0 は最外側の early return で素通りする（引数の null 有無に依らず）
        unsafe {
            free_array_list(std::ptr::null_mut(), std::ptr::null_mut(), 0);
        }
    }
}
