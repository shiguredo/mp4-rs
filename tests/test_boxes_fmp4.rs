//! `TfraBox::encode` のバッファ境界検査に関する回帰テスト
//!
//! `encode_variable_uint`（`src/boxes_fmp4.rs` 内の private 関数）は
//! `byte_count` 1〜3 のとき `buf.len()` を検査せず `buf[i]` へ直接代入していたため、
//! 短いバッファで `TfraBox::encode` を呼ぶとインデックス範囲外パニックに至っていた。
//! 本テストは、修正後に `ErrorKind::InsufficientBuffer` エラーが正しく返ることを、
//! 3 呼び出し位置（`traf_number` / `trun_number` / `sample_number`）× `byte_count`
//! 1〜3 の全 9 通りと、`byte_count = 4` の 1 ケース（`Encode for u32` 側の
//! 既存境界検査経路のサニティ）で検証する。
//!
//! テスト配置は shiguredo-rust の
//! 「単体テストのファイル名は `tests/test_<module>.rs` とし、
//! `src/<module>.rs` に対応させること」に従い `src/boxes_fmp4.rs` に対応させている。
//! 境界値のエラーパス検証は PBT では狙った境界（残バイト = `byte_count - 1`）を
//! 安定して当てにくく、目的（エラーパスの検証）とも合わないため単体テストとして置く。
//! 正常系のラウンドトリップは `pbt/tests/prop_fmp4_boxes.rs` の
//! `tfra_box_roundtrip` が担う。

use shiguredo_mp4::{
    Encode, ErrorKind,
    boxes::{TfraBox, TfraEntry},
};

/// `TfraBox::encode` の書き出しで、1 エントリ・version=0 の場合に
/// 「最初の可変長整数（`traf_number`）の直前」までに書かれるバイト数
///
/// 内訳:
/// - `BoxHeader` (`VARIABLE_SIZE` → U32(0) で 4 バイト) + `BoxType::Normal` (4 バイト) = 8
/// - `FullBoxHeader` (version + flags で 4 バイト)
/// - `track_id` (4 バイト)
/// - lengths フィールド (4 バイト)
/// - `number_of_entry` (4 バイト)
/// - version=0 の 1 エントリ分の `time` (4 バイト) + `moof_offset` (4 バイト) = 8
const OFFSET_BEFORE_FIRST_VARIABLE_UINT: usize = 8 + 4 + 4 + 4 + 4 + 8;

/// 1 エントリ・version=0 の `TfraBox` を組み立てる
fn make_single_entry_tfra(
    length_size_of_traf_num: u8,
    length_size_of_trun_num: u8,
    length_size_of_sample_num: u8,
) -> TfraBox {
    TfraBox {
        version: 0,
        track_id: 1,
        length_size_of_traf_num,
        length_size_of_trun_num,
        length_size_of_sample_num,
        entries: vec![TfraEntry {
            time: 0,
            moof_offset: 0,
            traf_number: 1,
            trun_number: 1,
            sample_number: 1,
        }],
    }
}

/// 与えられた長さのバッファで `TfraBox::encode` を呼び、
/// `ErrorKind::InsufficientBuffer` が返ることを確認する
#[track_caller]
fn assert_insufficient_buffer_err(tfra: &TfraBox, buf_len: usize, ctx: &str) {
    let mut buf = vec![0u8; buf_len];
    let err = tfra.encode(&mut buf).expect_err(&format!(
        "バッファ長 {buf_len} は不足しているはずが Ok が返った: {ctx}"
    ));
    assert_eq!(
        err.kind,
        ErrorKind::InsufficientBuffer,
        "エラー種別が InsufficientBuffer ではない: {ctx} (実際は {:?})",
        err.kind,
    );
}

/// `traf_number` の書き込みで、`byte_count` 1〜3 のそれぞれについて
/// 残バイトが `byte_count - 1` になるバッファで `InsufficientBuffer` が返ること
#[test]
fn traf_number_insufficient_buffer() {
    for length_size in 0u8..=2 {
        let byte_count = (length_size as usize) + 1;
        let tfra = make_single_entry_tfra(length_size, 0, 0);
        // `traf_number` 書き込み直前のオフセット + (byte_count - 1) で残バイトを byte_count 未満にする
        let buf_len = OFFSET_BEFORE_FIRST_VARIABLE_UINT + byte_count - 1;
        assert_insufficient_buffer_err(
            &tfra,
            buf_len,
            &format!("traf_number: length_size={length_size} byte_count={byte_count}"),
        );
    }
}

/// `trun_number` の書き込みで、`byte_count` 1〜3 のそれぞれについて
/// 残バイトが `byte_count - 1` になるバッファで `InsufficientBuffer` が返ること
///
/// `traf_number` は成功する必要があるので、`length_size_of_traf_num` を 0 に固定して
/// `traf_number` に 1 バイトを消費させ、その直後の `trun_number` を狙う
#[test]
fn trun_number_insufficient_buffer() {
    let length_size_traf: u8 = 0;
    let byte_count_traf: usize = (length_size_traf as usize) + 1;
    for length_size in 0u8..=2 {
        let byte_count = (length_size as usize) + 1;
        let tfra = make_single_entry_tfra(length_size_traf, length_size, 0);
        let buf_len = OFFSET_BEFORE_FIRST_VARIABLE_UINT + byte_count_traf + byte_count - 1;
        assert_insufficient_buffer_err(
            &tfra,
            buf_len,
            &format!("trun_number: length_size={length_size} byte_count={byte_count}"),
        );
    }
}

/// `sample_number` の書き込みで、`byte_count` 1〜3 のそれぞれについて
/// 残バイトが `byte_count - 1` になるバッファで `InsufficientBuffer` が返ること
///
/// `traf_number` と `trun_number` は成功する必要があるので、両方の `length_size` を 0 に
/// 固定して各 1 バイトを消費させ、その直後の `sample_number` を狙う
#[test]
fn sample_number_insufficient_buffer() {
    let length_size_traf: u8 = 0;
    let length_size_trun: u8 = 0;
    let byte_count_traf: usize = (length_size_traf as usize) + 1;
    let byte_count_trun: usize = (length_size_trun as usize) + 1;
    for length_size in 0u8..=2 {
        let byte_count = (length_size as usize) + 1;
        let tfra = make_single_entry_tfra(length_size_traf, length_size_trun, length_size);
        let buf_len =
            OFFSET_BEFORE_FIRST_VARIABLE_UINT + byte_count_traf + byte_count_trun + byte_count - 1;
        assert_insufficient_buffer_err(
            &tfra,
            buf_len,
            &format!("sample_number: length_size={length_size} byte_count={byte_count}"),
        );
    }
}

/// `byte_count = 4`（`Encode for u32` 側で既に境界検査が走る経路）のサニティチェック
///
/// 本 issue の修正対象は `byte_count` 1〜3 の 3 アームだが、
/// `encode_variable_uint` 全体の外形的な回帰として `byte_count = 4` 経由でも
/// `InsufficientBuffer` が返ることを同じテストファイル内で押さえておく
#[test]
fn byte_count_four_sanity_insufficient_buffer() {
    let length_size_traf: u8 = 3; // byte_count = 4
    let tfra = make_single_entry_tfra(length_size_traf, 0, 0);
    // `traf_number` 書き込み直前で残 3 バイトにする（byte_count = 4 未満）
    let buf_len = OFFSET_BEFORE_FIRST_VARIABLE_UINT + 3;
    assert_insufficient_buffer_err(
        &tfra,
        buf_len,
        "byte_count=4 の traf_number 直前で残 3 バイト",
    );
}
