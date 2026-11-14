// web ターゲットの生成物を Node.js 上で初期化し、ブラウザ向けの経路を確認する
//
// CI では `wasm-bindgen --target web` で生成した `crates/wasm/pkg/web/` を使う。
// ブラウザと同じく `initSync()` に WebAssembly.Module を渡して初期化する。
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { Mp4FileDemuxer, initSync, version } from '../pkg/web/mp4_wasm.js';

const wasmBytes = readFileSync(new URL('../pkg/web/mp4_wasm_bg.wasm', import.meta.url));
initSync({ module: new WebAssembly.Module(wasmBytes) });

test('web ターゲットの version() が動く', () => {
    assert.match(version(), /^\d+\.\d+\.\d+/);
});

test('web ターゲットで MP4 を demux できる', () => {
    const bytes = new Uint8Array(
        readFileSync(new URL('../../../tests/testdata/black-h264-video.mp4', import.meta.url)),
    );

    const demuxer = new Mp4FileDemuxer();
    try {
        for (;;) {
            const requiredJson = demuxer.requiredInput();
            if (requiredJson === undefined) {
                break;
            }
            const required = JSON.parse(requiredJson);
            demuxer.handleInput(
                required.position,
                bytes.subarray(required.position, required.position + required.size),
            );
        }

        const tracks = JSON.parse(demuxer.tracks());
        assert.equal(tracks.length, 1);

        const sample = JSON.parse(demuxer.nextSample());
        assert.equal(sample.track_id, tracks[0].track_id);
    } finally {
        demuxer.free();
    }
});
