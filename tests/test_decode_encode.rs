use shiguredo_mp4::{BoxType, Decode, Encode, Mp4File, Result, boxes::RootBox};

#[test]
fn decode_encode_black_h264_video_mp4() -> Result<()> {
    let input_bytes = include_bytes!("testdata/black-h264-video.mp4");
    let (file, file_size) = Mp4File::decode(&input_bytes[..])?;
    assert_eq!(file_size, input_bytes.len());

    // デコード時に未処理のボックスがないことを確認する。
    assert_eq!(collect_unknown_box_types(&file), Vec::new());

    // エンコード結果をデコードしたら同じ MP4 になっていることを確認する。
    let output_bytes = file.encode_to_vec()?;
    let (encoded_file, _) = Mp4File::decode(&output_bytes[..])?;
    assert_eq!(file, encoded_file);

    // エンコード結果のバイト列が正しいことを確認する。
    assert_eq!(input_bytes.len(), output_bytes.len());
    assert_eq!(&input_bytes[..], output_bytes);

    Ok(())
}

#[test]
fn decode_encode_black_h265_video_mp4() -> Result<()> {
    let input_bytes = include_bytes!("testdata/black-h265-video.mp4");
    let (file, file_size) = Mp4File::decode(&input_bytes[..])?;
    assert_eq!(file_size, input_bytes.len());

    // デコード時に未処理のボックスがないことを確認する。
    assert_eq!(collect_unknown_box_types(&file), Vec::new());

    // エンコード結果をデコードしたら同じ MP4 になっていることを確認する。
    let mut output_bytes = file.encode_to_vec()?;
    let (encoded_file, _) = Mp4File::decode(&output_bytes[..])?;
    assert_eq!(file, encoded_file);

    // エンコード結果のバイト列が正しいことを確認する。
    assert_eq!(input_bytes.len(), output_bytes.len());

    // ボックスの順番は入れ替わるのでソートした結果を比較する
    let mut input_bytes = input_bytes.to_vec();
    input_bytes.sort();
    output_bytes.sort();
    assert_eq!(&input_bytes[..], output_bytes);

    Ok(())
}

#[test]
fn decode_encode_black_h265_hvc1_video_mp4() -> Result<()> {
    let input_bytes = include_bytes!("testdata/black-h265-hvc1-video.mp4");
    let (file, file_size) = Mp4File::decode(&input_bytes[..])?;
    assert_eq!(file_size, input_bytes.len());

    // デコード時に未処理のボックスがないことを確認する。
    assert_eq!(collect_unknown_box_types(&file), Vec::new());

    // エンコード結果をデコードしたら同じ MP4 になっていることを確認する。
    let mut output_bytes = file.encode_to_vec()?;
    let (encoded_file, _) = Mp4File::decode(&output_bytes[..])?;
    assert_eq!(file, encoded_file);

    // エンコード結果のバイト列が正しいことを確認する。
    assert_eq!(input_bytes.len(), output_bytes.len());

    // ボックスの順番は入れ替わるのでソートした結果を比較する
    let mut input_bytes = input_bytes.to_vec();
    input_bytes.sort();
    output_bytes.sort();
    assert_eq!(&input_bytes[..], output_bytes);

    Ok(())
}

#[test]
fn decode_encode_black_vp9_video_mp4() -> Result<()> {
    let input_bytes = include_bytes!("testdata/black-vp9-video.mp4");
    let (file, file_size) = Mp4File::decode(&input_bytes[..])?;
    assert_eq!(file_size, input_bytes.len());

    // デコード時に未処理のボックスがないことを確認する。
    assert_eq!(collect_unknown_box_types(&file), Vec::new());

    // エンコード結果をデコードしたら同じ MP4 になっていることを確認する。
    let mut output_bytes = file.encode_to_vec()?;
    let (encoded_file, _) = Mp4File::decode(&output_bytes[..])?;
    assert_eq!(file, encoded_file);

    // エンコード結果のバイト列が正しいことを確認する。
    assert_eq!(input_bytes.len(), output_bytes.len());

    // ボックスの順番は入れ替わるのでソートした結果を比較する
    let mut input_bytes = input_bytes.to_vec();
    input_bytes.sort();
    output_bytes.sort();
    assert_eq!(&input_bytes[..], output_bytes);

    Ok(())
}

#[test]
fn decode_encode_black_av1_video_mp4() -> Result<()> {
    let input_bytes = include_bytes!("testdata/black-av1-video.mp4");
    let (file, file_size) = Mp4File::decode(&input_bytes[..])?;
    assert_eq!(file_size, input_bytes.len());

    // デコード時に未処理のボックスがないことを確認する。
    assert_eq!(collect_unknown_box_types(&file), Vec::new());

    // エンコード結果をデコードしたら同じ MP4 になっていることを確認する。
    let mut output_bytes = file.encode_to_vec()?;
    let (encoded_file, _) = Mp4File::decode(&output_bytes[..])?;
    assert_eq!(file, encoded_file);

    // エンコード結果のバイト列が正しいことを確認する。
    assert_eq!(input_bytes.len(), output_bytes.len());

    // ボックスの順番は入れ替わるのでソートした結果を比較する
    let mut input_bytes = input_bytes.to_vec();
    input_bytes.sort();
    output_bytes.sort();
    assert_eq!(&input_bytes[..], output_bytes);

    Ok(())
}

#[test]
fn decode_encode_beep_opus_audio_mp4() -> Result<()> {
    let input_bytes = include_bytes!("testdata/beep-opus-audio.mp4");
    let (file, file_size) = Mp4File::decode(&input_bytes[..])?;
    assert_eq!(file_size, input_bytes.len());

    // デコード時に未処理のボックスがないことを確認する。
    assert_eq!(collect_unknown_box_types(&file), Vec::new());

    // エンコード結果をデコードしたら同じ MP4 になっていることを確認する。
    let output_bytes = file.encode_to_vec()?;
    let (encoded_file, _) = Mp4File::decode(&output_bytes[..])?;
    assert_eq!(file, encoded_file);

    // エンコード結果のバイト列が正しいことを確認する。
    assert_eq!(input_bytes.len(), output_bytes.len());
    assert_eq!(&input_bytes[..], output_bytes);

    Ok(())
}

#[test]
fn decode_encode_beep_opus_audio_aac() -> Result<()> {
    let input_bytes = include_bytes!("testdata/beep-aac-audio.mp4");
    let (file, file_size) = Mp4File::decode(&input_bytes[..])?;
    assert_eq!(file_size, input_bytes.len());

    // デコード時に未処理のボックスがないことを確認する。
    assert_eq!(collect_unknown_box_types(&file), Vec::new());

    // エンコード結果をデコードしたら同じ MP4 になっていることを確認する。
    let output_bytes = file.encode_to_vec()?;
    let (encoded_file, _) = Mp4File::decode(&output_bytes[..])?;
    assert_eq!(file, encoded_file);

    // エンコード結果のバイト列が正しいことを確認する。
    //
    // [NOTE]
    // descriptor のサイズのエンコード形式は可変長だが、
    // ffmpeg で生成したテストファイルでは常に 4 バイト使われていた。
    // そのため、decode / encode を通すとバイト列の中身が変わってしまうので、
    // ここでは調整したサイズ (+ 12 部分）で比較している。
    // またバイト列の厳密な一致チェックはこのテストケースでは行わないようにした。
    assert_eq!(input_bytes.len(), output_bytes.len() + 12);

    Ok(())
}

#[test]
fn decode_encode_beep_flac_audio_mp4() -> Result<()> {
    let input_bytes = include_bytes!("testdata/beep-flac-audio.mp4");
    let (file, file_size) = Mp4File::decode(&input_bytes[..])?;
    assert_eq!(file_size, input_bytes.len());

    // デコード時に未処理のボックスがないことを確認する。
    assert_eq!(collect_unknown_box_types(&file), Vec::new());

    // エンコード結果をデコードしたら同じ MP4 になっていることを確認する。
    let output_bytes = file.encode_to_vec()?;
    let (encoded_file, _) = Mp4File::decode(&output_bytes[..])?;
    assert_eq!(file, encoded_file);

    // エンコード結果のバイト列が正しいことを確認する。
    assert_eq!(input_bytes.len(), output_bytes.len());
    assert_eq!(&input_bytes[..], output_bytes);

    Ok(())
}

/// fMP4 (fragmented MP4) のデコード/エンコード テスト
#[test]
fn decode_encode_black_h264_fmp4() -> Result<()> {
    let input_bytes = include_bytes!("testdata/black-h264-fmp4.mp4");
    let (file, file_size) = Mp4File::decode(&input_bytes[..])?;
    assert_eq!(file_size, input_bytes.len());

    // デコード時に未処理のボックスがないことを確認する。
    assert_eq!(collect_unknown_box_types(&file), Vec::new());

    // fMP4 には moof ボックスが含まれていることを確認する。
    let has_moof = file.boxes.iter().any(|b| matches!(b, RootBox::Moof(_)));
    assert!(has_moof, "fMP4 には moof ボックスが含まれている必要がある");

    // fMP4 には moov ボックスが含まれていることを確認する (mvex を含む)。
    let moov = file
        .boxes
        .iter()
        .find_map(|b| match b {
            RootBox::Moov(m) => Some(m),
            _ => None,
        })
        .expect("fMP4 には moov ボックスが含まれている必要がある");

    // fMP4 の moov には mvex ボックスが含まれていることを確認する。
    assert!(
        moov.mvex_box.is_some(),
        "fMP4 の moov には mvex ボックスが含まれている必要がある"
    );

    // fMP4 には mfra ボックスが含まれていることを確認する。
    let has_mfra = file.boxes.iter().any(|b| matches!(b, RootBox::Mfra(_)));
    assert!(has_mfra, "fMP4 には mfra ボックスが含まれている必要がある");

    // エンコード結果をデコードしたら同じ MP4 になっていることを確認する。
    let output_bytes = file.encode_to_vec()?;
    let (encoded_file, _) = Mp4File::decode(&output_bytes[..])?;
    assert_eq!(file, encoded_file);

    // エンコード結果のバイト列が正しいことを確認する。
    assert_eq!(input_bytes.len(), output_bytes.len());
    assert_eq!(&input_bytes[..], output_bytes);

    Ok(())
}

fn collect_unknown_box_types(mp4: &Mp4File) -> Vec<BoxType> {
    let mut stack = mp4.iter().collect::<Vec<_>>();
    let mut unknowns = Vec::new();

    while let Some(b) = stack.pop() {
        if b.is_unknown_box()
            && !matches!(
                b.box_type().as_bytes(),
                b"btrt" | b"fiel" | b"pasp" | b"sbgp" | b"sgpd" | b"udta"
            )
        {
            unknowns.push(b.box_type());
        }
        stack.extend(b.children());
    }

    unknowns
}
