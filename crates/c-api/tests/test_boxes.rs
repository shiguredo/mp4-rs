//! `Mp4SampleEntry::to_sample_entry` の null ポインタ検査に関する統合テスト
//!
//! FFI 境界で `count > 0` のときに配列ベースポインタに null が渡された場合に、
//! 未定義動作ではなく `Mp4Error::MP4_ERROR_NULL_POINTER` が返ることを検証する。
//! あわせて、既存の正当な入力（`count == 0` や全 `nalu_counts[i] == 0` 等）を
//! 過剰に拒否していないこと（非退行）も確認する。

use mp4::boxes::{
    Mp4SampleEntry, Mp4SampleEntryAvc1, Mp4SampleEntryData, Mp4SampleEntryHev1, Mp4SampleEntryHvc1,
    Mp4SampleEntryKind,
};
use mp4::error::Mp4Error;

/// Avc1 の共通フィールドを埋めた雛形を生成するヘルパー
///
/// SPS / PPS 系のポインタとカウントだけを個別テストで差し替えれば済むように、
/// それ以外のフィールドは適当だが型として妥当な固定値を入れる
fn base_avc1(
    sps_data: *const *const u8,
    sps_sizes: *const u32,
    sps_count: u32,
    pps_data: *const *const u8,
    pps_sizes: *const u32,
    pps_count: u32,
) -> Mp4SampleEntryAvc1 {
    Mp4SampleEntryAvc1 {
        width: 1920,
        height: 1080,
        avc_profile_indication: 100,
        profile_compatibility: 0,
        avc_level_indication: 51,
        length_size_minus_one: 3,
        sps_data,
        sps_sizes,
        sps_count,
        pps_data,
        pps_sizes,
        pps_count,
        is_chroma_format_present: false,
        chroma_format: 0,
        is_bit_depth_luma_minus8_present: false,
        bit_depth_luma_minus8: 0,
        is_bit_depth_chroma_minus8_present: false,
        bit_depth_chroma_minus8: 0,
    }
}

/// Hev1 の共通フィールドを埋めた雛形を生成するヘルパー
///
/// NALU 配列系のポインタとカウントだけを個別テストで差し替える
fn base_hev1(
    nalu_array_count: u32,
    nalu_types: *const u8,
    nalu_counts: *const u32,
    nalu_data: *const *const u8,
    nalu_sizes: *const u32,
) -> Mp4SampleEntryHev1 {
    Mp4SampleEntryHev1 {
        width: 1920,
        height: 1080,
        general_profile_space: 0,
        general_tier_flag: 0,
        general_profile_idc: 1,
        general_profile_compatibility_flags: 0,
        general_constraint_indicator_flags: 0,
        general_level_idc: 120,
        chroma_format_idc: 1,
        bit_depth_luma_minus8: 0,
        bit_depth_chroma_minus8: 0,
        min_spatial_segmentation_idc: 0,
        parallelism_type: 0,
        avg_frame_rate: 0,
        constant_frame_rate: 0,
        num_temporal_layers: 1,
        temporal_id_nested: 0,
        length_size_minus_one: 3,
        nalu_array_count,
        nalu_types,
        nalu_counts,
        nalu_data,
        nalu_sizes,
    }
}

/// Hvc1 の共通フィールドを埋めた雛形を生成するヘルパー
fn base_hvc1(
    nalu_array_count: u32,
    nalu_types: *const u8,
    nalu_counts: *const u32,
    nalu_data: *const *const u8,
    nalu_sizes: *const u32,
) -> Mp4SampleEntryHvc1 {
    Mp4SampleEntryHvc1 {
        width: 1920,
        height: 1080,
        general_profile_space: 0,
        general_tier_flag: 0,
        general_profile_idc: 1,
        general_profile_compatibility_flags: 0,
        general_constraint_indicator_flags: 0,
        general_level_idc: 120,
        chroma_format_idc: 1,
        bit_depth_luma_minus8: 0,
        bit_depth_chroma_minus8: 0,
        min_spatial_segmentation_idc: 0,
        parallelism_type: 0,
        avg_frame_rate: 0,
        constant_frame_rate: 0,
        num_temporal_layers: 1,
        temporal_id_nested: 0,
        length_size_minus_one: 3,
        nalu_array_count,
        nalu_types,
        nalu_counts,
        nalu_data,
        nalu_sizes,
    }
}

// --- AVC1 ---

/// Avc1: `sps_count > 0` で `sps_sizes` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn avc1_null_sps_sizes_returns_null_pointer_error() {
    let sps_data_arr: [*const u8; 1] = [std::ptr::null()];
    let pps_data_arr: [*const u8; 1] = [std::ptr::null()];
    let pps_sizes_arr: [u32; 1] = [0];
    let avc1 = base_avc1(
        sps_data_arr.as_ptr(),
        std::ptr::null(), // ここが null
        1,
        pps_data_arr.as_ptr(),
        pps_sizes_arr.as_ptr(),
        0,
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_AVC1,
        data: Mp4SampleEntryData { avc1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        matches!(result, Err(Mp4Error::MP4_ERROR_NULL_POINTER)),
        "sps_sizes が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Avc1: `pps_count > 0` で `pps_sizes` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn avc1_null_pps_sizes_returns_null_pointer_error() {
    let sps_data_arr: [*const u8; 1] = [std::ptr::null()];
    let sps_sizes_arr: [u32; 1] = [0];
    let pps_data_arr: [*const u8; 1] = [std::ptr::null()];
    let avc1 = base_avc1(
        sps_data_arr.as_ptr(),
        sps_sizes_arr.as_ptr(),
        0,
        pps_data_arr.as_ptr(),
        std::ptr::null(), // ここが null
        1,
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_AVC1,
        data: Mp4SampleEntryData { avc1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        matches!(result, Err(Mp4Error::MP4_ERROR_NULL_POINTER)),
        "pps_sizes が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Avc1 非退行: `sps_count == 0` でも `sps_data == NULL` は既存どおり弾く
///
/// 既存挙動を意識的に据え置いたことを回帰テストで担保する
#[test]
fn avc1_null_sps_data_with_count_zero_still_returns_null_pointer_error() {
    let pps_data_arr: [*const u8; 1] = [std::ptr::null()];
    let pps_sizes_arr: [u32; 1] = [0];
    let avc1 = base_avc1(
        std::ptr::null(), // ここが null
        std::ptr::null(),
        0, // count は 0 だが既存の無条件検査で弾く
        pps_data_arr.as_ptr(),
        pps_sizes_arr.as_ptr(),
        0,
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_AVC1,
        data: Mp4SampleEntryData { avc1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        matches!(result, Err(Mp4Error::MP4_ERROR_NULL_POINTER)),
        "sps_count == 0 でも sps_data == NULL は既存挙動どおり NULL_POINTER を返す必要がある"
    );
}

// --- HEV1 ---

/// Hev1: `nalu_array_count > 0` で `nalu_types` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hev1_null_nalu_types_returns_null_pointer_error() {
    let nalu_counts_arr: [u32; 1] = [0];
    let hev1 = base_hev1(
        1,
        std::ptr::null(), // ここが null
        nalu_counts_arr.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HEV1,
        data: Mp4SampleEntryData { hev1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        matches!(result, Err(Mp4Error::MP4_ERROR_NULL_POINTER)),
        "nalu_types が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hev1: `nalu_array_count > 0` で `nalu_counts` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hev1_null_nalu_counts_returns_null_pointer_error() {
    let nalu_types_arr: [u8; 1] = [32];
    let hev1 = base_hev1(
        1,
        nalu_types_arr.as_ptr(),
        std::ptr::null(), // ここが null
        std::ptr::null(),
        std::ptr::null(),
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HEV1,
        data: Mp4SampleEntryData { hev1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        matches!(result, Err(Mp4Error::MP4_ERROR_NULL_POINTER)),
        "nalu_counts が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hev1: `nalu_counts[0] > 0` の内側ループ直前で `nalu_data` が null のとき
/// `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hev1_null_nalu_data_returns_null_pointer_error() {
    let nalu_types_arr: [u8; 1] = [32];
    let nalu_counts_arr: [u32; 1] = [1];
    let nalu_sizes_arr: [u32; 1] = [1];
    let hev1 = base_hev1(
        1,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        std::ptr::null(), // ここが null
        nalu_sizes_arr.as_ptr(),
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HEV1,
        data: Mp4SampleEntryData { hev1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        matches!(result, Err(Mp4Error::MP4_ERROR_NULL_POINTER)),
        "nalu_data が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hev1: `nalu_counts[0] > 0` の内側ループ直前で `nalu_sizes` が null のとき
/// `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hev1_null_nalu_sizes_returns_null_pointer_error() {
    let nalu_types_arr: [u8; 1] = [32];
    let nalu_counts_arr: [u32; 1] = [1];
    let dummy_nalu: [u8; 1] = [0];
    let nalu_data_arr: [*const u8; 1] = [dummy_nalu.as_ptr()];
    let hev1 = base_hev1(
        1,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        nalu_data_arr.as_ptr(),
        std::ptr::null(), // ここが null
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HEV1,
        data: Mp4SampleEntryData { hev1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        matches!(result, Err(Mp4Error::MP4_ERROR_NULL_POINTER)),
        "nalu_sizes が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hev1 非退行: 全 `nalu_counts[i] == 0` なら `nalu_data` / `nalu_sizes` が null でも成功する
///
/// 内側ループが 1 度も走らない入力を過剰に弾かないことを回帰テストで担保する
#[test]
fn hev1_null_nalu_data_with_all_counts_zero_succeeds() {
    let nalu_types_arr: [u8; 2] = [32, 33];
    let nalu_counts_arr: [u32; 2] = [0, 0];
    let hev1 = base_hev1(
        2,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HEV1,
        data: Mp4SampleEntryData { hev1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        result.is_ok(),
        "全 nalu_counts[i] == 0 のときは nalu_data / nalu_sizes が null でも成功する必要がある"
    );
}

// --- HVC1 ---

/// Hvc1: `nalu_array_count > 0` で `nalu_types` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hvc1_null_nalu_types_returns_null_pointer_error() {
    let nalu_counts_arr: [u32; 1] = [0];
    let hvc1 = base_hvc1(
        1,
        std::ptr::null(), // ここが null
        nalu_counts_arr.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HVC1,
        data: Mp4SampleEntryData { hvc1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        matches!(result, Err(Mp4Error::MP4_ERROR_NULL_POINTER)),
        "nalu_types が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hvc1: `nalu_array_count > 0` で `nalu_counts` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hvc1_null_nalu_counts_returns_null_pointer_error() {
    let nalu_types_arr: [u8; 1] = [32];
    let hvc1 = base_hvc1(
        1,
        nalu_types_arr.as_ptr(),
        std::ptr::null(), // ここが null
        std::ptr::null(),
        std::ptr::null(),
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HVC1,
        data: Mp4SampleEntryData { hvc1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        matches!(result, Err(Mp4Error::MP4_ERROR_NULL_POINTER)),
        "nalu_counts が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hvc1: `nalu_counts[0] > 0` の内側ループ直前で `nalu_data` が null のとき
/// `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hvc1_null_nalu_data_returns_null_pointer_error() {
    let nalu_types_arr: [u8; 1] = [32];
    let nalu_counts_arr: [u32; 1] = [1];
    let nalu_sizes_arr: [u32; 1] = [1];
    let hvc1 = base_hvc1(
        1,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        std::ptr::null(), // ここが null
        nalu_sizes_arr.as_ptr(),
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HVC1,
        data: Mp4SampleEntryData { hvc1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        matches!(result, Err(Mp4Error::MP4_ERROR_NULL_POINTER)),
        "nalu_data が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hvc1: `nalu_counts[0] > 0` の内側ループ直前で `nalu_sizes` が null のとき
/// `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hvc1_null_nalu_sizes_returns_null_pointer_error() {
    let nalu_types_arr: [u8; 1] = [32];
    let nalu_counts_arr: [u32; 1] = [1];
    let dummy_nalu: [u8; 1] = [0];
    let nalu_data_arr: [*const u8; 1] = [dummy_nalu.as_ptr()];
    let hvc1 = base_hvc1(
        1,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        nalu_data_arr.as_ptr(),
        std::ptr::null(), // ここが null
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HVC1,
        data: Mp4SampleEntryData { hvc1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        matches!(result, Err(Mp4Error::MP4_ERROR_NULL_POINTER)),
        "nalu_sizes が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hvc1 非退行: 全 `nalu_counts[i] == 0` なら `nalu_data` / `nalu_sizes` が null でも成功する
#[test]
fn hvc1_null_nalu_data_with_all_counts_zero_succeeds() {
    let nalu_types_arr: [u8; 2] = [32, 33];
    let nalu_counts_arr: [u32; 2] = [0, 0];
    let hvc1 = base_hvc1(
        2,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        std::ptr::null(),
        std::ptr::null(),
    );
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HVC1,
        data: Mp4SampleEntryData { hvc1 },
    };
    let result = entry.to_sample_entry();
    assert!(
        result.is_ok(),
        "全 nalu_counts[i] == 0 のときは nalu_data / nalu_sizes が null でも成功する必要がある"
    );
}
