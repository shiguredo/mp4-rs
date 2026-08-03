//! `VpccBox::encode` の `codec_initialization_data` 長境界に関する回帰テスト
//!
//! 境界（`u16::MAX` / `u16::MAX + 1`）を安定して当てる必要があるため、
//! PBT ではなく単体テストとして置く。正常系のラウンドトリップは
//! `pbt/tests/prop_codec_boxes.rs` の `vpcc_box_roundtrip` が担う。

use shiguredo_mp4::{Decode, Encode, ErrorKind, Uint, boxes::VpccBox};

/// 指定した `codec_initialization_data` を持つ `VpccBox` を組み立てる
fn make_vpcc(codec_initialization_data: Vec<u8>) -> VpccBox {
    VpccBox {
        profile: 0,
        level: 10,
        bit_depth: Uint::new(8),
        chroma_subsampling: Uint::new(1),
        video_full_range_flag: Uint::new(0),
        colour_primaries: 1,
        transfer_characteristics: 1,
        matrix_coefficients: 1,
        codec_initialization_data,
    }
}

/// `codec_initialization_data.len() == u16::MAX` のとき encode が成功し、
/// roundtrip でデータが一致すること（PBT `arb_vpcc_box` が 0..50 バイトしか生成しないため、
/// 上限値ちょうどを over-reject しないことをここで押さえる。修正前挙動の回帰検出は
/// `..._exceeds_u16_max` が担う）
#[test]
fn vpcc_box_encode_codec_init_data_at_u16_max() {
    let vpcc = make_vpcc(vec![0xAB; usize::from(u16::MAX)]);

    let encoded = vpcc
        .encode_to_vec()
        .expect("u16::MAX バイトの codec_initialization_data は encode 可能であるはず");
    let (decoded, size) =
        VpccBox::decode(&encoded).expect("直前にエンコードした有効な VpccBox は必ずデコードできる");

    assert_eq!(size, encoded.len());
    // codec_initialization_data だけでなく全フィールドが roundtrip で保存されることを確認する
    // （65535 バイト特有のバッファ書き込みで直前のビットパックが壊れる回帰を検出できるように）
    assert_eq!(decoded, vpcc);
}

/// `codec_initialization_data.len() == u16::MAX + 1` のとき encode が
/// `InvalidInput` を返すこと（長さを黙って切り捨てない）
#[test]
fn vpcc_box_encode_codec_init_data_exceeds_u16_max() {
    let vpcc = make_vpcc(vec![0u8; usize::from(u16::MAX) + 1]);

    let err = vpcc
        .encode_to_vec()
        .expect_err("u16::MAX を超える codec_initialization_data は encode エラーであるはず");

    assert_eq!(
        err.kind,
        ErrorKind::InvalidInput,
        "エラー種別が InvalidInput ではない (実際は {:?})",
        err.kind,
    );
    // 現状 encode 側は with_box_type を通らないため box_type は None のはず。
    // encode 側でも box_type 付与するように変えたときにこの assert が落ちて意図的な変更だと気付ける
    assert_eq!(
        err.box_type, None,
        "encode 側は with_box_type を通っていないため box_type は None のはず (実際は {:?})",
        err.box_type,
    );
    // 実装側のエラー文言と密結合させないため、識別に必要な最小限のキーワードだけ確認する
    assert!(
        err.reason.contains("codec_initialization_data"),
        "エラー理由に対象フィールド名が含まれるはず (実際は {:?})",
        err.reason,
    );
}
