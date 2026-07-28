//! `Mp4SampleEntry::to_sample_entry` に関する統合テスト
//!
//! 主な検証内容:
//! - FFI 境界で `count > 0` のときに配列ベースポインタに null が渡された場合に、
//!   未定義動作ではなく `Mp4Error::MP4_ERROR_NULL_POINTER` が返ること
//! - 正当な入力（`count == 0`、全 `nalu_counts[i] == 0`、有効データ）が
//!   期待どおりの `SampleEntry` に変換されること
//! - 個別配列要素のポインタが null の場合も検知できること

use std::ptr::null;

use mp4::boxes::{
    Mp4SampleEntry, Mp4SampleEntryAvc1, Mp4SampleEntryData, Mp4SampleEntryHev1, Mp4SampleEntryHvc1,
    Mp4SampleEntryKind,
};
use mp4::error::Mp4Error;
use shiguredo_mp4::boxes::SampleEntry;

// ---- ヘルパー ----

/// Avc1 の共通フィールドを埋めた雛形を生成するヘルパー
///
/// SPS / PPS 系のポインタとカウントだけを個別テストで差し替える
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

/// Avc1 の `Mp4SampleEntry` を組み立てて公開 API 経由で `to_sample_entry` を呼ぶ
fn call_avc1(avc1: Mp4SampleEntryAvc1) -> Result<SampleEntry, Mp4Error> {
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_AVC1,
        data: Mp4SampleEntryData { avc1 },
    };
    entry.to_sample_entry()
}

/// Hev1 の `Mp4SampleEntry` を組み立てて公開 API 経由で `to_sample_entry` を呼ぶ
fn call_hev1(hev1: Mp4SampleEntryHev1) -> Result<SampleEntry, Mp4Error> {
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HEV1,
        data: Mp4SampleEntryData { hev1 },
    };
    entry.to_sample_entry()
}

/// Hvc1 の `Mp4SampleEntry` を組み立てて公開 API 経由で `to_sample_entry` を呼ぶ
fn call_hvc1(hvc1: Mp4SampleEntryHvc1) -> Result<SampleEntry, Mp4Error> {
    let entry = Mp4SampleEntry {
        kind: Mp4SampleEntryKind::MP4_SAMPLE_ENTRY_KIND_HVC1,
        data: Mp4SampleEntryData { hvc1 },
    };
    entry.to_sample_entry()
}

// ---- AVC1 ----

/// Avc1 正常系: 複数の SPS/PPS を渡すと `SampleEntry::Avc1` に正しく変換される
#[test]
fn avc1_valid_sps_pps_succeeds_with_expected_content() {
    // 実データとして 2 個の SPS と 2 個の PPS を用意する
    let sps0: [u8; 4] = [0x67, 0x42, 0xC0, 0x1E];
    let sps1: [u8; 5] = [0x67, 0x42, 0xC0, 0x1F, 0xAB];
    let pps0: [u8; 4] = [0x68, 0xCE, 0x38, 0x80];
    let pps1: [u8; 3] = [0x68, 0xCE, 0x38];

    let sps_data_arr: [*const u8; 2] = [sps0.as_ptr(), sps1.as_ptr()];
    let sps_sizes_arr: [u32; 2] = [sps0.len() as u32, sps1.len() as u32];
    let pps_data_arr: [*const u8; 2] = [pps0.as_ptr(), pps1.as_ptr()];
    let pps_sizes_arr: [u32; 2] = [pps0.len() as u32, pps1.len() as u32];

    let avc1 = base_avc1(
        sps_data_arr.as_ptr(),
        sps_sizes_arr.as_ptr(),
        2,
        pps_data_arr.as_ptr(),
        pps_sizes_arr.as_ptr(),
        2,
    );
    let result = call_avc1(avc1).expect("正常な入力なので成功する必要がある");
    let SampleEntry::Avc1(box_) = result else {
        panic!("Avc1 バリアントが返るべき");
    };

    // SPS / PPS リストの中身が期待どおりに移送されていることを確認する
    assert_eq!(box_.avcc_box.sps_list.len(), 2);
    assert_eq!(box_.avcc_box.sps_list[0], sps0);
    assert_eq!(box_.avcc_box.sps_list[1], sps1);
    assert_eq!(box_.avcc_box.pps_list.len(), 2);
    assert_eq!(box_.avcc_box.pps_list[0], pps0);
    assert_eq!(box_.avcc_box.pps_list[1], pps1);

    // プロファイル / レベル / 解像度も入力値と一致することを確認する
    assert_eq!(box_.avcc_box.avc_profile_indication, 100);
    assert_eq!(box_.avcc_box.avc_level_indication, 51);
    assert_eq!(box_.visual.width, 1920);
    assert_eq!(box_.visual.height, 1080);
}

/// Avc1: `sps_count > 0` で `sps_sizes` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn avc1_null_sps_sizes_returns_null_pointer_error() {
    let sps_data_arr: [*const u8; 1] = [null()];
    let pps_data_arr: [*const u8; 1] = [null()];
    let pps_sizes_arr: [u32; 1] = [0];
    let avc1 = base_avc1(
        sps_data_arr.as_ptr(),
        null(),
        1,
        pps_data_arr.as_ptr(),
        pps_sizes_arr.as_ptr(),
        0,
    );
    assert_eq!(
        call_avc1(avc1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "sps_sizes が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Avc1: `pps_count > 0` で `pps_sizes` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn avc1_null_pps_sizes_returns_null_pointer_error() {
    let sps_data_arr: [*const u8; 1] = [null()];
    let sps_sizes_arr: [u32; 1] = [0];
    let pps_data_arr: [*const u8; 1] = [null()];
    let avc1 = base_avc1(
        sps_data_arr.as_ptr(),
        sps_sizes_arr.as_ptr(),
        0,
        pps_data_arr.as_ptr(),
        null(),
        1,
    );
    assert_eq!(
        call_avc1(avc1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "pps_sizes が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Avc1: 配列内 i 番目の `sps_data[i]` が null のとき `MP4_ERROR_NULL_POINTER` を返す
///
/// ベースポインタ検査だけでは拾えない、既存の内側 null 検査経路を担保する
#[test]
fn avc1_null_sps_element_returns_null_pointer_error() {
    let sps0: [u8; 1] = [0x67];
    // 2 個目の要素だけ null にする
    let sps_data_arr: [*const u8; 2] = [sps0.as_ptr(), null()];
    let sps_sizes_arr: [u32; 2] = [1, 1];
    let pps_data_arr: [*const u8; 1] = [null()];
    let pps_sizes_arr: [u32; 1] = [0];
    let avc1 = base_avc1(
        sps_data_arr.as_ptr(),
        sps_sizes_arr.as_ptr(),
        2,
        pps_data_arr.as_ptr(),
        pps_sizes_arr.as_ptr(),
        0,
    );
    assert_eq!(
        call_avc1(avc1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "sps_data の配列内要素が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Avc1 非退行: `sps_count == 0` でも `sps_data == NULL` は既存どおり弾く
///
/// 既存挙動を意識的に据え置いたことを回帰テストで担保する
#[test]
fn avc1_null_sps_data_with_count_zero_still_returns_null_pointer_error() {
    let pps_data_arr: [*const u8; 1] = [null()];
    let pps_sizes_arr: [u32; 1] = [0];
    let avc1 = base_avc1(
        null(),
        null(),
        0, // count は 0 だが既存の無条件検査で弾く
        pps_data_arr.as_ptr(),
        pps_sizes_arr.as_ptr(),
        0,
    );
    assert_eq!(
        call_avc1(avc1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "sps_count == 0 でも sps_data == NULL は既存挙動どおり NULL_POINTER を返す必要がある"
    );
}

/// Avc1 非退行: `pps_count == 0` でも `pps_data == NULL` は既存どおり弾く
///
/// SPS 側と対称の据え置き挙動を回帰テストで担保する
#[test]
fn avc1_null_pps_data_with_count_zero_still_returns_null_pointer_error() {
    let sps_data_arr: [*const u8; 1] = [null()];
    let sps_sizes_arr: [u32; 1] = [0];
    let avc1 = base_avc1(
        sps_data_arr.as_ptr(),
        sps_sizes_arr.as_ptr(),
        0,
        null(),
        null(),
        0, // count は 0 だが既存の無条件検査で弾く
    );
    assert_eq!(
        call_avc1(avc1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "pps_count == 0 でも pps_data == NULL は既存挙動どおり NULL_POINTER を返す必要がある"
    );
}

// ---- HEV1 ----

/// Hev1 正常系: 複数 NALU 配列を渡すと `SampleEntry::Hev1` に正しく変換される
///
/// `nalu_counts = [2, 1, 3]` の非自明な累積により `nalu_data_index` の
/// プレフィックス和が正しく計算されていることも間接的に検証する。
#[test]
fn hev1_valid_multiple_nalu_arrays_succeeds_with_expected_content() {
    // 3 配列で合計 6 個の NALU を用意する
    // 各 NALU に別々のバイトを入れて、配列 → NALU 位置の対応が壊れると検出できるようにする
    let nalu_bytes: [[u8; 1]; 6] = [[0xAA], [0xBB], [0xCC], [0xDD], [0xEE], [0xFF]];
    let nalu_types_arr: [u8; 3] = [32, 33, 34]; // VPS / SPS / PPS を模した値
    let nalu_counts_arr: [u32; 3] = [2, 1, 3];
    let nalu_data_arr: [*const u8; 6] = [
        nalu_bytes[0].as_ptr(),
        nalu_bytes[1].as_ptr(),
        nalu_bytes[2].as_ptr(),
        nalu_bytes[3].as_ptr(),
        nalu_bytes[4].as_ptr(),
        nalu_bytes[5].as_ptr(),
    ];
    let nalu_sizes_arr: [u32; 6] = [1, 1, 1, 1, 1, 1];

    let hev1 = base_hev1(
        3,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        nalu_data_arr.as_ptr(),
        nalu_sizes_arr.as_ptr(),
    );
    let result = call_hev1(hev1).expect("正常な入力なので成功する必要がある");
    let SampleEntry::Hev1(box_) = result else {
        panic!("Hev1 バリアントが返るべき");
    };

    // 3 個の nalu_arrays が生成されていることを確認する
    assert_eq!(box_.hvcc_box.nalu_arrays.len(), 3);

    // 配列 0: nalu_types=32, nalus=[0xAA, 0xBB]
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nal_unit_type.get(), 32);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nalus.len(), 2);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nalus[0], nalu_bytes[0]);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nalus[1], nalu_bytes[1]);

    // 配列 1: nalu_types=33, nalus=[0xCC]
    // nalu_data_index(1, 0) = 2 が正しく計算されているかを検証する
    assert_eq!(box_.hvcc_box.nalu_arrays[1].nal_unit_type.get(), 33);
    assert_eq!(box_.hvcc_box.nalu_arrays[1].nalus.len(), 1);
    assert_eq!(box_.hvcc_box.nalu_arrays[1].nalus[0], nalu_bytes[2]);

    // 配列 2: nalu_types=34, nalus=[0xDD, 0xEE, 0xFF]
    // nalu_data_index(2, 0)=3, (2, 1)=4, (2, 2)=5 の累積を検証する
    assert_eq!(box_.hvcc_box.nalu_arrays[2].nal_unit_type.get(), 34);
    assert_eq!(box_.hvcc_box.nalu_arrays[2].nalus.len(), 3);
    assert_eq!(box_.hvcc_box.nalu_arrays[2].nalus[0], nalu_bytes[3]);
    assert_eq!(box_.hvcc_box.nalu_arrays[2].nalus[1], nalu_bytes[4]);
    assert_eq!(box_.hvcc_box.nalu_arrays[2].nalus[2], nalu_bytes[5]);
}

/// Hev1: `nalu_array_count > 0` で `nalu_types` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hev1_null_nalu_types_returns_null_pointer_error() {
    let nalu_counts_arr: [u32; 1] = [0];
    let hev1 = base_hev1(1, null(), nalu_counts_arr.as_ptr(), null(), null());
    assert_eq!(
        call_hev1(hev1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "nalu_types が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hev1: `nalu_array_count > 0` で `nalu_counts` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hev1_null_nalu_counts_returns_null_pointer_error() {
    let nalu_types_arr: [u8; 1] = [32];
    let hev1 = base_hev1(1, nalu_types_arr.as_ptr(), null(), null(), null());
    assert_eq!(
        call_hev1(hev1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
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
        null(),
        nalu_sizes_arr.as_ptr(),
    );
    assert_eq!(
        call_hev1(hev1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
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
        null(),
    );
    assert_eq!(
        call_hev1(hev1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "nalu_sizes が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hev1: 後続イテレーションで初めて内側 null 検査に到達する境界パターン
///
/// `nalu_counts = [0, 1]` の場合、`i=0` では内側 for が走らず、
/// `i=1` で初めて `nalu_data` / `nalu_sizes` の null 検査に到達する。
/// 実装が「初回イテレーションだけ検査」に壊れたら失敗する回帰テスト。
#[test]
fn hev1_late_null_nalu_data_returns_null_pointer_error() {
    let nalu_types_arr: [u8; 2] = [32, 33];
    let nalu_counts_arr: [u32; 2] = [0, 1];
    let nalu_sizes_arr: [u32; 1] = [1];
    let hev1 = base_hev1(
        2,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        null(),
        nalu_sizes_arr.as_ptr(),
    );
    assert_eq!(
        call_hev1(hev1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "後続イテレーションで初めて到達する null 検査でも NULL_POINTER を返す必要がある"
    );
}

/// Hev1: 配列内 i 番目の `nalu_data[i]` が null のとき `MP4_ERROR_NULL_POINTER` を返す
///
/// ベースポインタ検査だけでは拾えない、既存の内側 null 検査経路を担保する
#[test]
fn hev1_null_nalu_element_returns_null_pointer_error() {
    let nalu_bytes: [u8; 1] = [0xAA];
    let nalu_types_arr: [u8; 1] = [32];
    let nalu_counts_arr: [u32; 1] = [2];
    // 2 個目の要素だけ null にする
    let nalu_data_arr: [*const u8; 2] = [nalu_bytes.as_ptr(), null()];
    let nalu_sizes_arr: [u32; 2] = [1, 1];
    let hev1 = base_hev1(
        1,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        nalu_data_arr.as_ptr(),
        nalu_sizes_arr.as_ptr(),
    );
    assert_eq!(
        call_hev1(hev1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "nalu_data の配列内要素が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hev1 非退行: `nalu_array_count == 0` なら全ポインタが null でも成功し
/// `nalu_arrays` は空になる
#[test]
fn hev1_nalu_array_count_zero_with_all_nulls_succeeds() {
    let hev1 = base_hev1(0, null(), null(), null(), null());
    let result = call_hev1(hev1).expect("nalu_array_count == 0 では成功する必要がある");
    let SampleEntry::Hev1(box_) = result else {
        panic!("Hev1 バリアントが返るべき");
    };
    assert!(
        box_.hvcc_box.nalu_arrays.is_empty(),
        "nalu_array_count == 0 なら nalu_arrays は空になる必要がある"
    );
}

/// Hev1 非退行: 全 `nalu_counts[i] == 0` なら `nalu_data` / `nalu_sizes` が
/// null でも成功し、`nalu_arrays` は空の要素を並べる
#[test]
fn hev1_null_nalu_data_with_all_counts_zero_succeeds() {
    let nalu_types_arr: [u8; 2] = [32, 33];
    let nalu_counts_arr: [u32; 2] = [0, 0];
    let hev1 = base_hev1(
        2,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        null(),
        null(),
    );
    let result = call_hev1(hev1).expect("全カウント 0 では成功する必要がある");
    let SampleEntry::Hev1(box_) = result else {
        panic!("Hev1 バリアントが返るべき");
    };
    // 2 個の nalu_type が並び、各配列の nalus は空であることを確認する
    assert_eq!(box_.hvcc_box.nalu_arrays.len(), 2);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nal_unit_type.get(), 32);
    assert!(box_.hvcc_box.nalu_arrays[0].nalus.is_empty());
    assert_eq!(box_.hvcc_box.nalu_arrays[1].nal_unit_type.get(), 33);
    assert!(box_.hvcc_box.nalu_arrays[1].nalus.is_empty());
}

/// Hev1: `nalu_counts = [1, 0]` の逆パターンで、`i=1` では内側 for が走らないため
/// `nalu_data_index(1, ...)` を呼び出さずに完了する
///
/// 前 test と対で、count が 0 のイテレーションが末尾に来ても
/// 過剰にデリファレンスが起きないことを担保する
#[test]
fn hev1_first_only_valid_nalu_succeeds() {
    let nalu_bytes: [u8; 1] = [0xAA];
    let nalu_types_arr: [u8; 2] = [32, 33];
    let nalu_counts_arr: [u32; 2] = [1, 0];
    // データは 1 個だけ用意する（i=1 で参照されるべきでないため）
    let nalu_data_arr: [*const u8; 1] = [nalu_bytes.as_ptr()];
    let nalu_sizes_arr: [u32; 1] = [1];
    let hev1 = base_hev1(
        2,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        nalu_data_arr.as_ptr(),
        nalu_sizes_arr.as_ptr(),
    );
    let result = call_hev1(hev1).expect("count[1] == 0 なので成功する必要がある");
    let SampleEntry::Hev1(box_) = result else {
        panic!("Hev1 バリアントが返るべき");
    };
    assert_eq!(box_.hvcc_box.nalu_arrays.len(), 2);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nalus.len(), 1);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nalus[0], nalu_bytes);
    assert!(box_.hvcc_box.nalu_arrays[1].nalus.is_empty());
}

// ---- HVC1 ----

/// Hvc1 正常系: 複数 NALU 配列を渡すと `SampleEntry::Hvc1` に正しく変換される
///
/// Hev1 と同じ非自明な累積 `nalu_counts = [2, 1, 3]` で
/// `nalu_data_index` のプレフィックス和を間接検証する
#[test]
fn hvc1_valid_multiple_nalu_arrays_succeeds_with_expected_content() {
    let nalu_bytes: [[u8; 1]; 6] = [[0xAA], [0xBB], [0xCC], [0xDD], [0xEE], [0xFF]];
    let nalu_types_arr: [u8; 3] = [32, 33, 34];
    let nalu_counts_arr: [u32; 3] = [2, 1, 3];
    let nalu_data_arr: [*const u8; 6] = [
        nalu_bytes[0].as_ptr(),
        nalu_bytes[1].as_ptr(),
        nalu_bytes[2].as_ptr(),
        nalu_bytes[3].as_ptr(),
        nalu_bytes[4].as_ptr(),
        nalu_bytes[5].as_ptr(),
    ];
    let nalu_sizes_arr: [u32; 6] = [1, 1, 1, 1, 1, 1];

    let hvc1 = base_hvc1(
        3,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        nalu_data_arr.as_ptr(),
        nalu_sizes_arr.as_ptr(),
    );
    let result = call_hvc1(hvc1).expect("正常な入力なので成功する必要がある");
    let SampleEntry::Hvc1(box_) = result else {
        panic!("Hvc1 バリアントが返るべき");
    };

    assert_eq!(box_.hvcc_box.nalu_arrays.len(), 3);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nal_unit_type.get(), 32);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nalus.len(), 2);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nalus[0], nalu_bytes[0]);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nalus[1], nalu_bytes[1]);
    assert_eq!(box_.hvcc_box.nalu_arrays[1].nal_unit_type.get(), 33);
    assert_eq!(box_.hvcc_box.nalu_arrays[1].nalus[0], nalu_bytes[2]);
    assert_eq!(box_.hvcc_box.nalu_arrays[2].nal_unit_type.get(), 34);
    assert_eq!(box_.hvcc_box.nalu_arrays[2].nalus.len(), 3);
    assert_eq!(box_.hvcc_box.nalu_arrays[2].nalus[0], nalu_bytes[3]);
    assert_eq!(box_.hvcc_box.nalu_arrays[2].nalus[1], nalu_bytes[4]);
    assert_eq!(box_.hvcc_box.nalu_arrays[2].nalus[2], nalu_bytes[5]);
}

/// Hvc1: `nalu_array_count > 0` で `nalu_types` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hvc1_null_nalu_types_returns_null_pointer_error() {
    let nalu_counts_arr: [u32; 1] = [0];
    let hvc1 = base_hvc1(1, null(), nalu_counts_arr.as_ptr(), null(), null());
    assert_eq!(
        call_hvc1(hvc1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "nalu_types が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hvc1: `nalu_array_count > 0` で `nalu_counts` が null のときは `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hvc1_null_nalu_counts_returns_null_pointer_error() {
    let nalu_types_arr: [u8; 1] = [32];
    let hvc1 = base_hvc1(1, nalu_types_arr.as_ptr(), null(), null(), null());
    assert_eq!(
        call_hvc1(hvc1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
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
        null(),
        nalu_sizes_arr.as_ptr(),
    );
    assert_eq!(
        call_hvc1(hvc1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
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
        null(),
    );
    assert_eq!(
        call_hvc1(hvc1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "nalu_sizes が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hvc1: 後続イテレーションで初めて内側 null 検査に到達する境界パターン
#[test]
fn hvc1_late_null_nalu_data_returns_null_pointer_error() {
    let nalu_types_arr: [u8; 2] = [32, 33];
    let nalu_counts_arr: [u32; 2] = [0, 1];
    let nalu_sizes_arr: [u32; 1] = [1];
    let hvc1 = base_hvc1(
        2,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        null(),
        nalu_sizes_arr.as_ptr(),
    );
    assert_eq!(
        call_hvc1(hvc1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "後続イテレーションで初めて到達する null 検査でも NULL_POINTER を返す必要がある"
    );
}

/// Hvc1: 配列内 i 番目の `nalu_data[i]` が null のとき `MP4_ERROR_NULL_POINTER` を返す
#[test]
fn hvc1_null_nalu_element_returns_null_pointer_error() {
    let nalu_bytes: [u8; 1] = [0xAA];
    let nalu_types_arr: [u8; 1] = [32];
    let nalu_counts_arr: [u32; 1] = [2];
    let nalu_data_arr: [*const u8; 2] = [nalu_bytes.as_ptr(), null()];
    let nalu_sizes_arr: [u32; 2] = [1, 1];
    let hvc1 = base_hvc1(
        1,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        nalu_data_arr.as_ptr(),
        nalu_sizes_arr.as_ptr(),
    );
    assert_eq!(
        call_hvc1(hvc1).err(),
        Some(Mp4Error::MP4_ERROR_NULL_POINTER),
        "nalu_data の配列内要素が null のときは MP4_ERROR_NULL_POINTER を返す必要がある"
    );
}

/// Hvc1 非退行: `nalu_array_count == 0` なら全ポインタが null でも成功する
#[test]
fn hvc1_nalu_array_count_zero_with_all_nulls_succeeds() {
    let hvc1 = base_hvc1(0, null(), null(), null(), null());
    let result = call_hvc1(hvc1).expect("nalu_array_count == 0 では成功する必要がある");
    let SampleEntry::Hvc1(box_) = result else {
        panic!("Hvc1 バリアントが返るべき");
    };
    assert!(
        box_.hvcc_box.nalu_arrays.is_empty(),
        "nalu_array_count == 0 なら nalu_arrays は空になる必要がある"
    );
}

/// Hvc1 非退行: 全 `nalu_counts[i] == 0` なら `nalu_data` / `nalu_sizes` が
/// null でも成功する
#[test]
fn hvc1_null_nalu_data_with_all_counts_zero_succeeds() {
    let nalu_types_arr: [u8; 2] = [32, 33];
    let nalu_counts_arr: [u32; 2] = [0, 0];
    let hvc1 = base_hvc1(
        2,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        null(),
        null(),
    );
    let result = call_hvc1(hvc1).expect("全カウント 0 では成功する必要がある");
    let SampleEntry::Hvc1(box_) = result else {
        panic!("Hvc1 バリアントが返るべき");
    };
    assert_eq!(box_.hvcc_box.nalu_arrays.len(), 2);
    assert!(box_.hvcc_box.nalu_arrays[0].nalus.is_empty());
    assert!(box_.hvcc_box.nalu_arrays[1].nalus.is_empty());
}

/// Hvc1: `nalu_counts = [1, 0]` の逆パターン
#[test]
fn hvc1_first_only_valid_nalu_succeeds() {
    let nalu_bytes: [u8; 1] = [0xAA];
    let nalu_types_arr: [u8; 2] = [32, 33];
    let nalu_counts_arr: [u32; 2] = [1, 0];
    let nalu_data_arr: [*const u8; 1] = [nalu_bytes.as_ptr()];
    let nalu_sizes_arr: [u32; 1] = [1];
    let hvc1 = base_hvc1(
        2,
        nalu_types_arr.as_ptr(),
        nalu_counts_arr.as_ptr(),
        nalu_data_arr.as_ptr(),
        nalu_sizes_arr.as_ptr(),
    );
    let result = call_hvc1(hvc1).expect("count[1] == 0 なので成功する必要がある");
    let SampleEntry::Hvc1(box_) = result else {
        panic!("Hvc1 バリアントが返るべき");
    };
    assert_eq!(box_.hvcc_box.nalu_arrays.len(), 2);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nalus.len(), 1);
    assert_eq!(box_.hvcc_box.nalu_arrays[0].nalus[0], nalu_bytes);
    assert!(box_.hvcc_box.nalu_arrays[1].nalus.is_empty());
}
