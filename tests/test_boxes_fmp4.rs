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
