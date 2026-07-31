// メモリ内バッファを使用した MP4 のマルチプレックス/デマルチプレックステスト
//
// 以下の処理を実行する:
// 1. ダミーサンプルを使用してメモリ内バッファに MP4 ファイルを構築（マルチプレックス）
// 2. 構築した MP4 ファイルをデマルチプレックスしてサンプルを抽出
// 3. 元のサンプルとデマルチプレックスされたサンプルが一致することを確認

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "mp4.h"

#define MAX_BUFFER_SIZE (1024 * 1024)  // 1 MB
#define NUM_VIDEO_SAMPLES 5
#define VIDEO_WIDTH 1920
#define VIDEO_HEIGHT 1080
#define SAMPLE_DURATION_MICROS 33333  // ~30 fps
#define TIMESCALE 1000000  // マイクロ秒

// テスト用のダミーサンプルデータ構造体
typedef struct {
    uint32_t track_kind;
    uint64_t timestamp;
    uint32_t duration;
    uint32_t data_size;
    uint8_t data[4096];
} TestSample;

// メモリバッファを使用した MP4 ファイル構築テスト
int main(void) {
    printf("Starting mux/demux test with in-memory buffer\n");

    // メモリバッファを確保
    uint8_t *buffer = (uint8_t *)malloc(MAX_BUFFER_SIZE);
    if (buffer == NULL) {
        fprintf(stderr, "Failed to allocate buffer\n");
        return 1;
    }
    memset(buffer, 0, MAX_BUFFER_SIZE);
    uint32_t buffer_used = 0;

    // テスト用サンプルを準備
    TestSample original_samples[NUM_VIDEO_SAMPLES];
    for (int i = 0; i < NUM_VIDEO_SAMPLES; i++) {
        original_samples[i].track_kind = MP4_TRACK_KIND_VIDEO;
        original_samples[i].timestamp = i * SAMPLE_DURATION_MICROS;
        original_samples[i].duration = SAMPLE_DURATION_MICROS;
        original_samples[i].data_size = 1024;
        // ダミーデータを生成（サンプルごとに異なるパターン）
        for (int j = 0; j < (int)original_samples[i].data_size; j++) {
            original_samples[i].data[j] = (i * 17 + j) & 0xFF;
        }
    }

    // ===== マルチプレックス処理 =====
    printf("\n=== Muxing Phase ===\n");

    Mp4FileMuxer *muxer = mp4_file_muxer_new();
    if (muxer == NULL) {
        fprintf(stderr, "Failed to create muxer\n");
        free(buffer);
        return 1;
    }

    // faststart用に moov ボックスサイズを予約
    uint32_t sample_counts[] = {NUM_VIDEO_SAMPLES};
    uint32_t estimated_moov_size = mp4_estimate_maximum_moov_box_size(sample_counts, 1);
    mp4_file_muxer_set_reserved_moov_box_size(muxer, estimated_moov_size);

    // マルチプレックス処理を初期化
    Mp4Error ret = mp4_file_muxer_initialize(muxer);
    if (ret != MP4_ERROR_OK) {
        fprintf(stderr, "Failed to initialize muxer: %s\n", mp4_file_muxer_get_last_error(muxer));
        mp4_file_muxer_free(muxer);
        free(buffer);
        return 1;
    }
    printf("Muxer initialized\n");

    // 初期出力データをバッファに書き込む
    uint64_t output_offset;
    uint32_t output_size;
    const uint8_t *output_data;
    while (mp4_file_muxer_next_output(muxer, &output_offset, &output_size, &output_data) == MP4_ERROR_OK) {
        if (output_size == 0) break;
        if (output_offset + output_size > MAX_BUFFER_SIZE) {
            fprintf(stderr, "Buffer overflow: required %lu bytes, but buffer size is %d\n",
                    output_offset + output_size, MAX_BUFFER_SIZE);
            mp4_file_muxer_free(muxer);
            free(buffer);
            return 1;
        }
        memcpy(buffer + output_offset, output_data, output_size);
        if (output_offset + output_size > buffer_used) {
            buffer_used = output_offset + output_size;
        }
    }
    printf("Initial output written: %u bytes\n", buffer_used);

    // サンプルをマルチプレックスに追加
    for (int i = 0; i < NUM_VIDEO_SAMPLES; i++) {
        TestSample *sample = &original_samples[i];

        // サンプルデータをバッファに追記
        if (buffer_used + sample->data_size > MAX_BUFFER_SIZE) {
            fprintf(stderr, "Buffer overflow when writing sample %d\n", i);
            mp4_file_muxer_free(muxer);
            free(buffer);
            return 1;
        }
        memcpy(buffer + buffer_used, sample->data, sample->data_size);
        uint64_t sample_offset = buffer_used;
        buffer_used += sample->data_size;

        // VP08 サンプルエントリーを作成
        Mp4SampleEntryVp08 vp08_data = {
            .width = VIDEO_WIDTH,
            .height = VIDEO_HEIGHT,
            .bit_depth = 8,
            .chroma_subsampling = 1,
            .video_full_range_flag = false,
            .colour_primaries = 1,
            .transfer_characteristics = 1,
            .matrix_coefficients = 1,
        };

        Mp4SampleEntryData entry_data;
        entry_data.vp08 = vp08_data;

        Mp4SampleEntry sample_entry = {
            .kind = MP4_SAMPLE_ENTRY_KIND_VP08,
            .data = entry_data,
        };

        // マルチプレックスにサンプル情報を追加
        Mp4MuxSample mux_sample = {
            .track_kind = MP4_TRACK_KIND_VIDEO,
            .sample_entry = (i == 0) ? &sample_entry : NULL,  // 最初のサンプルのみ sample_entry を指定
            .keyframe = true,
            .timescale = TIMESCALE,
            .duration = sample->duration,
            .data_offset = sample_offset,
            .data_size = sample->data_size,
        };

        ret = mp4_file_muxer_append_sample(muxer, &mux_sample);
        if (ret != MP4_ERROR_OK) {
            fprintf(stderr, "Failed to append sample %d: %s\n", i, mp4_file_muxer_get_last_error(muxer));
            mp4_file_muxer_free(muxer);
            free(buffer);
            return 1;
        }
        printf("Sample %d appended (offset: %lu, size: %u)\n", i, sample_offset, sample->data_size);
    }

    // マルチプレックス処理を完了
    ret = mp4_file_muxer_finalize(muxer);
    if (ret != MP4_ERROR_OK) {
        fprintf(stderr, "Failed to finalize muxer: %s\n", mp4_file_muxer_get_last_error(muxer));
        mp4_file_muxer_free(muxer);
        free(buffer);
        return 1;
    }
    printf("Muxer finalized\n");

    // ファイナライズ後の出力データをバッファに書き込む
    while (mp4_file_muxer_next_output(muxer, &output_offset, &output_size, &output_data) == MP4_ERROR_OK) {
        if (output_size == 0) break;
        if (output_offset + output_size > MAX_BUFFER_SIZE) {
            fprintf(stderr, "Buffer overflow: required %lu bytes, but buffer size is %d\n",
                    output_offset + output_size, MAX_BUFFER_SIZE);
            mp4_file_muxer_free(muxer);
            free(buffer);
            return 1;
        }
        memcpy(buffer + output_offset, output_data, output_size);
        if (output_offset + output_size > buffer_used) {
            buffer_used = output_offset + output_size;
        }
    }
    printf("Finalized output written: total %u bytes\n", buffer_used);

    mp4_file_muxer_free(muxer);

    // ===== デマルチプレックス処理 =====
    printf("\n=== Demuxing Phase ===\n");

    Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
    if (demuxer == NULL) {
        fprintf(stderr, "Failed to create demuxer\n");
        free(buffer);
        return 1;
    }

    // バッファからデータを供給して初期化
    uint64_t required_pos = 0;
    int32_t required_size = 0;
    while (mp4_file_demuxer_get_required_input(demuxer, &required_pos, &required_size) == MP4_ERROR_OK) {
        if (required_size == 0) break;

        // 必要なサイズを計算（ファイル末尾を超えない範囲）
        uint32_t bytes_to_read = required_size;
        if (required_size == -1) {
            bytes_to_read = buffer_used - required_pos;
        }

        if (required_pos + bytes_to_read > buffer_used) {
            fprintf(stderr, "Insufficient data in buffer: required position %lu + size %u, but buffer has %u bytes\n",
                    required_pos, bytes_to_read, buffer_used);
            mp4_file_demuxer_free(demuxer);
            free(buffer);
            return 1;
        }

        ret = mp4_file_demuxer_handle_input(demuxer, required_pos, buffer + required_pos, bytes_to_read);
        if (ret != MP4_ERROR_OK) {
            fprintf(stderr, "Failed to handle input: %s\n", mp4_file_demuxer_get_last_error(demuxer));
            mp4_file_demuxer_free(demuxer);
            free(buffer);
            return 1;
        }
        printf("Input data supplied: position %lu, size %u\n", required_pos, bytes_to_read);
    }

    // トラック情報を取得
    const Mp4DemuxTrackInfo *tracks;
    uint32_t track_count;
    ret = mp4_file_demuxer_get_tracks(demuxer, &tracks, &track_count);
    if (ret != MP4_ERROR_OK) {
        fprintf(stderr, "Failed to get tracks: %s\n", mp4_file_demuxer_get_last_error(demuxer));
        mp4_file_demuxer_free(demuxer);
        free(buffer);
        return 1;
    }
    printf("Found %u tracks\n", track_count);

    for (uint32_t i = 0; i < track_count; i++) {
        printf("  Track %u: ID=%u, Kind=%d, Duration=%lu, Timescale=%u\n",
               i, tracks[i].track_id, tracks[i].kind, tracks[i].duration, tracks[i].timescale);
    }

    // ===== サンプル比較 =====
    printf("\n=== Sample Comparison ===\n");

    int demuxed_sample_count = 0;
    int all_match = 1;
    Mp4DemuxSample demux_sample;

    while (mp4_file_demuxer_next_sample(demuxer, &demux_sample) == MP4_ERROR_OK) {
        if (demuxed_sample_count >= NUM_VIDEO_SAMPLES) {
            fprintf(stderr, "Too many samples demuxed\n");
            all_match = 0;
            break;
        }

        TestSample *original = &original_samples[demuxed_sample_count];

        // メタデータの比較
        printf("Sample %d:\n", demuxed_sample_count);
        printf("  Original: timestamp=%lu, duration=%u, data_size=%u\n",
               original->timestamp, original->duration, original->data_size);
        printf("  Demuxed:  timestamp=%lu, duration=%u, data_size=%zu\n",
               demux_sample.timestamp, demux_sample.duration, demux_sample.data_size);

        // タイムスタンプの確認（タイムスケール単位での比較）
        if (demux_sample.timestamp != original->timestamp) {
            fprintf(stderr, "  ERROR: timestamp mismatch\n");
            all_match = 0;
        }

        // 尺の確認
        if (demux_sample.duration != original->duration) {
            fprintf(stderr, "  ERROR: duration mismatch\n");
            all_match = 0;
        }

        // サイズの確認
        if (demux_sample.data_size != original->data_size) {
            fprintf(stderr, "  ERROR: data_size mismatch\n");
            all_match = 0;
        }

        // サンプルデータの確認
        if (demux_sample.data_offset + demux_sample.data_size > buffer_used) {
            fprintf(stderr, "  ERROR: invalid data offset/size\n");
            all_match = 0;
        } else {
            if (memcmp(buffer + demux_sample.data_offset, original->data, original->data_size) != 0) {
                fprintf(stderr, "  ERROR: sample data mismatch\n");
                all_match = 0;
            } else {
                printf("  OK: sample data matches\n");
            }
        }

        demuxed_sample_count++;
    }

    if (demuxed_sample_count != NUM_VIDEO_SAMPLES) {
        fprintf(stderr, "ERROR: expected %d samples, but got %d\n", NUM_VIDEO_SAMPLES, demuxed_sample_count);
        all_match = 0;
    }

    mp4_file_demuxer_free(demuxer);
    free(buffer);

    // 結果を出力
    printf("\n=== Test Result ===\n");
    if (all_match && demuxed_sample_count == NUM_VIDEO_SAMPLES) {
        printf("SUCCESS: All samples matched\n");
        return 0;
    } else {
        printf("FAILURE: Sample comparison failed\n");
        return 1;
    }
}
