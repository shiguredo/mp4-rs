/**
 * Node.js で mp4-rs の WASM を使って MP4 ファイルに無音の Opus を 10 秒分書き込むサンプル
 *
 * # 使用方法
 *
 * ```bash
 * cargo build -p wasm --target wasm32-unknown-unknown --profile release-wasm
 * wasm-bindgen --target nodejs --out-dir crates/wasm/pkg/node \
 *   target/wasm32-unknown-unknown/release-wasm/mp4_wasm.wasm
 * node crates/wasm/examples/mux.js /path/to/output.mp4
 * ```
 */
import fs from 'node:fs';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const wasm = require('../pkg/node/mp4_wasm.js');

// 無音の Opus フレームデータ（3 バイト）
const OPUS_SILENCE_FRAME = new Uint8Array([0xf8, 0xff, 0xfe]);

function createMP4WithOpus(outputPath) {
    console.log('Creating MP4 file with 10 seconds of silent Opus audio...');

    // 10 秒間の Opus（48 kHz）は 20 ms フレームで 500 フレームになる
    const NUM_FRAMES = 500;
    const FRAME_DURATION = 960;

    const file = fs.openSync(outputPath, 'w');
    let muxer;
    try {
        // 0 を指定して faststart を無効にする。faststart にする場合は、
        // 出力する moov のサイズ以上を予約して先頭に空き領域を確保する
        muxer = new wasm.Mp4FileMuxer(0);
        const initial = muxer.initialBoxes();
        fs.writeSync(file, initial, 0, initial.length, 0);
        let currentOffset = initial.length;

        const opusEntry = {
            kind: 'opus',
            channelCount: 2,
            sampleRate: 48000,
            sampleSize: 16,
            preSkip: 0,
            inputSampleRate: 48000,
            outputGain: 0,
        };

        console.log(`Adding ${NUM_FRAMES} Opus frames (${(NUM_FRAMES * FRAME_DURATION) / 48000} seconds)...`);
        for (let index = 0; index < NUM_FRAMES; index += 1) {
            fs.writeSync(file, OPUS_SILENCE_FRAME, 0, OPUS_SILENCE_FRAME.length, currentOffset);
            muxer.appendSample(
                JSON.stringify({
                    track_kind: 'audio',
                    keyframe: true,
                    timescale: 48000,
                    duration: FRAME_DURATION,
                    composition_time_offset: null,
                    data_offset: currentOffset,
                    data_size: OPUS_SILENCE_FRAME.length,
                    sample_entry: opusEntry,
                }),
            );
            currentOffset += OPUS_SILENCE_FRAME.length;
            if ((index + 1) % 100 === 0) {
                console.log(`  Added ${index + 1} frames...`);
            }
        }
        console.log(`Total frames added: ${NUM_FRAMES}`);

        console.log('Finalizing muxer...');
        muxer.finalize();
        for (;;) {
            const output = muxer.nextOutput();
            if (output === undefined) {
                break;
            }
            const data = output.takeData();
            fs.writeSync(file, data, 0, data.length, output.offset);
            console.log(`  Wrote ${data.length} bytes at offset ${output.offset}`);
            output.free();
        }
        console.log(`\nSuccessfully created '${outputPath}'`);
    } finally {
        if (muxer !== undefined) {
            muxer.free();
        }
        fs.closeSync(file);
    }
}

const args = process.argv.slice(2);
if (args.length === 0) {
    console.error('Usage: node mux.js <output_mp4>');
    process.exit(1);
}

createMP4WithOpus(args[0]);
