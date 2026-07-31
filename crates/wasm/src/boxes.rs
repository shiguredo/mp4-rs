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
        // `mp4_alloc` と同じ方針で、確保失敗は abort に寄せる（死なない OOM を残さない）
        std::alloc::handle_alloc_error(layout);
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
///
/// 返り値は「空入力 → `(null, 0)`」または「非空入力 → `(非 null, 非 0)`」の二択で、
/// `size == 0` のチェックがそのまま null チェックを兼ねる
/// （確保失敗時は `mp4_alloc` が abort するため `(null, 非 0)` は生産されない）
pub fn allocate_and_copy_bytes(data: &[u8]) -> (*const u8, u32) {
    if data.is_empty() {
        return (std::ptr::null(), 0);
    }

    let size = data.len() as u32;
    let ptr = unsafe {
        let allocated = crate::mp4_alloc(size);
        std::ptr::copy_nonoverlapping(data.as_ptr(), allocated, data.len());
        allocated as *const u8
    };
    (ptr, size)
}

/// 複数のバイト列をメモリに割り当ててコピーする
///
/// JSON から複数の配列（SPS/PPS や NALU リストなど）を割り当てる際に使用する。
/// 各要素バイト列は `mp4_alloc`（u8 用途）で確保し、ポインタ配列とサイズ配列は
/// それぞれ `*const u8` / `u32` のアライメントで確保する。
/// 空要素は `(null, 0)`、非空要素の確保失敗は `mp4_alloc` / `allocate_and_copy_aligned`
/// 側で abort するため、`(null, 非ゼロ)` の非常態は生産されない
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

/// HEVC（hev1 / hvc1）NALU 配列の JSON シリアライズ用構造体
pub(crate) struct HevcNaluArrays {
    pub(crate) nalu_types: *const u8,
    pub(crate) nalu_counts: *const u32,
    pub(crate) nalu_data: *const *const u8,
    pub(crate) nalu_sizes: *const u32,
    pub(crate) nalu_array_count: u32,
}

impl nojson::DisplayJson for HevcNaluArrays {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.array(|f| {
            let mut nalu_index_base = 0u32;
            for i in 0..self.nalu_array_count as usize {
                let nalu_type = unsafe { *self.nalu_types.add(i) };
                let nalu_count = unsafe { *self.nalu_counts.add(i) };

                f.element(nojson::object(|f| {
                    f.member("naluType", nalu_type)?;
                    f.member(
                        "units",
                        nojson::array(|f| {
                            for j in 0..nalu_count {
                                let nalu_index = nalu_index_base + j;
                                let nalu_ptr = unsafe { *self.nalu_data.add(nalu_index as usize) };
                                let nalu_size =
                                    unsafe { *self.nalu_sizes.add(nalu_index as usize) } as usize;
                                // パース時に格納されたポインタ／サイズを読む（ここでは確保しない）。
                                // 空要素は (null, 0)。`from_raw_parts` は size 0 でも非 null ポインタを
                                // 要求するため、size == 0 の枝を先に落として空配列として出力する
                                // （フォーマット側ではエラーにはしない）
                                let nalu = if nalu_size == 0 {
                                    &[][..]
                                } else {
                                    unsafe { std::slice::from_raw_parts(nalu_ptr, nalu_size) }
                                };
                                f.element(nalu)?;
                            }
                            Ok(())
                        }),
                    )
                }))?;

                nalu_index_base += nalu_count;
            }
            Ok(())
        })
    }
}

/// HEVC（hev1 / hvc1）サンプルエントリーの共通フィールド（割り当て済み）
///
/// `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` と同型のフィールドを持ち、
/// 公開関数側で対応する `#[repr(C)]` 構造体へ写し替える。
///
/// この構造体は `parse_json_hevc_sample_entry_fields` から
/// `hevc_fields_to_hev1` / `_hvc1` への一回限りの受け渡し用途に限定し、
/// フィールドの部分書き換えは行わないこと。生ポインタと個数の invariant を
/// 局所コードで壊さないため。
///
/// `parse_json_hevc_sample_entry_fields` のフェーズ 2 の確保失敗は
/// `handle_alloc_error` でプロセス abort するため、OOM 途中の部分確保を
/// `Drop` で回収する必要はなく、本構造体は `Drop` を実装していない。
/// 所有権設計を変える場合は別途見直すこと
pub(crate) struct HevcSampleEntryFields {
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) general_profile_space: u8,
    pub(crate) general_tier_flag: u8,
    pub(crate) general_profile_idc: u8,
    pub(crate) general_profile_compatibility_flags: u32,
    pub(crate) general_constraint_indicator_flags: u64,
    pub(crate) general_level_idc: u8,
    pub(crate) chroma_format_idc: u8,
    pub(crate) bit_depth_luma_minus8: u8,
    pub(crate) bit_depth_chroma_minus8: u8,
    pub(crate) min_spatial_segmentation_idc: u16,
    pub(crate) parallelism_type: u8,
    pub(crate) avg_frame_rate: u16,
    pub(crate) constant_frame_rate: u8,
    pub(crate) num_temporal_layers: u8,
    pub(crate) temporal_id_nested: u8,
    pub(crate) length_size_minus_one: u8,
    pub(crate) nalu_array_count: u32,
    pub(crate) nalu_types: *const u8,
    pub(crate) nalu_counts: *const u32,
    pub(crate) nalu_data: *const *const u8,
    pub(crate) nalu_sizes: *const u32,
}

/// JSON から HEVC 共通フィールドをパースしてメモリを確保する
///
/// パースとメモリ確保を交互に行うと、途中でパースが失敗したときに
/// 確保済みバッファがリークする。まず全フィールドを Rust 型に落としてから
/// 一括でメモリを確保して、パース失敗時には確保処理に到達しないようにする
pub(crate) fn parse_json_hevc_sample_entry_fields(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<HevcSampleEntryFields, nojson::JsonParseError> {
    // フェーズ 1: JSON → Rust 型
    // NALU 配列を走査して nalu_types_vec / nalu_counts_vec / nalu_data_vec を構築する
    let nalu_arrays_value = value.to_member("naluArrays")?.required()?;

    let mut nalu_types_vec = Vec::new();
    let mut nalu_counts_vec = Vec::new();
    let mut nalu_data_vec = Vec::new();

    for nalu_array in nalu_arrays_value.to_array()? {
        let nalu_type: u8 = nalu_array.to_member("naluType")?.required()?.try_into()?;
        nalu_types_vec.push(nalu_type);

        let units_value = nalu_array.to_member("units")?.required()?;

        let mut nalu_count = 0u32;
        for unit in units_value.to_array()? {
            let nalu_bytes: Vec<u8> = unit.try_into()?;
            nalu_data_vec.push(nalu_bytes);
            nalu_count += 1;
        }
        nalu_counts_vec.push(nalu_count);
    }

    // 残りのスカラーフィールド
    let width: u16 = value.to_member("width")?.required()?.try_into()?;
    let height: u16 = value.to_member("height")?.required()?.try_into()?;
    let general_profile_space: u8 = value
        .to_member("generalProfileSpace")?
        .required()?
        .try_into()?;
    let general_tier_flag: u8 = value.to_member("generalTierFlag")?.required()?.try_into()?;
    let general_profile_idc: u8 = value
        .to_member("generalProfileIdc")?
        .required()?
        .try_into()?;
    let general_profile_compatibility_flags: u32 = value
        .to_member("generalProfileCompatibilityFlags")?
        .required()?
        .try_into()?;
    let general_constraint_indicator_flags: u64 = value
        .to_member("generalConstraintIndicatorFlags")?
        .required()?
        .try_into()?;
    let general_level_idc: u8 = value.to_member("generalLevelIdc")?.required()?.try_into()?;
    let chroma_format_idc: u8 = value.to_member("chromaFormatIdc")?.required()?.try_into()?;
    let bit_depth_luma_minus8: u8 = value
        .to_member("bitDepthLumaMinus8")?
        .required()?
        .try_into()?;
    let bit_depth_chroma_minus8: u8 = value
        .to_member("bitDepthChromaMinus8")?
        .required()?
        .try_into()?;
    let min_spatial_segmentation_idc: u16 = value
        .to_member("minSpatialSegmentationIdc")?
        .required()?
        .try_into()?;
    let parallelism_type: u8 = value.to_member("parallelismType")?.required()?.try_into()?;
    let avg_frame_rate: u16 = value.to_member("avgFrameRate")?.required()?.try_into()?;
    let constant_frame_rate: u8 = value
        .to_member("constantFrameRate")?
        .required()?
        .try_into()?;
    let num_temporal_layers: u8 = value
        .to_member("numTemporalLayers")?
        .required()?
        .try_into()?;
    let temporal_id_nested: u8 = value
        .to_member("temporalIdNested")?
        .required()?
        .try_into()?;
    let length_size_minus_one: u8 = value
        .to_member("lengthSizeMinusOne")?
        .required()?
        .try_into()?;
    let nalu_array_count = nalu_types_vec.len() as u32;

    // フェーズ 2: メモリ確保
    // `nalu_types` は u8 なので `mp4_alloc`（align 1）経由の `allocate_and_copy_bytes` で足りる
    // `nalu_counts` は u32 の連続バッファとして read されるため、`allocate_and_copy_u32_array`
    // 経由で u32 align に載せる（align 1 経路では contract 違反で UB になる）
    let (nalu_types, _) = allocate_and_copy_bytes(unsafe {
        std::slice::from_raw_parts(nalu_types_vec.as_ptr(), nalu_types_vec.len())
    });
    let (nalu_counts, _) = allocate_and_copy_u32_array(&nalu_counts_vec);
    let (nalu_data, nalu_sizes, _) = allocate_and_copy_array_list(&nalu_data_vec);

    Ok(HevcSampleEntryFields {
        width,
        height,
        general_profile_space,
        general_tier_flag,
        general_profile_idc,
        general_profile_compatibility_flags,
        general_constraint_indicator_flags,
        general_level_idc,
        chroma_format_idc,
        bit_depth_luma_minus8,
        bit_depth_chroma_minus8,
        min_spatial_segmentation_idc,
        parallelism_type,
        avg_frame_rate,
        constant_frame_rate,
        num_temporal_layers,
        temporal_id_nested,
        length_size_minus_one,
        nalu_array_count,
        nalu_types,
        nalu_counts,
        nalu_data,
        nalu_sizes,
    })
}

/// HEVC（hev1 / hvc1）サンプルエントリーのポインタフィールドを解放する
///
/// `parse_json_hevc_sample_entry_fields()` で割り当てられたメモリを解放する。
/// 公開の `mp4_sample_entry_hev1_free` / `_hvc1_free` からフィールドを取り出して呼ぶ
pub(crate) fn free_hevc_sample_entry_fields(
    nalu_array_count: &mut u32,
    nalu_types: &mut *const u8,
    nalu_counts: &mut *const u32,
    nalu_data: &mut *const *const u8,
    nalu_sizes: &mut *const u32,
) {
    // 全 NALU 総数を関数先頭のローカルに持たせておく理由は、後段の `free_array_list` の
    // count 引数に使うため。総数の算出は `nalu_counts` の解放より前に済ませないと
    // use-after-free になる。
    //
    // 各フィールドの確保サイズは要素型と `nalu_array_count` から求まる:
    // - `nalu_types`: 要素 `u8` × `nalu_array_count`
    // - `nalu_counts`: 要素 `u32` × `nalu_array_count`
    // - `nalu_data` / `nalu_sizes`: 要素数は「NALU 配列の個数」ではなく「全 NALU の総数」
    let mut total_nalu_count: u32 = 0;

    if !nalu_types.is_null() {
        unsafe {
            crate::mp4_free(nalu_types.cast_mut(), *nalu_array_count);
        }
        *nalu_types = std::ptr::null();
    }

    if !nalu_counts.is_null() {
        unsafe {
            let counts = std::slice::from_raw_parts(*nalu_counts, *nalu_array_count as usize);
            for count in counts {
                total_nalu_count = total_nalu_count
                    .checked_add(*count)
                    .expect("invariant broken: total nalu count exceeds u32::MAX");
            }

            // 確保は allocate_and_copy_u32_array（u32 align）なので、対になる free を使う
            free_u32_array(nalu_counts.cast_mut(), *nalu_array_count);
        }
        *nalu_counts = std::ptr::null();
    }

    if !nalu_data.is_null() {
        unsafe {
            free_array_list(
                *nalu_data as *mut *mut u8,
                *nalu_sizes as *mut u32,
                total_nalu_count,
            );
            *nalu_data = std::ptr::null();
            *nalu_sizes = std::ptr::null();
        }
    }

    *nalu_array_count = 0;
}

/// HEVC（hev1 / hvc1）テスト用 JSON の既定スカラーフィールド一覧
///
/// キー名は camelCase の JSON メンバー名、値は JSON 数値リテラル文字列。
/// `build_hevc_test_json` / `build_hevc_test_json_omitting` の両者が参照し、
/// フィールドの追加・値変更時にリテラルを 1 箇所に集約する
#[cfg(test)]
const HEVC_TEST_JSON_SCALAR_FIELDS: &[(&str, &str)] = &[
    ("width", "1920"),
    ("height", "1080"),
    ("generalProfileSpace", "0"),
    ("generalTierFlag", "0"),
    ("generalProfileIdc", "2"),
    ("generalProfileCompatibilityFlags", "1610612736"),
    ("generalConstraintIndicatorFlags", "12682136550675546112"),
    ("generalLevelIdc", "120"),
    ("chromaFormatIdc", "1"),
    ("bitDepthLumaMinus8", "0"),
    ("bitDepthChromaMinus8", "0"),
    ("minSpatialSegmentationIdc", "0"),
    ("parallelismType", "0"),
    ("avgFrameRate", "0"),
    ("constantFrameRate", "0"),
    ("numTemporalLayers", "1"),
    ("temporalIdNested", "0"),
    ("lengthSizeMinusOne", "3"),
];

/// HEVC（hev1 / hvc1）テスト用 JSON を組み立てる
///
/// スカラーフィールドは回帰テストで共有する既定値を使い、差が出る `kind` と
/// `naluArrays` だけを呼び出し側から差し替える
#[cfg(test)]
pub(crate) fn build_hevc_test_json(kind: &str, nalu_arrays_json: &str) -> String {
    build_hevc_test_json_omitting(kind, nalu_arrays_json, None)
}

/// スカラーフィールドを 1 つ欠落させた HEVC テスト用 JSON を組み立てる
///
/// `parse_json_hevc_sample_entry_fields` の「必須フィールド欠落時にパース失敗する」
/// 経路を検証するテスト用。文字列ベースの `.replace` はヘルパーの整形に依存して
/// silent に no-op 化しうるため、`HEVC_TEST_JSON_SCALAR_FIELDS` のキー名を
/// フィルタする構造化された欠落 API を提供する。
///
/// `omit_field` が `None` のとき、`build_hevc_test_json` と等価
#[cfg(test)]
pub(crate) fn build_hevc_test_json_omitting(
    kind: &str,
    nalu_arrays_json: &str,
    omit_field: Option<&str>,
) -> String {
    // 一致するフィールド名を飛ばしてスカラー行を組み立てる
    let body = HEVC_TEST_JSON_SCALAR_FIELDS
        .iter()
        .filter(|(name, _)| Some(*name) != omit_field)
        .map(|(name, value)| format!("    \"{name}\": {value}"))
        .collect::<Vec<_>>()
        .join(",\n");

    format!("{{\n    \"kind\": \"{kind}\",\n{body},\n    \"naluArrays\": {nalu_arrays_json}\n}}")
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

    /// `build_hevc_test_json_omitting` が指定フィールドを実際に欠落させ、
    /// 残りのフィールド行を保つことを検証する
    #[test]
    fn test_build_hevc_test_json_omitting_actually_omits_field() {
        let with_width = build_hevc_test_json_omitting("hev1", "[]", None);
        assert!(with_width.contains("\"width\": 1920"));

        let without_width = build_hevc_test_json_omitting("hev1", "[]", Some("width"));
        assert!(!without_width.contains("\"width\""));
        // 他の必須フィールドは残る
        assert!(without_width.contains("\"height\": 1080"));
        assert!(without_width.contains("\"lengthSizeMinusOne\": 3"));
    }

    /// 必須スカラーフィールドを 1 つずつ欠落させた JSON を渡し、
    /// いずれのケースでも `parse_json_hevc_sample_entry_fields` が `Err` を返すことを検証する。
    ///
    /// closed issue 0047 で確立した「フェーズ 1 で全 JSON フィールドを Rust 型に落として
    /// からフェーズ 2 で一括メモリ確保する」順序の invariant を、18 個の全スカラー欠落
    /// パターンで守るための表駆動テスト
    #[test]
    fn test_parse_json_hevc_sample_entry_fields_rejects_each_missing_scalar_field() {
        let arrays = r#"[{"naluType": 32, "units": [[1, 2]]}]"#;
        for &(field_name, _) in HEVC_TEST_JSON_SCALAR_FIELDS {
            let json_str = build_hevc_test_json_omitting("hev1", arrays, Some(field_name));
            let json = nojson::RawJson::parse(&json_str)
                .expect("有効な JSON（スカラー欠落は JSON 構造としては壊さない）");
            let result = parse_json_hevc_sample_entry_fields(json.value());
            assert!(
                result.is_err(),
                "スカラーフィールド {field_name} 欠落時はパース失敗すること"
            );
        }
    }

    /// `naluArrays` メンバ自体が欠落した JSON を渡すと `Err` を返すことを検証する。
    ///
    /// `parse_json_hevc_sample_entry_fields` はフェーズ 1 の先頭で `naluArrays` を読むため、
    /// この経路はスカラーの取り込みに到達する前に失敗する
    #[test]
    fn test_parse_json_hevc_sample_entry_fields_rejects_missing_nalu_arrays() {
        // スカラーは全揃いで `naluArrays` だけ欠落させる
        let body = HEVC_TEST_JSON_SCALAR_FIELDS
            .iter()
            .map(|(name, value)| format!("    \"{name}\": {value}"))
            .collect::<Vec<_>>()
            .join(",\n");
        let json_str = format!("{{\n    \"kind\": \"hev1\",\n{body}\n}}");

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let result = parse_json_hevc_sample_entry_fields(json.value());
        assert!(result.is_err(), "naluArrays 欠落時はパース失敗すること");
    }

    /// `naluArrays[i].naluType` が欠落した JSON を渡すと `Err` を返すことを検証する
    #[test]
    fn test_parse_json_hevc_sample_entry_fields_rejects_missing_nalu_type_in_array_element() {
        let arrays = r#"[{"units": [[1, 2]]}]"#;
        let json_str = build_hevc_test_json_omitting("hev1", arrays, None);
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let result = parse_json_hevc_sample_entry_fields(json.value());
        assert!(result.is_err(), "naluType 欠落時はパース失敗すること");
    }

    /// `naluArrays[i].units` が欠落した JSON を渡すと `Err` を返すことを検証する
    #[test]
    fn test_parse_json_hevc_sample_entry_fields_rejects_missing_units_in_array_element() {
        let arrays = r#"[{"naluType": 32}]"#;
        let json_str = build_hevc_test_json_omitting("hev1", arrays, None);
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let result = parse_json_hevc_sample_entry_fields(json.value());
        assert!(result.is_err(), "units 欠落時はパース失敗すること");
    }
}
