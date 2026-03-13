// MP4 ファイルをデマルチプレックスして、メディアトラックとサンプル情報を表示する例
//
// このプログラムは、MP4 ファイルをデマルチプレックスして、含まれるメディアトラックとサンプルの情報を表示する
//
// # ビルド
//
//
// ```bash
// # mp4-rs のプロジェクトルートで libmp4.a をビルド
// cargo build
//
// # demux.c をコンパイル
// cc -o target/debug/demux \
//    -I crates/c-api/include/ \
//    crates/c-api/examples/demux.c \
//    target/debug/libmp4.a
// ```
//
// # 実行方法
// ```bash
// ./target/debug/demux /path/to/MP4_FILE
// ```
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "mp4.h"

#define BUFFER_SIZE (1024 * 1024)  // 1MB のバッファサイズ

// トラック種別を文字列に変換
const char *get_track_kind_name(enum Mp4TrackKind kind) {
    switch (kind) {
        case MP4_TRACK_KIND_AUDIO:
            return "Audio";
        case MP4_TRACK_KIND_VIDEO:
            return "Video";
        default:
            return "Unknown";
    }
}

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
        case MP4_SAMPLE_ENTRY_KIND_FLAC:
            return "FLAC";
        default:
            return "Unknown";
    }
}

// サンプルエントリー情報を表示
void print_sample_entry_info(const struct Mp4SampleEntry *sample_entry) {
    printf("    Codec: %s\n", get_sample_entry_kind_name(sample_entry->kind));

    switch (sample_entry->kind) {
        case MP4_SAMPLE_ENTRY_KIND_AVC1: {
            const struct Mp4SampleEntryAvc1 *avc1 = &sample_entry->data.avc1;
            printf("    Resolution: %ux%u\n", avc1->width, avc1->height);
            printf("    Profile: %u, Level: %u\n", avc1->avc_profile_indication,
                   avc1->avc_level_indication);
            printf("    SPS count: %u, PPS count: %u\n", avc1->sps_count, avc1->pps_count);
            break;
        }
        case MP4_SAMPLE_ENTRY_KIND_HEV1: {
            const struct Mp4SampleEntryHev1 *hev1 = &sample_entry->data.hev1;
            printf("    Resolution: %ux%u\n", hev1->width, hev1->height);
            printf("    Profile: %u, Level: %u\n", hev1->general_profile_idc,
                   hev1->general_level_idc);
            printf("    Chroma format: %u, Bit depth (luma): %u\n", hev1->chroma_format_idc,
                   hev1->bit_depth_luma_minus8 + 8);
            break;
        }
        case MP4_SAMPLE_ENTRY_KIND_HVC1: {
            const struct Mp4SampleEntryHvc1 *hvc1 = &sample_entry->data.hvc1;
            printf("    Resolution: %ux%u\n", hvc1->width, hvc1->height);
            printf("    Profile: %u, Level: %u\n", hvc1->general_profile_idc,
                   hvc1->general_level_idc);
            printf("    Chroma format: %u, Bit depth (luma): %u\n", hvc1->chroma_format_idc,
                   hvc1->bit_depth_luma_minus8 + 8);
            break;
        }
        case MP4_SAMPLE_ENTRY_KIND_VP09: {
            const struct Mp4SampleEntryVp09 *vp09 = &sample_entry->data.vp09;
            printf("    Resolution: %ux%u\n", vp09->width, vp09->height);
            printf("    Profile: %u, Level: %u, Bit depth: %u\n", vp09->profile, vp09->level,
                   vp09->bit_depth);
            break;
        }
        case MP4_SAMPLE_ENTRY_KIND_AV01: {
            const struct Mp4SampleEntryAv01 *av01 = &sample_entry->data.av01;
            printf("    Resolution: %ux%u\n", av01->width, av01->height);
            printf("    Profile: %u, Level: %u, Bit depth: %s\n", av01->seq_profile,
                   av01->seq_level_idx_0, av01->high_bitdepth ? "10" : "8");
            break;
        }
        case MP4_SAMPLE_ENTRY_KIND_OPUS: {
            const struct Mp4SampleEntryOpus *opus = &sample_entry->data.opus;
            printf("    Channels: %u, Sample rate: %u Hz\n", opus->channel_count,
                   opus->sample_rate);
            break;
        }
        case MP4_SAMPLE_ENTRY_KIND_MP4A: {
            const struct Mp4SampleEntryMp4a *mp4a = &sample_entry->data.mp4a;
            printf("    Channels: %u, Sample rate: %u Hz\n", mp4a->channel_count,
                   mp4a->sample_rate);
            break;
        }
        case MP4_SAMPLE_ENTRY_KIND_FLAC: {
            const struct Mp4SampleEntryFlac *flac = &sample_entry->data.flac;
            printf("    Channels: %u, Sample rate: %u Hz\n", flac->channel_count,
                   flac->sample_rate);
            break;
        }
        default:
            break;
    }
}

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <mp4_file>\n", argv[0]);
        return 1;
    }

    const char *filepath = argv[1];
    FILE *file = fopen(filepath, "rb");
    if (!file) {
        fprintf(stderr, "Error: Could not open file '%s'\n", filepath);
        return 1;
    }

    // デマルチプレックサーを作成
    struct Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
    if (!demuxer) {
        fprintf(stderr, "Error: Could not create demuxer\n");
        fclose(file);
        return 1;
    }

    // 入力バッファを割り当て
    uint8_t *buffer = (uint8_t *)malloc(BUFFER_SIZE);
    if (!buffer) {
        fprintf(stderr, "Error: Could not allocate buffer\n");
        mp4_file_demuxer_free(demuxer);
        fclose(file);
        return 1;
    }

    // ファイルデータを読み込み、デマルチプレックス処理を進める
    while (true) {
        uint64_t required_position;
        int32_t required_size;

        // 次に必要な入力データの位置とサイズを取得
        enum Mp4Error err =
            mp4_file_demuxer_get_required_input(demuxer, &required_position, &required_size);
        if (err != MP4_ERROR_OK) {
            fprintf(stderr, "Error: Failed to get required input: %d\n", err);
            const char *error_msg = mp4_file_demuxer_get_last_error(demuxer);
            if (error_msg) {
                fprintf(stderr, "  %s\n", error_msg);
            }
            break;
        }

        // 必要なデータサイズが 0 の場合は初期化完了
        if (required_size == 0) {
            break;
        }

        // ファイルをシークして、必要なデータを読み込む
        if (fseek(file, required_position, SEEK_SET) != 0) {
            fprintf(stderr, "Error: Could not seek to position %lu\n", required_position);
            break;
        }

        // 読み込むサイズを決定
        size_t read_size = BUFFER_SIZE;
        if (required_size > 0) {
            // 特定のサイズが要求されている場合
            read_size = (size_t)required_size;
        } else {
            // ファイル末尾までの読み込みが必要な場合
            long current_pos = ftell(file);
            fseek(file, 0, SEEK_END);
            long file_size = ftell(file);
            fseek(file, required_position, SEEK_SET);

            read_size = file_size - required_position;
        }

        // バッファサイズを超えていたらエラーにする
        // （実際には、許容可能な範囲内ならバッファをリサイズすべき）
        if (read_size > BUFFER_SIZE) {
            fprintf(stderr, "Error: read_size (%zu) exceeds BUFFER_SIZE (%zu). \n",
                    read_size, (size_t)BUFFER_SIZE);
            break;
        }

        size_t bytes_read = fread(buffer, 1, read_size, file);
        if (bytes_read == 0 && ferror(file)) {
            fprintf(stderr, "Error: Failed to read file at position %lu\n", required_position);
            break;
        }

        // デマルチプレックサーにデータを入力
        mp4_file_demuxer_handle_input(demuxer, required_position, buffer, (uint32_t)bytes_read);
    }

    // トラック情報を取得
    const struct Mp4DemuxTrackInfo *tracks;
    uint32_t track_count;

    enum Mp4Error err = mp4_file_demuxer_get_tracks(demuxer, &tracks, &track_count);
    if (err != MP4_ERROR_OK) {
        fprintf(stderr, "Error: Failed to get tracks: %d\n", err);
        const char *error_msg = mp4_file_demuxer_get_last_error(demuxer);
        if (error_msg) {
            fprintf(stderr, "  %s\n", error_msg);
        }
        free(buffer);
        mp4_file_demuxer_free(demuxer);
        fclose(file);
        return 1;
    }

    printf("Found %u track(s)\n\n", track_count);

    // トラック情報を表示
    for (uint32_t i = 0; i < track_count; i++) {
        printf("Track %u:\n", i + 1);
        printf("  Track ID: %u\n", tracks[i].track_id);
        printf("  Kind: %s\n", get_track_kind_name(tracks[i].kind));
        printf("  Duration: %lu (timescale: %u)\n", tracks[i].duration, tracks[i].timescale);
        printf("\n");
    }

    // サンプル情報を表示
    uint32_t sample_count = 0;
    uint32_t keyframe_count = 0;

    printf("Samples:\n");
    struct Mp4DemuxSample sample;

    // 時系列順にサンプルを取得
    while (true) {
        err = mp4_file_demuxer_next_sample(demuxer, &sample);

        if (err == MP4_ERROR_NO_MORE_SAMPLES) {
            break;
        }

        if (err != MP4_ERROR_OK) {
            fprintf(stderr, "Error: Failed to get next sample: %d\n", err);
            const char *error_msg = mp4_file_demuxer_get_last_error(demuxer);
            if (error_msg) {
                fprintf(stderr, "  %s\n", error_msg);
            }
            break;
        }

        sample_count++;

        printf("  Sample %u:\n", sample_count);
        printf("    Track ID: %u\n", sample.track->track_id);
        printf("    Keyframe: %s\n", sample.keyframe ? "Yes" : "No");
        printf("    Timestamp: %lu\n", sample.timestamp);
        printf("    Duration: %u\n", sample.duration);
        printf("    Data offset: 0x%lx\n", sample.data_offset);
        printf("    Data size: %lu bytes\n", sample.data_size);

        // 最初のサンプルのエントリ情報を表示
        if (sample_count == 1) {
            print_sample_entry_info(sample.sample_entry);
        }

        if (sample.keyframe) {
            keyframe_count++;
        }

        printf("\n");

        // 最初の10個のサンプルのみ表示
        if (sample_count >= 10) {
            printf("  ... (showing first 10 samples)\n");
            break;
        }
    }

    printf("Total: %u samples, %u keyframes\n", sample_count, keyframe_count);

    // リソースを解放
    free(buffer);
    mp4_file_demuxer_free(demuxer);
    fclose(file);

    return 0;
}

