//! `src/basic_types.rs` の意図的なエラーパス・境界値に関する回帰テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_basic_types.rs` が担う。
//! 本ファイルは PBT で安定して狙いにくい `decode_header_and_payload` の
//! size=0 / largesize=0 / サイズ下限の挙動を固定する。

mod decode_header_and_payload_size_zero {
    use shiguredo_mp4::{BoxHeader, BoxSize, BoxType, ErrorKind};

    /// size=0（`BoxSize::VARIABLE_SIZE`）のとき、ペイロードはヘッダー直後からバッファ末尾までになること
    #[test]
    fn u32_size_zero_uses_buffer_tail_as_payload() {
        // size=0 (4) + type "mdat" (4) + ペイロード 3 バイト
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"mdat");
        buf.extend_from_slice(&[0xAA, 0xBB, 0xCC]);

        let (header, payload) = BoxHeader::decode_header_and_payload(&buf)
            .expect("VARIABLE_SIZE はバッファ末尾までをボックスとしてデコードできる");

        assert_eq!(header.box_type, BoxType::Normal(*b"mdat"));
        assert_eq!(header.box_size, BoxSize::VARIABLE_SIZE);
        assert_eq!(
            payload,
            &buf[8..],
            "ペイロードはヘッダー直後からバッファ末尾までであること"
        );
    }

    /// largesize=0（`BoxSize::U64(0)`）は仕様未定義のためエラーになること
    #[test]
    fn u64_largesize_zero_returns_invalid_data() {
        // size=1 (4) + type "mdat" (4) + largesize=0 (8)
        let mut buf = Vec::new();
        buf.extend_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(b"mdat");
        buf.extend_from_slice(&0u64.to_be_bytes());
        // ペイロードを足しても largesize=0 はエラーになる
        buf.extend_from_slice(&[0xAA, 0xBB, 0xCC]);

        let err = BoxHeader::decode_header_and_payload(&buf)
            .expect_err("U64(0) は可変長として扱わずエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert!(
            err.reason.contains("box size is smaller than header size"),
            "エラー理由はサイズ下限違反であること: 実際は {}",
            err.reason
        );
    }

    /// size > 0 かつヘッダーより小さい場合はエラーになること（回帰防止）
    ///
    /// 現行では `BoxHeader::decode` 側の下限検査で `InvalidInput` になる。
    /// `decode_header_and_payload` 経由でも引き続きエラーになることを固定する。
    #[test]
    fn size_smaller_than_header_returns_error() {
        // size=4（ヘッダー 8 バイト未満）+ type "mdat"
        let mut buf = Vec::new();
        buf.extend_from_slice(&4u32.to_be_bytes());
        buf.extend_from_slice(b"mdat");

        let err = BoxHeader::decode_header_and_payload(&buf)
            .expect_err("ヘッダーより小さい size はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }
}
