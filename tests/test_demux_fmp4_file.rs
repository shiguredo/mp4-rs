//! `Fmp4FileDemuxer` の意図的なエラーパスと、
//! `Fmp4SegmentMuxer` の出力を書き換えないと作れない入力の単体テスト
//!
//! 意図的なエラーパスの対象は次の 4 つである。
//!
//! - `moof` と `mdat` の間にサイズが 0 のボックスがあるファイル。
//!   32 ビットの size=0 はコンテナの最後のボックスなので、その後ろに `mdat` は存在し得ず、
//!   size=1 + largesize=0 は仕様上の意味が定められていない。どちらも読み飛ばし先を決められないためエラーにする
//! - `mdat` より先に `moof` が出るファイル。`mdat` が存在しない壊れたファイルとしてエラーにする
//! - `moof` と `mdat` の間のボックスの largesize が大きすぎて、次の位置を計算できないファイル
//! - `moof` と `mdat` の間のボックスの宣言サイズがファイルの末尾を超えるファイル。
//!   読み飛ばし先がファイルの外になるため、`mdat` が見つからないエラーにする
//!
//! サイズが 0 のボックスの検査がないと `mdat` を探す位置が進まず、`required_input()` が同じ範囲を要求し続けて
//! 入力の供給ループが終わらなくなる。これを検出するため、供給ループには回数の上限を設ける。
//!
//! `moof` と `mdat` の間、`mdat` の後ろのボックスを読み飛ばせる正常系は PBT で検証する。
//! 意図的なエラーパスは固定入力で契約を検証するため、PBT ではなく単体テストとして置く。
//!
//! 取り出し順と `traf` の並び順が入れ替わる入力の `sample_entry` の約束のように、
//! `Fmp4SegmentMuxer` の出力を書き換えないと作れない入力も、
//! `rewrite_init_segment` / `rewrite_media_segment_moof` で組み立てて単体テストとして置く。

use std::num::NonZeroU32;

use shiguredo_mp4::{
    Decode, Encode, ErrorKind, TrackKind, Uint,
    boxes::{
        Avc1Box, AvccBox, FtypBox, MoofBox, MoovBox, SampleEntry, TfdtBox, VisualSampleEntryFields,
    },
    demux::{DemuxError, Fmp4FileDemuxer, Input},
    mux::{Fmp4SegmentMuxer, Sample},
};

const VIDEO_TIMESCALE: u32 = 90_000;

/// 入力の供給ループの回数の上限
///
/// 正当なファイルの供給は `ftyp` + `moov` + `moof` + `mdat` で 10 回に満たない。
/// `moof` と `mdat` の間で位置が進まない実装を検出できるだけの余裕を持たせる
const MAX_FEED_COUNT: usize = 64;

/// 指定解像度の `SampleEntry::Avc1` を組み立てる
fn create_avc1_sample_entry(width: u16, height: u16) -> SampleEntry {
    SampleEntry::Avc1(Avc1Box {
        visual: VisualSampleEntryFields {
            data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            width,
            height,
            horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
            vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
            frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
            compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
            depth: VisualSampleEntryFields::DEFAULT_DEPTH,
        },
        avcc_box: AvccBox {
            avc_profile_indication: 66,
            profile_compatibility: 0,
            avc_level_indication: 30,
            length_size_minus_one: Uint::new(3),
            sps_list: vec![],
            pps_list: vec![],
            chroma_format: None,
            bit_depth_luma_minus8: None,
            bit_depth_chroma_minus8: None,
            sps_ext_list: vec![],
        },
        unknown_boxes: vec![],
    })
}

/// payload が `data_size` バイトの映像サンプル 1 個分の `Sample` を組み立てる
fn create_video_sample(data_size: usize) -> Sample {
    Sample {
        track_kind: TrackKind::Video,
        timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("タイムスケールは非ゼロ"),
        sample_entry: Some(create_avc1_sample_entry(320, 240)),
        duration: 3000,
        keyframe: true,
        composition_time_offset: None,
        data_offset: 0,
        data_size,
    }
}

/// 正当な init セグメントとメディアセグメント（`moof` + `mdat` payload 付き）を組み立てる
///
/// demux 側の検証では構文的に正しいバイト列が必要なため、別 muxer の公開 API だけで生成する。
fn build_init_and_media_segments() -> (Vec<u8>, Vec<u8>) {
    // payload 長はこのファイルで検証するエラーパスには無関係の任意値
    let payload = [0u8; 16];

    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let mut media_segment = muxer
        .create_media_segment_metadata(&[create_video_sample(payload.len())])
        .expect("media セグメントの作成に失敗した");
    media_segment.extend_from_slice(&payload);

    let init_segment = muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");

    (init_segment, media_segment)
}

/// メディアセグメントの `moof` の直後に `between_bytes` を挿し込む
///
/// `trun` の `data_offset` は補正しない。このファイルで作る入力は、
/// サンプルを読む前に `mdat` を探す段階でエラーになるためである。
fn insert_bytes_between_moof_and_mdat(media_segment: &[u8], between_bytes: &[u8]) -> Vec<u8> {
    let (_moof_box, moof_size) =
        MoofBox::decode(media_segment).expect("media セグメントからの moof デコードに失敗した");

    let mut inserted = media_segment[..moof_size].to_vec();
    inserted.extend_from_slice(between_bytes);
    inserted.extend_from_slice(&media_segment[moof_size..]);
    inserted
}

/// `required_input()` が `Some` の間、要求された範囲だけを `handle_input` に渡す
///
/// 上限を超えた場合はテスト失敗としてパニックする。`required_input()` が同じ範囲を要求し続けると、
/// この関数が終わらずテストがハングするためである
fn feed_required_input(demuxer: &mut Fmp4FileDemuxer, file_data: &[u8]) {
    let mut count = 0;
    while let Some(required) = demuxer.required_input() {
        let start = required.position as usize;
        let end = match required.size {
            Some(required_size) => start.saturating_add(required_size).min(file_data.len()),
            None => file_data.len(),
        };
        demuxer.handle_input(Input {
            position: required.position,
            data: file_data.get(start..end).unwrap_or(&[]),
        });

        count += 1;
        assert!(
            count <= MAX_FEED_COUNT,
            "入力の供給が {MAX_FEED_COUNT} 回を超えた。required_input() が同じ範囲を要求し続けている"
        );
    }
}

/// `moof` と `mdat` の間にサイズが 0 のボックスがあるファイルは、
/// 入力の供給が終わった後に `DecodeError` を返すこと
///
/// 32 ビットの size=0 と、size=1 + largesize=0 の両方を確認する
#[test]
fn decode_error_size_zero_box_between_moof_and_mdat() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    // 32 ビットの size=0 の `free` ボックス（payload 4 バイト）と、size=1 + largesize=0 の `free` ボックス
    let mut variable_size_box = Vec::new();
    variable_size_box.extend_from_slice(&0u32.to_be_bytes());
    variable_size_box.extend_from_slice(b"free");
    variable_size_box.extend_from_slice(&[0u8; 4]);

    let mut large_variable_size_box = Vec::new();
    large_variable_size_box.extend_from_slice(&1u32.to_be_bytes());
    large_variable_size_box.extend_from_slice(b"free");
    large_variable_size_box.extend_from_slice(&0u64.to_be_bytes());

    let cases = [
        ("32 ビットの size=0", variable_size_box),
        ("size=1 + largesize=0", large_variable_size_box),
    ];

    for (label, between_bytes) in cases {
        let mut file_data = init_segment.clone();
        file_data.extend_from_slice(&insert_bytes_between_moof_and_mdat(
            &media_segment,
            &between_bytes,
        ));

        let mut demuxer = Fmp4FileDemuxer::new();
        feed_required_input(&mut demuxer, &file_data);

        match demuxer.next_sample() {
            Err(DemuxError::DecodeError(error)) => {
                assert_eq!(
                    error.kind,
                    ErrorKind::InvalidData,
                    "{label} のファイルでは InvalidData を期待した"
                );
                assert_eq!(
                    error.reason, "found box with size=0 between moof and mdat in media segment",
                    "{label} のファイルで、moof と mdat の間の size=0 のボックスを示すエラー理由を期待した"
                );
            }
            Err(other) => panic!("{label} のファイルで DecodeError を期待したが {other:?} だった"),
            Ok(sample) => {
                panic!("{label} のファイルで DecodeError を期待したが Ok({sample:?}) だった")
            }
        }
    }
}

/// `mdat` より先に `moof` が出るファイルは `DecodeError` を返すこと
///
/// `moof` と `mdat` の間の読み飛ばしは `mdat` だけを終了条件にする。
/// `moof` は読み飛ばさず、`mdat` が存在しない壊れたファイルとしてエラーにする
#[test]
fn decode_error_moof_before_mdat() {
    let (init_segment, media_segment) = build_init_and_media_segments();
    let (_moof_box, moof_size) =
        MoofBox::decode(&media_segment).expect("media セグメントからの moof デコードに失敗した");

    // `moof` の直後に、もう 1 つ `moof` を置く
    let mut file_data = init_segment;
    file_data.extend_from_slice(&insert_bytes_between_moof_and_mdat(
        &media_segment,
        &media_segment[..moof_size],
    ));

    let mut demuxer = Fmp4FileDemuxer::new();
    feed_required_input(&mut demuxer, &file_data);

    match demuxer.next_sample() {
        Err(DemuxError::DecodeError(error)) => {
            assert_eq!(error.kind, ErrorKind::InvalidData, "InvalidData を期待した");
            assert_eq!(
                error.reason, "expected mdat box after moof",
                "mdat の代わりに moof が出たことを示すエラー理由を期待した"
            );
        }
        Err(other) => panic!("DecodeError を期待したが {other:?} だった"),
        Ok(sample) => panic!("DecodeError を期待したが Ok({sample:?}) だった"),
    }
}

/// `moof` と `mdat` の間のボックスの largesize が大きすぎて次の位置を計算できないと `DecodeError` を返すこと
///
/// 64 ビット環境では位置の加算がオーバーフローし、32 ビット環境ではボックスサイズを `usize` に変換できない。
/// どちらも次のボックスを読めないため、同じ種別のエラーにする。
///
/// largesize を 32 ビットに切り詰めるような誤りがあった場合も、エラー理由の違いで検出できる
#[test]
fn decode_error_box_offset_overflow_between_moof_and_mdat() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    // size=1 + 種別 `free` + largesize=u64::MAX のヘッダーを `moof` と `mdat` の間に置く
    let mut between_bytes = Vec::new();
    between_bytes.extend_from_slice(&1u32.to_be_bytes());
    between_bytes.extend_from_slice(b"free");
    between_bytes.extend_from_slice(&u64::MAX.to_be_bytes());

    let mut file_data = init_segment;
    file_data.extend_from_slice(&insert_bytes_between_moof_and_mdat(
        &media_segment,
        &between_bytes,
    ));

    let mut demuxer = Fmp4FileDemuxer::new();
    feed_required_input(&mut demuxer, &file_data);

    match demuxer.next_sample() {
        Err(DemuxError::DecodeError(error)) => {
            assert_eq!(error.kind, ErrorKind::InvalidData, "InvalidData を期待した");
            let expected_reason = if cfg!(target_pointer_width = "64") {
                "box offset overflow"
            } else {
                "box size exceeds usize::MAX"
            };
            assert_eq!(
                error.reason, expected_reason,
                "位置を計算できないことを示すエラー理由を期待した"
            );
        }
        Err(other) => panic!("DecodeError を期待したが {other:?} だった"),
        Ok(sample) => panic!("DecodeError を期待したが Ok({sample:?}) だった"),
    }
}

/// `moof` と `mdat` の間のボックスの宣言サイズがファイルの末尾を超えると `DecodeError` を返すこと
///
/// 宣言サイズの分だけ `mdat` を探す位置が進み、ファイルの末尾を超える。
/// その位置には `mdat` がないため、`mdat` が見つからないエラーになる。
///
/// `mdat` は置かない。置くと、宣言サイズの読み飛ばし先が `mdat` のヘッダーの位置に入り、
/// 狙った `mdat box not found after moof` ではなくサイズが 0 のボックスのエラーになることがある
#[test]
fn decode_error_box_after_moof_exceeds_file() {
    let (init_segment, media_segment) = build_init_and_media_segments();
    let (_moof_box, moof_size) =
        MoofBox::decode(&media_segment).expect("media セグメントからの moof デコードに失敗した");

    // `moof` の後ろに、size=100 を宣言した `free` ボックスのヘッダーと 8 バイトだけ置く（合計 16 バイト）
    let mut file_data = init_segment;
    file_data.extend_from_slice(&media_segment[..moof_size]);
    file_data.extend_from_slice(&100u32.to_be_bytes());
    file_data.extend_from_slice(b"free");
    file_data.extend_from_slice(&[0u8; 8]);

    let mut demuxer = Fmp4FileDemuxer::new();
    feed_required_input(&mut demuxer, &file_data);

    match demuxer.next_sample() {
        Err(DemuxError::DecodeError(error)) => {
            assert_eq!(error.kind, ErrorKind::InvalidData, "InvalidData を期待した");
            assert_eq!(
                error.reason, "mdat box not found after moof",
                "mdat が見つからないことを示すエラー理由を期待した"
            );
        }
        Err(other) => panic!("DecodeError を期待したが {other:?} だった"),
        Ok(sample) => panic!("DecodeError を期待したが Ok({sample:?}) だった"),
    }
}

/// 要求された位置が入力の終端より後ろになるときは、位置 0 からファイル全体を渡しても受理されないこと
///
/// 読み飛ばすボックスの宣言サイズがファイルの末尾を超えると、読み飛ばした先の位置が入力の終端より後ろになる。
/// この場合は入力の終端をファイルの終端とみなせないため、入力を拒否して `InvalidInput` の `DecodeError` になる
#[test]
fn decode_error_whole_file_input_beyond_file_end() {
    let (init_segment, media_segment) = build_init_and_media_segments();
    let (_moof_box, moof_size) =
        MoofBox::decode(&media_segment).expect("media セグメントからの moof デコードに失敗した");

    // `moof` の後ろに、size=100 を宣言した `free` ボックスのヘッダーと 8 バイトだけ置く（合計 16 バイト）
    let mut file_data = init_segment;
    file_data.extend_from_slice(&media_segment[..moof_size]);
    file_data.extend_from_slice(&100u32.to_be_bytes());
    file_data.extend_from_slice(b"free");
    file_data.extend_from_slice(&[0u8; 8]);

    // 要求された位置にかかわらず、常に位置 0 からファイル全体を渡す
    let mut demuxer = Fmp4FileDemuxer::new();
    let mut count = 0;
    while demuxer.required_input().is_some() {
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        count += 1;
        assert!(
            count <= MAX_FEED_COUNT,
            "入力の供給が {MAX_FEED_COUNT} 回を超えた。required_input() が同じ範囲を要求し続けている"
        );
    }

    match demuxer.next_sample() {
        Err(DemuxError::DecodeError(error)) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidInput,
                "要求された位置が入力の終端より後ろにある入力では InvalidInput を期待した"
            );
        }
        Err(other) => panic!("DecodeError を期待したが {other:?} だった"),
        Ok(sample) => panic!("DecodeError を期待したが Ok({sample:?}) だった"),
    }
}

/// init セグメントの `moov` を `f` で書き換えたバイト列を返す
fn rewrite_init_segment(init_segment: &[u8], f: impl FnOnce(&mut MoovBox)) -> Vec<u8> {
    let (ftyp_box, ftyp_box_size) =
        FtypBox::decode(init_segment).expect("init セグメントからの ftyp デコードに失敗した");
    let (mut moov_box, moov_box_size) = MoovBox::decode(&init_segment[ftyp_box_size..])
        .expect("init セグメントからの moov デコードに失敗した");
    assert_eq!(
        ftyp_box_size + moov_box_size,
        init_segment.len(),
        "init セグメントは ftyp + moov のみを含む"
    );
    f(&mut moov_box);

    let mut rewritten = ftyp_box
        .encode_to_vec()
        .expect("ftyp のエンコードに失敗した");
    rewritten.extend_from_slice(
        &moov_box
            .encode_to_vec()
            .expect("moov のエンコードに失敗した"),
    );
    rewritten
}

/// メディアセグメントの `moof` を `f` で書き換え、`trun` の `data_offset` を `moof` のサイズの変化分だけ補正する
fn rewrite_media_segment_moof(media_segment: &[u8], f: impl FnOnce(&mut MoofBox)) -> Vec<u8> {
    let (mut moof_box, moof_box_size) =
        MoofBox::decode(media_segment).expect("media セグメントからの moof デコードに失敗した");
    f(&mut moof_box);

    // `data_offset` の補正は、書き換えた後のすべての `trun` の基準が `moof` の先頭であることを前提にしている
    for traf_box in &moof_box.traf_boxes {
        assert!(
            traf_box.tfhd_box.default_base_is_moof,
            "書き換えた後の traf は default_base_is_moof = true である"
        );
        assert_eq!(
            traf_box.tfhd_box.base_data_offset, None,
            "書き換えた後の traf は base_data_offset を明示しない"
        );
    }

    let rewritten_moof_size = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した")
        .len();
    let size_delta = i32::try_from(rewritten_moof_size as i64 - moof_box_size as i64)
        .expect("moof のサイズの差は i32 に収まる");
    for traf_box in &mut moof_box.traf_boxes {
        for trun_box in &mut traf_box.trun_boxes {
            let data_offset = trun_box.data_offset.expect("muxer は data_offset を書く");
            trun_box.data_offset = Some(
                data_offset
                    .checked_add(size_delta)
                    .expect("補正した data_offset は i32 に収まる"),
            );
        }
    }

    let mut rewritten = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した");
    rewritten.extend_from_slice(&media_segment[moof_box_size..]);
    rewritten
}

/// 取り出し順と `traf` の並び順が入れ替わっても、`sample_entry` がファイル全体の取り出し順で変わること
///
/// 同じトラックの `traf` が複数あり sample description index が変わる入力では、内部の demuxer が
/// 返す `sample_entry` の有無が `traf` の並び順で決まる。取り出し順はタイムスタンプの順なので、
/// 並べ替えた後の直前のサンプルからサンプルエントリーの変化を判定し直す必要がある
#[test]
fn sample_entry_follows_retrieval_order_across_segments() {
    // 1 セグメント目は sample description index が 1 と 2 の 2 つの `traf`、2 セグメント目は index 2 の 1 つの `traf` にする
    let (init_segment, media_segment) = build_init_and_media_segments();

    let (first_entry, second_entry) = {
        let (_ftyp_box, ftyp_box_size) =
            FtypBox::decode(&init_segment).expect("init セグメントからの ftyp デコードに失敗した");
        let (moov_box, _) = MoovBox::decode(&init_segment[ftyp_box_size..])
            .expect("init セグメントからの moov デコードに失敗した");
        (
            moov_box.trak_boxes[0]
                .mdia_box
                .minf_box
                .stbl_box
                .stsd_box
                .entries[0]
                .clone(),
            create_avc1_sample_entry(640, 480),
        )
    };
    assert_ne!(
        first_entry, second_entry,
        "2 つのサンプルエントリーの内容が違う"
    );

    // stsd に 2 つ目のサンプルエントリーを足す
    let init_segment = rewrite_init_segment(&init_segment, |moov_box| {
        moov_box.trak_boxes[0]
            .mdia_box
            .minf_box
            .stbl_box
            .stsd_box
            .entries
            .push(second_entry.clone());
    });

    // 1 セグメント目: `traf` を複製し、1 つ目を index 1 で tfdt 90000、2 つ目を index 2 で tfdt 0 にする。
    // 並べ替えると index 2 のサンプルが先に来る
    let first_segment = rewrite_media_segment_moof(&media_segment, |moof_box| {
        let mut duplicated_traf_box = moof_box.traf_boxes[0].clone();
        moof_box.traf_boxes[0].tfhd_box.sample_description_index = Some(1);
        moof_box.traf_boxes[0].tfdt_box = Some(TfdtBox {
            version: 1,
            base_media_decode_time: 90_000,
        });
        duplicated_traf_box.tfhd_box.sample_description_index = Some(2);
        duplicated_traf_box.tfdt_box = Some(TfdtBox {
            version: 0,
            base_media_decode_time: 0,
        });
        moof_box.traf_boxes.push(duplicated_traf_box);
    });

    // 2 セグメント目: index 2 の `traf` 1 つだけにする。
    // 内部の demuxer は直前の `traf` と同じ index なので `sample_entry` を付けないが、
    // 取り出し順で直前のサンプルは index 1 のサンプルなので、`sample_entry` を付ける必要がある
    let mut second_muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let mut second_segment = second_muxer
        .create_media_segment_metadata(&[create_video_sample(16)])
        .expect("media セグメントの作成に失敗した");
    second_segment.extend_from_slice(&[0u8; 16]);
    let second_segment = rewrite_media_segment_moof(&second_segment, |moof_box| {
        moof_box.traf_boxes[0].tfhd_box.sample_description_index = Some(2);
        moof_box.traf_boxes[0].tfdt_box = Some(TfdtBox {
            version: 1,
            base_media_decode_time: 100_000,
        });
    });

    let mut file_data = init_segment;
    file_data.extend_from_slice(&first_segment);
    file_data.extend_from_slice(&second_segment);

    let mut demuxer = Fmp4FileDemuxer::new();
    let mut samples = Vec::new();
    loop {
        match demuxer.next_sample() {
            Ok(Some(sample)) => samples.push((sample.timestamp, sample.sample_entry.cloned())),
            Ok(None) => break,
            Err(DemuxError::InputRequired(_)) => feed_required_input(&mut demuxer, &file_data),
            Err(error) => panic!("next_sample エラー: {error}"),
        }
    }

    // 各 `traf` は 1 サンプルなので、1 セグメント目は index 2、index 1 の順になり、
    // 2 セグメント目は index 2 の 1 サンプルになる。
    // 直前のサンプルからエントリーが変わるところにだけ `sample_entry` が付く。
    // 2 セグメント目のサンプルは、内部の demuxer の直前の `traf` と同じ index だが、
    // 取り出し順で直前のサンプルは index 1 なので `sample_entry` が付く
    let expected = vec![
        (0, Some(second_entry.clone())),
        (90_000, Some(first_entry.clone())),
        (100_000, Some(second_entry.clone())),
    ];
    assert_eq!(
        samples, expected,
        "取り出し順でサンプルエントリーが変わるところにだけ sample_entry が付く"
    );
}
