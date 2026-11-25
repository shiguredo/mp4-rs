//! `src/boxes_fmp4.rs` の意図的なエラーパスに関する回帰テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_fmp4_boxes.rs` が担う。
//! 本ファイルは PBT で安定して狙いにくい境界・不整合入力のエラーパスを固定する。

/// `StypBox` の既知バイト列と意図的なエラーパスに関するテスト
///
/// ISO/IEC 14496-12:2022 8.16.2 は `styp` の構文を `ftyp` と同じ
/// major_brand (4 バイト) + minor_version (4 バイト) + compatible_brands の列と定めている。
/// 本モジュールは、その構文どおりのバイト列になることと、
/// 種別違い・compatible_brands の端数という 2 つの不整合入力を拒否することを固定する。
mod styp_box {
    use shiguredo_mp4::{
        BaseBox, BoxType, Decode, Encode, ErrorKind,
        boxes::{Brand, StypBox},
    };

    /// `StypBox` のエンコード結果が既知のバイト列になり、そのバイト列をデコードし直すと元の値に戻ること
    #[test]
    fn styp_box_known_bytes() {
        let styp = StypBox {
            major_brand: Brand::ISO6,
            minor_version: 0,
            compatible_brands: vec![Brand::ISO6, Brand::new(*b"cmff")],
        };
        let encoded = styp.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 (ヘッダー) + 4 (major_brand) + 4 (minor_version) + 4 * 2 (compatible_brands)
        expected.extend_from_slice(&24u32.to_be_bytes());
        expected.extend_from_slice(b"styp");
        expected.extend_from_slice(b"iso6");
        expected.extend_from_slice(&0u32.to_be_bytes());
        expected.extend_from_slice(b"iso6");
        expected.extend_from_slice(b"cmff");
        assert_eq!(
            encoded, expected,
            "StypBox のエンコード結果が既知のバイト列と一致しない"
        );

        let (decoded, size) = StypBox::decode(&encoded)
            .expect("直前にエンコードした有効な StypBox は必ずデコードできる");
        assert_eq!(
            size,
            encoded.len(),
            "decode の消費バイト数がエンコード結果の長さと一致しない"
        );
        assert_eq!(
            decoded, styp,
            "デコードした StypBox がエンコード前の値と一致しない"
        );
        assert_eq!(
            decoded.box_type(),
            BoxType::Normal(*b"styp"),
            "StypBox の box 種別が styp でない"
        );
    }

    /// ボックス種別が `styp` でない入力を `InvalidData` で拒否すること
    ///
    /// `styp` と `ftyp` は構文が同じなので、種別だけを `ftyp` にした入力を渡す。
    #[test]
    fn styp_box_rejects_other_box_type() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&16u32.to_be_bytes());
        buf.extend_from_slice(b"ftyp");
        buf.extend_from_slice(b"iso6");
        buf.extend_from_slice(&0u32.to_be_bytes());

        let err = StypBox::decode(&buf).expect_err("box 種別が styp でない入力は拒否される");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "box 種別違いのエラー種別が InvalidData でない: {:?}",
            err.kind
        );
        assert_eq!(
            err.box_type,
            Some(StypBox::TYPE),
            "エラーに StypBox の box 種別が設定されていない"
        );
    }

    /// `compatible_brands` のバイト数が 4 の倍数でない入力を `InvalidData` で拒否すること
    ///
    /// `Brand` は 4 バイトなので、端数がある入力をブランドの列として解釈できない。
    #[test]
    fn styp_box_rejects_partial_compatible_brand() {
        let mut buf = Vec::new();
        // size = 8 (ヘッダー) + 4 (major_brand) + 4 (minor_version) + 3 (端数)
        buf.extend_from_slice(&19u32.to_be_bytes());
        buf.extend_from_slice(b"styp");
        buf.extend_from_slice(b"iso6");
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"iso");

        let err =
            StypBox::decode(&buf).expect_err("compatible_brands に端数がある入力は拒否される");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "端数のエラー種別が InvalidData でない: {:?}",
            err.kind
        );
        assert!(
            err.reason.contains("multiple of 4"),
            "エラー理由に 4 の倍数でないことが含まれない: {}",
            err.reason
        );
    }
}

/// `TfraBox::encode` のバッファ境界検査に関する回帰テスト
///
/// `encode_variable_uint`（`src/boxes_fmp4.rs` 内の private 関数）は
/// `byte_count` 1〜3 のとき `buf.len()` を検査せず `buf[i]` へ直接代入していたため、
/// 短いバッファで `TfraBox::encode` を呼ぶとインデックス範囲外パニックに至っていた。
/// 本テストは、修正後に `ErrorKind::InsufficientBuffer` エラーが正しく返ることを、
/// 3 呼び出し位置（`traf_number` / `trun_number` / `sample_number`）× `byte_count`
/// 1〜3 の全 9 通りと、`byte_count = 4` を 1 ケース加えた計 10 ケースで検証する。
mod encode_variable_uint_insufficient_buffer {
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
            "エラー種別が `InsufficientBuffer` ではない: {ctx} (実際は {:?})",
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
                OFFSET_BEFORE_FIRST_VARIABLE_UINT + byte_count_traf + byte_count_trun + byte_count
                    - 1;
            assert_insufficient_buffer_err(
                &tfra,
                buf_len,
                &format!("sample_number: length_size={length_size} byte_count={byte_count}"),
            );
        }
    }

    /// `byte_count = 4` のサニティチェック
    ///
    /// 境界検査追加の対象は `byte_count` 1〜3 の 3 アームだが、
    /// `encode_variable_uint` 全体の外形的な回帰として `byte_count = 4` でも
    /// `InsufficientBuffer` が返ることを同じテストファイル内で押さえておく
    /// （`byte_count = 4` の書き込みは `value.encode(buf)` に委譲され、
    /// そこでも `check_buffer_size` が走るが、修正後は関数冒頭の検査が先に発火する）
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
}

/// `TfraBox::encode` が 0..=3 の範囲外の長さフィールドを拒否することの回帰テスト
///
/// ISO/IEC 14496-12:2015 の 8.8.10.2 と 8.8.10.3 では、3 つの長さフィールドはそれぞれ 2 ビットで 0..=3 を取る。
/// 以前は、バッファの長さが足りる場合、範囲外の値を渡すと次のように振る舞っていた。
///
/// - エントリーがある場合は `InvalidData` を返していた。ただし、オーバーフロー検査が有効なビルドでの 255 では、
///   バイト数を求める `+ 1` が `u8` で溢れてパニックしていた
/// - エントリーがない場合はエラーにせず、下位 2 ビットに切り詰めて書き出していた
mod tfra_length_size_out_of_range {
    use shiguredo_mp4::{
        Encode, ErrorKind,
        boxes::{TfraBox, TfraEntry},
    };

    /// version、長さフィールド、エントリーの有無を指定した `TfraBox` を組み立てる（エントリーありの場合は 1 件）
    fn make_tfra(
        version: u8,
        length_size_of_traf_num: u8,
        length_size_of_trun_num: u8,
        length_size_of_sample_num: u8,
        with_entry: bool,
    ) -> TfraBox {
        let entries = if with_entry {
            vec![TfraEntry {
                time: 0,
                moof_offset: 0,
                traf_number: 1,
                trun_number: 1,
                sample_number: 1,
            }]
        } else {
            Vec::new()
        };
        TfraBox {
            version,
            track_id: 1,
            length_size_of_traf_num,
            length_size_of_trun_num,
            length_size_of_sample_num,
            entries,
        }
    }

    /// 長さフィールドを 1 つずつ、境界直後の 4 と `u8::MAX` にしてエンコードすると、
    /// version、エントリーの有無、バッファの長さにかかわらず `InvalidInput` になること
    #[test]
    fn length_size_out_of_range_is_invalid_input() {
        for value in [4, u8::MAX] {
            // 対象のフィールドだけを範囲外にし、残りは 0 にする
            for (name, (traf, trun, sample)) in [
                ("length_size_of_traf_num", (value, 0, 0)),
                ("length_size_of_trun_num", (0, value, 0)),
                ("length_size_of_sample_num", (0, 0, value)),
            ] {
                for version in [0, 1] {
                    for with_entry in [false, true] {
                        let tfra = make_tfra(version, traf, trun, sample, with_entry);
                        let ctx = format!(
                            "{name}={value} (version={version}, エントリーあり={with_entry})"
                        );

                        let err = tfra.encode_to_vec().expect_err(&format!(
                            "範囲外の長さフィールドなのに Ok が返った: {ctx}"
                        ));
                        assert_eq!(
                            err.kind,
                            ErrorKind::InvalidInput,
                            "エラー種別が `InvalidInput` ではない: {ctx} (reason={})",
                            err.reason
                        );
                        assert!(
                            err.reason.contains(name),
                            "理由文字列が対象のフィールド名を含まない: {ctx} (reason={})",
                            err.reason
                        );

                        // 検査がヘッダーの書き込みより前にあることを、空のバッファでも `InvalidInput` になることで確かめる
                        let err = tfra.encode(&mut []).expect_err(&format!(
                            "空のバッファに encode したのに Ok が返った: {ctx}"
                        ));
                        assert_eq!(
                            err.kind,
                            ErrorKind::InvalidInput,
                            "空のバッファでのエラー種別が `InvalidInput` ではない: {ctx} (reason={})",
                            err.reason
                        );
                    }
                }
            }
        }
    }
}

/// `tfdt` と `tfra` の未対応バージョン（0 と 1 以外）の扱いに関する回帰テスト
///
/// ISO/IEC 14496-12:2015 の 8.8.12.2 と 8.8.12.3 は `tfdt` のバージョン 0 と 1 を定義し、
/// このライブラリは `tfra` も 0 と 1 に対応する。
/// 同規格の 4.2 は、認識できないバージョンのボックスを無視して読み飛ばすことを求めている。
/// 以前は未対応バージョンをバージョン 0 と同じ 32 ビットの構文として読んでいた。エンコードでは、
/// 値が 32 ビットに収まればそのバージョンのまま書き出し、収まらなければバージョン 1 に置き換えていた。
///
/// 親の `traf` / `mfra` が未対応バージョンの子を読み飛ばすことは、PBT
/// （`pbt/tests/prop_fmp4_boxes.rs` の `traf_box_skips_unsupported_version_tfdt` など）で確認する
mod tfdt_tfra_unsupported_version {
    use shiguredo_mp4::{
        Decode, Encode, ErrorKind,
        boxes::{MfraBox, MfroBox, TfdtBox, TfhdBox, TfraBox, TfraEntry, TrafBox},
    };

    /// 本文を指定した FullBox（flags は 0）のバイト列を、通常ヘッダー（32 ビットの size と種別）付きで組み立てる
    fn full_box_bytes(box_type: &[u8; 4], version: u8, body: &[u8]) -> Vec<u8> {
        wrap_box(box_type, &[&[version, 0, 0, 0], body])
    }

    /// ペイロードのバイト列（子ボックス列など）を連結し、通常ヘッダー（32 ビットの size と種別）で包む
    fn wrap_box(box_type: &[u8; 4], children: &[&[u8]]) -> Vec<u8> {
        let payload_len: usize = children.iter().map(|c| c.len()).sum();
        let size = u32::try_from(8 + payload_len).expect("テスト用のボックスは u32 に収まる大きさ");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&size.to_be_bytes());
        bytes.extend_from_slice(box_type);
        for child in children {
            bytes.extend_from_slice(child);
        }
        bytes
    }

    /// traf の先頭に置く tfhd のバイト列
    fn tfhd_bytes() -> Vec<u8> {
        TfhdBox {
            track_id: 1,
            base_data_offset: None,
            sample_description_index: None,
            default_sample_duration: None,
            default_sample_size: None,
            default_sample_flags: None,
            duration_is_empty: false,
            default_base_is_moof: false,
        }
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない")
    }

    /// 未対応バージョンの tfdt / tfra を単体でデコードすると、バージョン別のフィールドを読む前に `InvalidData` になること
    ///
    /// FullBox ヘッダーの後にバイトを置かないため、フィールドを先に読むと `InsufficientBuffer` になる
    #[test]
    fn standalone_decode_rejects_unsupported_version() {
        for version in [2, u8::MAX] {
            let ctx = format!("version={version}");

            let err = TfdtBox::decode(&full_box_bytes(b"tfdt", version, &[])).expect_err(&format!(
                "未対応バージョンの tfdt なのに Ok が返った: {ctx}"
            ));
            assert_eq!(
                err.kind,
                ErrorKind::InvalidData,
                "tfdt のエラー種別が `InvalidData` ではない: {ctx} (reason={})",
                err.reason
            );

            let err = TfraBox::decode(&full_box_bytes(b"tfra", version, &[])).expect_err(&format!(
                "未対応バージョンの tfra なのに Ok が返った: {ctx}"
            ));
            assert_eq!(
                err.kind,
                ErrorKind::InvalidData,
                "tfra のエラー種別が `InvalidData` ではない: {ctx} (reason={})",
                err.reason
            );
        }
    }

    /// 保持するバージョンが範囲外の tfdt / tfra をエンコードすると、値が 32 ビットに収まるかどうかにかかわらず
    /// `InvalidInput` になること
    ///
    /// 空のバッファでも `InvalidInput` になることで、検査がヘッダーの書き込みより前にあることも確かめる
    #[test]
    fn encode_rejects_unsupported_version() {
        for version in [2, u8::MAX] {
            for value in [0, u64::from(u32::MAX) + 1] {
                let ctx = format!("version={version} value={value}");

                let tfdt = TfdtBox {
                    version,
                    base_media_decode_time: value,
                };
                let tfra = TfraBox {
                    version,
                    track_id: 1,
                    length_size_of_traf_num: 0,
                    length_size_of_trun_num: 0,
                    length_size_of_sample_num: 0,
                    entries: vec![TfraEntry {
                        time: value,
                        moof_offset: 0,
                        traf_number: 1,
                        trun_number: 1,
                        sample_number: 1,
                    }],
                };

                for (name, result) in [
                    ("tfdt", tfdt.encode_to_vec().map(|_| ())),
                    ("tfdt (空のバッファ)", tfdt.encode(&mut []).map(|_| ())),
                    ("tfra", tfra.encode_to_vec().map(|_| ())),
                    ("tfra (空のバッファ)", tfra.encode(&mut []).map(|_| ())),
                ] {
                    let err = result.expect_err(&format!(
                        "範囲外のバージョンなのに Ok が返った: {name} {ctx}"
                    ));
                    assert_eq!(
                        err.kind,
                        ErrorKind::InvalidInput,
                        "エラー種別が `InvalidInput` ではない: {name} {ctx} (reason={})",
                        err.reason
                    );
                }
            }
        }
    }

    /// サイズが 0 の未対応バージョンの子は、親の残り全体を子の範囲とはみなさず `InvalidData` になること
    ///
    /// エラーの `box_type` は、traf / mfra のどちらでも親ではなく子（tfdt / tfra）になる。
    /// traf では、対応バージョンの tfdt を採用済みかどうかにかかわらず確かめる。
    /// mfra では mfro を先に置くため、誤って残り全体を読み飛ばすと Ok になる
    #[test]
    fn parent_rejects_unsupported_version_child_with_size_zero() {
        let mut tfdt = full_box_bytes(b"tfdt", 2, &[0, 0, 0, 0]);
        // size フィールドを 0（ファイルの末尾まで）に書き換える
        tfdt[..4].copy_from_slice(&0u32.to_be_bytes());
        let adopted_tfdt = TfdtBox {
            version: 0,
            base_media_decode_time: 1,
        }
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない");
        for (name, traf) in [
            ("tfdt なし", wrap_box(b"traf", &[&tfhd_bytes(), &tfdt])),
            (
                "tfdt を採用済み",
                wrap_box(b"traf", &[&tfhd_bytes(), &adopted_tfdt, &tfdt]),
            ),
        ] {
            let err = TrafBox::decode(&traf).expect_err(&format!(
                "サイズ 0 の未対応バージョンの tfdt を含む traf なのに Ok が返った: {name}"
            ));
            assert_eq!(
                err.kind,
                ErrorKind::InvalidData,
                "エラー種別が `InvalidData` ではない: {name}"
            );
            assert_eq!(
                err.box_type,
                Some(TfdtBox::TYPE),
                "エラーの box_type が tfdt ではない: {name}"
            );
        }

        let mut tfra = full_box_bytes(b"tfra", 2, &[0, 0, 0, 0]);
        // size フィールドを 0（ファイルの末尾まで）に書き換える
        tfra[..4].copy_from_slice(&0u32.to_be_bytes());
        let mfro = MfroBox { size: 0 }
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");
        let mfra = wrap_box(b"mfra", &[&mfro, &tfra]);
        let err = MfraBox::decode(&mfra)
            .expect_err("サイズ 0 の未対応バージョンの tfra を含む mfra なのに Ok が返った");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が `InvalidData` ではない: mfra"
        );
        assert_eq!(
            err.box_type,
            Some(TfraBox::TYPE),
            "エラーの box_type が tfra ではない: mfra"
        );
    }

    /// 子の外形の検査でエラーになる tfdt / tfra は、バージョンにかかわらず読み飛ばさず、
    /// 従来どおり子の `box_type` を設定したエラーを返すこと
    ///
    /// 子のサイズが親の残りより 1 バイト大きい。mfra では mfro を先に置くため、誤って読み飛ばすと Ok になる
    #[test]
    fn parent_returns_error_for_child_exceeding_parent() {
        for version in [0, 2] {
            // 本文はバージョン 0 の base_media_decode_time（4 バイト）にし、エラーの原因を親の範囲の超過だけにする
            let mut tfdt = full_box_bytes(b"tfdt", version, &[0, 0, 0, 0]);
            let oversized =
                u32::try_from(tfdt.len() + 1).expect("テスト用のボックスは u32 に収まる大きさ");
            tfdt[..4].copy_from_slice(&oversized.to_be_bytes());
            let traf = wrap_box(b"traf", &[&tfhd_bytes(), &tfdt]);
            let err = TrafBox::decode(&traf).expect_err(&format!(
                "親の範囲を超える tfdt を含む traf なのに Ok が返った: version={version}"
            ));
            assert_eq!(
                err.kind,
                ErrorKind::InsufficientBuffer,
                "エラー種別が `InsufficientBuffer` ではない: tfdt version={version}"
            );
            assert_eq!(
                err.box_type,
                Some(TfdtBox::TYPE),
                "エラーの box_type が tfdt ではない: version={version}"
            );

            // 本文はエントリー数 0 のバージョン 0 の構文（track_id、長さフィールド、エントリー数の 12 バイト）にし、
            // エラーの原因を親の範囲の超過だけにする
            let mut tfra = full_box_bytes(b"tfra", version, &[0; 12]);
            let oversized =
                u32::try_from(tfra.len() + 1).expect("テスト用のボックスは u32 に収まる大きさ");
            tfra[..4].copy_from_slice(&oversized.to_be_bytes());
            let mfro = MfroBox { size: 0 }
                .encode_to_vec()
                .expect("Vec への書き込みは失敗しない");
            let mfra = wrap_box(b"mfra", &[&mfro, &tfra]);
            let err = MfraBox::decode(&mfra).expect_err(&format!(
                "親の範囲を超える tfra を含む mfra なのに Ok が返った: version={version}"
            ));
            assert_eq!(
                err.kind,
                ErrorKind::InsufficientBuffer,
                "エラー種別が `InsufficientBuffer` ではない: tfra version={version}"
            );
            assert_eq!(
                err.box_type,
                Some(TfraBox::TYPE),
                "エラーの box_type が tfra ではない: version={version}"
            );
        }
    }

    /// 対応バージョンの子のデコードエラーは、引き続き親から返ること
    ///
    /// バージョン 0 の tfdt / tfra で、FullBox ヘッダーの後に必要なフィールドがない
    #[test]
    fn parent_returns_error_for_supported_version_child() {
        let tfdt = full_box_bytes(b"tfdt", 0, &[]);
        let traf = wrap_box(b"traf", &[&tfhd_bytes(), &tfdt]);
        let err = TrafBox::decode(&traf)
            .expect_err("壊れたバージョン 0 の tfdt を含む traf はエラーになる");
        assert_eq!(err.box_type, Some(TfdtBox::TYPE));

        let tfra = full_box_bytes(b"tfra", 0, &[]);
        let mfro = MfroBox { size: 0 }
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");
        let mfra = wrap_box(b"mfra", &[&tfra, &mfro]);
        let err = MfraBox::decode(&mfra)
            .expect_err("壊れたバージョン 0 の tfra を含む mfra はエラーになる");
        assert_eq!(err.box_type, Some(TfraBox::TYPE));
    }

    /// 子の範囲内で FullBox ヘッダーを読めない tfdt は、読み飛ばさずに従来どおり扱うこと
    ///
    /// ペイロードが 2 バイト（0xFF 0xFF）しかない tfdt の後ろに trun を置く。FullBox ヘッダーを子の範囲の外まで読むと、
    /// tfdt のペイロードと trun の先頭 2 バイトを FullBox ヘッダーとして読み、バージョンが 255（未対応）になって
    /// tfdt を読み飛ばしてしまう。読み飛ばすと、前半では `unknown_boxes` が空になり、後半ではエラーにならない
    #[test]
    fn traf_does_not_skip_tfdt_without_full_box_header() {
        let tfdt = TfdtBox {
            version: 0,
            base_media_decode_time: 1,
        }
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない");
        // ペイロードが 2 バイトしかなく、FullBox ヘッダー（4 バイト）を読めない tfdt
        let short_tfdt = wrap_box(b"tfdt", &[&[0xFF, 0xFF]]);
        let trun = full_box_bytes(b"trun", 0, &[0, 0, 0, 0]);

        // 採用済みの tfdt の後にある場合は、重複した tfdt として UnknownBox に残る
        let traf = wrap_box(b"traf", &[&tfhd_bytes(), &tfdt, &short_tfdt, &trun]);
        let (decoded, _) =
            TrafBox::decode(&traf).expect("外形が正しい重複した tfdt を含む traf はデコードできる");
        assert_eq!(decoded.tfdt_box.map(|b| b.base_media_decode_time), Some(1));
        assert_eq!(decoded.unknown_boxes.len(), 1);
        assert_eq!(decoded.unknown_boxes[0].box_type, TfdtBox::TYPE);
        assert_eq!(decoded.trun_boxes.len(), 1);

        // 最初の tfdt の場合は、型付きのデコードでエラーになる
        let traf = wrap_box(b"traf", &[&tfhd_bytes(), &short_tfdt, &trun]);
        let err = TrafBox::decode(&traf)
            .expect_err("FullBox ヘッダーを読めない最初の tfdt を含む traf はエラーになる");
        assert_eq!(err.kind, ErrorKind::InsufficientBuffer);
        assert_eq!(err.box_type, Some(TfdtBox::TYPE));
    }
}

/// `TrunBox::encode` がサンプル間の per-sample `Option` 不整合を拒否することの回帰テスト
///
/// ISO/IEC 14496-12 8.8.8 の trun では duration / size / flags / composition_time_offset の
/// 有無フラグ (`tr_flags`) が run 全体共通のため、サンプルごとに `Some` / `None` が混在する
/// 入力は表現できない。以前は先頭サンプルだけでフラグを決め、後続の値を黙って落としていた
/// （先頭 `Some`・後続 `None` の逆方向は `unwrap_or(0)` で 0 を書き込んでいた）。
///
/// PBT (`pbt/tests/prop_fmp4_boxes.rs::arb_trun_box`) は一貫性のあるサンプルだけを生成する
/// ため、このエラーパスは本ファイルで固定する。
mod trun_sample_option_consistency {
    use shiguredo_mp4::{
        Encode, ErrorKind, SampleFlags,
        boxes::{TrunBox, TrunSample, TrunSamples},
    };

    /// 全フィールド `None` のベースサンプル
    fn empty_sample() -> TrunSample {
        TrunSample {
            duration: None,
            size: None,
            flags: None,
            composition_time_offset: None,
        }
    }

    /// 先頭と 2 番目で指定フィールドの `Option` 有無だけを食い違わせた `TrunBox` を作る
    fn trun_with_inconsistent_field(
        set_first: impl FnOnce(&mut TrunSample),
        set_second: impl FnOnce(&mut TrunSample),
    ) -> TrunBox {
        let mut first = empty_sample();
        let mut second = empty_sample();
        set_first(&mut first);
        set_second(&mut second);
        TrunBox {
            data_offset: None,
            first_sample_flags: None,
            samples: TrunSamples::PerSample(vec![first, second]),
        }
    }

    /// encode が per-sample Option 整合性チェックの経路で `ErrorKind::InvalidInput` を返すことを確認する
    ///
    /// `TrunBox::encode` には他の `InvalidInput` 経路（cto の version 0 / 1 範囲外など）もあるので、
    /// `err.kind` の一致だけでは狙った validate 経路を確実には特定できない。
    /// `validate_sample_option_consistency` が返す文言に含まれる特徴的な句 `"inconsistent Option presence"` を
    /// 併せて確認し、別経路の `InvalidInput` を「合格」と誤認するリスクを消す。
    #[track_caller]
    fn assert_invalid_input_on_encode(trun: &TrunBox, ctx: &str) {
        let err = trun
            .encode_to_vec()
            .expect_err(&format!("不整合入力なのに Ok が返った: {ctx}"));
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が `InvalidInput` ではない: {ctx} (実際は {:?}, reason={})",
            err.kind,
            err.reason,
        );
        assert!(
            err.reason.contains("inconsistent Option presence"),
            "エラーが per-sample Option 整合性チェックの経路から出ていない: {ctx} (reason={})",
            err.reason,
        );
    }

    /// 先頭 `None`・後続 `Some` の不整合で各フィールドが `InvalidInput` になること
    #[test]
    fn none_then_some_is_invalid_input() {
        assert_invalid_input_on_encode(
            &trun_with_inconsistent_field(|_| {}, |s| s.duration = Some(100)),
            "duration: None → Some",
        );
        assert_invalid_input_on_encode(
            &trun_with_inconsistent_field(|_| {}, |s| s.size = Some(200)),
            "size: None → Some",
        );
        assert_invalid_input_on_encode(
            &trun_with_inconsistent_field(|_| {}, |s| s.flags = Some(SampleFlags::empty())),
            "flags: None → Some",
        );
        assert_invalid_input_on_encode(
            &trun_with_inconsistent_field(|_| {}, |s| s.composition_time_offset = Some(10)),
            "composition_time_offset: None → Some",
        );
    }

    /// 先頭 `Some`・後続 `None` の不整合で各フィールドが `InvalidInput` になること
    #[test]
    fn some_then_none_is_invalid_input() {
        assert_invalid_input_on_encode(
            &trun_with_inconsistent_field(|s| s.duration = Some(100), |_| {}),
            "duration: Some → None",
        );
        assert_invalid_input_on_encode(
            &trun_with_inconsistent_field(|s| s.size = Some(200), |_| {}),
            "size: Some → None",
        );
        assert_invalid_input_on_encode(
            &trun_with_inconsistent_field(|s| s.flags = Some(SampleFlags::empty()), |_| {}),
            "flags: Some → None",
        );
        assert_invalid_input_on_encode(
            &trun_with_inconsistent_field(|s| s.composition_time_offset = Some(10), |_| {}),
            "composition_time_offset: Some → None",
        );
    }
}

// ===== 境界値テスト =====

mod fmp4_boxes_boundary_tests {
    use shiguredo_mp4::{
        BaseBox, Decode, Encode, ErrorKind, FullBox, SampleFlags,
        boxes::{
            MehdBox, MfhdBox, MoofBox, MvexBox, SidxBox, SidxReference, TfdtBox, TfhdBox, TfraBox,
            TfraEntry, TrafBox, TrexBox, TrunBox, TrunSample, TrunSamples,
        },
    };

    /// MehdBox: version 0 (32-bit duration)
    #[test]
    fn mehd_box_version0() {
        let mehd = MehdBox {
            fragment_duration: u32::MAX as u64,
        };
        assert_eq!(mehd.full_box_version(), 0);

        let encoded = mehd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MehdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MehdBox は必ずデコードできる");
        assert_eq!(decoded.fragment_duration, u32::MAX as u64);
    }

    /// MehdBox: version 1 (64-bit duration)
    #[test]
    fn mehd_box_version1() {
        let mehd = MehdBox {
            fragment_duration: u32::MAX as u64 + 1,
        };
        assert_eq!(mehd.full_box_version(), 1);

        let encoded = mehd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MehdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MehdBox は必ずデコードできる");
        assert_eq!(decoded.fragment_duration, u32::MAX as u64 + 1);
    }

    /// TfdtBox: version 0 (32-bit time)
    #[test]
    fn tfdt_box_version0() {
        let tfdt = TfdtBox {
            version: 0,
            base_media_decode_time: u32::MAX as u64,
        };
        assert_eq!(tfdt.full_box_version(), 0);

        let encoded = tfdt.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfdtBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfdtBox は必ずデコードできる");
        assert_eq!(decoded.base_media_decode_time, u32::MAX as u64);
    }

    /// TfdtBox: version 1 (64-bit time)
    #[test]
    fn tfdt_box_version1() {
        let tfdt = TfdtBox {
            version: 1,
            base_media_decode_time: u32::MAX as u64 + 1,
        };
        assert_eq!(tfdt.full_box_version(), 1);

        let encoded = tfdt.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfdtBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfdtBox は必ずデコードできる");
        assert_eq!(decoded.base_media_decode_time, u32::MAX as u64 + 1);
    }

    /// TfhdBox: 全フラグなし
    #[test]
    fn tfhd_box_no_flags() {
        let tfhd = TfhdBox {
            track_id: 1,
            base_data_offset: None,
            sample_description_index: None,
            default_sample_duration: None,
            default_sample_size: None,
            default_sample_flags: None,
            duration_is_empty: false,
            default_base_is_moof: false,
        };
        assert_eq!(tfhd.full_box_flags().get(), 0);

        let encoded = tfhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfhdBox は必ずデコードできる");
        assert_eq!(decoded.track_id, 1);
        assert!(decoded.base_data_offset.is_none());
    }

    /// TfhdBox: 全フラグあり
    #[test]
    fn tfhd_box_all_flags() {
        let tfhd = TfhdBox {
            track_id: 1,
            base_data_offset: Some(100),
            sample_description_index: Some(1),
            default_sample_duration: Some(1024),
            default_sample_size: Some(512),
            default_sample_flags: Some(SampleFlags::new(0x01010000)),
            duration_is_empty: true,
            default_base_is_moof: true,
        };

        let flags = tfhd.full_box_flags().get();
        assert!(flags & TfhdBox::FLAG_BASE_DATA_OFFSET_PRESENT != 0);
        assert!(flags & TfhdBox::FLAG_SAMPLE_DESCRIPTION_INDEX_PRESENT != 0);
        assert!(flags & TfhdBox::FLAG_DEFAULT_SAMPLE_DURATION_PRESENT != 0);
        assert!(flags & TfhdBox::FLAG_DEFAULT_SAMPLE_SIZE_PRESENT != 0);
        assert!(flags & TfhdBox::FLAG_DEFAULT_SAMPLE_FLAGS_PRESENT != 0);
        assert!(flags & TfhdBox::FLAG_DURATION_IS_EMPTY != 0);
        assert!(flags & TfhdBox::FLAG_DEFAULT_BASE_IS_MOOF != 0);

        let encoded = tfhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfhdBox は必ずデコードできる");
        assert_eq!(decoded.base_data_offset, Some(100));
        assert_eq!(decoded.sample_description_index, Some(1));
        assert_eq!(decoded.default_sample_duration, Some(1024));
        assert_eq!(decoded.default_sample_size, Some(512));
        assert!(decoded.duration_is_empty);
        assert!(decoded.default_base_is_moof);
    }

    /// デコードした TrunBox のサンプル行を返す（サンプルごとの値を持つ形式であることを前提にする）
    #[track_caller]
    fn per_sample_rows(trun: &TrunBox) -> &[TrunSample] {
        match &trun.samples {
            TrunSamples::PerSample(samples) => samples,
            TrunSamples::CountOnly(count) => {
                panic!("サンプルごとの値を持つ形式を期待したが、件数だけの形式 ({count} 件) だった")
            }
        }
    }

    /// version 0 の `trun` のバイト列を組み立てる
    ///
    /// `fields` には `sample_count` より後ろのフィールド（`data_offset`、`first_sample_flags`、サンプル行）を
    /// 並べたバイト列を渡す
    fn trun_bytes(tr_flags: u32, sample_count: u32, fields: &[u8]) -> Vec<u8> {
        // 通常ヘッダー 8 バイト、FullBox ヘッダー 4 バイト、sample_count 4 バイト
        let size = u32::try_from(16 + fields.len()).expect("trun のサイズは u32 に収まる");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&size.to_be_bytes());
        bytes.extend_from_slice(b"trun");
        // version 0 なので、FullBox ヘッダーの 4 バイトは tr_flags の値そのものになる
        bytes.extend_from_slice(&tr_flags.to_be_bytes());
        bytes.extend_from_slice(&sample_count.to_be_bytes());
        bytes.extend_from_slice(fields);
        bytes
    }

    /// TrunBox: サンプルごとのフィールドのフラグが立っていても、`sample_count` が 0 なら件数だけの形式になる
    ///
    /// 行が 0 個なので保持する値がない。エンコードはサンプル行がないためフラグを立てず、
    /// 再デコードしても同じ値に戻る。このようなバイト列はこのクレートのエンコードでは作れないため、
    /// PBT ではなくバイト列を固定して確認する
    #[test]
    fn trun_box_zero_sample_count_with_per_sample_flags_is_count_only() {
        let flags = TrunBox::FLAG_DATA_OFFSET_PRESENT
            | TrunBox::FLAG_SAMPLE_DURATION_PRESENT
            | TrunBox::FLAG_SAMPLE_SIZE_PRESENT;
        let bytes = trun_bytes(flags, 0, &8i32.to_be_bytes());
        let (decoded, size) =
            TrunBox::decode(&bytes).expect("サンプル行が 0 個の trun はデコードできる");
        assert_eq!(size, bytes.len());
        assert_eq!(decoded.samples, TrunSamples::CountOnly(0));
        assert_eq!(decoded.data_offset, Some(8));

        // サンプル行がないので、エンコードはサンプルごとのフィールドのフラグを立てない
        let reencoded = decoded
            .encode_to_vec()
            .expect("デコードした TrunBox は必ずエンコードできる");
        assert_eq!(
            reencoded,
            trun_bytes(TrunBox::FLAG_DATA_OFFSET_PRESENT, 0, &8i32.to_be_bytes())
        );
        let (redecoded, _) =
            TrunBox::decode(&reencoded).expect("再エンコードした TrunBox は必ずデコードできる");
        assert_eq!(redecoded, decoded, "再デコードしても同じ値に戻る");
    }

    /// TrunBox: サンプル行が残りのペイロードに収まらない `sample_count` は `InvalidData` になる
    ///
    /// 残りのペイロードは `data_offset` を読んだ後から数える。ちょうど収まる件数は受理する
    #[test]
    fn trun_box_per_sample_rows_must_fit_in_payload() {
        let flags = TrunBox::FLAG_DATA_OFFSET_PRESENT | TrunBox::FLAG_SAMPLE_DURATION_PRESENT;
        // data_offset（4 バイト）の後に、duration だけのサンプル行（4 バイト）を 1 つ置く
        let mut fields = 8i32.to_be_bytes().to_vec();
        fields.extend_from_slice(&1024u32.to_be_bytes());

        let bytes = trun_bytes(flags, 1, &fields);
        let (decoded, size) =
            TrunBox::decode(&bytes).expect("サンプル行がちょうど収まる件数はデコードできる");
        assert_eq!(size, bytes.len());
        assert_eq!(decoded.data_offset, Some(8));
        assert_eq!(
            decoded.samples,
            TrunSamples::PerSample(vec![TrunSample {
                duration: Some(1024),
                size: None,
                flags: None,
                composition_time_offset: None,
            }])
        );

        let bytes = trun_bytes(flags, 2, &fields);
        let err = TrunBox::decode(&bytes)
            .expect_err("サンプル行が 1 つ分足りない件数はデコードに失敗する");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert_eq!(err.reason, "TrunBox sample_count exceeds available payload");
    }

    /// TrunBox: サンプルごとのフィールドがない場合の件数の上限
    ///
    /// 上限ちょうどの件数は件数だけの形式でデコードでき、上限を 1 つ超える件数は `InvalidData` になる
    #[test]
    fn trun_box_count_only_sample_count_limit() {
        const MAX_SAMPLES_WITHOUT_DATA: u32 = 16_777_216;

        let bytes = trun_bytes(0, MAX_SAMPLES_WITHOUT_DATA, &[]);
        let (decoded, size) =
            TrunBox::decode(&bytes).expect("上限ちょうどの件数は件数だけの形式でデコードできる");
        assert_eq!(size, bytes.len());
        assert_eq!(
            decoded.samples,
            TrunSamples::CountOnly(MAX_SAMPLES_WITHOUT_DATA)
        );

        let bytes = trun_bytes(0, MAX_SAMPLES_WITHOUT_DATA + 1, &[]);
        let err = TrunBox::decode(&bytes).expect_err("上限を超える件数はデコードに失敗する");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert_eq!(
            err.reason,
            "TrunBox sample_count exceeds maximum allowed when no per-sample data is present"
        );
    }

    /// TrunBox: 複数のサンプル
    #[test]
    fn trun_box_multiple_samples() {
        let trun = TrunBox {
            data_offset: None,
            first_sample_flags: Some(SampleFlags::new(0x02000000)),
            samples: TrunSamples::PerSample(vec![
                TrunSample {
                    duration: Some(1024),
                    size: Some(512),
                    flags: Some(SampleFlags::new(0x01010000)),
                    composition_time_offset: Some(0),
                },
                TrunSample {
                    duration: Some(1024),
                    size: Some(256),
                    flags: Some(SampleFlags::new(0x01010000)),
                    composition_time_offset: Some(1024),
                },
            ]),
        };

        let encoded = trun.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TrunBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrunBox は必ずデコードできる");
        let samples = per_sample_rows(&decoded);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].duration, Some(1024));
        assert_eq!(samples[1].size, Some(256));
    }

    /// TrunBox: 負の composition_time_offset (version 1)
    #[test]
    fn trun_box_negative_cto() {
        let trun = TrunBox {
            data_offset: None,
            first_sample_flags: None,
            samples: TrunSamples::PerSample(vec![TrunSample {
                duration: Some(1024),
                size: Some(512),
                flags: None,
                composition_time_offset: Some(-100),
            }]),
        };

        assert_eq!(trun.full_box_version(), 1);

        let encoded = trun.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TrunBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrunBox は必ずデコードできる");
        assert_eq!(
            per_sample_rows(&decoded)[0].composition_time_offset,
            Some(-100)
        );
    }

    /// 指定した composition_time_offset を持つ TrunBox を組み立てるヘルパー
    fn trun_with_cto(cto: i64) -> TrunBox {
        TrunBox {
            data_offset: None,
            first_sample_flags: None,
            samples: TrunSamples::PerSample(vec![TrunSample {
                duration: Some(1024),
                size: Some(512),
                flags: None,
                composition_time_offset: Some(cto),
            }]),
        }
    }

    /// TrunBox: version 0 の composition_time_offset 境界値 roundtrip
    #[test]
    fn trun_box_cto_version0_boundaries() {
        for cto in [0i64, i32::MAX as i64, i32::MAX as i64 + 1, u32::MAX as i64] {
            let trun = trun_with_cto(cto);
            assert_eq!(
                trun.full_box_version(),
                0,
                "cto={cto} は version 0 で表現できる"
            );

            let encoded = trun
                .encode_to_vec()
                .unwrap_or_else(|e| panic!("cto={cto} の encode は成功する: {e:?}"));
            let (decoded, _) = TrunBox::decode(&encoded)
                .unwrap_or_else(|e| panic!("cto={cto} の decode は成功する: {e:?}"));
            assert_eq!(
                per_sample_rows(&decoded)[0].composition_time_offset,
                Some(cto),
                "cto={cto} の roundtrip 値が一致する"
            );
        }
    }

    /// TrunBox: version 1 の負値 composition_time_offset 境界値 roundtrip
    #[test]
    fn trun_box_cto_version1_negative_boundaries() {
        for cto in [-1i64, i32::MIN as i64] {
            let trun = trun_with_cto(cto);
            assert_eq!(trun.full_box_version(), 1, "cto={cto} は version 1 が必要");

            let encoded = trun
                .encode_to_vec()
                .unwrap_or_else(|e| panic!("cto={cto} の encode は成功する: {e:?}"));
            let (decoded, _) = TrunBox::decode(&encoded)
                .unwrap_or_else(|e| panic!("cto={cto} の decode は成功する: {e:?}"));
            assert_eq!(
                per_sample_rows(&decoded)[0].composition_time_offset,
                Some(cto),
                "cto={cto} の roundtrip 値が一致する"
            );
        }
    }

    /// TrunBox: `u32::MAX` を超える composition_time_offset は encode 時にエラーとなる
    #[test]
    fn trun_box_cto_above_u32_max_is_encode_error() {
        let trun = trun_with_cto(u32::MAX as i64 + 1);
        assert_eq!(trun.full_box_version(), 0);
        assert!(
            trun.encode_to_vec().is_err(),
            "cto > u32::MAX は encode 時にエラーとなる"
        );
    }

    /// TrunBox: `i32::MIN` を下回る composition_time_offset は encode 時にエラーとなる
    #[test]
    fn trun_box_cto_below_i32_min_is_encode_error() {
        let trun = trun_with_cto(i32::MIN as i64 - 1);
        assert_eq!(trun.full_box_version(), 1);
        assert!(
            trun.encode_to_vec().is_err(),
            "cto < i32::MIN は encode 時にエラーとなる"
        );
    }

    /// TrunBox: 負値と `> i32::MAX` の値が混在する場合は encode 時にエラーとなる
    #[test]
    fn trun_box_cto_mixed_negative_and_above_i32_max_is_encode_error() {
        let trun = TrunBox {
            data_offset: None,
            first_sample_flags: None,
            samples: TrunSamples::PerSample(vec![
                TrunSample {
                    duration: Some(1024),
                    size: Some(512),
                    flags: None,
                    composition_time_offset: Some(-1),
                },
                TrunSample {
                    duration: Some(1024),
                    size: Some(512),
                    flags: None,
                    composition_time_offset: Some(i32::MAX as i64 + 1),
                },
            ]),
        };
        assert_eq!(trun.full_box_version(), 1);
        assert!(
            trun.encode_to_vec().is_err(),
            "負値と > i32::MAX の混在は version 1 でも書けないため encode 時にエラーとなる"
        );
    }

    /// SidxBox: version 0 (32-bit values)
    #[test]
    fn sidx_box_version0() {
        let sidx = SidxBox {
            reference_id: 1,
            timescale: 90000,
            earliest_presentation_time: u32::MAX as u64,
            first_offset: u32::MAX as u64,
            references: vec![],
        };
        assert_eq!(sidx.full_box_version(), 0);

        let encoded = sidx.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = SidxBox::decode(&encoded)
            .expect("直前にエンコードした有効な SidxBox は必ずデコードできる");
        assert_eq!(decoded.earliest_presentation_time, u32::MAX as u64);
    }

    /// SidxBox: version 1 (64-bit values)
    #[test]
    fn sidx_box_version1() {
        let sidx = SidxBox {
            reference_id: 1,
            timescale: 90000,
            earliest_presentation_time: u32::MAX as u64 + 1,
            first_offset: 0,
            references: vec![],
        };
        assert_eq!(sidx.full_box_version(), 1);

        let encoded = sidx.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = SidxBox::decode(&encoded)
            .expect("直前にエンコードした有効な SidxBox は必ずデコードできる");
        assert_eq!(decoded.earliest_presentation_time, u32::MAX as u64 + 1);
    }

    /// SidxBox: 複数の参照
    #[test]
    fn sidx_box_multiple_references() {
        let sidx = SidxBox {
            reference_id: 1,
            timescale: 90000,
            earliest_presentation_time: 0,
            first_offset: 0,
            references: vec![
                SidxReference {
                    reference_type: false,
                    referenced_size: 10000,
                    subsegment_duration: 90000,
                    starts_with_sap: true,
                    sap_type: 1,
                    sap_delta_time: 0,
                },
                SidxReference {
                    reference_type: true,
                    referenced_size: 5000,
                    subsegment_duration: 45000,
                    starts_with_sap: false,
                    sap_type: 0,
                    sap_delta_time: 1000,
                },
            ],
        };

        let encoded = sidx.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = SidxBox::decode(&encoded)
            .expect("直前にエンコードした有効な SidxBox は必ずデコードできる");
        assert_eq!(decoded.references.len(), 2);
        assert!(!decoded.references[0].reference_type);
        assert!(decoded.references[1].reference_type);
        assert_eq!(decoded.references[0].referenced_size, 10000);
        assert_eq!(decoded.references[1].sap_delta_time, 1000);
    }

    /// MoofBox: 最小構成
    #[test]
    fn moof_box_minimal() {
        let moof = MoofBox {
            mfhd_box: MfhdBox { sequence_number: 1 },
            traf_boxes: vec![],
            unknown_boxes: vec![],
        };

        let encoded = moof.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MoofBox::decode(&encoded)
            .expect("直前にエンコードした有効な MoofBox は必ずデコードできる");
        assert_eq!(decoded.mfhd_box.sequence_number, 1);
        assert!(decoded.traf_boxes.is_empty());
    }

    /// MoofBox: 複数の traf
    #[test]
    fn moof_box_multiple_traf() {
        let moof = MoofBox {
            mfhd_box: MfhdBox { sequence_number: 1 },
            traf_boxes: vec![
                TrafBox {
                    tfhd_box: TfhdBox {
                        track_id: 1,
                        base_data_offset: None,
                        sample_description_index: None,
                        default_sample_duration: None,
                        default_sample_size: None,
                        default_sample_flags: None,
                        duration_is_empty: false,
                        default_base_is_moof: true,
                    },
                    tfdt_box: Some(TfdtBox {
                        version: 0,
                        base_media_decode_time: 0,
                    }),
                    trun_boxes: vec![],
                    unknown_boxes: vec![],
                },
                TrafBox {
                    tfhd_box: TfhdBox {
                        track_id: 2,
                        base_data_offset: None,
                        sample_description_index: None,
                        default_sample_duration: None,
                        default_sample_size: None,
                        default_sample_flags: None,
                        duration_is_empty: false,
                        default_base_is_moof: true,
                    },
                    tfdt_box: None,
                    trun_boxes: vec![],
                    unknown_boxes: vec![],
                },
            ],
            unknown_boxes: vec![],
        };

        let encoded = moof.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MoofBox::decode(&encoded)
            .expect("直前にエンコードした有効な MoofBox は必ずデコードできる");
        assert_eq!(decoded.traf_boxes.len(), 2);
        assert_eq!(decoded.traf_boxes[0].tfhd_box.track_id, 1);
        assert_eq!(decoded.traf_boxes[1].tfhd_box.track_id, 2);
        assert!(decoded.traf_boxes[0].tfdt_box.is_some());
        assert!(decoded.traf_boxes[1].tfdt_box.is_none());
    }

    /// MvexBox: 最小構成
    #[test]
    fn mvex_box_minimal() {
        let mvex = MvexBox {
            mehd_box: None,
            trex_boxes: vec![],
            unknown_boxes: vec![],
        };

        let encoded = mvex.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MvexBox::decode(&encoded)
            .expect("直前にエンコードした有効な MvexBox は必ずデコードできる");
        assert!(decoded.mehd_box.is_none());
        assert!(decoded.trex_boxes.is_empty());
    }

    /// MvexBox: mehd と複数の trex
    #[test]
    fn mvex_box_full() {
        let mvex = MvexBox {
            mehd_box: Some(MehdBox {
                fragment_duration: 1000000,
            }),
            trex_boxes: vec![
                TrexBox {
                    track_id: 1,
                    default_sample_description_index: 1,
                    default_sample_duration: 1024,
                    default_sample_size: 0,
                    default_sample_flags: SampleFlags::new(0x01010000),
                },
                TrexBox {
                    track_id: 2,
                    default_sample_description_index: 1,
                    default_sample_duration: 1024,
                    default_sample_size: 0,
                    default_sample_flags: SampleFlags::new(0x02000000),
                },
            ],
            unknown_boxes: vec![],
        };

        let encoded = mvex.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MvexBox::decode(&encoded)
            .expect("直前にエンコードした有効な MvexBox は必ずデコードできる");
        assert!(decoded.mehd_box.is_some());
        assert_eq!(
            decoded
                .mehd_box
                .expect("直前の is_some 検証で Some であることを確認済み")
                .fragment_duration,
            1000000
        );
        assert_eq!(decoded.trex_boxes.len(), 2);
        assert_eq!(decoded.trex_boxes[0].track_id, 1);
        assert_eq!(decoded.trex_boxes[1].track_id, 2);
    }

    /// BaseBox::box_type テスト
    #[test]
    fn fmp4_box_types() {
        use shiguredo_mp4::BoxType;

        assert_eq!(MoofBox::TYPE, BoxType::Normal(*b"moof"));
        assert_eq!(MfhdBox::TYPE, BoxType::Normal(*b"mfhd"));
        assert_eq!(TrafBox::TYPE, BoxType::Normal(*b"traf"));
        assert_eq!(TfhdBox::TYPE, BoxType::Normal(*b"tfhd"));
        assert_eq!(TrunBox::TYPE, BoxType::Normal(*b"trun"));
        assert_eq!(TfdtBox::TYPE, BoxType::Normal(*b"tfdt"));
        assert_eq!(SidxBox::TYPE, BoxType::Normal(*b"sidx"));
        assert_eq!(MvexBox::TYPE, BoxType::Normal(*b"mvex"));
        assert_eq!(MehdBox::TYPE, BoxType::Normal(*b"mehd"));
        assert_eq!(TrexBox::TYPE, BoxType::Normal(*b"trex"));
    }

    /// MoofBox の children テスト
    #[test]
    fn moof_box_children() {
        let moof = MoofBox {
            mfhd_box: MfhdBox { sequence_number: 1 },
            traf_boxes: vec![TrafBox {
                tfhd_box: TfhdBox {
                    track_id: 1,
                    base_data_offset: None,
                    sample_description_index: None,
                    default_sample_duration: None,
                    default_sample_size: None,
                    default_sample_flags: None,
                    duration_is_empty: false,
                    default_base_is_moof: true,
                },
                tfdt_box: None,
                trun_boxes: vec![],
                unknown_boxes: vec![],
            }],
            unknown_boxes: vec![],
        };

        let children: Vec<_> = moof.children().collect();
        assert_eq!(children.len(), 2); // mfhd + 1 traf
    }

    /// TfraBox: version = 0 の上限値ラウンドトリップ
    #[test]
    fn tfra_box_version0_max_boundaries() {
        let tfra = TfraBox {
            version: 0,
            track_id: u32::MAX,
            length_size_of_traf_num: 3,
            length_size_of_trun_num: 3,
            length_size_of_sample_num: 3,
            entries: vec![TfraEntry {
                time: u32::MAX as u64,
                moof_offset: u32::MAX as u64,
                traf_number: u32::MAX,
                trun_number: u32::MAX,
                sample_number: u32::MAX,
            }],
        };
        assert_eq!(tfra.full_box_version(), 0);

        let encoded = tfra.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfraBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfraBox は必ずデコードできる");
        assert_eq!(decoded, tfra);
    }

    /// TfraBox: self.version = 0 でも time > u32::MAX で自動的に version = 1 に昇格する
    #[test]
    fn tfra_box_version_auto_promotion() {
        let tfra = TfraBox {
            version: 0,
            track_id: 1,
            length_size_of_traf_num: 0,
            length_size_of_trun_num: 0,
            length_size_of_sample_num: 0,
            entries: vec![TfraEntry {
                time: u32::MAX as u64 + 1,
                moof_offset: 0,
                traf_number: 1,
                trun_number: 1,
                sample_number: 1,
            }],
        };
        assert_eq!(
            tfra.full_box_version(),
            1,
            "time が u32::MAX を超えるので version は 1 に昇格する"
        );

        let encoded = tfra.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfraBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfraBox は必ずデコードできる");
        assert_eq!(
            decoded.version, 1,
            "decode 側は書かれた版数 1 をそのまま self.version に戻す"
        );
        assert_eq!(decoded.entries, tfra.entries);
    }

    /// TfraBox: `length_size_of_*` = 0 の最小構成ラウンドトリップ
    #[test]
    fn tfra_box_length_size_zero() {
        let tfra = TfraBox {
            version: 0,
            track_id: 1,
            length_size_of_traf_num: 0,
            length_size_of_trun_num: 0,
            length_size_of_sample_num: 0,
            entries: vec![TfraEntry {
                time: 0,
                moof_offset: 0,
                traf_number: 0xFF,
                trun_number: 0xFF,
                sample_number: 0xFF,
            }],
        };

        let encoded = tfra.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfraBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfraBox は必ずデコードできる");
        assert_eq!(decoded, tfra);
    }

    /// TfraBox: entries が空
    #[test]
    fn tfra_box_empty_entries() {
        let tfra = TfraBox {
            version: 0,
            track_id: 1,
            length_size_of_traf_num: 0,
            length_size_of_trun_num: 0,
            length_size_of_sample_num: 0,
            entries: vec![],
        };

        let encoded = tfra.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TfraBox::decode(&encoded)
            .expect("直前にエンコードした有効な TfraBox は必ずデコードできる");
        assert_eq!(decoded, tfra);
    }
}
