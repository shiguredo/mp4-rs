/**
 * Node.js で mp4-rs の WASM を使って MP4 ファイルをデマルチプレックスする例
 *
 * # 使用方法
 *
 * ```bash
 * cargo build -p wasm --target wasm32-unknown-unknown --profile release-wasm
 * wasm-bindgen --target nodejs --out-dir crates/wasm/pkg/node \
 *   target/wasm32-unknown-unknown/release-wasm/mp4_wasm.wasm
 * node crates/wasm/examples/demux.js /path/to/input.mp4
 * ```
 */
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const wasm = require('../pkg/node/mp4_wasm.js');

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// 必要な入力データをすべて供給する
function feedAllInput(target, bytes) {
    for (;;) {
        const requiredJson = target.requiredInput();
        if (requiredJson === undefined) {
            break;
        }
        const required = JSON.parse(requiredJson);
        const start = required.position;
        // size が null の場合はファイル末尾までのデータが必要であることを意味する
        const end = required.size === null ? bytes.length : start + required.size;
        target.handleInput(start, bytes.subarray(start, end));
    }
}

function demuxMP4File(filePath) {
    if (!fs.existsSync(filePath)) {
        console.error(`Error: File not found: ${filePath}`);
        process.exit(1);
    }

    const bytes = fs.readFileSync(filePath);
    console.log(`Demuxing ${path.basename(filePath)}...`);

    const demuxer = new wasm.Mp4FileDemuxer();
    try {
        feedAllInput(demuxer, bytes);

        const tracks = JSON.parse(demuxer.tracks());
        console.log(`\nFound ${tracks.length} track(s)\n`);
        for (const track of tracks) {
            console.log(`Track ${track.track_id}:`);
            console.log(`  Kind: ${track.kind}`);
            console.log(`  Duration: ${track.duration} (timescale: ${track.timescale})`);
            console.log(`  Duration (seconds): ${track.duration / track.timescale}`);
            console.log();
        }

        console.log('Samples:');
        let sampleCount = 0;
        let keyframeCount = 0;
        for (;;) {
            const sampleJson = demuxer.nextSample();
            if (sampleJson === undefined) {
                break;
            }
            const sample = JSON.parse(sampleJson);
            sampleCount += 1;
            if (sample.keyframe) {
                keyframeCount += 1;
            }
            if (sampleCount > 10) {
                continue;
            }
            console.log(`  Sample ${sampleCount}:`);
            console.log(`    Track ID: ${sample.track_id}`);
            console.log(`    Keyframe: ${sample.keyframe ? 'Yes' : 'No'}`);
            console.log(`    Timestamp: ${sample.timestamp}`);
            console.log(`    Duration: ${sample.duration}`);
            console.log(`    Data offset: 0x${sample.data_offset.toString(16)}`);
            console.log(`    Data size: ${sample.data_size} bytes`);
            if (sample.sample_entry !== undefined) {
                console.log(`    Codec: ${sample.sample_entry.kind}`);
            }
            console.log();
        }
        if (sampleCount > 10) {
            console.log('  ... (showing first 10 samples)\n');
        }
        console.log(`Total samples processed: ${sampleCount}, Keyframes: ${keyframeCount}`);
    } finally {
        demuxer.free();
    }
}

const args = process.argv.slice(2);
if (args.length === 0) {
    console.error('Usage: node demux.js <mp4_file>');
    process.exit(1);
}

demuxMP4File(args[0]);
