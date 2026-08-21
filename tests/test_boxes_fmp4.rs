//! `src/boxes_fmp4.rs` の意図的なエラーパスに関する回帰テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_fmp4_boxes.rs` が担う。
//! 本ファイルは PBT で安定して狙いにくい境界・不整合入力のエラーパスを固定する。

mod encode_variable_uint_insufficient_buffer {
    //! `TfraBox::encode` のバッファ境界検査に関する回帰テスト
    //!
    //! `encode_variable_uint`（`src/boxes_fmp4.rs` 内の private 関数）は
    //! `byte_count` 1〜3 のとき `buf.len()` を検査せず `buf[i]` へ直接代入していたため、
    //! 短いバッファで `TfraBox::encode` を呼ぶとインデックス範囲外パニックに至っていた。
    //! 本テストは、修正後に `ErrorKind::InsufficientBuffer` エラーが正しく返ることを、
    //! 3 呼び出し位置（`traf_number` / `trun_number` / `sample_number`）× `byte_count`
    //! 1〜3 の全 9 通りと、`byte_count = 4` を 1 ケース加えた計 10 ケースで検証する。

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

mod trun_sample_option_consistency {
    //! `TrunBox::encode` がサンプル間の per-sample `Option` 不整合を拒否することの回帰テスト
    //!
    //! ISO/IEC 14496-12 8.8.8 の trun では duration / size / flags / composition_time_offset の
    //! 有無フラグ (`tr_flags`) が run 全体共通のため、サンプルごとに `Some` / `None` が混在する
    //! 入力は表現できない。以前は先頭サンプルだけでフラグを決め、後続の値を黙って落としていた
    //! （先頭 `Some`・後続 `None` の逆方向は `unwrap_or(0)` で 0 を書き込んでいた）。
    //!
    //! PBT (`pbt/tests/prop_fmp4_boxes.rs::arb_trun_box`) は一貫性のあるサンプルだけを生成する
    //! ため、このエラーパスは本ファイルで固定する。

    use shiguredo_mp4::{
        Encode, ErrorKind, SampleFlags,
        boxes::{TrunBox, TrunSample},
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
            samples: vec![first, second],
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

// ===== pbt/tests/prop_fmp4_boxes.rs の boundary_tests から移動 =====

// ===== 境界値テスト =====

mod fmp4_boxes_boundary_tests {
    use shiguredo_mp4::{
        BaseBox, Decode, Encode, FullBox, SampleFlags,
        boxes::{
            MehdBox, MfhdBox, MoofBox, MvexBox, SidxBox, SidxReference, TfdtBox, TfhdBox, TfraBox,
            TfraEntry, TrafBox, TrexBox, TrunBox, TrunSample,
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

    /// TrunBox: 空のサンプルリスト
    #[test]
    fn trun_box_empty_samples() {
        let trun = TrunBox {
            data_offset: Some(8),
            first_sample_flags: None,
            samples: vec![],
        };

        let encoded = trun.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TrunBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrunBox は必ずデコードできる");
        assert!(decoded.samples.is_empty());
        assert_eq!(decoded.data_offset, Some(8));
    }

    /// TrunBox: 複数のサンプル
    #[test]
    fn trun_box_multiple_samples() {
        let trun = TrunBox {
            data_offset: None,
            first_sample_flags: Some(SampleFlags::new(0x02000000)),
            samples: vec![
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
            ],
        };

        let encoded = trun.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TrunBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrunBox は必ずデコードできる");
        assert_eq!(decoded.samples.len(), 2);
        assert_eq!(decoded.samples[0].duration, Some(1024));
        assert_eq!(decoded.samples[1].size, Some(256));
    }

    /// TrunBox: 負の composition_time_offset (version 1)
    #[test]
    fn trun_box_negative_cto() {
        let trun = TrunBox {
            data_offset: None,
            first_sample_flags: None,
            samples: vec![TrunSample {
                duration: Some(1024),
                size: Some(512),
                flags: None,
                composition_time_offset: Some(-100),
            }],
        };

        assert_eq!(trun.full_box_version(), 1);

        let encoded = trun.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TrunBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrunBox は必ずデコードできる");
        assert_eq!(decoded.samples[0].composition_time_offset, Some(-100));
    }

    /// 指定した composition_time_offset を持つ TrunBox を組み立てるヘルパー
    fn trun_with_cto(cto: i64) -> TrunBox {
        TrunBox {
            data_offset: None,
            first_sample_flags: None,
            samples: vec![TrunSample {
                duration: Some(1024),
                size: Some(512),
                flags: None,
                composition_time_offset: Some(cto),
            }],
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
                decoded.samples[0].composition_time_offset,
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
                decoded.samples[0].composition_time_offset,
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
            samples: vec![
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
            ],
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
