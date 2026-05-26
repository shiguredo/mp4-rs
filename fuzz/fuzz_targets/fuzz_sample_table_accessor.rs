#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::aux::SampleTableAccessor;
use shiguredo_mp4::boxes::StblBox;
use shiguredo_mp4::Decode;
use std::num::NonZeroU32;

fuzz_target!(|data: &[u8]| {
    // 任意のバイト列を StblBox としてデコードし、
    // SampleTableAccessor の全メソッドを呼び出してもパニックしないことを確認する

    let Ok((stbl_box, _)) = StblBox::decode(data) else {
        return;
    };

    let Ok(accessor) = SampleTableAccessor::new(stbl_box) else {
        return;
    };

    // 基本メソッド
    let _ = accessor.sample_count();
    let _ = accessor.chunk_count();
    let _ = accessor.stbl_box();

    // 境界値でのアクセス
    if let Some(sample) = accessor.get_sample(NonZeroU32::MIN) {
        exercise_sample_accessor(&sample);
    }
    let _ = accessor.get_sample(NonZeroU32::MAX);

    if let Some(chunk) = accessor.get_chunk(NonZeroU32::MIN) {
        exercise_chunk_accessor(&chunk);
    }
    let _ = accessor.get_chunk(NonZeroU32::MAX);

    // タイムスタンプ検索
    if let Some(sample) = accessor.get_sample_by_timestamp(0) {
        exercise_sample_accessor(&sample);
    }
    if let Some(sample) = accessor.get_sample_by_timestamp(u64::MAX) {
        exercise_sample_accessor(&sample);
    }

    // 全サンプル走査
    for sample in accessor.samples() {
        exercise_sample_accessor(&sample);
    }

    // 全チャンク走査
    for chunk in accessor.chunks() {
        exercise_chunk_accessor(&chunk);
    }
});

fn exercise_sample_accessor(sample: &shiguredo_mp4::aux::SampleAccessor<'_, StblBox>) {
    let _ = sample.index();
    let _ = sample.duration();
    let _ = sample.timestamp();
    let _ = sample.data_size();
    let _ = sample.data_offset();
    let _ = sample.is_sync_sample();
    let _ = sample.sync_sample();
    let _ = sample.composition_time_offset();
    let _ = sample.chunk();
}

fn exercise_chunk_accessor(chunk: &shiguredo_mp4::aux::ChunkAccessor<'_, StblBox>) {
    let _ = chunk.index();
    let _ = chunk.offset();
    let _ = chunk.sample_entry();
    let _ = chunk.sample_entry_index();
    let _ = chunk.sample_count();
    for sample in chunk.samples() {
        exercise_sample_accessor(&sample);
    }
}
