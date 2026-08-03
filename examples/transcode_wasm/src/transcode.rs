use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use futures::{
    executor::{LocalPool, LocalSpawner},
    stream::FusedStream,
    task::LocalSpawnExt,
};
use shiguredo_mp4::{
    BaseBox, BoxHeader, BoxSize, BoxType, Decode, Encode, Uint,
    boxes::{
        Av01Box, Av1cBox, Avc1Box, AvccBox, Hev1Box, HvccBox, SampleEntry, VisualSampleEntryFields,
        Vp08Box, Vp09Box, VpccBox,
    },
};

use crate::{
    Error, Result,
    mp4::{Chunk, InputMp4, Mp4FileSummary, OutputMp4Builder, Track},
    wasm::WebCodec,
};

// B フレームが使われている場合には（おそらく）デコーダーのキューをある程度埋める必要があるので、
// どの程度の個数を一度に詰め込むかを指定するための定数値。
// 理想的には、入力ストリームから適切な値を取得すべきだが、簡単のために十分に大きな固定値を使用している。
const DECODE_QUEQUE_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u16,
    pub height: u16,
    pub data: Vec<u8>,
}

impl<'text, 'raw> TryFrom<nojson::RawJsonValue<'text, 'raw>> for VideoFrame {
    type Error = nojson::JsonParseError;

    fn try_from(
        value: nojson::RawJsonValue<'text, 'raw>,
    ) -> std::result::Result<Self, Self::Error> {
        let width = value.to_member("width")?.required()?.try_into()?;
        let height = value.to_member("height")?.required()?.try_into()?;
        let data = value
            .to_member("data")?
            .map(|v| v.try_into())?
            .unwrap_or_default();
        Ok(Self {
            width,
            height,
            data,
        })
    }
}

#[derive(Debug, Clone)]
pub struct VideoEncoderConfig {
    pub codec: String,
    pub bitrate: u32,
    pub width: u16,
    pub height: u16,
}

impl nojson::DisplayJson for VideoEncoderConfig {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            f.member("codec", &self.codec)?;
            f.member("bitrate", self.bitrate)?;
            f.member("width", self.width)?;
            f.member("height", self.height)
        })
    }
}

impl<'text, 'raw> TryFrom<nojson::RawJsonValue<'text, 'raw>> for VideoEncoderConfig {
    type Error = nojson::JsonParseError;

    fn try_from(
        value: nojson::RawJsonValue<'text, 'raw>,
    ) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            codec: value.to_member("codec")?.required()?.try_into()?,
            bitrate: value.to_member("bitrate")?.required()?.try_into()?,
            width: value.to_member("width")?.required()?.try_into()?,
            height: value.to_member("height")?.required()?.try_into()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TranscodeOptions {
    pub video_encoder_config: VideoEncoderConfig,
}

impl<'text, 'raw> TryFrom<nojson::RawJsonValue<'text, 'raw>> for TranscodeOptions {
    type Error = nojson::JsonParseError;

    fn try_from(
        value: nojson::RawJsonValue<'text, 'raw>,
    ) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            video_encoder_config: value.try_into()?,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TranscodeProgress {
    pub done: bool,
    pub rate: f32,
}

impl nojson::DisplayJson for TranscodeProgress {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            f.member("done", self.done)?;
            f.member("rate", self.rate)
        })
    }
}

#[derive(Debug)]
pub struct Transcoder {
    options: TranscodeOptions,
    input_mp4: Option<InputMp4>,
    output_mp4: Vec<u8>,
    executor: LocalPool,
    executing: bool,
    transcode_result_rx: futures::channel::mpsc::UnboundedReceiver<Result<Track>>,
    output_tracks: Vec<Track>,
    transcode_target_sample_count: usize,
    transcoded_sample_count: Arc<AtomicUsize>,
    transcode_error: Option<Error>,
}

impl Transcoder {
    pub fn new(options: TranscodeOptions) -> Self {
        let (_transcode_result_tx, transcode_result_rx) = futures::channel::mpsc::unbounded(); // dummy
        Self {
            options,
            input_mp4: None,
            output_mp4: Vec::new(),
            executor: LocalPool::new(),
            executing: false,
            transcode_result_rx,
            output_tracks: Vec::new(),
            transcode_target_sample_count: 0,
            transcoded_sample_count: Arc::new(AtomicUsize::new(0)),
            transcode_error: None,
        }
    }

    pub fn parse_input_mp4_file(&mut self, mp4: &[u8]) -> Result<Mp4FileSummary> {
        let mp4 = InputMp4::parse(mp4)?;
        let summary = mp4.summary();
        self.input_mp4 = Some(mp4);
        Ok(summary)
    }

    pub fn start_transcode(&mut self) -> Result<()> {
        let input_mp4 = self
            .input_mp4
            .take()
            .ok_or_else(|| Error::new("input MP4 is not parsed yet"))?;
        for track in &input_mp4.tracks {
            if track.is_audio {
                continue;
            }

            // 入力映像は H.264 のみ
            if let Some(sample_entry) = track.chunks.iter().find_map(|c| {
                if matches!(c.sample_entry, SampleEntry::Avc1(_)) {
                    None
                } else {
                    Some(&c.sample_entry)
                }
            }) {
                return Err(Error::new(format!(
                    "Only H.264 is supported for input video codec: {}",
                    sample_entry.box_type()
                )));
            }
        }

        let (transcode_result_tx, transcode_result_rx) = futures::channel::mpsc::unbounded(); // dummy
        let _ = self.options;
        for track in input_mp4.tracks {
            self.transcode_target_sample_count += track
                .chunks
                .iter()
                .filter(|c| matches!(c.sample_entry, SampleEntry::Avc1(_)))
                .map(|c| c.samples.len())
                .sum::<usize>();

            let transcoder = TrackTranscoder {
                track,
                spawner: self.executor.spawner(),
                transcoded_sample_count: Arc::clone(&self.transcoded_sample_count),
                options: self.options.clone(),
            };
            let transcode_result_tx = transcode_result_tx.clone();
            self.executor
                .spawner()
                .spawn_local(async move {
                    let result = transcoder.run().await;
                    let _ = transcode_result_tx.unbounded_send(result);
                })
                .map_err(|e| Error::new(e.to_string()))?;
        }
        self.transcode_result_rx = transcode_result_rx;
        Ok(())
    }

    pub fn poll_transcode(&mut self) -> Result<TranscodeProgress> {
        if let Some(e) = self.transcode_error.clone() {
            return Err(e);
        }

        if !self.executing {
            self.executing = true;
            self.executor.run_until_stalled();
            self.executing = false;

            let mut do_continue = true;
            while do_continue {
                do_continue = false;
                match self.transcode_result_rx.try_recv() {
                    Err(_) => {
                        // メッセージなし (Empty) または全ての変換が終了した (Closed)
                    }
                    Ok(result) => {
                        // 特定のトラックの変換が完了した or 失敗した
                        self.transcode_error = result.as_ref().err().cloned();
                        self.output_tracks.push(result?);
                        do_continue = true;
                    }
                }
            }
        }

        Ok(TranscodeProgress {
            done: self.transcode_result_rx.is_terminated(),
            rate: if self.transcode_target_sample_count > 0 {
                self.transcoded_sample_count.load(Ordering::SeqCst) as f32
                    / self.transcode_target_sample_count as f32
            } else {
                1.0
            },
        })
    }

    pub fn build_output_mp4_file(&mut self) -> Result<()> {
        let builder = OutputMp4Builder::new(std::mem::take(&mut self.output_tracks));
        let mp4 = builder.build()?;
        self.output_mp4 = mp4.encode_to_vec().map_err(|e| Error::new(e.to_string()))?;
        Ok(())
    }

    pub fn get_output_mp4_file(&mut self) -> &Vec<u8> {
        &self.output_mp4
    }
}

#[derive(Debug)]
struct TrackTranscoder {
    track: Track,
    spawner: LocalSpawner,
    transcoded_sample_count: Arc<AtomicUsize>,
    options: TranscodeOptions,
}

impl TrackTranscoder {
    async fn run(self) -> Result<Track> {
        let mut output_track = Track {
            is_audio: self.track.is_audio,
            chunks: Vec::new(),
        };

        // サンプルエントリーが同じチャンクはまとめる
        let mut current_chunk = None;
        for chunk in &self.track.chunks {
            let Some(current) = &mut current_chunk else {
                current_chunk = Some(chunk.clone());
                continue;
            };
            if current.sample_entry != chunk.sample_entry {
                output_track
                    .chunks
                    .push(std::mem::replace(current, chunk.clone()));
                continue;
            }
            current.samples.extend(chunk.samples.iter().cloned());
        }
        if let Some(chunk) = current_chunk {
            output_track.chunks.push(chunk);
        }

        for chunk in &mut output_track.chunks {
            if !matches!(chunk.sample_entry, SampleEntry::Avc1(_)) {
                // H.264 以外 (= 音声) は無変換
                continue;
            }
            self.transcode_chunk(chunk).await?;
        }

        Ok(output_track)
    }

    async fn transcode_chunk(&self, chunk: &mut Chunk) -> Result<()> {
        let SampleEntry::Avc1(sample_entry) = &chunk.sample_entry else {
            unreachable!();
        };

        let decoder = WebCodec::create_h264_decoder(sample_entry).await?;
        let encoder = WebCodec::create_encoder(&self.options.video_encoder_config).await?;

        let decoder_id = decoder.0;
        let encoder_id = encoder.0;
        let mut transcodings = VecDeque::new();
        for mut sample in std::mem::take(&mut chunk.samples) {
            transcodings.push_back(
                self.spawner
                    .spawn_local_with_handle(async move {
                        let decoded =
                            WebCodec::decode(decoder_id, sample.keyframe, &sample.data).await?;
                        let encoded =
                            WebCodec::encode(encoder_id, sample.keyframe, decoded).await?;
                        sample.description = encoded.description;
                        sample.data = encoded.data;
                        Ok::<_, Error>(sample)
                    })
                    .map_err(|e| Error::new(e.to_string()))?,
            );
            if transcodings.len() > DECODE_QUEQUE_SIZE {
                let sample = transcodings
                    .pop_front()
                    .ok_or_else(|| Error::new("transcoding queue is empty"))?
                    .await?;
                chunk.samples.push(sample);
                self.transcoded_sample_count.fetch_add(1, Ordering::SeqCst);
            }
        }
        std::mem::drop(decoder); // もうデコードすべきサンプルがないことをデコーダーに伝える

        for sample in transcodings {
            chunk.samples.push(sample.await?);
            self.transcoded_sample_count.fetch_add(1, Ordering::SeqCst);
        }

        chunk.sample_entry = self.build_output_sample_entry(chunk)?;

        Ok(())
    }

    fn build_output_sample_entry(&self, chunk: &Chunk) -> Result<SampleEntry> {
        let visual = VisualSampleEntryFields {
            data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            width: self.options.video_encoder_config.width,
            height: self.options.video_encoder_config.height,
            horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
            vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
            frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
            compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
            depth: VisualSampleEntryFields::DEFAULT_DEPTH,
        };
        match self.options.video_encoder_config.codec.as_str() {
            "vp8" => Ok(SampleEntry::Vp08(Vp08Box {
                visual,
                vpcc_box: VpccBox {
                    profile: 0,
                    level: 0,
                    bit_depth: Uint::new(8),
                    chroma_subsampling: Uint::new(1),
                    video_full_range_flag: Uint::new(0),
                    colour_primaries: 1,
                    transfer_characteristics: 1,
                    matrix_coefficients: 1,
                    codec_initialization_data: Vec::new(),
                },
                unknown_boxes: Vec::new(),
            })),
            "vp09.00.10.08" => {
                // See: https://www.webmproject.org/vp9/mp4/ (Codecs Parameter String)
                //      `vp09.<profile>.<level>.<bitDepth>[...]`
                Ok(SampleEntry::Vp09(Vp09Box {
                    visual,
                    vpcc_box: VpccBox {
                        profile: 0,
                        level: 10,
                        bit_depth: Uint::new(8),
                        chroma_subsampling: Uint::new(1),
                        video_full_range_flag: Uint::new(0),
                        colour_primaries: 1,
                        transfer_characteristics: 1,
                        matrix_coefficients: 1,
                        codec_initialization_data: Vec::new(),
                    },
                    unknown_boxes: Vec::new(),
                }))
            }
            "av01.0.01M.08" => {
                // See: https://aomediacodec.github.io/av1-isobmff/#codecsparam
                //      `av01.<profile>.<level><tier>.<bitDepth>[...]`
                Ok(SampleEntry::Av01(Av01Box {
                    visual,
                    av1c_box: Av1cBox {
                        seq_profile: Uint::new(0),
                        seq_level_idx_0: Uint::new(1),
                        seq_tier_0: Uint::new(0),
                        high_bitdepth: Uint::new(0),
                        twelve_bit: Uint::new(0),
                        monochrome: Uint::new(0),
                        chroma_subsampling_x: Uint::new(1),
                        chroma_subsampling_y: Uint::new(1),
                        chroma_sample_position: Uint::new(0),
                        initial_presentation_delay_minus_one: None,
                        config_obus: Vec::new(),
                    },
                    unknown_boxes: Vec::new(),
                }))
            }
            "avc1.42e02a" => Ok(SampleEntry::Avc1(Avc1Box {
                visual,
                avcc_box: Self::decode_description(chunk, AvccBox::TYPE)?,
                unknown_boxes: Vec::new(),
            })),
            "hev1.1.6.L90.B0" => Ok(SampleEntry::Hev1(Hev1Box {
                visual,
                hvcc_box: Self::decode_description(chunk, HvccBox::TYPE)?,
                unknown_boxes: Vec::new(),
            })),
            codec => Err(Error::new(format!("Not yet implemented: {codec}"))),
        }
    }

    fn decode_description<T: Decode>(chunk: &Chunk, box_type: BoxType) -> Result<T> {
        let description = chunk
            .samples
            .first()
            .ok_or_else(|| Error::new("no samples in chunk"))?
            .description
            .as_ref()
            .ok_or_else(|| Error::new("no description in sample"))?;
        let header = BoxHeader {
            box_type,
            box_size: BoxSize::with_payload_size(box_type, description.len() as u64),
        };
        let mut data = Vec::new();
        header
            .encode(&mut data)
            .map_err(|e| Error::new(e.to_string()))?;
        data.extend_from_slice(description);

        T::decode(&data[..])
            .map_err(|e| Error::new(e.to_string()))
            .map(|(desc, _)| desc)
    }
}
