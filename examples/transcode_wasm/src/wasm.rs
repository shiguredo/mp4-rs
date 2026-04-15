use std::{future::Future, marker::PhantomData};

use futures::{TryFutureExt, channel::oneshot};
use shiguredo_mp4::Encode;
use shiguredo_mp4::boxes::Avc1Box;

use crate::mp4::Mp4FileSummary;
use crate::transcode::{
    TranscodeOptions, TranscodeProgress, Transcoder, VideoEncoderConfig, VideoFrame,
};
use crate::{Error, JsonResult, Result};

pub struct VideoDecoderConfig {
    pub codec: String,
    pub coded_width: u16,
    pub coded_height: u16,
    pub description: Vec<u8>,
}

impl nojson::DisplayJson for VideoDecoderConfig {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            f.member("codec", &self.codec)?;
            f.member("codedWidth", self.coded_width)?;
            f.member("codedHeight", self.coded_height)?;
            f.member("description", &self.description)
        })
    }
}

pub struct Encoded {
    pub description: Option<Vec<u8>>,
    pub data: Vec<u8>,
}

unsafe extern "C" {
    pub fn consoleLog(msg: *const u8, msg_len: i32);

    #[expect(improper_ctypes)]
    pub fn createVideoDecoder(
        result_future: *mut oneshot::Sender<Result<CoderId>>,
        config: JsonVec<VideoDecoderConfig>,
    );

    #[expect(improper_ctypes)]
    pub fn decode(
        result_future: *mut oneshot::Sender<Result<VideoFrame>>,
        coder_id: CoderId,
        keyframe: bool,
        data_offset: *const u8,
        data_len: u32,
    );

    #[expect(improper_ctypes)]
    pub fn createVideoEncoder(
        result_future: *mut oneshot::Sender<Result<CoderId>>,
        config: JsonVec<VideoEncoderConfig>,
    );

    #[expect(improper_ctypes)]
    pub fn encode(
        result_future: *mut oneshot::Sender<Result<Encoded>>,
        coder_id: CoderId,
        keyframe: bool,
        width: u32,
        height: u32,
        data_offset: *const u8,
        data_len: u32,
    );

    pub fn closeCoder(coder_id: CoderId);
}

pub struct WebCodec;

pub type CoderId = u32;

impl WebCodec {
    pub fn create_h264_decoder(config: &Avc1Box) -> impl Future<Output = Result<Coder>> {
        let (tx, rx) = oneshot::channel::<Result<_>>();

        let mut description = [0; 1024]; // 十分なサイズのバッファを用意しておく
        let encoded_size = config
            .avcc_box
            .encode(&mut description)
            .expect("unreachable");
        let description = description[8..encoded_size].to_vec(); // ボックスヘッダ部分は取り除いて Vec にする

        let config = VideoDecoderConfig {
            codec: format!(
                "avc1.{:02x}{:02x}{:02x}",
                config.avcc_box.avc_profile_indication,
                config.avcc_box.profile_compatibility,
                config.avcc_box.avc_level_indication
            ),
            description,
            coded_width: config.visual.width,
            coded_height: config.visual.height,
        };
        unsafe {
            createVideoDecoder(Box::into_raw(Box::new(tx)), JsonVec::new(config));
        }
        rx.map_ok_or_else(|e| Err(Error::new(e.to_string())), |r| r.map(Coder))
    }

    pub fn decode(
        decoder: CoderId,
        keyframe: bool,
        encoded_data: &[u8],
    ) -> impl Future<Output = Result<VideoFrame>> {
        let (tx, rx) = oneshot::channel::<Result<_>>();
        unsafe {
            decode(
                Box::into_raw(Box::new(tx)),
                decoder,
                keyframe,
                encoded_data.as_ptr(),
                encoded_data.len() as u32,
            );
        }
        rx.map_ok_or_else(|e| Err(Error::new(e.to_string())), |r| r)
    }

    pub fn create_encoder(config: &VideoEncoderConfig) -> impl Future<Output = Result<Coder>> {
        let (tx, rx) = oneshot::channel::<Result<_>>();
        unsafe {
            createVideoEncoder(Box::into_raw(Box::new(tx)), JsonVec::new(config.clone()));
        }
        rx.map_ok_or_else(|e| Err(Error::new(e.to_string())), |r| r.map(Coder))
    }

    pub fn encode(
        encoder: CoderId,
        keyframe: bool,
        frame: VideoFrame,
    ) -> impl Future<Output = Result<Encoded>> {
        let (tx, rx) = oneshot::channel::<Result<_>>();
        unsafe {
            encode(
                Box::into_raw(Box::new(tx)),
                encoder,
                keyframe,
                frame.width as u32,
                frame.height as u32,
                frame.data.as_ptr(),
                frame.data.len() as u32,
            );
        }
        rx.map_ok_or_else(|e| Err(Error::new(e.to_string())), |r| r)
    }
}

#[derive(Debug)]
pub struct Coder(pub CoderId);

impl Drop for Coder {
    fn drop(&mut self) {
        unsafe {
            closeCoder(self.0);
        }
    }
}

#[unsafe(no_mangle)]
#[expect(non_snake_case)]
pub fn newTranscoder(options: JsonVec<TranscodeOptions>) -> *mut Transcoder {
    std::panic::set_hook(Box::new(|info| {
        let msg = info.to_string();
        unsafe {
            consoleLog(msg.as_ptr(), msg.len() as i32);
        }
    }));

    let options = unsafe { options.into_value() };
    Box::into_raw(Box::new(Transcoder::new(options)))
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn freeTranscoder(transcoder: *mut Transcoder) {
    let _ = unsafe { Box::from_raw(transcoder) };
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn notifyCreateVideoDecoderResult(
    transcoder: *mut Transcoder,
    result_future: *mut oneshot::Sender<Result<CoderId>>,
    result: JsonVec<JsonResult<CoderId>>,
) {
    let result = unsafe { result.into_value() }.0;
    let tx = unsafe { Box::from_raw(result_future) };
    let _ = tx.send(result);
    let _ = pollTranscode(transcoder);
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn notifyCreateVideoEncoderResult(
    transcoder: *mut Transcoder,
    result_future: *mut oneshot::Sender<Result<CoderId>>,
    result: JsonVec<JsonResult<CoderId>>,
) {
    let result = unsafe { result.into_value() }.0;
    let tx = unsafe { Box::from_raw(result_future) };
    let _ = tx.send(result);
    let _ = pollTranscode(transcoder);
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn notifyDecodeResult(
    transcoder: *mut Transcoder,
    result_future: *mut oneshot::Sender<Result<VideoFrame>>,
    result: JsonVec<JsonResult<VideoFrame>>,
    decoded_data: *mut Vec<u8>,
) {
    let result = unsafe { result.into_value() }.0;
    let tx = unsafe { Box::from_raw(result_future) };
    let _ = tx.send(result.map(|mut frame| {
        frame.data = *unsafe { Box::from_raw(decoded_data) };
        frame
    }));
    let _ = pollTranscode(transcoder);
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn notifyEncodeResult(
    transcoder: *mut Transcoder,
    result_future: *mut oneshot::Sender<Result<Encoded>>,
    result: JsonVec<JsonResult<Option<Vec<u8>>>>,
    encoded_data: *mut Vec<u8>,
) {
    let result = unsafe { result.into_value() }.0;
    let tx = unsafe { Box::from_raw(result_future) };
    let _ = tx.send(result.map(|description| Encoded {
        description,
        data: *unsafe { Box::from_raw(encoded_data) },
    }));
    let _ = pollTranscode(transcoder);
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn parseInputMp4File(
    transcoder: *mut Transcoder,
    input_mp4: *mut Vec<u8>,
) -> JsonVec<JsonResult<Mp4FileSummary>> {
    let transcoder = unsafe { &mut *transcoder };
    let input_mp4 = unsafe { Box::from_raw(input_mp4) };
    let result = transcoder.parse_input_mp4_file(&input_mp4);
    JsonVec::new(JsonResult(result))
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn startTranscode(transcoder: *mut Transcoder) -> JsonVec<JsonResult<()>> {
    let transcoder = unsafe { &mut *transcoder };
    let result = transcoder.start_transcode();
    JsonVec::new(JsonResult(result))
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn pollTranscode(transcoder: *mut Transcoder) -> JsonVec<JsonResult<TranscodeProgress>> {
    let transcoder = unsafe { &mut *transcoder };
    let result = transcoder.poll_transcode();
    JsonVec::new(JsonResult(result))
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn buildOutputMp4File(transcoder: *mut Transcoder) -> JsonVec<JsonResult<()>> {
    let transcoder = unsafe { &mut *transcoder };
    let result = transcoder.build_output_mp4_file();
    JsonVec::new(JsonResult(result))
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn getOutputMp4File(transcoder: *mut Transcoder) -> *const Vec<u8> {
    let transcoder = unsafe { &mut *transcoder };
    transcoder.get_output_mp4_file() as *const _
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn vecOffset(v: *mut Vec<u8>) -> *mut u8 {
    unsafe { &mut *v }.as_mut_ptr()
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn vecLen(v: *mut Vec<u8>) -> i32 {
    unsafe { &*v }.len() as i32
}

#[unsafe(no_mangle)]
#[expect(non_snake_case)]
pub fn allocateVec(len: i32) -> *mut Vec<u8> {
    Box::into_raw(Box::new(vec![0; len as usize]))
}

#[unsafe(no_mangle)]
#[expect(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub fn freeVec(v: *mut Vec<u8>) {
    let _ = unsafe { Box::from_raw(v) };
}

#[repr(transparent)]
pub struct JsonVec<T> {
    bytes: *mut Vec<u8>,
    _ty: PhantomData<T>,
}

impl<T: nojson::DisplayJson> JsonVec<T> {
    fn new(value: T) -> Self {
        let text = nojson::Json(&value).to_string();
        let bytes = Box::into_raw(Box::new(text.into_bytes()));
        Self {
            bytes,
            _ty: PhantomData,
        }
    }
}

impl<T> JsonVec<T>
where
    T: for<'text, 'raw> TryFrom<nojson::RawJsonValue<'text, 'raw>, Error = nojson::JsonParseError>,
{
    unsafe fn into_value(self) -> T {
        let bytes = unsafe { Box::from_raw(self.bytes) };
        let text = std::str::from_utf8(&bytes).expect("valid UTF-8 JSON");
        let json = nojson::RawJson::parse(text).expect("valid JSON");
        T::try_from(json.value()).expect("valid JSON shape")
    }
}
