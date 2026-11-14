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
 * node --test crates/wasm/tests/wasm_api.test.mjs
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
function demuxFmp4File(bytes) {
    const demuxer = new wasm.Fmp4FileDemuxer();
    let tracks;
    const samples = [];
    try {
        for (;;) {
            const requiredJson = demuxer.requiredInput();
            if (requiredJson !== undefined) {
                const required = JSON.parse(requiredJson);
                const size = required.size ?? bytes.length - required.position;
                demuxer.handleInput(
                    required.position,
                    bytes.subarray(required.position, required.position + size),
                );
                continue;
            }
            if (tracks === undefined) {
                tracks = JSON.parse(demuxer.tracks());
            }
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

test('demux したサンプルエントリーを mux に戻すと内容が保たれる', () => {
    // コーデックごとに、mux したファイルを読み戻して sample_entry を比較する。
    // 出力と入力の型が食い違うと、ここで失敗する
    const videoKinds = new Set(['avc1', 'hev1', 'hvc1', 'vp08', 'vp09', 'av01']);
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
            const entry = samples[0].sample_entry;
            assert.equal(entry.kind, expectedKind, `${fileName} の種別`);

            // 型付き実装を持たない子ボックスと、映像共通フィールドが
            // JSON に含まれていること（再 mux で失われないための前提）
            if (fileName === 'black-h264-video.mp4') {
                const types = entry.unknownBoxes.map((box) => box.type);
                assert.ok(types.includes('pasp'), `pasp が含まれること: ${types}`);
                assert.ok(types.includes('btrt'), `btrt が含まれること: ${types}`);
                assert.equal(typeof entry.compressorname, 'string');
                assert.ok(entry.compressorname.length > 0, 'compressorname が入っていること');
            }

            const muxed = muxToBytes([
                {
                    metadata: {
                        track_kind: videoKinds.has(expectedKind) ? 'video' : 'audio',
                        keyframe: true,
                        timescale: 48000,
                        duration: 960,
                        composition_time_offset: null,
                        sample_entry: entry,
                    },
                    data: new Uint8Array(samples[0].data_size),
                },
            ]);

            const roundtrip = demuxFile(muxed);
            try {
                assert.deepEqual(
                    roundtrip.samples[0].sample_entry,
                    entry,
                    `${fileName} の sample_entry が往復すること`,
                );
            } finally {
                roundtrip.demuxer.free();
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

test('深すぎる JSON は trap せず例外になり、インスタンスは使えるままである', () => {
    const deep = '['.repeat(200) + ']'.repeat(200);
    const json = `{"track_kind":"audio","keyframe":true,"timescale":48000,"duration":960,"x":${deep}}`;
    const muxer = new wasm.Mp4FileMuxer(0);
    try {
        assert.throws(() => muxer.appendSample(json), /nesting depth/);
    } finally {
        muxer.free();
    }
    // trap していれば version() が失敗する
    assert.equal(wasm.version(), readRootVersion());
});

test('ビット幅を超えるサンプルエントリーは例外になる', () => {
    const muxer = new wasm.Mp4FileMuxer(0);
    try {
        assert.throws(
            () =>
                muxer.appendSample(
                    JSON.stringify({
                        track_kind: 'video',
                        keyframe: true,
                        timescale: 48000,
                        duration: 960,
                        composition_time_offset: null,
                        data_offset: muxer.initialBoxes().length,
                        data_size: 1,
                        sample_entry: { ...AVC1_ENTRY, lengthSizeMinusOne: 4 },
                    }),
                ),
            /lengthSizeMinusOne must be less than/,
        );
    } finally {
        muxer.free();
    }
});

test('seek は 44100 のサンプル境界でも 1 サンプル前にずれない', () => {
    const bytes = readTestData('beep-aac-audio.mp4');
    const { demuxer } = demuxFile(bytes);
    try {
        // 1024 / 2048 / 3072 は AAC のサンプル境界
        for (const target of [1024, 2048, 3072]) {
            demuxer.seek(target, 44100);
            assert.equal(
                JSON.parse(demuxer.nextSample()).timestamp,
                target,
                `seek(${target}, 44100)`,
            );
        }
    } finally {
        demuxer.free();
    }
});

test('hvc1 の arrayCompleteness は 1 のまま往復する', () => {
    const { demuxer, samples } = demuxFile(readTestData('black-h265-hvc1-video.mp4'));
    try {
        const entry = samples[0].sample_entry;
        const parameterSets = entry.naluArrays.filter((array) =>
            [32, 33, 34].includes(array.naluType),
        );
        assert.ok(parameterSets.length > 0, 'パラメータセットの配列があること');
        for (const array of parameterSets) {
            assert.equal(array.arrayCompleteness, 1, `naluType ${array.naluType}`);
        }

        const muxed = muxToBytes([
            {
                metadata: {
                    track_kind: 'video',
                    keyframe: true,
                    timescale: 48000,
                    duration: 960,
                    composition_time_offset: null,
                    sample_entry: entry,
                },
                data: new Uint8Array(samples[0].data_size),
            },
        ]);
        const roundtrip = demuxFile(muxed);
        try {
            assert.deepEqual(roundtrip.samples[0].sample_entry, entry);
        } finally {
            roundtrip.demuxer.free();
        }
    } finally {
        demuxer.free();
    }
});

test('種別判定器は不正な入力でその場で例外を投げる', () => {
    const bytes = readTestData('black-h264-video.mp4');
    const detector = new wasm.Mp4FileKindDetector();
    try {
        assert.throws(
            () => detector.handleInput(0, bytes.subarray(0, 4)),
            /failed to handle input/,
        );
    } finally {
        detector.free();
    }
});

test('finalize まで待たずに拒否すべき入力は appendSample で例外になる', () => {
    const cases = [
        // 高プロファイルの avc1 で拡張フィールドを省略
        {
            track_kind: 'video',
            sample_entry: {
                kind: 'avc1',
                width: 640,
                height: 480,
                avcProfileIndication: 100,
                profileCompatibility: 0,
                avcLevelIndication: 40,
                lengthSizeMinusOne: 3,
                sps: [[1]],
                pps: [[2]],
            },
        },
        // ctts で表現できないコンポジション時間オフセット
        {
            track_kind: 'video',
            composition_time_offset: -2147483649,
            sample_entry: AVC1_ENTRY,
        },
        // hvcC の NALU 長は 16 ビット
        {
            track_kind: 'video',
            sample_entry: {
                kind: 'hev1',
                width: 640,
                height: 480,
                generalProfileSpace: 0,
                generalTierFlag: 0,
                generalProfileIdc: 2,
                generalProfileCompatibilityFlags: 1610612736,
                generalConstraintIndicatorFlags: 193514046488576,
                generalLevelIdc: 120,
                chromaFormatIdc: 1,
                bitDepthLumaMinus8: 0,
                bitDepthChromaMinus8: 0,
                minSpatialSegmentationIdc: 0,
                parallelismType: 0,
                avgFrameRate: 0,
                constantFrameRate: 0,
                numTemporalLayers: 1,
                temporalIdNested: 0,
                lengthSizeMinusOne: 3,
                naluArrays: [
                    { naluType: 32, arrayCompleteness: 0, units: [new Array(65536).fill(0)] },
                ],
            },
        },
        // SPS の個数上限は 31
        {
            track_kind: 'video',
            sample_entry: {
                ...AVC1_ENTRY,
                sps: Array.from({ length: 32 }, () => [1]),
            },
        },
        // NALU 配列の個数上限は 255
        {
            track_kind: 'video',
            sample_entry: {
                kind: 'hev1',
                width: 640,
                height: 480,
                generalProfileSpace: 0,
                generalTierFlag: 0,
                generalProfileIdc: 2,
                generalProfileCompatibilityFlags: 1610612736,
                generalConstraintIndicatorFlags: 193514046488576,
                generalLevelIdc: 120,
                chromaFormatIdc: 1,
                bitDepthLumaMinus8: 0,
                bitDepthChromaMinus8: 0,
                minSpatialSegmentationIdc: 0,
                parallelismType: 0,
                avgFrameRate: 0,
                constantFrameRate: 0,
                numTemporalLayers: 1,
                temporalIdNested: 0,
                lengthSizeMinusOne: 3,
                naluArrays: Array.from({ length: 256 }, () => ({
                    naluType: 32,
                    arrayCompleteness: 0,
                    units: [[0]],
                })),
            },
        },
    ];

    for (const entry of cases) {
        const muxer = new wasm.Mp4FileMuxer(0);
        try {
            assert.throws(
                () =>
                    muxer.appendSample(
                        JSON.stringify({
                            keyframe: true,
                            timescale: 48000,
                            duration: 960,
                            composition_time_offset: null,
                            data_offset: muxer.initialBoxes().length,
                            data_size: 1,
                            ...entry,
                        }),
                    ),
                /failed to parse sample JSON/,
            );
        } finally {
            muxer.free();
        }
    }
});

test('sidx の時間があふれる場合は内部状態を進めない', () => {
    const muxer = new wasm.Fmp4SegmentMuxer(0);
    try {
        const overflow = [
            {
                track_kind: 'video',
                timescale: 90000,
                duration: 0xffffffff,
                keyframe: true,
                composition_time_offset: null,
                data_size: VIDEO_FRAME.length,
                sample_entry: AVC1_ENTRY,
            },
            {
                track_kind: 'video',
                timescale: 90000,
                duration: 0xffffffff,
                keyframe: true,
                composition_time_offset: null,
                data_size: VIDEO_FRAME.length,
            },
        ];
        assert.throws(
            () =>
                muxer.createMediaSegmentWithSidx(
                    JSON.stringify(overflow),
                    new Uint8Array(VIDEO_FRAME.length * 2),
                ),
            /subsegment_duration/,
        );

        // 失敗したセグメントの時刻が進んでいないこと
        const good = [
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
        const segment = muxer.createMediaSegment(JSON.stringify(good), VIDEO_FRAME);
        const init = muxer.initSegment();

        const demuxer = new wasm.Fmp4SegmentDemuxer();
        try {
            demuxer.handleInitSegment(init);
            const samples = JSON.parse(demuxer.handleMediaSegment(segment));
            assert.equal(samples[0].timestamp, 0);
        } finally {
            demuxer.free();
        }
    } finally {
        muxer.free();
    }
});

const CTTS_BASE = (muxer) => ({
    track_kind: 'video',
    timescale: 90000,
    duration: 3000,
    keyframe: true,
    data_offset: muxer.initialBoxes().length,
    data_size: 0,
    sample_entry: AVC1_ENTRY,
});

test('ctts の u32 値は finalize まで成功する', () => {
    const muxer = new wasm.Mp4FileMuxer(0);
    try {
        muxer.appendSample(
            JSON.stringify({ ...CTTS_BASE(muxer), composition_time_offset: 3000000000 }),
        );
        muxer.finalize();
    } finally {
        muxer.free();
    }
});

test('ctts の負値と i32 を超える値の混在は例外になる', () => {
    const muxer = new wasm.Mp4FileMuxer(0);
    try {
        muxer.appendSample(
            JSON.stringify({ ...CTTS_BASE(muxer), composition_time_offset: 3000000000 }),
        );
        // 負値を混ぜると ctts version 1 になり、u32 の値を表現できない
        assert.throws(
            () =>
                muxer.appendSample(
                    JSON.stringify({ ...CTTS_BASE(muxer), composition_time_offset: -1 }),
                ),
            /cannot mix/,
        );
    } finally {
        muxer.free();
    }
});

test('appendSample が失敗した場合は ctts の状態を汚さない', () => {
    const muxer = new wasm.Mp4FileMuxer(0);
    try {
        const base = {
            track_kind: 'video',
            timescale: 90000,
            duration: 3000,
            keyframe: true,
            data_offset: muxer.initialBoxes().length,
            data_size: 0,
        };
        // sample_entry を欠落させてコアの append を失敗させる
        assert.throws(
            () => muxer.appendSample(JSON.stringify({ ...base, composition_time_offset: 3000000000 })),
            /failed to append sample/,
        );
        // 失敗したサンプルは取り込まれていないため、負値はそのまま受理される
        muxer.appendSample(
            JSON.stringify({ ...base, composition_time_offset: -1, sample_entry: AVC1_ENTRY }),
        );
    } finally {
        muxer.free();
    }
});

test('不正な入力は handleInput で例外になる', () => {
    const bytes = readTestData('black-h264-video.mp4');
    const demuxer = new wasm.Mp4FileDemuxer();
    try {
        // 必要な位置と違うデータを渡す
        assert.throws(
            () => demuxer.handleInput(100, bytes.subarray(100, 140)),
            /failed to handle input/,
        );
    } finally {
        demuxer.free();
    }
});

test('メモリを枯渇させる引数は例外になる', () => {
    // faststart の予約サイズは 512 MiB まで
    assert.throws(
        () => new wasm.Mp4FileMuxer(513 * 1024 * 1024),
        /reservedMoovBoxSize must be at most 512 MiB/,
    );
    // 作成時刻は u64 に収まる範囲まで
    assert.throws(
        () => new wasm.Fmp4SegmentMuxer(2 ** 64 - 4096),
        /creationTimestampSecs is too large/,
    );
});

test('サンプルを追加せずに finalize すると例外になる', () => {
    const muxer = new wasm.Mp4FileMuxer(0);
    try {
        assert.throws(() => muxer.finalize(), /no samples have been appended/);
    } finally {
        muxer.free();
    }
});

test('fMP4 ファイルを Fmp4FileDemuxer で読める', () => {
    const { demuxer, tracks, samples } = demuxFmp4File(readTestData('black-h264-fmp4.mp4'));
    try {
        assert.equal(tracks.length, 1);
        assert.equal(samples.length, 1);
        assert.equal(samples[0].timestamp, 0);
        assert.equal(samples[0].track_id, tracks[0].track_id);
    } finally {
        demuxer.free();
    }
});

test('生成した fMP4 ファイルを Fmp4FileDemuxer で読める', () => {
    const muxer = new wasm.Fmp4SegmentMuxer(0);
    let initSegment;
    let first;
    let second;
    try {
        const metadata = (duration) => ({
            track_kind: 'video',
            timescale: 90000,
            duration,
            keyframe: true,
            composition_time_offset: null,
            data_size: VIDEO_FRAME.length,
            sample_entry: AVC1_ENTRY,
        });
        first = muxer.createMediaSegment(JSON.stringify([metadata(3000)]), VIDEO_FRAME);
        second = muxer.createMediaSegment(JSON.stringify([metadata(3000)]), VIDEO_FRAME);
        initSegment = muxer.initSegment();
    } finally {
        muxer.free();
    }

    const file = new Uint8Array([...initSegment, ...first, ...second]);
    const { demuxer, tracks, samples } = demuxFmp4File(file);
    try {
        assert.equal(tracks.length, 1);
        assert.equal(samples.length, 2);
        assert.deepEqual(
            samples.map((sample) => sample.timestamp),
            [0, 3000],
        );
    } finally {
        demuxer.free();
    }
});
