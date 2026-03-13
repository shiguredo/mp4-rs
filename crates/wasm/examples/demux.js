/**
 * Node.js で mp4-rs の WASM を使って MP4 ファイルをデマルチプレックスする例
 *
 * # 使用方法
 *
 * ```bash
 * cargo build -p wasm --target wasm32-unknown-unknown --profile release-wasm
 * node crates/wasm/examples/demux.js /path/to/input.mp4
 * ```
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// wasm ファイルを読み込んで初期化する
const wasmPath = path.join(__dirname, '../../../target/wasm32-unknown-unknown/release-wasm/mp4_wasm.wasm');
const wasmBuffer = fs.readFileSync(wasmPath);
const wasmInstance = await WebAssembly.instantiate(wasmBuffer);

const {
    memory,
    mp4_file_demuxer_new,
    mp4_file_demuxer_free,
    mp4_file_demuxer_get_required_input,
    mp4_file_demuxer_handle_input,
    mp4_file_demuxer_get_tracks,
    mp4_file_demuxer_next_sample,
    mp4_file_demuxer_get_last_error,
    mp4_demux_track_info_to_json,
    mp4_demux_sample_to_json,
    mp4_vec_ptr,
    mp4_vec_len,
    mp4_vec_free,
    mp4_alloc,
    mp4_free,
} = wasmInstance.instance.exports;

// メモリバッファへのアクセスを簡略化する関数群
function readCString(ptr) {
    const view = new Uint8Array(memory.buffer, ptr);
    let length = 0;
    while (view[length] !== 0) length++;
    return new TextDecoder().decode(view.slice(0, length));
}

function readU8Array(ptr, len) {
    return new Uint8Array(memory.buffer, ptr, len);
}

function readJSON(vecPtr) {
    const ptr = mp4_vec_ptr(vecPtr);
    const len = mp4_vec_len(vecPtr);
    const bytes = readU8Array(ptr, len);
    const jsonStr = new TextDecoder().decode(bytes);
    mp4_vec_free(vecPtr);
    return JSON.parse(jsonStr);
}

async function demuxMP4File(filePath) {
    if (!fs.existsSync(filePath)) {
        console.error(`Error: File not found: ${filePath}`);
        process.exit(1);
    }

    const fileSize = fs.statSync(filePath).size;
    const file = fs.openSync(filePath, 'r');

    // デマルチプレックサーを作成
    const demuxerPtr = mp4_file_demuxer_new();
    if (!demuxerPtr) {
        console.error('Error: Could not create demuxer');
        process.exit(1);
    }

    // ファイルデータを読み込んでデマルチプレックス処理を進める
    console.log(`Demuxing ${path.basename(filePath)}...`);

    while (true) {
        // ポインタを確保（読み書き用）
        const requiredPosPtr = mp4_alloc(8);  // uint64_t
        const requiredSizePtr = mp4_alloc(4); // int32_t

        // 次に必要な入力データの位置とサイズを取得
        const err = mp4_file_demuxer_get_required_input(
            demuxerPtr,
            requiredPosPtr,
            requiredSizePtr
        );

        if (err !== 0) { // MP4_ERROR_OK
            const errorMsg = readCString(mp4_file_demuxer_get_last_error(demuxerPtr));
            console.error(`Error: Failed to get required input: ${errorMsg}`);
            mp4_file_demuxer_free(demuxerPtr);
            fs.closeSync(file);
            process.exit(1);
        }

        // 値を読み取る
        // [NOTE] wasm は little endian なので、ホストが big endian の場合に備えて DataView を使用している
        const requiredPosView = new DataView(memory.buffer, requiredPosPtr, 8);
        const requiredSizeView = new DataView(memory.buffer, requiredSizePtr, 4);

        const requiredPos = requiredPosView.getBigUint64(0, true); // true = リトルエンディアン
        let requiredSize = requiredSizeView.getInt32(0, true); // true = リトルエンディアン

        mp4_free(requiredPosPtr, 8);
        mp4_free(requiredSizePtr, 4);

        // 必要なデータサイズが 0 の場合は初期化完了
        if (requiredSize === 0) {
            break;
        }

        // 読み込むサイズを決定
        let readSize = requiredSize;
        if (requiredSize === -1) {
            // ファイル末尾までの読み込みが必要
            readSize = fileSize - Number(requiredPos);
        }

        // ファイルからデータを読み込む
        const buffer = Buffer.alloc(readSize);
        fs.readSync(file, buffer, 0, readSize, Number(requiredPos));

        // WASM メモリにコピー
        const wasmBuffer = mp4_alloc(readSize);
        const wasmView = new Uint8Array(memory.buffer, wasmBuffer, readSize);
        wasmView.set(buffer);

        // デマルチプレックサーにデータを入力
        mp4_file_demuxer_handle_input(
            demuxerPtr,
            requiredPos,
            wasmBuffer,
            readSize
        );

        mp4_free(wasmBuffer, readSize);
    }

    // トラック情報を取得
    const tracksPtr = mp4_alloc(8); // ポインタ
    const trackCountPtr = mp4_alloc(4); // uint32_t

    const tracksErr = mp4_file_demuxer_get_tracks(
        demuxerPtr,
        tracksPtr,
        trackCountPtr
    );

    if (tracksErr !== 0) {
        const errorMsg = readCString(mp4_file_demuxer_get_last_error(demuxerPtr));
        console.error(`Error: Failed to get tracks: ${errorMsg}`);
        mp4_file_demuxer_free(demuxerPtr);
        fs.closeSync(file);
        process.exit(1);
    }

    // トラック数を取得
    // [NOTE] wasm は little endian なので、ホストが big endian の場合に備えて DataView を使用している
    const trackCountView = new DataView(memory.buffer, trackCountPtr, 4);
    const trackCount = trackCountView.getUint32(0, true); // true = リトルエンディアン

    console.log(`\nFound ${trackCount} track(s)\n`);

    // トラック情報を表示
    const tracksPointerView = new DataView(memory.buffer, tracksPtr, 8);
    const tracksPointer = Number(tracksPointerView.getBigUint64(0, true)); // true = リトルエンディアン

    for (let i = 0; i < trackCount; i++) {
        // Mp4DemuxTrackInfo のサイズを計算（id: u32, kind: u32, duration: u64, timescale: u32）
        const trackInfoSize = 4 + 4 + 8 + 4; // = 20 bytes
        const trackInfoPtr = tracksPointer + i * trackInfoSize;

        const jsonPtr = mp4_demux_track_info_to_json(trackInfoPtr);
        const trackInfo = readJSON(jsonPtr);

        console.log(`Track ${i + 1}:`);
        console.log(`  Track ID: ${trackInfo.track_id}`);
        console.log(`  Kind: ${trackInfo.kind}`);
        console.log(`  Duration: ${trackInfo.duration} (timescale: ${trackInfo.timescale})`);
        console.log(`  Duration (seconds): ${Number(trackInfo.duration) / trackInfo.timescale}`);
        console.log();
    }

    mp4_free(tracksPtr, 8);
    mp4_free(trackCountPtr, 4);

    // サンプル情報を取得・表示
    console.log('Samples:');
    let sampleCount = 0;
    let keyframeCount = 0;

    const samplePtr = mp4_alloc(256); // Mp4DemuxSample の最大サイズ

    while (sampleCount < 10) {
        const sampleErr = mp4_file_demuxer_next_sample(demuxerPtr, samplePtr);

        if (sampleErr === 8) { // MP4_ERROR_NO_MORE_SAMPLES
            break;
        }

        if (sampleErr !== 0) {
            const errorMsg = readCString(mp4_file_demuxer_get_last_error(demuxerPtr));
            console.error(`Error: Failed to get next sample: ${errorMsg}`);
            break;
        }

        sampleCount++;

        const jsonPtr = mp4_demux_sample_to_json(samplePtr);
        const sample = readJSON(jsonPtr);

        console.log(`  Sample ${sampleCount}:`);
        console.log(`    Track ID: ${sample.track_id}`);
        console.log(`    Keyframe: ${sample.keyframe ? 'Yes' : 'No'}`);
        console.log(`    Timestamp: ${sample.timestamp}`);
        console.log(`    Duration: ${sample.duration}`);
        console.log(`    Data offset: 0x${sample.data_offset.toString(16)}`);
        console.log(`    Data size: ${sample.data_size} bytes`);

        if (sample.sample_entry) {
            console.log(`    Codec: ${sample.sample_entry.kind}`);
        }

        console.log();

        if (sample.keyframe) {
            keyframeCount++;
        }
    }

    if (sampleCount >= 10) {
        console.log(`  ... (showing first 10 samples)\n`);
    }

    console.log(`Total samples processed: ${sampleCount}, Keyframes: ${keyframeCount}`);

    mp4_free(samplePtr, 256);

    // リソースを解放
    mp4_file_demuxer_free(demuxerPtr);
    fs.closeSync(file);
}

// メイン処理
const args = process.argv.slice(2);
if (args.length === 0) {
    console.error('Usage: node demux.js <mp4_file>');
    process.exit(1);
}

const filePath = args[0];
await demuxMP4File(filePath);
