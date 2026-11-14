//! FLAC サンプルエントリー（`fLaC`）の JSON 変換を提供する wasm 専用モジュール

use shiguredo_mp4::Uint;
use shiguredo_mp4::boxes::{AudioSampleEntryFields, DflaBox, FlacBox, FlacMetadataBlock};

/// FLAC サンプルエントリーを JSON オブジェクトとしてフォーマットする
///
/// `sampleRate` は音声共通フィールドの 16.16 固定小数点数を、整数部と小数部を
/// 合わせた 1 つの実数として出力する。
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &FlacBox) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "flac")?;
        f.member("dataReferenceIndex", data.audio.data_reference_index.get())?;
        f.member("channelCount", data.audio.channelcount)?;
        f.member(
            "sampleRate",
            crate::boxes::sample_rate_to_json(data.audio.samplerate),
        )?;
        f.member("sampleSize", data.audio.samplesize)?;
        // 既存の JSON 表現に合わせて STREAMINFO のブロックデータだけを公開する。
        // FLAC の仕様上 `dfLa` の先頭ブロックは必ず STREAMINFO になる
        let streaminfo_data = data
            .dfla_box
            .metadata_blocks
            .first()
            .map_or(&[][..], |block| block.block_data.as_slice());
        f.member("streaminfoData", streaminfo_data)?;
        // STREAMINFO 以外のブロックがある場合だけ `metadataBlocks` を出力する。
        // 1 個だけの場合は従来どおり `streaminfoData` だけで往復できる
        if data.dfla_box.metadata_blocks.len() > 1 {
            f.member(
                "metadataBlocks",
                nojson::array(|f| {
                    for block in &data.dfla_box.metadata_blocks {
                        f.element(nojson::object(|f| {
                            f.member("type", block.block_type.get())?;
                            f.member("last", block.last_metadata_block_flag.get() == 1)?;
                            f.member("data", block.block_data.as_slice())
                        }))?;
                    }
                    Ok(())
                }),
            )?;
        }
        crate::boxes::fmt_unknown_boxes(f, &data.unknown_boxes)
    })
}

/// `dfLa` のメタデータブロック列を検証する
///
/// Xiph の FLAC 仕様では、先頭ブロックは STREAMINFO でなければならず、
/// 最後のブロックだけが `last` フラグを持つ。コアの `DflaBox::decode` と同じ制約を
/// 入力側でも検査し、読み戻せない MP4 を生成しないようにする。
fn validate_metadata_blocks(
    member: nojson::RawJsonValue<'_, '_>,
    blocks: &[FlacMetadataBlock],
) -> Result<(), nojson::JsonParseError> {
    if blocks.is_empty() {
        return Err(member.invalid("metadataBlocks must contain at least one block"));
    }
    if blocks[0].block_type != FlacMetadataBlock::BLOCK_TYPE_STREAMINFO {
        return Err(member.invalid("the first metadata block must be STREAMINFO"));
    }
    for (index, block) in blocks.iter().enumerate() {
        let is_last = block.last_metadata_block_flag.get() == 1;
        if is_last != (index + 1 == blocks.len()) {
            return Err(member.invalid("only the last metadata block may have last=true"));
        }
        if block.block_data.len() > 0xFF_FFFF {
            return Err(member.invalid("metadata block data must be at most 24 bits"));
        }
    }
    Ok(())
}

/// JSON オブジェクトから FLAC サンプルエントリーを構築する
///
/// `metadataBlocks` があればそれを優先し、無ければ `streaminfoData` から
/// STREAMINFO 1 個だけのブロック列を組み立てる。``
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<FlacBox, nojson::JsonParseError> {
    let channel_count: u16 = value.to_member("channelCount")?.required()?.try_into()?;
    let sample_rate = crate::boxes::sample_rate_from_json(value)?;
    let sample_size: u16 = value.to_member("sampleSize")?.required()?.try_into()?;
    // `metadataBlocks` があればそれを優先し、無ければ従来の `streaminfoData` から
    // STREAMINFO 1 個だけのブロック列を組み立てる
    let metadata_blocks = match value.to_member("metadataBlocks")?.optional() {
        None => Vec::new(),
        Some(member) if member.kind() == nojson::JsonValueKind::Null => Vec::new(),
        Some(member) => {
            let mut blocks = Vec::new();
            for block in member.to_array()? {
                let block_type_value = block.to_member("type")?.required()?;
                let block_type: u8 = block_type_value.try_into()?;
                if block_type > 127 {
                    return Err(block_type_value.invalid("type must fit in 7 bits"));
                }
                let last: bool = block.to_member("last")?.required()?.try_into()?;
                let block_data: Vec<u8> = block.to_member("data")?.required()?.try_into()?;
                blocks.push(FlacMetadataBlock {
                    last_metadata_block_flag: Uint::new(u8::from(last)),
                    block_type: Uint::new(block_type),
                    block_data,
                });
            }
            validate_metadata_blocks(member, &blocks)?;
            blocks
        }
    };
    let metadata_blocks = if metadata_blocks.is_empty() {
        let streaminfo_data: Vec<u8> = value.to_member("streaminfoData")?.required()?.try_into()?;
        vec![FlacMetadataBlock {
            // 唯一のメタデータブロックなので、最後のブロックとして扱う
            last_metadata_block_flag: Uint::from(true),
            block_type: FlacMetadataBlock::BLOCK_TYPE_STREAMINFO,
            block_data: streaminfo_data,
        }]
    } else {
        metadata_blocks
    };

    Ok(FlacBox {
        audio: AudioSampleEntryFields {
            data_reference_index: crate::boxes::parse_audio_data_reference_index(value)?,
            channelcount: channel_count,
            samplesize: sample_size,
            samplerate: sample_rate,
        },
        dfla_box: DflaBox { metadata_blocks },
        unknown_boxes: crate::boxes::parse_unknown_boxes(value, &[b"dfLa"])?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiguredo_mp4::FixedPointNumber;

    /// テスト用の FLAC サンプルエントリーを構築する
    fn build_flac_box(streaminfo_data: Vec<u8>) -> FlacBox {
        FlacBox {
            audio: AudioSampleEntryFields {
                data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                channelcount: 2,
                samplesize: 16,
                samplerate: FixedPointNumber::new(44100, 0),
            },
            dfla_box: DflaBox {
                metadata_blocks: vec![FlacMetadataBlock {
                    last_metadata_block_flag: Uint::from(true),
                    block_type: FlacMetadataBlock::BLOCK_TYPE_STREAMINFO,
                    block_data: streaminfo_data,
                }],
            },
            unknown_boxes: Vec::new(),
        }
    }

    #[test]
    fn test_flac_to_json() {
        let sample_entry = build_flac_box(vec![0x00, 0x10, 0x00, 0x10]);

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            json.contains(r#""kind":"flac""#),
            "kind が flac であること: {json}"
        );
        assert!(
            json.contains(r#""channelCount":2"#),
            "channelCount が 2 であること: {json}"
        );
        assert!(
            json.contains(r#""sampleRate":44100"#),
            "sampleRate が 44100 であること: {json}"
        );
        assert!(
            json.contains(r#""sampleSize":16"#),
            "sampleSize が 16 であること: {json}"
        );
        assert!(
            json.contains(r#""streaminfoData":[0,16,0,16]"#),
            "streaminfoData が数値配列で出力されること: {json}"
        );
    }

    #[test]
    fn test_json_to_flac() {
        let json_str = r#"{"kind": "flac", "channelCount": 2, "sampleRate": 44100, "sampleSize": 16, "streaminfoData": [0, 16, 0, 16]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let sample_entry = parse_json(json.value()).expect("flac の JSON をパースできること");

        assert_eq!(
            sample_entry.audio.channelcount, 2,
            "channelCount が 2 であること"
        );
        assert_eq!(
            sample_entry.audio.samplerate,
            FixedPointNumber::new(44100, 0),
            "sampleRate が 44100 であること"
        );
        assert_eq!(
            sample_entry.audio.samplesize, 16,
            "sampleSize が 16 であること"
        );

        assert_eq!(
            sample_entry.dfla_box.metadata_blocks.len(),
            1,
            "メタデータブロックが 1 個であること"
        );
        let block = &sample_entry.dfla_box.metadata_blocks[0];
        assert_eq!(
            block.block_data,
            vec![0, 16, 0, 16],
            "STREAMINFO のブロックデータが [0, 16, 0, 16] であること"
        );
        assert!(
            block.last_metadata_block_flag.as_bool(),
            "最後のメタデータブロックであること"
        );
        assert_eq!(
            block.block_type,
            FlacMetadataBlock::BLOCK_TYPE_STREAMINFO,
            "ブロック種別が STREAMINFO であること"
        );

        assert_eq!(
            sample_entry.audio.data_reference_index,
            AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            "data_reference_index が既定値であること"
        );
        assert!(
            sample_entry.unknown_boxes.is_empty(),
            "unknown_boxes が空であること"
        );
    }

    /// 空の streaminfoData を parse して JSON に再出力する往復テスト
    ///
    /// 空配列を渡した場合も、出力側で空配列として表現できることを検証する。
    #[test]
    fn test_json_to_flac_empty_streaminfo_roundtrip() {
        let json_str = r#"{"kind": "flac", "channelCount": 2, "sampleRate": 44100, "sampleSize": 16, "streaminfoData": []}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let sample_entry =
            parse_json(json.value()).expect("空 streaminfoData の flac JSON をパースできること");

        let block = &sample_entry.dfla_box.metadata_blocks[0];
        assert!(
            block.block_data.is_empty(),
            "STREAMINFO のブロックデータが空であること"
        );

        let out = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            out.contains(r#""streaminfoData":[]"#),
            "空の streaminfoData が [] として再出力されること: {out}"
        );
    }

    #[test]
    fn test_json_to_flac_rejects_missing_channel_count() {
        // streaminfoData は揃っているが必須フィールドの channelCount が欠落している
        let json_str = r#"{"kind": "flac", "sampleRate": 44100, "sampleSize": 16, "streaminfoData": [0, 16, 0, 16]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let result = parse_json(json.value());
        assert!(result.is_err(), "channelCount 欠落時はパース失敗すること");
    }

    /// 複数のメタデータブロックが往復すること
    #[test]
    fn test_flac_json_roundtrip_preserves_metadata_blocks() {
        let json_str = r#"{"kind":"flac","channelCount":2,"sampleRate":44100,"sampleSize":16,"streaminfoData":[1,2,3],"metadataBlocks":[{"type":0,"last":false,"data":[1,2,3]},{"type":1,"last":true,"data":[0,0]}]}"#;
        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let entry = parse_json(json.value()).expect("有効な flac JSON");
        assert_eq!(entry.dfla_box.metadata_blocks.len(), 2);

        let output = nojson::json(|f| fmt_json(f, &entry)).to_string();
        let raw = nojson::RawJson::parse(&output).expect("有効な JSON");
        let parsed = parse_json(raw.value()).expect("再パース");
        assert_eq!(parsed, entry);
    }

    /// 先頭が STREAMINFO でない、または last の位置が不正な場合は拒否すること
    #[test]
    fn test_flac_json_rejects_invalid_metadata_blocks() {
        let cases = [
            // 先頭が STREAMINFO ではない
            r#"{"kind":"flac","channelCount":2,"sampleRate":44100,"sampleSize":16,"streaminfoData":[1],"metadataBlocks":[{"type":1,"last":true,"data":[0]}]}"#,
            // 最後以外に last=true がある
            r#"{"kind":"flac","channelCount":2,"sampleRate":44100,"sampleSize":16,"streaminfoData":[1],"metadataBlocks":[{"type":0,"last":true,"data":[1]},{"type":1,"last":true,"data":[0]}]}"#,
            // 最後のブロックに last がない
            r#"{"kind":"flac","channelCount":2,"sampleRate":44100,"sampleSize":16,"streaminfoData":[1],"metadataBlocks":[{"type":0,"last":false,"data":[1]}]}"#,
        ];
        for json_str in cases {
            let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
            assert!(parse_json(json.value()).is_err(), "{json_str}");
        }
    }
}
