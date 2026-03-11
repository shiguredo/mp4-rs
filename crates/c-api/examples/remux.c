// MP4 ファイルをリマルチプレックスするサンプルプログラム
//
// 入力 MP4 ファイルを読み込んでデマルチプレックスし、
// すべてのサンプルを新しい MP4 ファイルに書き直すプログラムである
//
// # ビルド
//
// ```bash
// # mp4-rs のプロジェクトルートで libmp4.a をビルド
// cargo build
//
// # remux.c をコンパイル
// cc -o target/debug/remux \
//    -I crates/c-api/include/ \
//    crates/c-api/examples/remux.c \
//    target/debug/libmp4.a
// ```
//
// # 実行方法
// ```bash
// ./target/debug/remux /path/to/input.mp4 /path/to/output.mp4
// ```
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "mp4.h"

#define BUFFER_SIZE (1024 * 1024)  // 1MB のバッファサイズ

// サンプルエントリー種別を文字列に変換
const char *get_sample_entry_kind_name(enum Mp4SampleEntryKind kind) {
    switch (kind) {
        case MP4_SAMPLE_ENTRY_KIND_AVC1:
            return "AVC1 (H.264)";
        case MP4_SAMPLE_ENTRY_KIND_HEV1:
            return "HEV1 (H.265/HEVC)";
        case MP4_SAMPLE_ENTRY_KIND_HVC1:
            return "HVC1 (H.265/HEVC)";
        case MP4_SAMPLE_ENTRY_KIND_VP08:
            return "VP08 (VP8)";
        case MP4_SAMPLE_ENTRY_KIND_VP09:
            return "VP09 (VP9)";
        case MP4_SAMPLE_ENTRY_KIND_AV01:
            return "AV01 (AV1)";
        case MP4_SAMPLE_ENTRY_KIND_OPUS:
            return "Opus";
        case MP4_SAMPLE_ENTRY_KIND_MP4A:
            return "MP4A (AAC)";
        default:
            return "Unknown";
    }
}

int main(int argc, char *argv[]) {
    if (argc < 3) {
        fprintf(stderr, "Usage: %s <input_mp4> <output_mp4>\n", argv[0]);
        return 1;
    }

    const char *input_filepath = argv[1];
    const char *output_filepath = argv[2];

    // ==================== デマルチプレックサーのセットアップ ====================
    FILE *input_file = fopen(input_filepath, "rb");
    if (!input_file) {
        fprintf(stderr, "Error: Could not open input file '%s'\n", input_filepath);
        return 1;
    }

    // デマルチプレックサーを作成
    struct Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
    if (!demuxer) {
        fprintf(stderr, "Error: Could not create demuxer\n");
        fclose(input_file);
        return 1;
    }

    // 読み込み用バッファを割り当て
    uint8_t *read_buffer = (uint8_t *)malloc(BUFFER_SIZE);
    if (!read_buffer) {
        fprintf(stderr, "Error: Could not allocate read buffer\n");
        mp4_file_demuxer_free(demuxer);
        fclose(input_file);
        return 1;
    }

    // ==================== マルチプレックサーのセットアップ ====================
    FILE *output_file = fopen(output_filepath, "wb");
    if (!output_file) {
        fprintf(stderr, "Error: Could not open output file '%s'\n", output_filepath);
        free(read_buffer);
        mp4_file_demuxer_free(demuxer);
        fclose(input_file);
        return 1;
    }

    // マルチプレックサーを作成
    struct Mp4FileMuxer *muxer = mp4_file_muxer_new();
    if (!muxer) {
        fprintf(stderr, "Error: Could not create muxer\n");
        fclose(output_file);
        free(read_buffer);
        mp4_file_demuxer_free(demuxer);
        fclose(input_file);
        return 1;
    }

    // マルチプレックサーを初期化
    enum Mp4Error err = mp4_file_muxer_initialize(muxer);
    if (err != MP4_ERROR_OK) {
        fprintf(stderr, "Error: Failed to initialize muxer: %d\n", err);
        const char *error_msg = mp4_file_muxer_get_last_error(muxer);
        if (error_msg) {
            fprintf(stderr, "  %s\n", error_msg);
        }
        mp4_file_muxer_free(muxer);
        fclose(output_file);
        free(read_buffer);
        mp4_file_demuxer_free(demuxer);
        fclose(input_file);
        return 1;
    }

    // マルチプレックサーの初期出力データを書き込む
    uint64_t output_offset;
    uint32_t output_size;
    const uint8_t *output_data;
    uint64_t current_output_data_offset = 0;

    printf("Writing initial muxer boxes...\n");
    while (mp4_file_muxer_next_output(muxer, &output_offset, &output_size, &output_data) ==
           MP4_ERROR_OK) {
        if (output_size == 0) break;

        if (fseek(output_file, output_offset, SEEK_SET) != 0) {
            fprintf(stderr, "Error: Failed to seek in output file\n");
            goto cleanup;
        }

        if (fwrite(output_data, 1, output_size, output_file) != output_size) {
            fprintf(stderr, "Error: Failed to write to output file\n");
            goto cleanup;
        }

        printf("  Wrote %u bytes at offset %lu\n", output_size, output_offset);

        // 次のサンプルデータの開始位置を追跡
        current_output_data_offset = output_offset + output_size;
    }

    printf("Sample data will start at offset: %lu\n\n", current_output_data_offset);

    // ==================== 入力ファイルのデマルチプレックス ====================
    printf("Demuxing input file...\n");

    // デマルチプレックサーが初期化完了するまで、必要なデータを読み込んで入力する
    while (true) {
        uint64_t required_position;
        int32_t required_size;

        err = mp4_file_demuxer_get_required_input(demuxer, &required_position, &required_size);
        if (err != MP4_ERROR_OK) {
            fprintf(stderr, "Error: Failed to get required input: %d\n", err);
            goto cleanup;
        }

        // 必要なデータサイズが 0 の場合は初期化完了
        if (required_size == 0) {
            break;
        }

        if (fseek(input_file, required_position, SEEK_SET) != 0) {
            fprintf(stderr, "Error: Could not seek to position %lu\n", required_position);
            goto cleanup;
        }

        // 読み込むサイズを決定
        size_t read_size = BUFFER_SIZE;
        if (required_size > 0) {
            // 特定のサイズが要求されている場合
            read_size = (size_t)required_size;
        } else if (required_size == -1) {
            // ファイル末尾までの読み込みが必要な場合
            long current_pos = ftell(input_file);
            fseek(input_file, 0, SEEK_END);
            long file_size = ftell(input_file);
            fseek(input_file, required_position, SEEK_SET);

            read_size = file_size - required_position;
        }

        // バッファサイズを超えていたらエラーにする
        // （実際には、許容可能な範囲内ならバッファをリサイズすべき）
        if (read_size > BUFFER_SIZE) {
            fprintf(stderr, "Error: read_size (%zu) exceeds BUFFER_SIZE (%zu). \n",
                    read_size, (size_t)BUFFER_SIZE);
            goto cleanup;
        }

        size_t bytes_read = fread(read_buffer, 1, read_size, input_file);
        if (bytes_read == 0 && ferror(input_file)) {
            fprintf(stderr, "Error: Failed to read input file\n");
            goto cleanup;
        }

        // デマルチプレックサーにデータを入力
        mp4_file_demuxer_handle_input(demuxer, required_position, read_buffer,
                                      (uint32_t)bytes_read);
    }

    // トラック情報を取得
    const struct Mp4DemuxTrackInfo *tracks;
    uint32_t track_count;

    err = mp4_file_demuxer_get_tracks(demuxer, &tracks, &track_count);
    if (err != MP4_ERROR_OK) {
        fprintf(stderr, "Error: Failed to get tracks: %d\n", err);
        goto cleanup;
    }

    printf("Found %u track(s)\n\n", track_count);

    // ==================== サンプルのリマルチプレックス ====================
    printf("Remuxing samples...\n");

    uint32_t sample_count = 0;
    struct Mp4DemuxSample demux_sample;

    // 時系列順にサンプルを処理
    while (true) {
        err = mp4_file_demuxer_next_sample(demuxer, &demux_sample);

        if (err == MP4_ERROR_NO_MORE_SAMPLES) {
            break;
        }

        if (err != MP4_ERROR_OK) {
            fprintf(stderr, "Error: Failed to get next sample: %d\n", err);
            const char *error_msg = mp4_file_demuxer_get_last_error(demuxer);
            if (error_msg) {
                fprintf(stderr, "  %s\n", error_msg);
            }
            goto cleanup;
        }

        // 入力ファイルからサンプルデータを読み込む
        if (fseek(input_file, demux_sample.data_offset, SEEK_SET) != 0) {
            fprintf(stderr, "Error: Could not seek to sample data offset %lu\n",
                    demux_sample.data_offset);
            goto cleanup;
        }

        // サンプルデータ用のメモリを割り当て
        uint8_t *sample_data = (uint8_t *)malloc(demux_sample.data_size);
        if (!sample_data) {
            fprintf(stderr, "Error: Could not allocate memory for sample data\n");
            goto cleanup;
        }

        // サンプルデータを読み込む
        size_t bytes_read = fread(sample_data, 1, demux_sample.data_size, input_file);
        if (bytes_read != demux_sample.data_size) {
            fprintf(stderr, "Error: Failed to read sample data (expected %zu bytes, got %zu)\n",
                    demux_sample.data_size, bytes_read);
            free(sample_data);
            goto cleanup;
        }

        // サンプルデータを出力ファイルに追記
        if (fwrite(sample_data, 1, demux_sample.data_size, output_file) !=
            demux_sample.data_size) {
            fprintf(stderr, "Error: Failed to write sample data to output file\n");
            free(sample_data);
            goto cleanup;
        }

        free(sample_data);

        // デマルチプレックスサンプルからマルチプレックスサンプルを構築
        struct Mp4MuxSample mux_sample = {
            .track_kind = demux_sample.track->kind,
            .sample_entry = demux_sample.sample_entry,
            .keyframe = demux_sample.keyframe,
            .timescale = demux_sample.track->timescale,
            .duration = demux_sample.duration,
            .data_offset = current_output_data_offset,
            .data_size = (uint32_t)demux_sample.data_size,
            .has_composition_time_offset = demux_sample.has_composition_time_offset,
            .composition_time_offset = (int32_t)demux_sample.composition_time_offset,
        };

        // マルチプレックサーにサンプルを追加
        err = mp4_file_muxer_append_sample(muxer, &mux_sample);
        if (err != MP4_ERROR_OK) {
            fprintf(stderr, "Error: Failed to append sample: %d\n", err);
            const char *error_msg = mp4_file_muxer_get_last_error(muxer);
            if (error_msg) {
                fprintf(stderr, "  %s\n", error_msg);
            }
            goto cleanup;
        }

        sample_count++;

        // 次のサンプルデータの開始位置を更新
        current_output_data_offset += demux_sample.data_size;

        if (sample_count % 100 == 0) {
            printf("  Processed %u samples\n", sample_count);
        }
    }

    printf("Total samples processed: %u\n\n", sample_count);

    // ==================== マルチプレックサーの終了処理 ====================
    printf("Finalizing muxer...\n");

    err = mp4_file_muxer_finalize(muxer);
    if (err != MP4_ERROR_OK) {
        fprintf(stderr, "Error: Failed to finalize muxer: %d\n", err);
        const char *error_msg = mp4_file_muxer_get_last_error(muxer);
        if (error_msg) {
            fprintf(stderr, "  %s\n", error_msg);
        }
        goto cleanup;
    }

    // マルチプレックサーの最終出力データを書き込む
    while (mp4_file_muxer_next_output(muxer, &output_offset, &output_size, &output_data) ==
           MP4_ERROR_OK) {
        if (output_size == 0) break;

        if (fseek(output_file, output_offset, SEEK_SET) != 0) {
            fprintf(stderr, "Error: Failed to seek in output file\n");
            goto cleanup;
        }

        if (fwrite(output_data, 1, output_size, output_file) != output_size) {
            fprintf(stderr, "Error: Failed to write to output file\n");
            goto cleanup;
        }

        printf("  Wrote final %u bytes at offset %lu\n", output_size, output_offset);
    }

    printf("\nSuccessfully remuxed '%s' to '%s'\n", input_filepath, output_filepath);

cleanup:
    // リソースを解放
    mp4_file_muxer_free(muxer);
    fclose(output_file);
    free(read_buffer);
    mp4_file_demuxer_free(demuxer);
    fclose(input_file);

    return 0;
}
