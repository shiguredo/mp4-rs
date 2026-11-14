/**
 * wasm-bindgen API の統合テスト
 *
 * `crates/wasm/pkg/node` に生成したバインディングを Node.js から呼び出し、
 * mux / demux / fMP4 / 種別判定が往復することを検証する。
 *
 * # 実行方法
 *
 * ```bash
 * cargo build -p wasm --target wasm32-unknown-unknown --profile release-wasm
 * wasm-bindgen --target nodejs --out-dir crates/wasm/pkg/node \
 *   target/wasm32-unknown-unknown/release-wasm/mp4_wasm.wasm
 * node --test crates/wasm/tests/
 * ```
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';

const require = createRequire(import.meta.url);
const wasm = require('../pkg/node/mp4_wasm.js');

/** テストデータのディレクトリ */
const TESTDATA_DIR = new URL('../../../tests/testdata/', import.meta.url);

/** リポジトリルートの Cargo.toml */
const ROOT_CARGO_TOML = new URL('../../../Cargo.toml', import.meta.url);

/** テストデータを読み込む */
function readTestData(name) {
    return new Uint8Array(fs.readFileSync(new URL(name, TESTDATA_DIR)));
}

/** ルートの Cargo.toml からパッケージバージョンを読み取る */
function readRootVersion() {
    const text = fs.readFileSync(ROOT_CARGO_TOML, 'utf8');
    const match = /^version\s*=\s*"([^"]+)"/m.exec(text);
    assert.ok(match, 'Cargo.toml に version があること');
    return match[1];
}

/** 必要な入力データをすべて供給する */
function feedAllInput(target, bytes) {
    for (;;) {
        const requiredJson = target.requiredInput();
        if (requiredJson === undefined) {
            return;
        }
        const required = JSON.parse(requiredJson);
        const start = required.position;
        const end = required.size === null ? bytes.length : start + required.size;
        target.handleInput(start, bytes.subarray(start, end));
    }
}

/** MP4 ファイルを最後までデマルチプレックスする */
function demuxFile(bytes) {
    const demuxer = new wasm.Mp4FileDemuxer();
    try {
        feedAllInput(demuxer, bytes);
        const tracks = JSON.parse(demuxer.tracks());
        const samples = [];
        for (;;) {
            const sampleJson = demuxer.nextSample();
            if (sampleJson === undefined) {
                break;
            }
            samples.push(JSON.parse(sampleJson));
        }
        return { demuxer, tracks, samples };
    } catch (error) {
        demuxer.free();
        throw error;
    }
}

/**
 * サンプルを mux して 1 つの MP4 ファイルを組み立てる
 *
 * 書き込み先の位置は `initialBoxes()` の長さから始め、サンプルデータを
 * 追加するたびに進める。`finalize()` の出力は最後に重ね書きする。
 */
function muxFile(samples, reservedMoovBoxSize) {
    const muxer = new wasm.Mp4FileMuxer(reservedMoovBoxSize);
    const chunks = [];
    try {
        const initial = muxer.initialBoxes();
        chunks.push({ offset: 0, data: initial });
        let position = initial.length;

        for (const sample of samples) {
            chunks.push({ offset: position, data: sample.data });
            muxer.appendSample(
                JSON.stringify({
                    ...sample.metadata,
                    data_offset: position,
                    data_size: sample.data.length,
                }),
            );
            position += sample.data.length;
        }

        muxer.finalize();
        const faststartEnabled = muxer.isFaststartEnabled();
        const moovBoxSize = muxer.moovBoxSize();

        for (;;) {
            const output = muxer.nextOutput();
            if (output === undefined) {
                break;
            }
            chunks.push({ offset: output.offset, data: output.takeData() });
            output.free();
        }

        return { chunks, faststartEnabled, moovBoxSize };
    } finally {
        muxer.free();
    }
}

/** チャンクをオフセットに従って 1 つのバイト列に組み立てる */
function assembleChunks(chunks) {
    let size = 0;
    for (const chunk of chunks) {
        size = Math.max(size, chunk.offset + chunk.data.length);
    }
    const bytes = new Uint8Array(size);
    for (const chunk of chunks) {
        bytes.set(chunk.data, chunk.offset);
    }
    return bytes;
}

/** mux して 1 つの MP4 ファイルにする */
function muxToBytes(samples, reservedMoovBoxSize = 0) {
    const { chunks } = muxFile(samples, reservedMoovBoxSize);
    return assembleChunks(chunks);
}

/** テスト用の AVC1 サンプルエントリー */
const AVC1_ENTRY = {
    kind: 'avc1',
    width: 640,
    height: 480,
    avcProfileIndication: 66,
    profileCompatibility: 0,
    avcLevelIndication: 30,
    lengthSizeMinusOne: 3,
    sps: [[103, 66, 0, 30]],
    pps: [[104, 206, 60]],
};

/** テスト用の Opus サンプルエントリー */
const OPUS_ENTRY = {
    kind: 'opus',
    channelCount: 2,
    sampleRate: 48000,
    sampleSize: 16,
    preSkip: 0,
    inputSampleRate: 48000,
    outputGain: 0,
};

/** テスト用の VP8 サンプルエントリー */
const VP08_ENTRY = {
    kind: 'vp08',
    width: 640,
    height: 480,
    bitDepth: 8,
    chromaSubsampling: 1,
    videoFullRangeFlag: true,
    colourPrimaries: 1,
    transferCharacteristics: 1,
    matrixCoefficients: 1,
};

/** テスト用の映像サンプルデータ */
const VIDEO_FRAME = new Uint8Array([0x00, 0x00, 0x00, 0x01, 0x09, 0xf0]);

/** テスト用の音声サンプルデータ */
const AUDIO_FRAME = new Uint8Array([0xf8, 0xff, 0xfe]);

test('version() が Cargo.toml のバージョンと一致する', () => {
    assert.equal(wasm.version(), readRootVersion());
});

test('1 トラックの MP4 を mux して demux できる', () => {
    const samples = [];
    for (let index = 0; index < 500; index += 1) {
        samples.push({
            metadata: {
                track_kind: 'audio',
                keyframe: true,
                timescale: 48000,
                duration: 960,
                composition_time_offset: null,
                sample_entry: index === 0 ? OPUS_ENTRY : undefined,
            },
            data: AUDIO_FRAME,
        });
    }

    const bytes = muxToBytes(samples);
    const { demuxer, tracks, samples: demuxed } = demuxFile(bytes);
    try {
        assert.equal(tracks.length, 1, 'トラック数が 1 であること');
        assert.equal(tracks[0].kind, 'audio');
        assert.equal(tracks[0].timescale, 48000);
        assert.equal(demuxed.length, 500, 'サンプル数が 500 であること');
        assert.equal(demuxed[0].data_size, AUDIO_FRAME.length);
        assert.equal(demuxed[0].composition_time_offset, null);
        assert.equal(demuxed[1].sample_entry, undefined, '2 件目以降は sample_entry が省略されること');
    } finally {
        demuxer.free();
    }
});

test('2 トラックの MP4 を mux して demux できる', () => {
    const samples = [];
    for (let index = 0; index < 10; index += 1) {
        samples.push({
            metadata: {
                track_kind: 'video',
                keyframe: index === 0,
                timescale: 90000,
                duration: 3000,
                composition_time_offset: null,
                sample_entry: index === 0 ? AVC1_ENTRY : undefined,
            },
            data: VIDEO_FRAME,
        });
        samples.push({
            metadata: {
                track_kind: 'audio',
                keyframe: true,
                timescale: 48000,
                duration: 960,
                composition_time_offset: null,
                sample_entry: index === 0 ? OPUS_ENTRY : undefined,
            },
            data: AUDIO_FRAME,
        });
    }

    const bytes = muxToBytes(samples);
    const { demuxer, tracks, samples: demuxed } = demuxFile(bytes);
    try {
        assert.equal(tracks.length, 2, 'トラック数が 2 であること');
        const kinds = tracks.map((track) => track.kind).sort();
        assert.deepEqual(kinds, ['audio', 'video']);
        assert.equal(demuxed.length, 20, 'サンプル数が 20 であること');
        const videoSamples = demuxed.filter((sample) => sample.track_id === tracks.find((t) => t.kind === 'video').track_id);
        const audioSamples = demuxed.filter((sample) => sample.track_id === tracks.find((t) => t.kind === 'audio').track_id);
        assert.equal(videoSamples.length, 10);
        assert.equal(audioSamples.length, 10);
        assert.equal(videoSamples[0].sample_entry.kind, 'avc1');
        assert.equal(audioSamples[0].sample_entry.kind, 'opus');
    } finally {
        demuxer.free();
    }
});

test('faststart 形式で mux できる', () => {
    const samples = [
        {
            metadata: {
                track_kind: 'audio',
                keyframe: true,
                timescale: 48000,
                duration: 960,
                composition_time_offset: null,
                sample_entry: OPUS_ENTRY,
            },
            data: AUDIO_FRAME,
        },
    ];

    const { chunks, faststartEnabled, moovBoxSize } = muxFile(samples, 4096);
    assert.ok(faststartEnabled, 'faststart が有効になること');
    assert.ok(moovBoxSize > 0, 'moov のサイズが 0 より大きいこと');

    const bytes = assembleChunks(chunks);
    const { demuxer, samples: demuxed } = demuxFile(bytes);
    try {
        assert.equal(demuxed.length, 1);
    } finally {
        demuxer.free();
    }
});

test('demux したサンプルエントリーをそのまま mux に戻せる', () => {
    // コーデックごとに往復を確認する。VP8 / VP9 の videoFullRangeFlag のように
    // 出力と入力の型が食い違うと、ここで失敗する
    const cases = [
        ['black-h264-video.mp4', 'avc1'],
        ['black-h265-video.mp4', 'hev1'],
        ['black-h265-hvc1-video.mp4', 'hvc1'],
        ['black-vp9-video.mp4', 'vp09'],
        ['black-av1-video.mp4', 'av01'],
        ['beep-aac-audio.mp4', 'mp4a'],
        ['beep-opus-audio.mp4', 'opus'],
        ['beep-flac-audio.mp4', 'flac'],
    ];

    for (const [fileName, expectedKind] of cases) {
        const { demuxer, samples } = demuxFile(readTestData(fileName));
        try {
            assert.equal(samples[0].sample_entry.kind, expectedKind, `${fileName} の種別`);

            const muxer = new wasm.Mp4FileMuxer(0);
            try {
                muxer.appendSample(
                    JSON.stringify({
                        track_kind: expectedKind === 'avc1' || expectedKind === 'hev1' || expectedKind === 'hvc1' || expectedKind === 'vp09' || expectedKind === 'av01'
                            ? 'video'
                            : 'audio',
                        keyframe: true,
                        timescale: 48000,
                        duration: 960,
                        composition_time_offset: null,
                        data_offset: muxer.initialBoxes().length,
                        data_size: samples[0].data_size,
                        sample_entry: samples[0].sample_entry,
                    }),
                );
                muxer.finalize();
            } finally {
                muxer.free();
            }
        } finally {
            demuxer.free();
        }
    }
});

test('VP8 の videoFullRangeFlag は boolean で出力し、0 / 1 も受理する', () => {
    const samples = [
        {
            metadata: {
                track_kind: 'video',
                keyframe: true,
                timescale: 90000,
                duration: 3000,
                composition_time_offset: null,
                sample_entry: VP08_ENTRY,
            },
            data: VIDEO_FRAME,
        },
    ];

    const { demuxer, samples: demuxed } = demuxFile(muxToBytes(samples));
    try {
        assert.equal(
            demuxed[0].sample_entry.videoFullRangeFlag,
            true,
            'boolean で出力されること',
        );

        // 旧実装が出力していた 0 / 1 も受理すること
        for (const [value, expected] of [[0, false], [1, true]]) {
            const muxer = new wasm.Mp4FileMuxer(0);
            try {
                muxer.appendSample(
                    JSON.stringify({
                        track_kind: 'video',
                        keyframe: true,
                        timescale: 90000,
                        duration: 3000,
                        composition_time_offset: null,
                        data_offset: muxer.initialBoxes().length,
                        data_size: VIDEO_FRAME.length,
                        sample_entry: { ...VP08_ENTRY, videoFullRangeFlag: value },
                    }),
                );
                assert.equal(
                    muxer.finalize(),
                    undefined,
                    `${value} を受理すること (${expected})`,
                );
            } finally {
                muxer.free();
            }
        }
    } finally {
        demuxer.free();
    }
});

test('fMP4 のメディアセグメントを mux して demux できる', () => {
    // 映像と音声を交互に並べ、muxer がトラック単位に並べ替えることを確認する
    const metadata = [
        {
            track_kind: 'video',
            timescale: 90000,
            duration: 3000,
            keyframe: true,
            composition_time_offset: null,
            data_size: VIDEO_FRAME.length,
            sample_entry: AVC1_ENTRY,
        },
        {
            track_kind: 'audio',
            timescale: 48000,
            duration: 960,
            keyframe: true,
            composition_time_offset: null,
            data_size: AUDIO_FRAME.length,
            sample_entry: OPUS_ENTRY,
        },
        {
            track_kind: 'video',
            timescale: 90000,
            duration: 3000,
            keyframe: false,
            composition_time_offset: null,
            data_size: VIDEO_FRAME.length,
        },
        {
            track_kind: 'audio',
            timescale: 48000,
            duration: 960,
            keyframe: true,
            composition_time_offset: null,
            data_size: AUDIO_FRAME.length,
        },
    ];
    const sampleData = new Uint8Array([
        ...VIDEO_FRAME,
        ...AUDIO_FRAME,
        ...VIDEO_FRAME,
        ...AUDIO_FRAME,
    ]);

    const muxer = new wasm.Fmp4SegmentMuxer(0);
    let segment;
    let initSegment;
    try {
        segment = muxer.createMediaSegment(JSON.stringify(metadata), sampleData);
        initSegment = muxer.initSegment();
    } finally {
        muxer.free();
    }

    const demuxer = new wasm.Fmp4SegmentDemuxer();
    try {
        demuxer.handleInitSegment(initSegment);
        const tracks = JSON.parse(demuxer.tracks());
        assert.equal(tracks.length, 2);

        const samples = JSON.parse(demuxer.handleMediaSegment(segment));
        assert.equal(samples.length, 4, 'サンプル数が 4 であること');

        // 並べ替え後も各サンプルのペイロードが元のデータと一致すること
        const videoSamples = samples.filter((sample) => sample.data_size === VIDEO_FRAME.length);
        const audioSamples = samples.filter((sample) => sample.data_size === AUDIO_FRAME.length);
        assert.equal(videoSamples.length, 2);
        assert.equal(audioSamples.length, 2);
        for (const sample of samples) {
            const payload = segment.subarray(
                sample.data_offset,
                sample.data_offset + sample.data_size,
            );
            assert.ok(
                payload.every((byte, index) => byte === (sample.data_size === VIDEO_FRAME.length ? VIDEO_FRAME[index] : AUDIO_FRAME[index])),
                'ペイロードが壊れていないこと',
            );
        }
    } finally {
        demuxer.free();
    }
});

test('sidx 付きのメディアセグメントを生成できる', () => {
    const metadata = [
        {
            track_kind: 'video',
            timescale: 90000,
            duration: 3000,
            keyframe: true,
            composition_time_offset: null,
            data_size: VIDEO_FRAME.length,
            sample_entry: AVC1_ENTRY,
        },
    ];
    const muxer = new wasm.Fmp4SegmentMuxer(0);
    try {
        const segment = muxer.createMediaSegmentWithSidx(JSON.stringify(metadata), VIDEO_FRAME);
        assert.ok(segment.length > VIDEO_FRAME.length, 'sidx が付与されること');
    } finally {
        muxer.free();
    }
});

test('ファイル種別を判定できる', () => {
    for (const [fileName, expected] of [
        ['black-h264-video.mp4', 'mp4'],
        ['black-h264-fmp4.mp4', 'fragmented-mp4'],
    ]) {
        const bytes = readTestData(fileName);
        const detector = new wasm.Mp4FileKindDetector();
        try {
            feedAllInput(detector, bytes);
            assert.equal(detector.fileKind(), expected, `${fileName} の判定`);
        } finally {
            detector.free();
        }
    }
});

test('seek は 1 秒未満の時刻も切り捨てない', () => {
    const bytes = readTestData('beep-opus-audio.mp4');

    // 0.5 秒を指す 2 通りの呼び方で同じサンプルに到達すること
    const timestamps = [];
    for (const [timestamp, timescale] of [[0.5, 1], [24000, 48000]]) {
        const demuxer = demuxFile(bytes).demuxer;
        try {
            demuxer.seek(timestamp, timescale);
            timestamps.push(JSON.parse(demuxer.nextSample()).timestamp);
        } finally {
            demuxer.free();
        }
    }
    assert.equal(timestamps[0], timestamps[1], '同じサンプルに到達すること');
    assert.ok(timestamps[0] > 0, '先頭より後ろにシークすること');
});

test('不正な数値引数は例外になる', () => {
    assert.throws(() => new wasm.Mp4FileMuxer(-1), /reservedMoovBoxSize/);
    assert.throws(() => new wasm.Mp4FileMuxer(Number.NaN), /reservedMoovBoxSize/);
    assert.throws(() => new wasm.Fmp4SegmentMuxer(-1), /creationTimestampSecs/);

    const muxer = new wasm.Mp4FileMuxer(0);
    try {
        assert.throws(() => muxer.advancePosition(-1), /size/);
        assert.throws(() => muxer.advancePosition(Number.NaN), /size/);
        assert.throws(() => muxer.appendSample('{}'), /failed to parse sample JSON/);
    } finally {
        muxer.free();
    }

    const demuxer = new wasm.Mp4FileDemuxer();
    try {
        assert.throws(() => demuxer.handleInput(-1, new Uint8Array(0)), /position/);
        assert.throws(() => demuxer.seek(-1, 48000), /timestamp/);
        assert.throws(() => demuxer.seek(1, 0.5), /timescale/);
    } finally {
        demuxer.free();
    }
});
