#ifndef SHIGUREDO_MP4_H
#define SHIGUREDO_MP4_H

/* Generated with cbindgen:0.29.2 */

#include <stdbool.h>
#include <stdint.h>

/**
 * 発生する可能性のあるエラーの種類を表現する列挙型
 */
typedef enum Mp4Error {
  /**
   * エラーが発生しなかったことを示す
   */
  MP4_ERROR_OK = 0,
  /**
   * 入力引数ないしパラメーターが無効である
   */
  MP4_ERROR_INVALID_INPUT,
  /**
   * 入力データが破損しているか無効な形式である
   */
  MP4_ERROR_INVALID_DATA,
  /**
   * 操作に対する内部状態が無効である
   */
  MP4_ERROR_INVALID_STATE,
  /**
   * 入力データの読み込みが必要である
   */
  MP4_ERROR_INPUT_REQUIRED,
  /**
   * 出力データの書き込みが必要である
   */
  MP4_ERROR_OUTPUT_REQUIRED,
  /**
   * NULL ポインタが渡された
   */
  MP4_ERROR_NULL_POINTER,
  /**
   * これ以上読み込むサンプルが存在しない
   */
  MP4_ERROR_NO_MORE_SAMPLES,
  /**
   * 操作またはデータ形式がサポートされていない
   */
  MP4_ERROR_UNSUPPORTED,
  /**
   * 上記以外のエラーが発生した
   */
  MP4_ERROR_OTHER,
} Mp4Error;

/**
 * MP4 ファイル内のトラックの種類を表す列挙型
 */
typedef enum Mp4TrackKind {
  /**
   * 音声トラック
   */
  MP4_TRACK_KIND_AUDIO = 0,
  /**
   * 映像トラック
   */
  MP4_TRACK_KIND_VIDEO = 1,
} Mp4TrackKind;

/**
 * サンプルエントリーの種類を表す列挙型
 *
 * MP4 ファイル内で使用されるコーデックの種類を識別するために使用される
 */
typedef enum Mp4SampleEntryKind {
  /**
   * AVC1 (H.264)
   */
  MP4_SAMPLE_ENTRY_KIND_AVC1,
  /**
   * HEV1 (H.265/HEVC)
   */
  MP4_SAMPLE_ENTRY_KIND_HEV1,
  /**
   * HVC1 (H.265/HEVC)
   */
  MP4_SAMPLE_ENTRY_KIND_HVC1,
  /**
   * VP08 (VP8)
   */
  MP4_SAMPLE_ENTRY_KIND_VP08,
  /**
   * VP09 (VP9)
   */
  MP4_SAMPLE_ENTRY_KIND_VP09,
  /**
   * AV01 (AV1)
   */
  MP4_SAMPLE_ENTRY_KIND_AV01,
  /**
   * Opus
   */
  MP4_SAMPLE_ENTRY_KIND_OPUS,
  /**
   * MP4A (AAC)
   */
  MP4_SAMPLE_ENTRY_KIND_MP4A,
  /**
   * FLAC
   */
  MP4_SAMPLE_ENTRY_KIND_FLAC,
} Mp4SampleEntryKind;

typedef enum Mp4FileKind {
  MP4_FILE_KIND_MP4 = 0,
  MP4_FILE_KIND_FRAGMENTED_MP4 = 1,
} Mp4FileKind;

/**
 * fMP4 Demuxer の状態を保持する C 構造体
 *
 * # 関連関数
 *
 * - `fmp4_segment_demuxer_new()`: インスタンスを生成する
 * - `fmp4_segment_demuxer_free()`: リソースを解放する
 * - `fmp4_segment_demuxer_get_last_error()`: 最後のエラーメッセージを取得する
 * - `fmp4_segment_demuxer_handle_init_segment()`: 初期化セグメントを処理する
 * - `fmp4_segment_demuxer_get_tracks()`: トラック情報を取得する
 * - `fmp4_segment_demuxer_handle_media_segment()`: メディアセグメントを処理する
 * - `fmp4_segment_demuxer_free_samples()`: サンプル配列を解放する
 */
typedef struct Fmp4SegmentDemuxer Fmp4SegmentDemuxer;

/**
 * fMP4 Muxer の状態を保持する C 構造体
 *
 * # 関連関数
 *
 * - `fmp4_segment_muxer_new()`: インスタンスを生成する
 * - `fmp4_segment_muxer_free()`: リソースを解放する
 * - `fmp4_segment_muxer_get_last_error()`: 最後のエラーメッセージを取得する
 * - `fmp4_segment_muxer_write_init_segment()`: 初期化セグメントを生成する
 * - `fmp4_segment_muxer_write_media_segment_metadata()`: メディアセグメント先頭メタデータを生成する
 * - `fmp4_segment_muxer_write_media_segment_metadata_with_sidx()`: sidx 付きメディアセグメント先頭メタデータを生成する
 * - `fmp4_segment_muxer_write_mfra()`: `mfra` ボックスを生成する
 */
typedef struct Fmp4SegmentMuxer Fmp4SegmentMuxer;

/**
 * MP4 ファイルをデマルチプレックスして、メディアサンプルを時系列順に取得するための構造体
 *
 * # 関連関数
 *
 * この構造体は、以下の関数を通して操作する必要がある:
 * - `mp4_file_demuxer_new()`: `Mp4FileDemuxer` インスタンスを生成する
 * - `mp4_file_demuxer_free()`: リソースを解放する
 * - `mp4_file_demuxer_get_required_input()`: 次の処理に必要な入力データの位置とサイズを取得する
 * - `mp4_file_demuxer_handle_input()`: ファイルデータを入力として受け取る
 * - `mp4_file_demuxer_get_tracks()`: MP4 ファイル内のすべてのメディアトラック情報を取得する
 * - `mp4_file_demuxer_next_sample()`: 時系列順に次のサンプルを取得する
 * - `mp4_file_demuxer_prev_sample()`: 時系列順に前のサンプルを取得する
 * - `mp4_file_demuxer_seek()`: 指定した時刻へシークする
 * - `mp4_file_demuxer_get_last_error()`: 最後に発生したエラーのメッセージを取得する
 *
 * # Examples
 *
 * ```c
 * // Mp4FileDemuxer インスタンスを生成
 * Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
 *
 * // 入力ファイルデータを供給して初期化
 * while (true) {
 *     uint64_t required_pos;
 *     int32_t required_size;
 *     mp4_file_demuxer_get_required_input(demuxer, &required_pos, &required_size);
 *     if (required_size == 0) break;
 *
 *     // NOTE: 実際には `required_size == -1` の場合には、ファイル末尾までを読み込む必要がある
 *     uint8_t buffer[required_size];
 *     size_t bytes_read = read_file_data(required_pos, buffer, sizeof(required_size));
 *     mp4_file_demuxer_handle_input(demuxer, required_pos, buffer, bytes_read);
 * }
 *
 * // トラック情報を取得
 * const Mp4DemuxTrackInfo *tracks;
 * uint32_t track_count;
 * Mp4Error ret = mp4_file_demuxer_get_tracks(demuxer, &tracks, &track_count);
 * if (ret == MP4_ERROR_OK) {
 *     // トラック情報を処理...
 * }
 *
 * // サンプルを取得
 * Mp4DemuxSample sample;
 * while (mp4_file_demuxer_next_sample(demuxer, &sample) == MP4_ERROR_OK) {
 *     // サンプルを処理...
 * }
 *
 * // 前のサンプルを取得
 * while (mp4_file_demuxer_prev_sample(demuxer, &sample) == MP4_ERROR_OK) {
 *     // サンプルを処理...
 * }
 *
 * // timestamp=1500, timescale=1000 にシーク
 * mp4_file_demuxer_seek(demuxer, 1500, 1000);
 *
 * // リソース解放
 * mp4_file_demuxer_free(demuxer);
 * ```
 */
typedef struct Mp4FileDemuxer Mp4FileDemuxer;

typedef struct Mp4FileKindDetector Mp4FileKindDetector;

/**
 * メディアトラック（音声・映像）を含んだ MP4 ファイルの構築（マルチプレックス）処理を行うための構造体
 *
 * # 関連関数
 *
 * この構造体は、以下の関数を通して操作する必要がある:
 * - `mp4_file_muxer_new()`: `Mp4FileMuxer` インスタンスを生成する
 * - `mp4_file_muxer_free()`: リソースを解放する
 * - `mp4_file_muxer_set_reserved_moov_box_size()`: faststart 用に事前確保する moov ボックスのサイズを設定する
 * - `mp4_file_muxer_initialize()`: マルチプレックス処理を初期化する
 * - `mp4_file_muxer_append_sample()`: サンプルを追加する
 * - `mp4_file_muxer_next_output()`: 出力データを取得する
 * - `mp4_file_muxer_finalize()`: マルチプレックス処理を完了する
 * - `mp4_file_muxer_get_last_error()`: 最後に発生したエラーのメッセージを取得する
 *
 * # 使用例
 *
 * ```c
 * #include <stdio.h>
 * #include <stdlib.h>
 * #include <stdint.h>
 * #include <string.h>
 * #include "mp4.h"
 *
 * int main() {
 *     // 1. Mp4FileMuxer インスタンスを生成
 *     Mp4FileMuxer *muxer = mp4_file_muxer_new();
 *
 *     // ファイルをオープン
 *     FILE *fp = fopen("output.mp4", "wb");
 *     if (fp == NULL) {
 *         fprintf(stderr, "Failed to open output file\n");
 *         mp4_file_muxer_free(muxer);
 *         return 1;
 *     }
 *
 *     // 2. オプション設定（必要に応じて）
 *     mp4_file_muxer_set_reserved_moov_box_size(muxer, 8192);
 *
 *     // 3. マルチプレックス処理を初期化
 *     Mp4Error ret = mp4_file_muxer_initialize(muxer);
 *     if (ret != MP4_ERROR_OK) {
 *         fprintf(stderr, "初期化失敗: %s\n", mp4_file_muxer_get_last_error(muxer));
 *         mp4_file_muxer_free(muxer);
 *         fclose(fp);
 *         return 1;
 *     }
 *
 *     // 4. 初期出力データをファイルに書き込む
 *     uint64_t output_offset;
 *     uint32_t output_size;
 *     const uint8_t *output_data;
 *     while (mp4_file_muxer_next_output(muxer, &output_offset, &output_size, &output_data) == MP4_ERROR_OK) {
 *         if (output_size == 0) break;
 *         fseek(fp, output_offset, SEEK_SET);
 *         fwrite(output_data, 1, output_size, fp);
 *     }
 *
 *     // 5. サンプルを追加
 *
 *     // サンプルデータを準備（例：4096 バイトのダミー VP8 フレームデータ）
 *     uint8_t video_sample_data[4096];
 *     memset(video_sample_data, 0, sizeof(video_sample_data));
 *
 *     // サンプルデータをファイルに書き込み
 *     fwrite(video_sample_data, 1, sizeof(video_sample_data), fp);
 *
 *     // VP08（VP8）サンプルエントリーを作成
 *     Mp4SampleEntryVp08 vp08_data = {
 *         .width = 1920,
 *         .height = 1080,
 *         .bit_depth = 8,
 *         .chroma_subsampling = 1,  // 4:2:0
 *         .video_full_range_flag = false,
 *         .colour_primaries = 1,     // BT.709
 *         .transfer_characteristics = 1,  // BT.709
 *         .matrix_coefficients = 1,  // BT.709
 *     };
 *
 *     Mp4SampleEntryData sample_entry_data;
 *     sample_entry_data.vp08 = vp08_data;
 *
 *     Mp4SampleEntry sample_entry = {
 *         .kind = MP4_SAMPLE_ENTRY_KIND_VP08,
 *         .data = sample_entry_data,
 *     };
 *
 *     Mp4MuxSample video_sample = {
 *         .track_kind = MP4_TRACK_KIND_VIDEO,
 *         .sample_entry = &sample_entry,
 *         .keyframe = true,
 *         .timescale = 30,  // 30 fps
 *         .duration = 1,
 *         .data_offset = output_offset + output_size,
 *         .data_size = sizeof(video_sample_data),
 *     };
 *     ret = mp4_file_muxer_append_sample(muxer, &video_sample);
 *     if (ret != MP4_ERROR_OK) {
 *         fprintf(stderr, "Failed to append sample: %s\n", mp4_file_muxer_get_last_error(muxer));
 *         mp4_file_muxer_free(muxer);
 *         fclose(fp);
 *         return 1;
 *     }
 *
 *     // 6. マルチプレックス処理を完了
 *     ret = mp4_file_muxer_finalize(muxer);
 *     if (ret != MP4_ERROR_OK) {
 *         fprintf(stderr, "ファイナライズ失敗: %s\n", mp4_file_muxer_get_last_error(muxer));
 *         mp4_file_muxer_free(muxer);
 *         fclose(fp);
 *         return 1;
 *     }
 *
 *     // 7. ファイナライズ後のボックスデータをファイルに書き込む
 *     while (mp4_file_muxer_next_output(muxer, &output_offset, &output_size, &output_data) == MP4_ERROR_OK) {
 *         if (output_size == 0) break;
 *         fseek(fp, output_offset, SEEK_SET);
 *         fwrite(output_data, 1, output_size, fp);
 *     }
 *
 *     // 8. リソース解放
 *     mp4_file_muxer_free(muxer);
 *     fclose(fp);
 *
 *     printf("MP4 file created successfully: output.mp4\n");
 *     return 0;
 * }
 * ```
 */
typedef struct Mp4FileMuxer Mp4FileMuxer;

/**
 * MP4 デマルチプレックス処理中に抽出されたメディアトラックの情報を表す構造体
 */
typedef struct Mp4DemuxTrackInfo {
  /**
   * このトラックを識別するための ID
   */
  uint32_t track_id;
  /**
   * トラックの種類（音声または映像）
   */
  enum Mp4TrackKind kind;
  /**
   * トラックの尺（タイムスケール単位で表現）
   *
   * 実際の時間（秒単位）を得るには、この値を `timescale` で除算すること。
   *
   * fMP4 の場合は init segment 由来の値であり、実際には 0 になることが多い。
   * その場合は「未確定ないし実質不明相当」とみなしてよい。
   */
  uint64_t duration;
  /**
   * このトラック内で使用されているタイムスケール
   *
   * タイムスタンプと尺の単位を定義する値で、1 秒間の単位数を表す
   * 例えば `timescale` が 1000 の場合、タイムスタンプは 1 ms 単位で表現される
   */
  uint32_t timescale;
} Mp4DemuxTrackInfo;

/**
 * AVC1（H.264）コーデック用のサンプルエントリー
 *
 * H.264 ビデオコーデックの詳細情報を保持する構造体で、
 * 解像度、プロファイル、レベル、SPS/PPS パラメータセットなどの情報が含まれる
 *
 * 各フィールドの詳細については MP4 やコーデックの仕様を参照のこと
 *
 * # 使用例
 *
 * SPS / PPS リストへのアクセス例:
 * ```c
 * Mp4SampleEntry entry = // ...;
 *
 * if (entry.kind == MP4_SAMPLE_ENTRY_KIND_AVC1) {
 *     Mp4SampleEntryAvc1 *avc1 = &entry.data.avc1;
 *
 *     // すべての SPS パラメータセットを処理
 *     for (uint32_t i = 0; i < avc1->sps_count; i++) {
 *         const uint8_t *sps_data = avc1->sps_data[i];
 *         uint32_t sps_size = avc1->sps_sizes[i];
 *         // SPS データを処理...
 *     }
 *
 *     // すべての PPS パラメータセットを処理
 *     for (uint32_t i = 0; i < avc1->pps_count; i++) {
 *         const uint8_t *pps_data = avc1->pps_data[i];
 *         uint32_t pps_size = avc1->pps_sizes[i];
 *         // PPS データを処理...
 *     }
 * }
 * ```
 */
typedef struct Mp4SampleEntryAvc1 {
  uint16_t width;
  uint16_t height;
  uint8_t avc_profile_indication;
  uint8_t profile_compatibility;
  uint8_t avc_level_indication;
  uint8_t length_size_minus_one;
  const uint8_t *const *sps_data;
  const uint32_t *sps_sizes;
  uint32_t sps_count;
  const uint8_t *const *pps_data;
  const uint32_t *pps_sizes;
  uint32_t pps_count;
  bool is_chroma_format_present;
  uint8_t chroma_format;
  bool is_bit_depth_luma_minus8_present;
  uint8_t bit_depth_luma_minus8;
  bool is_bit_depth_chroma_minus8_present;
  uint8_t bit_depth_chroma_minus8;
} Mp4SampleEntryAvc1;

/**
 * HEV1（H.265/HEVC）コーデック用のサンプルエントリー
 *
 * H.265 ビデオコーデックの詳細情報を保持する構造体で、
 * 解像度、プロファイル、レベル、NALU パラメータセットなどの情報が含まれる
 *
 * 各フィールドの詳細については MP4 やコーデックの仕様を参照のこと
 *
 * # 使用例
 *
 * NALU リストへのアクセス例:
 * ```c
 * Mp4SampleEntry entry = // ...;
 *
 * if (entry.kind == MP4_SAMPLE_ENTRY_KIND_HEV1) {
 *     Mp4SampleEntryHev1 *hev1 = &entry.data.hev1;
 *
 *     // すべての NALU 配列を処理
 *     uint32_t nalu_index = 0;
 *     for (uint32_t i = 0; i < hev1->nalu_array_count; i++) {
 *         uint8_t nalu_type = hev1->nalu_types[i];
 *         uint32_t nalu_count = hev1->nalu_counts[i];
 *
 *         // この NALU タイプのすべてのユニットを処理
 *         for (uint32_t j = 0; j < nalu_count; j++) {
 *             const uint8_t *nalu_data = hev1->nalu_data[nalu_index];
 *             uint32_t nalu_size = hev1->nalu_sizes[nalu_index];
 *             // NALU データを処理...
 *             nalu_index++;
 *         }
 *     }
 * }
 * ```
 */
typedef struct Mp4SampleEntryHev1 {
  uint16_t width;
  uint16_t height;
  uint8_t general_profile_space;
  uint8_t general_tier_flag;
  uint8_t general_profile_idc;
  uint32_t general_profile_compatibility_flags;
  uint64_t general_constraint_indicator_flags;
  uint8_t general_level_idc;
  uint8_t chroma_format_idc;
  uint8_t bit_depth_luma_minus8;
  uint8_t bit_depth_chroma_minus8;
  uint16_t min_spatial_segmentation_idc;
  uint8_t parallelism_type;
  uint16_t avg_frame_rate;
  uint8_t constant_frame_rate;
  uint8_t num_temporal_layers;
  uint8_t temporal_id_nested;
  uint8_t length_size_minus_one;
  uint32_t nalu_array_count;
  const uint8_t *nalu_types;
  const uint32_t *nalu_counts;
  const uint8_t *const *nalu_data;
  const uint32_t *nalu_sizes;
} Mp4SampleEntryHev1;

/**
 * HVC1（H.265/HEVC）コーデック用のサンプルエントリー
 *
 * H.265 ビデオコーデックの詳細情報を保持する構造体で、
 * 解像度、プロファイル、レベル、NALU パラメータセットなどの情報が含まれる
 *
 * 各フィールドの詳細については MP4 やコーデックの仕様を参照のこと
 *
 * # 使用例
 *
 * NALU リストへのアクセス例:
 * ```c
 * Mp4SampleEntry entry = // ...;
 *
 * if (entry.kind == MP4_SAMPLE_ENTRY_KIND_HVC1) {
 *     Mp4SampleEntryHvc1 *hvc1 = &entry.data.hvc1;
 *
 *     // すべての NALU 配列を処理
 *     uint32_t nalu_index = 0;
 *     for (uint32_t i = 0; i < hvc1->nalu_array_count; i++) {
 *         uint8_t nalu_type = hvc1->nalu_types[i];
 *         uint32_t nalu_count = hvc1->nalu_counts[i];
 *
 *         // この NALU タイプのすべてのユニットを処理
 *         for (uint32_t j = 0; j < nalu_count; j++) {
 *             const uint8_t *nalu_data = hvc1->nalu_data[nalu_index];
 *             uint32_t nalu_size = hvc1->nalu_sizes[nalu_index];
 *             // NALU データを処理...
 *             nalu_index++;
 *         }
 *     }
 * }
 * ```
 */
typedef struct Mp4SampleEntryHvc1 {
  uint16_t width;
  uint16_t height;
  uint8_t general_profile_space;
  uint8_t general_tier_flag;
  uint8_t general_profile_idc;
  uint32_t general_profile_compatibility_flags;
  uint64_t general_constraint_indicator_flags;
  uint8_t general_level_idc;
  uint8_t chroma_format_idc;
  uint8_t bit_depth_luma_minus8;
  uint8_t bit_depth_chroma_minus8;
  uint16_t min_spatial_segmentation_idc;
  uint8_t parallelism_type;
  uint16_t avg_frame_rate;
  uint8_t constant_frame_rate;
  uint8_t num_temporal_layers;
  uint8_t temporal_id_nested;
  uint8_t length_size_minus_one;
  uint32_t nalu_array_count;
  const uint8_t *nalu_types;
  const uint32_t *nalu_counts;
  const uint8_t *const *nalu_data;
  const uint32_t *nalu_sizes;
} Mp4SampleEntryHvc1;

/**
 * VP08（VP8）コーデック用のサンプルエントリー
 *
 * VP8 ビデオコーデックの詳細情報を保持する構造体で、
 * 解像度、ビット深度、色彩空間情報などが含まれる
 *
 * 各フィールドの詳細については MP4 やコーデックの仕様を参照のこと
 *
 * # 使用例
 *
 * 基本的な使用例:
 * ```c
 * Mp4SampleEntry entry = // ...;
 *
 * if (entry.kind == MP4_SAMPLE_ENTRY_KIND_VP08) {
 *     Mp4SampleEntryVp08 *vp08 = &entry.data.vp08;
 *     printf("解像度: %dx%d\n", vp08->width, vp08->height);
 *     printf("ビット深度: %d\n", vp08->bit_depth);
 *     printf("フルレンジ: %s\n", vp08->video_full_range_flag ? "有効" : "無効");
 * }
 * ```
 */
typedef struct Mp4SampleEntryVp08 {
  uint16_t width;
  uint16_t height;
  uint8_t bit_depth;
  uint8_t chroma_subsampling;
  bool video_full_range_flag;
  uint8_t colour_primaries;
  uint8_t transfer_characteristics;
  uint8_t matrix_coefficients;
} Mp4SampleEntryVp08;

/**
 * VP09（VP9）コーデック用のサンプルエントリー
 *
 * VP9 ビデオコーデックの詳細情報を保持する構造体で、
 * 解像度、プロファイル、レベル、ビット深度、色彩空間情報、
 * およびコーデック初期化データなどが含まれる
 *
 * 各フィールドの詳細については MP4 やコーデックの仕様を参照のこと
 *
 * # 使用例
 *
 * 基本的な使用例:
 * ```c
 * Mp4SampleEntry entry = // ...;
 *
 * if (entry.kind == MP4_SAMPLE_ENTRY_KIND_VP09) {
 *     Mp4SampleEntryVp09 *vp09 = &entry.data.vp09;
 *     printf("解像度: %dx%d\n", vp09->width, vp09->height);
 *     printf("プロファイル: %d\n", vp09->profile);
 *     printf("レベル: %d\n", vp09->level);
 *     printf("ビット深度: %d\n", vp09->bit_depth);
 * }
 * ```
 */
typedef struct Mp4SampleEntryVp09 {
  uint16_t width;
  uint16_t height;
  uint8_t profile;
  uint8_t level;
  uint8_t bit_depth;
  uint8_t chroma_subsampling;
  bool video_full_range_flag;
  uint8_t colour_primaries;
  uint8_t transfer_characteristics;
  uint8_t matrix_coefficients;
} Mp4SampleEntryVp09;

/**
 * AV01（AV1）コーデック用のサンプルエントリー
 *
 * AV1 ビデオコーデックの詳細情報を保持する構造体で、
 * 解像度、プロファイル、レベル、ビット深度、色彩空間情報、
 * およびコーデック設定 OBU（Open Bitstream Unit）などが含まれる
 *
 * 各フィールドの詳細については MP4 やコーデックの仕様を参照のこと
 *
 * # 使用例
 *
 * 基本的な使用例:
 * ```c
 * Mp4SampleEntry entry = // ...;
 *
 * if (entry.kind == MP4_SAMPLE_ENTRY_KIND_AV01) {
 *     Mp4SampleEntryAv01 *av01 = &entry.data.av01;
 *     printf("解像度: %dx%d\n", av01->width, av01->height);
 *     printf("プロファイル: %d\n", av01->seq_profile);
 *     printf("レベル: %d\n", av01->seq_level_idx_0);
 *     printf("ビット深度: %s\n", av01->high_bitdepth ? "10-12bit" : "8bit");
 *
 *     // コーデック設定 OBU にアクセス
 *     if (av01->config_obus_size > 0) {
 *         const uint8_t *config_data = av01->config_obus;
 *         uint32_t config_size = av01->config_obus_size;
 *         // 設定 OBU を処理...
 *     }
 * }
 * ```
 */
typedef struct Mp4SampleEntryAv01 {
  uint16_t width;
  uint16_t height;
  uint8_t seq_profile;
  uint8_t seq_level_idx_0;
  uint8_t seq_tier_0;
  uint8_t high_bitdepth;
  uint8_t twelve_bit;
  uint8_t monochrome;
  uint8_t chroma_subsampling_x;
  uint8_t chroma_subsampling_y;
  uint8_t chroma_sample_position;
  bool initial_presentation_delay_present;
  uint8_t initial_presentation_delay_minus_one;
  const uint8_t *config_obus;
  uint32_t config_obus_size;
} Mp4SampleEntryAv01;

/**
 * Opus 音声コーデック用のサンプルエントリー
 *
 * Opus 音声コーデックの詳細情報を保持する構造体で、
 * チャンネル数、サンプルレート、サンプルサイズ、
 * およびOpus固有のパラメータなどが含まれる
 *
 * 各フィールドの詳細については MP4 やコーデックの仕様を参照のこと
 *
 * # 使用例
 *
 * 基本的な使用例:
 * ```c
 * Mp4SampleEntry entry = // ...;
 *
 * if (entry.kind == MP4_SAMPLE_ENTRY_KIND_OPUS) {
 *     Mp4SampleEntryOpus *opus = &entry.data.opus;
 *     printf("チャンネル数: %d\n", opus->channel_count);
 *     printf("サンプルレート: %d Hz\n", opus->sample_rate);
 *     printf("プリスキップ: %d サンプル\n", opus->pre_skip);
 *     printf("入力サンプルレート: %d Hz\n", opus->input_sample_rate);
 *     printf("出力ゲイン: %d dB\n", opus->output_gain);
 * }
 * ```
 */
typedef struct Mp4SampleEntryOpus {
  uint8_t channel_count;
  uint16_t sample_rate;
  uint16_t sample_size;
  uint16_t pre_skip;
  uint32_t input_sample_rate;
  int16_t output_gain;
} Mp4SampleEntryOpus;

/**
 * MP4A（AAC）音声コーデック用のサンプルエントリー
 *
 * AAC 音声コーデックの詳細情報を保持する構造体で、
 * チャンネル数、サンプルレート、サンプルサイズ、バッファサイズ、ビットレート情報、
 * およびデコーダ固有情報などが含まれる
 *
 * 各フィールドの詳細については MP4 やコーデックの仕様を参照のこと
 *
 * # 使用例
 *
 * 基本的な使用例:
 * ```c
 * Mp4SampleEntry entry = // ...;
 *
 * if (entry.kind == MP4_SAMPLE_ENTRY_KIND_MP4A) {
 *     Mp4SampleEntryMp4a *mp4a = &entry.data.mp4a;
 *     printf("チャンネル数: %d\n", mp4a->channel_count);
 *     printf("サンプルレート: %d Hz\n", mp4a->sample_rate);
 *     printf("サンプルサイズ: %d bits\n", mp4a->sample_size);
 *     printf("最大ビットレート: %d bps\n", mp4a->max_bitrate);
 *     printf("平均ビットレート: %d bps\n", mp4a->avg_bitrate);
 *
 *     // デコーダ固有情報にアクセス
 *     if (mp4a->dec_specific_info_size > 0) {
 *         const uint8_t *dec_info = mp4a->dec_specific_info;
 *         uint32_t dec_info_size = mp4a->dec_specific_info_size;
 *         // デコーダ固有情報を処理...
 *     }
 * }
 * ```
 */
typedef struct Mp4SampleEntryMp4a {
  uint8_t channel_count;
  uint16_t sample_rate;
  uint16_t sample_size;
  uint32_t buffer_size_db;
  uint32_t max_bitrate;
  uint32_t avg_bitrate;
  const uint8_t *dec_specific_info;
  uint32_t dec_specific_info_size;
} Mp4SampleEntryMp4a;

/**
 * FLAC コーデック用のサンプルエントリー
 */
typedef struct Mp4SampleEntryFlac {
  uint8_t channel_count;
  uint16_t sample_rate;
  uint16_t sample_size;
  const uint8_t *streaminfo_data;
  uint32_t streaminfo_size;
} Mp4SampleEntryFlac;

/**
 * MP4 サンプルエントリーの詳細データを格納するユニオン型
 *
 * このユニオン型は、`Mp4SampleEntry` の `kind` フィールドで指定されたコーデック種別に応じて、
 * 対応する構造体へのアクセスを提供する
 */
typedef union Mp4SampleEntryData {
  /**
   * AVC1（H.264）コーデック用のサンプルエントリー
   */
  struct Mp4SampleEntryAvc1 avc1;
  /**
   * HEV1（H.265/HEVC）コーデック用のサンプルエントリー
   */
  struct Mp4SampleEntryHev1 hev1;
  /**
   * HVC1（H.265/HEVC）コーデック用のサンプルエントリー
   */
  struct Mp4SampleEntryHvc1 hvc1;
  /**
   * VP08（VP8）コーデック用のサンプルエントリー
   */
  struct Mp4SampleEntryVp08 vp08;
  /**
   * VP09（VP9）コーデック用のサンプルエントリー
   */
  struct Mp4SampleEntryVp09 vp09;
  /**
   * AV01（AV1）コーデック用のサンプルエントリー
   */
  struct Mp4SampleEntryAv01 av01;
  /**
   * Opus 音声コーデック用のサンプルエントリー
   */
  struct Mp4SampleEntryOpus opus;
  /**
   * MP4A（AAC）音声コーデック用のサンプルエントリー
   */
  struct Mp4SampleEntryMp4a mp4a;
  /**
   * FLAC 音声コーデック用のサンプルエントリー
   */
  struct Mp4SampleEntryFlac flac;
} Mp4SampleEntryData;

/**
 * MP4 サンプルエントリー
 *
 * MP4 ファイル内で使用されるメディアサンプル（フレーム単位の音声または映像データ）の
 * 詳細情報を保持する構造体
 *
 * 各サンプルはコーデック種別ごとに異なる詳細情報を持つため、
 * この構造体は `kind` フィールドでコーデック種別を識別し、
 * `data` ユニオンフィールドで対応するコーデック固有の詳細情報にアクセスする設計となっている
 *
 * # サンプルエントリーとは
 *
 * サンプルエントリー（Sample Entry）は、MP4 ファイル形式において、
 * メディアサンプル（動画フレームや音声フレーム）の属性情報を定義するメタデータである
 *
 * MP4 ファイルの各トラック内には、使用されるすべての異なるコーデック設定に対応する
 * サンプルエントリーが格納される
 *
 * サンプルデータ自体はこのサンプルエントリーを参照することで、
 * どのコーデックを使用し、どのような属性を持つかが定義される
 *
 * # 使用例
 *
 * ```c
 * // AVC1（H.264）コーデック用のサンプルエントリーを作成し、
 * // その詳細情報にアクセスする例
 * Mp4SampleEntry entry = // ...;
 *
 * if (entry.kind == MP4_SAMPLE_ENTRY_KIND_AVC1) {
 *     Mp4SampleEntryAvc1 *avc1 = &entry.data.avc1;
 *     printf("解像度: %dx%d\n", avc1->width, avc1->height);
 *     printf("プロファイル: %d\n", avc1->avc_profile_indication);
 * }
 * ```
 */
typedef struct Mp4SampleEntry {
  /**
   * このサンプルエントリーで使用されているコーデックの種別
   *
   * この値によって、`data` ユニオンフィールド内のどのメンバーが有効であるかが決まる
   *
   * 例えば、`kind` が `MP4_SAMPLE_ENTRY_KIND_AVC1` である場合、
   * `data.avc1` メンバーにアクセス可能であり、その他のメンバーはアクセス不可となる
   */
  enum Mp4SampleEntryKind kind;
  /**
   * コーデック種別に応じた詳細情報を保持するユニオン
   *
   * `kind` で指定されたメンバー以外にアクセスすると未定義動作となるため、
   * 必ず事前に `kind` フィールドを確認してからアクセスすること
   */
  union Mp4SampleEntryData data;
} Mp4SampleEntry;

/**
 * MP4 デマルチプレックス処理によって抽出されたメディアサンプルを表す構造体
 *
 * MP4 ファイル内の各サンプル（フレーム単位の音声または映像データ）のメタデータと
 * ファイル内の位置情報を保持する
 *
 * この構造体が参照しているポインタのメモリ管理は各 demuxer が行っており、
 * 対応する demuxer インスタンスが破棄されるまでは安全に参照可能である
 */
typedef struct Mp4DemuxSample {
  /**
   * サンプルが属するトラックの情報へのポインタ
   *
   * このポインタの参照先には対応する demuxer インスタンスが有効な間のみアクセス可能である
   */
  const struct Mp4DemuxTrackInfo *track;
  /**
   * サンプルの詳細情報（コーデック設定など）へのポインタ
   *
   * 値が NULL の場合は「サンプルエントリーの内容が前のサンプルと同じ」であることを意味する
   *
   * このポインタの参照先には対応する demuxer インスタンスが有効な間のみアクセス可能である
   */
  const struct Mp4SampleEntry *sample_entry;
  /**
   * このサンプルがキーフレームであるかの判定
   *
   * `true` の場合、このサンプルはキーフレームであり、このポイントから復号を開始できる
   *
   * 音声の場合には、通常はすべてのサンプルがキーフレーム扱いとなる
   */
  bool keyframe;
  /**
   * サンプルのタイムスタンプ（タイムスケール単位）
   *
   * 実際の時間（秒単位）を得るには、この値を対応する `Mp4DemuxTrackInfo` の
   * `timescale` で除算すること
   */
  uint64_t timestamp;
  /**
   * サンプルの尺（タイムスケール単位）
   *
   * 実際の時間（秒単位）を得るには、この値を対応する `Mp4DemuxTrackInfo` の
   * `timescale` で除算すること
   */
  uint32_t duration;
  /**
   * コンポジション時間オフセットが存在するかどうか
   */
  bool has_composition_time_offset;
  /**
   * コンポジション時間オフセット（タイムスケール単位）
   *
   * `has_composition_time_offset` が true の場合のみ有効。
   * PTS = timestamp + composition_time_offset で計算できる。
   *
   * 通常 MP4 の `ctts` と fMP4 の `trun` の両方を共通の sample 型で扱うため、
   * `i64` で公開している。
   * 仕様上すべての入力が 64 bit 必須という意味ではない。
   */
  int64_t composition_time_offset;
  /**
   * ファイル内におけるサンプルデータの開始位置（バイト単位）
   *
   * file demuxer ではファイル先頭からの絶対位置、
   * segment demuxer では `fmp4_segment_demuxer_handle_media_segment()` に渡した
   * 入力バッファ先頭からの相対位置を表す。
   */
  uint64_t data_offset;
  /**
   * サンプルデータのサイズ（バイト単位）
   *
   * `data_offset` から `data_offset + data_size` までの範囲がサンプルデータとなる
   */
  uintptr_t data_size;
} Mp4DemuxSample;

/**
 * fMP4 Muxer 生成時のオプションを表す C 構造体
 */
typedef struct Fmp4SegmentMuxerOptions {
  /**
   * ファイル作成時刻（UNIX エポックからの秒数）
   */
  uint64_t creation_timestamp_secs;
} Fmp4SegmentMuxerOptions;

/**
 * fMP4 メディアセグメントに追加するサンプルを表す C 構造体
 */
typedef struct Fmp4SegmentSample {
  /**
   * トラックの種別
   */
  enum Mp4TrackKind track_kind;
  /**
   * タイムスケール（0 は無効）
   */
  uint32_t timescale;
  /**
   * サンプルの詳細情報（コーデック情報）
   *
   * 最初のサンプルでは必須。以後、同じトラックで変更がなければ NULL を指定できる。
   */
  const struct Mp4SampleEntry *sample_entry;
  /**
   * サンプルの尺（トラックのタイムスケール単位）
   */
  uint32_t duration;
  /**
   * キーフレームかどうか
   */
  bool keyframe;
  /**
   * コンポジション時間オフセットが有効かどうか
   */
  bool has_composition_time_offset;
  /**
   * コンポジション時間オフセット（`has_composition_time_offset` が true の場合のみ有効）
   *
   * demux API と合わせて `i64` で公開している。
   * ただし fMP4 の `trun` に書けるのは `i32::MIN ..= i32::MAX` の範囲だけであり、
   * 範囲外の値を指定すると mux 関数はエラーを返す。
   */
  int64_t composition_time_offset;
  /**
   * セグメント内の `mdat` payload 領域先頭から見たサンプルデータの相対オフセット
   *
   * `fmp4_segment_muxer_write_media_segment_metadata()` の返り値には payload 自体は含まれない。
   * 呼び出し側は返された `moof + mdat header` の直後に、
   * ここで指定した位置関係になるよう payload を配置する必要がある。
   *
   * 同じトラックに属するサンプル群は、`data_offset` の昇順で
   * 隙間なく連続した 1 区間に配置されている必要がある。
   * 複数トラックを含む場合は、トラックごとの区間同士も隙間なく並んでいる必要がある。
   */
  uint64_t data_offset;
  /**
   * サンプルデータのサイズ（バイト単位）
   */
  uint32_t data_size;
} Fmp4SegmentSample;

/**
 * MP4 ファイルに追加（マルチプレックス）するメディアサンプルを表す構造体
 *
 * # 使用例
 *
 * ```c
 * // H.264 ビデオサンプルを作成
 * Mp4MuxSample video_sample = {
 *     .track_kind = MP4_TRACK_KIND_VIDEO,
 *     .sample_entry = &avc1_entry,
 *     .keyframe = true,
 *     .timescale = 30,
 *     .duration = 1,  // 30 FPS
 *     .has_composition_time_offset = false,
 *     .composition_time_offset = 0,
 *     .data_offset = 1024,
 *     .data_size = 4096,
 * };
 *
 * // Opus 音声サンプルを作成
 * Mp4MuxSample audio_sample = {
 *     .track_kind = MP4_TRACK_KIND_AUDIO,
 *     .sample_entry = &opus_entry,
 *     .keyframe = true,  // 音声では通常は常に true
 *     .timescale = 1000,
 *     .duration = 20,  // 20 ms
 *     .has_composition_time_offset = false,
 *     .composition_time_offset = 0,
 *     .data_offset = 5120,
 *     .data_size = 256,
 * };
 * ```
 */
typedef struct Mp4MuxSample {
  /**
   * サンプルが属するトラックの種別
   */
  enum Mp4TrackKind track_kind;
  /**
   * サンプルの詳細情報（コーデック種別など）へのポインタ
   *
   * 最初のサンプルでは必須
   *
   * 以降は（コーデック設定に変更がない間は）省略可能で、NULL が渡された場合は前のサンプルと同じ値が使用される
   */
  const struct Mp4SampleEntry *sample_entry;
  /**
   * キーフレームであるかどうか
   *
   * `true` の場合、このサンプルはキーフレームであり、
   * このポイントから復号（再生）を開始できることを意味する
   */
  bool keyframe;
  /**
   * サンプルのタイムスケール（時間単位）
   *
   * `duration` フィールドの値は、このタイムスケール単位での長さを表す
   *
   * # Examples
   *
   * - 映像サンプル（30 fps）: `timescale = 30` なら `duration = 1` は 1/30 秒
   * - 音声サンプル（48 kHz）: `timescale = 48000` なら `duration = 1920` は 1920/48000 秒
   *
   * # NOTE
   *
   * 同じトラック内のすべてのサンプルは同じタイムスケール値を使用する必要がある
   *
   * 異なるタイムスケール値を指定すると
   * `mp4_file_muxer_append_sample()` 呼び出し時に `MP4_ERROR_INVALID_INPUT` エラーが発生する
   */
  uint32_t timescale;
  /**
   * サンプルの尺（タイムスケール単位）
   *
   * # サンプルのタイムスタンプについて
   *
   * MP4 ではサンプルのタイムスタンプを直接指定する方法がなく、
   * あるサンプルのタイムスタンプは「それ以前のサンプルの尺の累積」として表現される
   *
   * そのため、映像および音声サンプルの冒頭ないし途中でタイムスタンプのギャップが発生する場合には
   * 利用側で以下のような対処が求められる:
   * - 映像:
   *   - 黒画像などを生成してギャップ分を補完するか、サンプルの尺を調整する
   *   - たとえば、ギャップが発生した直前のサンプルの尺にギャップ期間分を加算する
   * - 音声:
   *   - 無音などを生成してギャップ分を補完する
   *   - 音声はサンプルデータに対する尺の長さが固定なので、映像のように MP4 レイヤーで尺の調整はできない
   *
   * なお、MP4 の枠組みでもギャップを表現するためのボックスは存在するが
   * プレイヤーの対応がまちまちであるため `Mp4FileMuxer` では現状サポートしておらず、
   * 上述のような個々のプレイヤーの実装への依存性が低い方法を推奨している
   */
  uint32_t duration;
  /**
   * コンポジション時間オフセットが有効かどうか
   *
   * `true` の場合、`composition_time_offset` を用いて `ctts` ボックスが生成される
   */
  bool has_composition_time_offset;
  /**
   * コンポジション時間オフセット（トラックのタイムスケール単位）
   *
   * `has_composition_time_offset` が true の場合のみ有効。
   * 値の意味は `PTS = DTS + composition_time_offset` である。
   *
   * demux API と往復しやすいように `i64` で公開しているが、
   * 実際に mux 時に受理される範囲は次の通り:
   * - file mux: `i32::MIN ..= u32::MAX`
   * - fMP4 segment mux: `i32::MIN ..= i32::MAX`
   *
   * 範囲外の値を指定した場合、対応する mux 関数はエラーを返す。
   */
  int64_t composition_time_offset;
  /**
   * 出力ファイル内におけるサンプルデータの開始位置（バイト単位）
   */
  uint64_t data_offset;
  /**
   * サンプルデータのサイズ（バイト単位）
   */
  uint32_t data_size;
} Mp4MuxSample;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * ライブラリのバージョンを取得する
 *
 * # 戻り値
 *
 * バージョン文字列へのポインタ（NULL終端）
 */
const char *mp4_library_version(void);

/**
 * 新しい `Mp4FileDemuxer` インスタンスを作成して、それへのポインタを返す
 *
 * この関数が返したポインタは、使用後に `mp4_file_demuxer_free()` で破棄する必要がある
 *
 * # 使用例
 *
 * ```c
 * // Mp4FileDemuxer インスタンスを生成
 * Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
 * if (demuxer == NULL) {
 *     fprintf(stderr, "Failed to create demuxer\n");
 *     return;
 * }
 *
 * // 処理を実行...
 *
 * // リソース解放
 * mp4_file_demuxer_free(demuxer);
 * ```
 */
struct Mp4FileDemuxer *mp4_file_demuxer_new(void);

/**
 * `Mp4FileDemuxer` インスタンスを破棄して、割り当てられたリソースを解放する
 *
 * この関数は、`mp4_file_demuxer_new()` で作成された `Mp4FileDemuxer` インスタンスを破棄し、
 * その内部で割り当てられたすべてのメモリを解放する。
 *
 * # 引数
 *
 * - `demuxer`: 破棄する `Mp4FileDemuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、この関数は何もしない
 */
void mp4_file_demuxer_free(struct Mp4FileDemuxer *demuxer);

/**
 * `Mp4FileDemuxer` で最後に発生したエラーのメッセージを取得する
 *
 * この関数は、デマルチプレックス処理中に発生した最後のエラーのメッセージ（NULL 終端）を返す
 *
 * エラーが発生していない場合は、空文字列へのポインタを返す
 *
 * # 引数
 *
 * - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
 *
 * # 戻り値
 *
 *
 * - メッセージが存在する場合: NULL 終端のエラーメッセージへのポインタ
 * - メッセージが存在しない場合: NULL 終端の空文字列へのポインタ
 * - `demuxer` 引数が NULL の場合: NULL 終端の空文字列へのポインタ
 *
 * # 使用例
 *
 * ```c
 * Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
 *
 * Mp4Error ret = // なんらかの処理;
 *
 * // エラーが発生した場合、メッセージを取得
 * if (ret != MP4_ERROR_OK) {
 *     const char *error_msg = mp4_file_demuxer_get_last_error(demuxer);
 *     fprintf(stderr, "エラー: %s\n", error_msg);
 * }
 * ```
 */
const char *mp4_file_demuxer_get_last_error(const struct Mp4FileDemuxer *demuxer);

/**
 * `Mp4FileDemuxer` で次の処理を進めるために必要な I/O の位置とサイズを取得する
 *
 * この関数は、処理を進めるために必要な I/O がない場合には `out_required_input_size` に 0 を設定して返し、
 * それ以外の場合は、ファイルから読み込む必要があるデータの位置とサイズを出力引数に設定して返す
 *
 * この関数から取得した位置とサイズの情報をもとに、呼び出し元がファイルなどからデータを読み込み、
 * `mp4_file_demuxer_handle_input()` に渡す必要がある
 *
 * なお、現在の `Mp4FileDemuxer` の実装は fragmented MP4 には対応していないため、
 * サンプルの取得に必要なメタデータ（moovボックス）の読み込み（初期化）が終わったら、
 * 以後はこの関数が追加の入力データを要求することはない
 *
 * # 引数
 *
 * - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * - `out_required_input_position`: 必要なデータの開始位置（バイト単位）を受け取るポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * - `out_required_input_size`: 必要なデータのサイズ（バイト単位）を受け取るポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *   - なお、ここに設定されるサイズはあくまでもヒントであり、厳密に一致したサイズのデータを提供する必要はない
 *     - 通常は、より大きな範囲のデータを一度に渡した方が効率がいい
 *   - 0 が設定された場合は、これ以上の入力データが不要であることを意味する
 *   - -1 が設定された場合は、ファイルの末尾までのデータが必要であることを意味する
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に処理された
 * - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
 *
 * # 使用例
 *
 * ```c
 * Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
 * FILE *fp = fopen("input.mp4", "rb");
 *
 * // 初期化が完了するまでループ
 * while (true) {
 *     uint64_t required_pos;
 *     int32_t required_size;
 *     mp4_file_demuxer_get_required_input(demuxer, &required_pos, &required_size);
 *     if (required_size == 0) break; // 初期化完了
 *
 *     // ファイルから必要なデータを読み込む
 *     //
 *     // NOTE: 実際には `required_size == -1` の場合には、ファイル末尾までを読み込む必要がある
 *     uint8_t buffer[required_size];
 *     fseek(fp, required_pos, SEEK_SET);
 *     size_t bytes_read = fread(buffer, 1, required_size, fp);
 *
 *     // demuxer にデータを供給
 *     mp4_file_demuxer_handle_input(demuxer, required_pos, buffer, bytes_read);
 * }
 * ```
 */
enum Mp4Error mp4_file_demuxer_get_required_input(struct Mp4FileDemuxer *demuxer,
                                                  uint64_t *out_required_input_position,
                                                  int32_t *out_required_input_size);

/**
 * `Mp4FileDemuxer` にファイルデータを入力として供給し、デマルチプレックス処理を進める
 *
 * この関数は、`mp4_file_demuxer_get_required_input()` で取得した位置に対応するファイルデータを
 * 受け取り、デマルチプレックス処理を進める
 *
 * なお、この関数はデータの部分的な消費を行わないため、呼び出し元が必要なデータを一度に全て渡す必要がある
 * （固定長のバッファを使って複数回に分けてデータを供給することはできない）
 *
 * # 引数
 *
 * - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * - `input_position`: 入力データがファイル内で始まる位置（バイト単位）
 *   - `mp4_file_demuxer_get_required_input()` で取得した位置と一致していることが期待される
 *
 * - `input_data`: ファイルデータのバッファへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * - `input_data_size`: 入力データのサイズ（バイト単位）
 *   - 0 以上の値を指定する必要がある
 *   - `mp4_file_demuxer_get_required_input()` で取得したサイズより大きいサイズを指定することは問題ない
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に入力データが受け取られた
 *   - この場合でも `mp4_file_demuxer_get_required_input()` を使って、追加の入力が必要かどうかを確認する必要がある
 * - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
 *
 * # エラー状態への遷移
 *
 * 入力データの内容や範囲が不正な場合には `Mp4FileDemuxer` はエラー状態に遷移する。
 *
 * これは以下のようなケースで発生する:
 * - `input_position` が `mp4_file_demuxer_get_required_input()` で取得した位置と異なる
 * - `input_data_size` が要求されたサイズより不足している
 * - 入力ファイルデータが MP4 形式として不正である（ボックスのデコード失敗など）
 * - サポートされていないコーデックが使用されている
 *
 * エラー状態に遷移した後は、
 * - `mp4_file_demuxer_get_required_input()` は `out_required_input_size` に 0 を設定する
 * - `mp4_file_demuxer_get_tracks()` および `mp4_file_demuxer_next_sample()` の呼び出しはエラーを返す
 * - `mp4_file_demuxer_get_last_error()` でエラーメッセージを確認できる
 */
enum Mp4Error mp4_file_demuxer_handle_input(struct Mp4FileDemuxer *demuxer,
                                            uint64_t input_position,
                                            const uint8_t *input_data,
                                            uint32_t input_data_size);

/**
 * MP4 ファイル内に含まれるすべてのメディアトラック（音声および映像）の情報を取得する
 *
 * # 引数
 *
 * - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * - `out_tracks`: 取得したトラック情報の配列へのポインタを受け取るポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *   - このポインタの参照先には `Mp4FileDemuxer` インスタンスが有効な間のみアクセス可能である
 *
 * - `out_track_count`: トラック情報の個数を受け取るポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *   - MP4 ファイルにトラックが含まれていない場合は 0 が設定される
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常にトラック情報が取得された
 * - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
 * - `MP4_ERROR_INPUT_REQUIRED`: 初期化に必要な入力データが不足している
 *   - `mp4_file_demuxer_get_required_input()` および `mp4_file_demuxer_handle_input()` のハンドリングが必要
 * - その他のエラー: 入力ファイルが破損していたり、未対応のコーデックを含んでいる場合
 *
 * # 使用例
 *
 * ```c
 * Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
 *
 * // ファイルデータを供給（省略）...
 *
 * // トラック情報を取得
 * const Mp4DemuxTrackInfo *tracks;
 * uint32_t track_count;
 * Mp4Error ret = mp4_file_demuxer_get_tracks(demuxer, &tracks, &track_count);
 *
 * if (ret == MP4_ERROR_OK) {
 *    printf("Found %u tracks\n", track_count);
 *    for (uint32_t i = 0; i < track_count; i++) {
 *        printf("Track %u: ID=%u, Kind=%d, Duration=%lu, Timescale=%u\n",
 *               i, tracks[i].track_id, tracks[i].kind,
 *               tracks[i].duration, tracks[i].timescale);
 *    }
 * } else {
 *    fprintf(stderr, "Error: %d - %s\n", ret, mp4_file_demuxer_get_last_error(demuxer));
 * }
 * ```
 */
enum Mp4Error mp4_file_demuxer_get_tracks(struct Mp4FileDemuxer *demuxer,
                                          const struct Mp4DemuxTrackInfo **out_tracks,
                                          uint32_t *out_track_count);

/**
 * MP4 ファイルから時系列順に次のメディアサンプルを取得する
 *
 * すべてのトラックから、まだ取得していないもののなかで、
 * 最も早いタイムスタンプを持つサンプルを返す
 *
 * ファイルの先頭に達した場合は `MP4_ERROR_NO_MORE_SAMPLES` が返される
 *
 * # サンプルデータの読み込みについて
 *
 * この関数は、サンプルのメタデータ（タイムスタンプ、サイズ、ファイル内の位置など）のみを返すので、
 * 実際のサンプルデータ（音声フレームや映像フレーム）の読み込みは呼び出し元の責務となる
 *
 * サンプルデータを処理する場合には、返された `Mp4DemuxSample` の `data_offset` と `data_size` フィールドを使用して、
 * 入力ファイルから直接サンプルデータを読み込む必要がある
 *
 * # 引数
 *
 * - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * - `out_sample`: 取得したサンプル情報を受け取るポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常にサンプルが取得された
 * - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
 * - `MP4_ERROR_NO_MORE_SAMPLES`: すべてのサンプルを取得し終えた
 * - `MP4_ERROR_INPUT_REQUIRED`: 初期化に必要な入力データが不足している
 *   - `mp4_file_demuxer_get_required_input()` および `mp4_file_demuxer_handle_input()` のハンドリングが必要
 * - その他のエラー: 入力ファイルが破損していたり、未対応のコーデックを含んでいる場合
 *
 * # 使用例
 *
 * ```c
 * FILE *fp = fopen("input.mp4", "rb");
 * Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
 *
 * // ファイルデータを供給して初期化（省略）...
 *
 * // 時系列順にサンプルを取得
 * Mp4DemuxSample sample;
 * while (mp4_file_demuxer_next_sample(demuxer, &sample) == MP4_ERROR_OK) {
 *     printf("サンプル - トラックID: %u, タイムスタンプ: %lu, サイズ: %zu バイト\n",
 *            sample.track->track_id, sample.timestamp, sample.data_size);
 *
 *     // サンプルデータを入力ファイルから読み込む
 *     uint8_t sample_data[sample.data_size];
 *     fseek(fp, sample.data_offset, SEEK_SET);
 *     fread(sample_data, 1, sample.data_size, fp);
 *
 *     // サンプルを処理...
 * }
 * ```
 */
enum Mp4Error mp4_file_demuxer_next_sample(struct Mp4FileDemuxer *demuxer,
                                           struct Mp4DemuxSample *out_sample);

/**
 * MP4 ファイルから時系列順に前のメディアサンプルを取得する
 *
 * すべてのトラックのうち、現在位置より前にあるサンプルから、
 * 最も遅いタイムスタンプのものを返す
 *
 * ファイルの先頭に達した場合は `MP4_ERROR_NO_MORE_SAMPLES` が返される
 *
 * # サンプルデータの読み込みについて
 *
 * この関数は、サンプルのメタデータ（タイムスタンプ、サイズ、ファイル内の位置など）のみを返すので、
 * 実際のサンプルデータ（音声フレームや映像フレーム）の読み込みは呼び出し元の責務となる
 *
 * サンプルデータを処理する場合には、返された `Mp4DemuxSample` の `data_offset` と `data_size` フィールドを使用して、
 * 入力ファイルから直接サンプルデータを読み込む必要がある
 *
 * # 引数
 *
 * - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * - `out_sample`: 取得したサンプル情報を受け取るポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常にサンプルが取得された
 * - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
 * - `MP4_ERROR_NO_MORE_SAMPLES`: ファイルの先頭に達した
 * - `MP4_ERROR_INPUT_REQUIRED`: 初期化に必要な入力データが不足している
 *   - `mp4_file_demuxer_get_required_input()` および `mp4_file_demuxer_handle_input()` のハンドリングが必要
 * - その他のエラー: 入力ファイルが破損していたり、未対応のコーデックを含んでいる場合
 */
enum Mp4Error mp4_file_demuxer_prev_sample(struct Mp4FileDemuxer *demuxer,
                                           struct Mp4DemuxSample *out_sample);

/**
 * MP4 ファイルの指定時刻にシークする
 *
 * 各トラックで指定時刻を含むサンプルを選び、次回の `mp4_file_demuxer_next_sample()` が
 * その位置から開始されるようにする
 *
 * 同一タイムスタンプのサンプルが複数ある場合は、シーク後の `mp4_file_demuxer_next_sample()` の走査対象に含まれる
 *
 * # 引数
 *
 * - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 * - `timestamp`: シーク先の時刻を表すタイムスタンプ値（単位は `timescale` で指定）
 *   - 実際の秒数は `timestamp / timescale` で計算される
 * - `timescale`: タイムスケール（1 秒間の単位数）
 *   - 0 の場合は `MP4_ERROR_INVALID_INPUT` が返される
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常にシークできた
 * - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
 * - `MP4_ERROR_INVALID_INPUT`: 引数の値が不正
 * - `MP4_ERROR_INPUT_REQUIRED`: 初期化に必要な入力データが不足している
 *   - `mp4_file_demuxer_get_required_input()` および `mp4_file_demuxer_handle_input()` のハンドリングが必要
 * - その他のエラー: 入力ファイルが破損していたり、未対応のコーデックを含んでいる場合
 */
enum Mp4Error mp4_file_demuxer_seek(struct Mp4FileDemuxer *demuxer,
                                    uint64_t timestamp,
                                    uint32_t timescale);

/**
 * 新しい `Fmp4SegmentDemuxer` インスタンスを生成する
 *
 * # 戻り値
 *
 * インスタンスへのポインタ（返されたポインタは `fmp4_segment_demuxer_free()` で解放する）
 */
struct Fmp4SegmentDemuxer *fmp4_segment_demuxer_new(void);

/**
 * `Fmp4SegmentDemuxer` インスタンスを破棄してリソースを解放する
 *
 * # 引数
 *
 * - `demuxer`: 破棄するインスタンスへのポインタ（NULL の場合は何もしない）
 */
void fmp4_segment_demuxer_free(struct Fmp4SegmentDemuxer *demuxer);

/**
 * 最後に発生したエラーのメッセージを取得する
 *
 * # 引数
 *
 * - `demuxer`: インスタンスへのポインタ
 *
 * # 戻り値
 *
 * NULL 終端のエラーメッセージへのポインタ（エラーがない場合は空文字列）
 */
const char *fmp4_segment_demuxer_get_last_error(const struct Fmp4SegmentDemuxer *demuxer);

/**
 * 初期化セグメント（`ftyp` + `moov`）を処理してトラック情報を初期化する
 *
 * # 引数
 *
 * - `demuxer`: インスタンスへのポインタ
 * - `data`: 初期化セグメントデータへのポインタ
 * - `size`: データのサイズ（バイト単位）
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に処理された
 * - `MP4_ERROR_INVALID_STATE`: 既に初期化済み
 * - その他のエラー: 処理に失敗した
 */
enum Mp4Error fmp4_segment_demuxer_handle_init_segment(struct Fmp4SegmentDemuxer *demuxer,
                                                       const uint8_t *data,
                                                       uint32_t size);

/**
 * 初期化済みのトラック情報を取得する
 *
 * # 引数
 *
 * - `demuxer`: インスタンスへのポインタ
 * - `out_tracks`: トラック情報配列へのポインタを受け取るポインタ
 *   - このポインタの参照先は `demuxer` インスタンスが有効な間のみアクセス可能
 * - `out_count`: トラック数を受け取るポインタ
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に取得された
 * - `MP4_ERROR_INVALID_STATE`: 未初期化
 * - その他のエラー: 取得に失敗した
 */
enum Mp4Error fmp4_segment_demuxer_get_tracks(struct Fmp4SegmentDemuxer *demuxer,
                                              const struct Mp4DemuxTrackInfo **out_tracks,
                                              uint32_t *out_count);

/**
 * メディアセグメント（`moof` + `mdat` または `sidx` + `moof` + `mdat`）を処理して
 * サンプルの配列を返す
 *
 * # 引数
 *
 * - `demuxer`: インスタンスへのポインタ
 * - `data`: メディアセグメントデータへのポインタ
 * - `size`: データのサイズ（バイト単位）
 * - `out_samples`: 生成されたサンプル配列へのポインタを受け取るポインタ
 *   - 返された配列は `fmp4_segment_demuxer_free_samples()` で解放する必要がある
 * - `out_count`: サンプル数を受け取るポインタ
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に処理された
 * - `MP4_ERROR_INVALID_STATE`: 未初期化
 * - その他のエラー: 処理に失敗した
 */
enum Mp4Error fmp4_segment_demuxer_handle_media_segment(struct Fmp4SegmentDemuxer *demuxer,
                                                        const uint8_t *data,
                                                        uint32_t size,
                                                        struct Mp4DemuxSample **out_samples,
                                                        uint32_t *out_count);

/**
 * `fmp4_segment_demuxer_handle_media_segment()` で割り当てられたサンプル配列を解放する
 *
 * # 引数
 *
 * - `samples`: 解放するサンプル配列へのポインタ（NULL の場合は何もしない）
 * - `count`: サンプル数
 */
void fmp4_segment_demuxer_free_samples(struct Mp4DemuxSample *samples,
                                       uint32_t count);

/**
 * 新しい `Fmp4SegmentMuxer` インスタンスを生成する
 *
 * デフォルトオプションを使用する。
 *
 * # 引数
 *
 * # 戻り値
 *
 * 成功時はインスタンスへのポインタ、失敗時は NULL
 *
 * 返されたポインタは `fmp4_segment_muxer_free()` で解放する必要がある
 */
struct Fmp4SegmentMuxer *fmp4_segment_muxer_new(void);

/**
 * オプションを指定して新しい `Fmp4SegmentMuxer` インスタンスを生成する
 *
 * # 引数
 *
 * - `options`: オプションへのポインタ
 *   - NULL の場合はデフォルトオプションを使う
 *
 * # 戻り値
 *
 * 成功時はインスタンスへのポインタ、失敗時は NULL
 *
 * 返されたポインタは `fmp4_segment_muxer_free()` で解放する必要がある
 */
struct Fmp4SegmentMuxer *fmp4_segment_muxer_new_with_options(const struct Fmp4SegmentMuxerOptions *options);

/**
 * `Fmp4SegmentMuxer` インスタンスを破棄してリソースを解放する
 *
 * # 引数
 *
 * - `muxer`: 破棄するインスタンスへのポインタ（NULL の場合は何もしない）
 */
void fmp4_segment_muxer_free(struct Fmp4SegmentMuxer *muxer);

/**
 * 最後に発生したエラーのメッセージを取得する
 *
 * # 引数
 *
 * - `muxer`: インスタンスへのポインタ
 *
 * # 戻り値
 *
 * NULL 終端のエラーメッセージへのポインタ（エラーがない場合は空文字列）
 */
const char *fmp4_segment_muxer_get_last_error(const struct Fmp4SegmentMuxer *muxer);

/**
 * 初期化セグメント（`ftyp` + `moov`）のバイト列を生成する
 *
 * 返される init segment には、この関数を呼んだ時点までに
 * `fmp4_segment_muxer_write_media_segment_metadata()` ないし
 * `fmp4_segment_muxer_write_media_segment_metadata_with_sidx()` で観測したトラック情報と
 * sample entry が反映される。
 *
 * まだどのトラックも観測されていない状態ではエラーになる。
 *
 * # 引数
 *
 * - `muxer`: インスタンスへのポインタ
 * - `out_data`: 生成されたバイト列へのポインタを受け取るポインタ
 *   - 返されたポインタは `fmp4_bytes_free()` で解放する必要がある
 * - `out_size`: バイト列のサイズを受け取るポインタ
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に生成された
 * - その他のエラー: 生成に失敗した
 */
enum Mp4Error fmp4_segment_muxer_write_init_segment(struct Fmp4SegmentMuxer *muxer,
                                                    uint8_t **out_data,
                                                    uint32_t *out_size);

/**
 * メディアセグメント先頭のメタデータ（`moof` + `mdat` ヘッダー）のバイト列を生成する
 *
 * 返り値には `mdat` payload 自体は含まれない。
 * 呼び出し側は、この関数が返したバイト列の直後に
 * `Fmp4SegmentSample.data_offset` / `data_size` が示す payload を自前で配置すること。
 * その際、各トラックの payload はトラック単位で連続した 1 区間にまとめ、
 * トラック区間同士も `data_offset` 順に隙間なく並べる必要がある。
 *
 * # 引数
 *
 * - `muxer`: インスタンスへのポインタ
 * - `samples`: サンプル配列へのポインタ
 * - `sample_count`: サンプル数
 * - `out_data`: 生成されたバイト列へのポインタを受け取るポインタ
 *   - 返されたポインタは `fmp4_bytes_free()` で解放する必要がある
 * - `out_size`: バイト列のサイズを受け取るポインタ
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に生成された
 * - その他のエラー: 生成に失敗した
 */
enum Mp4Error fmp4_segment_muxer_write_media_segment_metadata(struct Fmp4SegmentMuxer *muxer,
                                                              const struct Fmp4SegmentSample *samples,
                                                              uint32_t sample_count,
                                                              uint8_t **out_data,
                                                              uint32_t *out_size);

/**
 * `sidx` ボックス付きのメディアセグメント先頭メタデータを生成する
 *
 * `fmp4_segment_muxer_write_media_segment_metadata()` と同じだが、先頭に `sidx` ボックスが付加される。
 * 返り値は `sidx + moof + mdat` ヘッダーであり、payload は含まれない。
 * payload 配置に関する制約も `fmp4_segment_muxer_write_media_segment_metadata()` と同じである。
 *
 * # 引数
 *
 * `fmp4_segment_muxer_write_media_segment_metadata()` と同じ
 */
enum Mp4Error fmp4_segment_muxer_write_media_segment_metadata_with_sidx(struct Fmp4SegmentMuxer *muxer,
                                                                        const struct Fmp4SegmentSample *samples,
                                                                        uint32_t sample_count,
                                                                        uint8_t **out_data,
                                                                        uint32_t *out_size);

/**
 * ランダムアクセスインデックス（`mfra`）のバイト列を生成する
 *
 * `mfra` はファイル末尾に付加することで、fragmented MP4 のランダムアクセスを補助する。
 * `fmp4_segment_muxer_write_init_segment()` と
 * `fmp4_segment_muxer_write_media_segment_metadata()` ないし
 * `fmp4_segment_muxer_write_media_segment_metadata_with_sidx()` を呼び出した後に使うこと。
 *
 * `tfra.moof_offset` は、この関数を呼んだ時点での init segment サイズを基準に計算される。
 * したがって、実際に `mfra` を付加するファイルでは、
 * この関数と同じ時点の init segment を先頭に配置する必要がある。
 * 途中で観測済みトラックや sample entry が増えて init segment が変わり得る場合は、
 * 最終的に先頭へ配置する init segment を確定させた後でこの関数を呼ぶこと。
 *
 * # 引数
 *
 * - `muxer`: インスタンスへのポインタ
 * - `out_data`: 生成されたバイト列へのポインタを受け取るポインタ
 *   - 返されたポインタは `fmp4_bytes_free()` で解放する必要がある
 * - `out_size`: バイト列のサイズを受け取るポインタ
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に生成された
 * - その他のエラー: 生成に失敗した
 */
enum Mp4Error fmp4_segment_muxer_write_mfra(struct Fmp4SegmentMuxer *muxer,
                                            uint8_t **out_data,
                                            uint32_t *out_size);

/**
 * `fmp4_segment_muxer_write_init_segment()` や `fmp4_segment_muxer_write_media_segment_metadata()` で
 * 割り当てられたバイト列を解放する
 *
 * # 引数
 *
 * - `data`: 解放するバイト列へのポインタ（NULL の場合は何もしない）
 * - `size`: バイト列のサイズ
 */
void fmp4_bytes_free(uint8_t *data,
                     uint32_t size);

/**
 * 新しい `Mp4FileKindDetector` インスタンスを生成する
 *
 * 返されたポインタは、使用後に `mp4_file_kind_detector_free()` で解放する必要がある。
 */
struct Mp4FileKindDetector *mp4_file_kind_detector_new(void);

/**
 * `Mp4FileKindDetector` インスタンスを破棄してリソースを解放する
 *
 * NULL が渡された場合は何もしない。
 */
void mp4_file_kind_detector_free(struct Mp4FileKindDetector *detector);

/**
 * 最後に発生したエラーメッセージを取得する
 *
 * エラーが発生していない場合は空文字列を返す。
 */
const char *mp4_file_kind_detector_get_last_error(const struct Mp4FileKindDetector *detector);

/**
 * 次の判定に必要な入力データの位置とサイズを取得する
 *
 * `out_required_input_size` には以下のいずれかが設定される:
 * - 0: 追加の入力が不要
 * - -1: ファイル末尾までの入力が必要
 * - それ以外の正値: そのサイズ以上の入力が必要
 *
 * ここで大きなサイズが要求されるのは実質的には `moov` ボックス本体であり、
 * `mdat` のような巨大ペイロードを丸ごと要求することはない想定である。
 * そのため、サイズ表現には `int32_t` を使っている。
 *
 * 判定器がエラー状態に遷移している場合は `MP4_ERROR_OK` ではなくエラーを返す。
 */
enum Mp4Error mp4_file_kind_detector_get_required_input(struct Mp4FileKindDetector *detector,
                                                        uint64_t *out_required_input_position,
                                                        int32_t *out_required_input_size);

/**
 * 入力データを供給して判定処理を進める
 *
 * `mp4_file_kind_detector_get_required_input()` が返した要求に従って入力を渡すこと。
 *
 * EOF を通知する場合には、要求された `input_position` に対して
 * `input_data = NULL` かつ `input_data_size = 0` を渡す。
 *
 * 入力が不正で判定器がエラー状態に遷移した場合は、その場でエラーを返す。
 */
enum Mp4Error mp4_file_kind_detector_handle_input(struct Mp4FileKindDetector *detector,
                                                  uint64_t input_position,
                                                  const uint8_t *input_data,
                                                  uint32_t input_data_size);

/**
 * 判定結果を取得する
 *
 * 戻り値が `MP4_ERROR_OK` の場合にのみ `out_kind` が有効になる。
 * まだ追加入力が必要な場合は `MP4_ERROR_INPUT_REQUIRED` が返る。
 */
enum Mp4Error mp4_file_kind_detector_get_file_kind(struct Mp4FileKindDetector *detector,
                                                   enum Mp4FileKind *out_kind);

/**
 * 構築する MP4 ファイルの moov ボックスの最大サイズを見積もるための関数
 *
 * この関数を使うことで `mp4_file_muxer_set_reserved_moov_box_size()` で指定する値を簡易的に決定することができる
 *
 * # 引数
 *
 * - `audio_sample_count`: 音声トラック内の予想サンプル数
 * - `video_sample_count`: 映像トラック内の予想サンプル数
 *
 * # 戻り値
 *
 * moov ボックスに必要な最大バイト数を返す
 *
 * # 使用例
 *
 * ```c
 * // 音声 1000 サンプル、映像 3000 フレームの場合
 * uint32_t required_size = mp4_estimate_maximum_moov_box_size(1000, 3000);
 * mp4_file_muxer_set_reserved_moov_box_size(muxer, required_size);
 * ```
 */
uint32_t mp4_estimate_maximum_moov_box_size(uint32_t audio_sample_count,
                                            uint32_t video_sample_count);

/**
 * 新しい `Mp4FileMuxer` インスタンスを作成して、それへのポインタを返す
 *
 * 返されたポインタは、使用後に `mp4_file_muxer_free()` で破棄する必要がある
 *
 * # 戻り値
 *
 * 新しく作成された `Mp4FileMuxer` インスタンスへのポインタ
 * （現在の実装では NULL ポインタが返されることはない）
 *
 * # 関連関数
 *
 * - `mp4_file_muxer_free()`: インスタンスを破棄してリソースを解放する
 * - `mp4_file_muxer_initialize()`: マルチプレックス処理を初期化する
 * - `mp4_file_muxer_set_reserved_moov_box_size()`: faststart 用に moov ボックスサイズを設定する
 *
 * # 使用例
 *
 * ```c
 * // Mp4FileMuxer インスタンスを生成
 * Mp4FileMuxer *muxer = mp4_file_muxer_new();
 *
 * // オプションを設定
 * mp4_file_muxer_set_reserved_moov_box_size(muxer, 8192);
 *
 * // マルチプレックス処理を初期化
 * Mp4Error ret = mp4_file_muxer_initialize(muxer);
 * if (ret != MP4_ERROR_OK) {
 *     fprintf(stderr, "初期化失敗: %s\n", mp4_file_muxer_get_last_error(muxer));
 *     mp4_file_muxer_free(muxer);
 *     return 1;
 * }
 *
 * // サンプルを追加...（省略）
 *
 * // マルチプレックス処理を完了
 * ret = mp4_file_muxer_finalize(muxer);
 * if (ret != MP4_ERROR_OK) {
 *     fprintf(stderr, "ファイナライズ失敗: %s\n", mp4_file_muxer_get_last_error(muxer));
 *     mp4_file_muxer_free(muxer);
 *     return 1;
 * }
 *
 * // リソース解放
 * mp4_file_muxer_free(muxer);
 * ```
 */
struct Mp4FileMuxer *mp4_file_muxer_new(void);

/**
 * `Mp4FileMuxer` インスタンスを破棄して、割り当てられたリソースを解放する
 *
 * この関数は、`mp4_file_muxer_new()` で作成された `Mp4FileMuxer` インスタンスを破棄し、
 * その内部で割り当てられたすべてのメモリを解放する
 *
 * # 引数
 *
 * - `muxer`: 破棄する `Mp4FileMuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、この関数は何もしない
 *
 * # 使用例
 *
 * ```c
 * // Mp4FileMuxer インスタンスを生成
 * Mp4FileMuxer *muxer = mp4_file_muxer_new();
 *
 * // マルチプレックス処理を実行（省略）...
 *
 * // リソース解放
 * mp4_file_muxer_free(muxer);
 * ```
 */
void mp4_file_muxer_free(struct Mp4FileMuxer *muxer);

/**
 * `Mp4FileMuxer` で最後に発生したエラーのメッセージを取得する
 *
 * このメソッドは、マルチプレックス処理中に発生した最後のエラーのメッセージ（NULL 終端）を返す
 *
 * エラーが発生していない場合は、空文字列へのポインタを返す
 *
 * # 引数
 *
 * - `muxer`: `Mp4FileMuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、NULL 終端の空文字列へのポインタを返す
 *
 * # 戻り値
 *
 * - メッセージが存在する場合: NULL 終端のエラーメッセージへのポインタ
 * - メッセージが存在しない場合: NULL 終端の空文字列へのポインタ
 * - `muxer` 引数が NULL の場合: NULL 終端の空文字列へのポインタ
 *
 * # 使用例
 *
 * ```c
 * Mp4FileMuxer *muxer = mp4_file_muxer_new();
 *
 * Mp4Error ret = mp4_file_muxer_initialize(muxer);
 *
 * // エラーが発生した場合、メッセージを取得
 * if (ret != MP4_ERROR_OK) {
 *     const char *error_msg = mp4_file_muxer_get_last_error(muxer);
 *     fprintf(stderr, "エラー: %s\n", error_msg);
 * }
 *
 * mp4_file_muxer_free(muxer);
 * ```
 */
const char *mp4_file_muxer_get_last_error(const struct Mp4FileMuxer *muxer);

/**
 * MP4 ファイルの moov ボックスの事前確保サイズを設定する
 *
 * この関数は、faststart 形式での MP4 ファイル構築時に、
 * ファイルの先頭付近に配置する moov ボックス用の領域を事前に確保するサイズを指定する
 *
 * # faststart 形式について
 *
 * faststart とは、MP4 ファイルの再生に必要なメタデータを含む moov ボックスを
 * ファイルの先頭付近に配置する形式である
 *
 * これにより、動画プレイヤーが再生を開始する際に、ファイル末尾へのシークを行ったり、
 * ファイル全体をロードする必要がなくなり、再生開始までの時間が短くなることが期待できる
 *
 * なお、実際の moov ボックスのサイズがここで指定した値よりも大きい場合は、
 * moov ボックスはファイル末尾に配置され、faststart 形式は無効になる
 *
 * # 引数
 *
 * - `muxer`: `Mp4FileMuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 * - `size`: 事前確保する moov ボックスのサイズ（バイト単位）
 *   - 0 を指定すると faststart は無効になる（デフォルト動作）
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に設定された
 * - `MP4_ERROR_NULL_POINTER`: `muxer` が NULL である
 *
 * # 注意
 *
 * この関数の呼び出しは `mp4_file_muxer_initialize()` の前に行う必要があり、
 * 初期化後の呼び出しは効果がない
 *
 * # 関連関数
 *
 * - `mp4_estimate_maximum_moov_box_size()`: 必要な moov ボックスサイズを見積もるために使える関数
 *
 * # 使用例
 *
 * ```c
 * Mp4FileMuxer *muxer = mp4_file_muxer_new();
 *
 * // 見積もり値を使用して moov ボックスサイズを設定
 * uint32_t estimated_size = mp4_estimate_maximum_moov_box_size(100, 3000);
 * mp4_file_muxer_set_reserved_moov_box_size(muxer, estimated_size);
 *
 * // マルチプレックス処理を初期化
 * mp4_file_muxer_initialize(muxer);
 * ```
 */
enum Mp4Error mp4_file_muxer_set_reserved_moov_box_size(struct Mp4FileMuxer *muxer,
                                                        uint32_t size);

/**
 * MP4 ファイルのマルチプレックス処理を初期化する
 *
 * この関数は、`mp4_file_muxer_new()` で作成した `Mp4FileMuxer` インスタンスを初期化し、
 * マルチプレックス処理を開始するための準備を行う
 *
 * 初期化によって生成された出力データは `mp4_file_muxer_next_output()` によって取得できる
 *
 * # 引数
 *
 * - `muxer`: `Mp4FileMuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に初期化された
 * - `MP4_ERROR_NULL_POINTER`: `muxer` が NULL である
 * - `MP4_ERROR_INVALID_STATE`: マルチプレックスが既に初期化済みである
 * - その他のエラー: 初期化に失敗した場合
 *
 * エラーが発生した場合は、`mp4_file_muxer_get_last_error()` でエラーメッセージを取得できる
 *
 * # オプション指定
 *
 * 以下のオプションを指定する場合はは `mp4_file_muxer_initialize()` 呼び出し前に行う必要がある:
 * - `mp4_file_muxer_set_reserved_moov_box_size()`: faststart 用に moov ボックスサイズを設定する
 */
enum Mp4Error mp4_file_muxer_initialize(struct Mp4FileMuxer *muxer);

/**
 * MP4 ファイルの構築に必要な次の出力データを取得する
 *
 * マルチプレックス処理中に生成される、MP4 ファイルに書き込むべきデータを取得する
 *
 * 出力データは複数ある可能性があるため、利用者はこの関数をループで呼び出す必要がある
 *
 * すべての出力データを取得し終えると、`out_output_size` に 0 が設定されて返る
 *
 * この関数のハンドリングが必要なのは、以下の関数の呼び出し後である:
 * - `mp4_file_muxer_initialize()`
 * - `mp4_file_muxer_finalize()`
 *
 * # 引数
 *
 * - `muxer`: `Mp4FileMuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * - `out_output_offset`: 出力データをファイルに書き込むべき位置（バイト単位）を受け取るポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * - `out_output_size`: 出力データのサイズ（バイト単位）を受け取るポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *   - 0 が設定された場合は、すべてのデータを取得し終えたことを意味する
 *
 * - `out_output_data`: 出力データのバッファへのポインタを受け取るポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *   - 注意: 同じ `Mp4FileMuxer` インスタンスに対して別の関数呼び出しを行うと、このポインタの参照先が無効になる可能性がある
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に出力データが取得された
 * - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
 *
 * # 使用例
 *
 * ```c
 * FILE *fp = fopen("output.mp4", "wb");
 * Mp4FileMuxer *muxer = mp4_file_muxer_new();
 *
 * // 初期化
 * mp4_file_muxer_initialize(muxer);
 *
 * // 初期出力データをファイルに書き込む
 * uint64_t output_offset;
 * uint32_t output_size;
 * const uint8_t *output_data;
 * while (mp4_file_muxer_next_output(muxer, &output_offset, &output_size, &output_data) == MP4_ERROR_OK) {
 *     if (output_size == 0) break;  // すべてのデータを取得し終えた
 *
 *     // 指定された位置にデータを書き込む
 *     fseek(fp, output_offset, SEEK_SET);
 *     fwrite(output_data, 1, output_size, fp);
 * }
 *
 * // サンプルを追加（省略）...
 *
 * // ファイナライズ
 * mp4_file_muxer_finalize(muxer);
 *
 * // ファイナライズ後の出力データをファイルに書き込む
 * while (mp4_file_muxer_next_output(muxer, &output_offset, &output_size, &output_data) == MP4_ERROR_OK) {
 *     if (output_size == 0) break;
 *     fseek(fp, output_offset, SEEK_SET);
 *     fwrite(output_data, 1, output_size, fp);
 * }
 *
 * mp4_file_muxer_free(muxer);
 * fclose(fp);
 * ```
 */
enum Mp4Error mp4_file_muxer_next_output(struct Mp4FileMuxer *muxer,
                                         uint64_t *out_output_offset,
                                         uint32_t *out_output_size,
                                         const uint8_t **out_output_data);

/**
 * MP4 ファイルに追記されたメディアサンプルの情報を `Mp4FileMuxer` に伝える
 *
 * この関数は、利用側で実際のサンプルデータをファイルに追記した後に、
 * そのサンプルの情報を `Mp4FileMuxer` に通知するために呼び出される
 *
 * `Mp4FileMuxer` はサンプルの情報を蓄積して、`mp4_file_muxer_finalize()` 呼び出し時に、
 * MP4 ファイルを再生するために必要なメタデータ（moov ボックス）を構築する
 *
 * なお、サンプルデータは `mp4_file_muxer_initialize()` によって生成された出力データの後ろに
 * 追記されていく想定となっている
 *
 * # 引数
 *
 * - `muxer`: `Mp4FileMuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * - `sample`: 追記されたサンプルの情報を表す `Mp4MuxSample` 構造体へのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常にサンプルが追加された
 * - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
 * - `MP4_ERROR_INVALID_STATE`: マルチプレックスが初期化されていないか、既にファイナライズ済み
 * - `MP4_ERROR_OUTPUT_REQUIRED`: 前回の呼び出しで生成された出力データが未処理（`mp4_file_muxer_next_output()` で取得されていない）
 * - `MP4_ERROR_POSITION_MISMATCH`: サンプルデータの位置がファイル内の期待された位置と一致していない
 * - その他のエラー: サンプル情報が不正な場合
 *
 * # 使用例
 *
 * ```c
 * // マルチプレックス処理を初期化
 * mp4_file_muxer_initialize(muxer);
 *
 * // 初期出力データをファイルに書きこむ
 * uint64_t output_offset;
 * uint32_t output_size;
 * const uint8_t *output_data;
 * while (mp4_file_muxer_next_output(muxer, &output_offset, &output_size, &output_data) == MP4_ERROR_OK) {
 *     if (output_size == 0) break;
 *     fseek(fp, output_offset, SEEK_SET);
 *     fwrite(output_data, 1, output_size, fp);
 * }
 * output_offset += output_size;
 *
 * // サンプルデータを準備してファイルに書きこむ
 * uint8_t sample_data[1024];
 * prepare_sample_data(sample_data, sizeof(sample_data));
 * fwrite(sample_data, 1, sizeof(sample_data), fp);
 * output_offset += sizeof(sample_data);
 *
 * // サンプルエントリーを作成
 * Mp4SampleEntry sample_entry = // ...;
 *
 * // サンプル情報を構築
 * Mp4MuxSample video_sample = {
 *     .track_kind = MP4_TRACK_KIND_VIDEO,
 *     .sample_entry = &sample_entry,
 *     .keyframe = true,
 *     .timescale = 30,
 *     .duration = 1,  // 30 FPS
 *     .has_composition_time_offset = false,
 *     .composition_time_offset = 0,
 *     .data_offset = output_offset,
 *     .data_size = sizeof(sample_data),
 * };
 *
 * // マルチプレックスにサンプル情報を通知
 * Mp4Error ret = mp4_file_muxer_append_sample(muxer, &video_sample);
 * if (ret != MP4_ERROR_OK) {
 *     fprintf(stderr, "Failed to append sample: %s\n", mp4_file_muxer_get_last_error(muxer));
 *     return 1;
 * }
 * ```
 */
enum Mp4Error mp4_file_muxer_append_sample(struct Mp4FileMuxer *muxer,
                                           const struct Mp4MuxSample *sample);

/**
 * サンプルデータ以外のバイト列のサイズ分だけ内部の書き込み位置を進める
 *
 * OBS の Hybrid MP4 のように、サンプルデータの間に moof / mdat ヘッダなどの
 * 非サンプルデータが挿入される場合に使用する。
 *
 * `size` が 0 より大きい場合は、次の `mp4_file_muxer_append_sample()` 呼び出し時に
 * 強制的に新しいチャンクが開始される。
 * これは、非サンプルデータの挿入によりチャンク内のサンプルデータの連続性が
 * 失われるためである。
 *
 * # 引数
 *
 * - `muxer`: `Mp4FileMuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 * - `size`: 進めるバイト数
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常に書き込み位置が更新された
 * - `MP4_ERROR_NULL_POINTER`: `muxer` が NULL である
 * - `MP4_ERROR_INVALID_STATE`: マルチプレックスが初期化されていないか、既にファイナライズ済み
 * - `MP4_ERROR_OUTPUT_REQUIRED`: 前回の呼び出しで生成された出力データが未処理（`mp4_file_muxer_next_output()` で取得されていない）
 *
 * エラーが発生した場合は、`mp4_file_muxer_get_last_error()` でエラーメッセージを取得できる
 *
 * # 使用例
 *
 * ```c
 * // fMP4 フラグメントヘッダのサイズ分だけ位置を進める
 * Mp4Error ret = mp4_file_muxer_advance_position(muxer, fragment_header_size);
 * if (ret != MP4_ERROR_OK) {
 *     fprintf(stderr, "Failed to advance position: %s\n", mp4_file_muxer_get_last_error(muxer));
 *     return 1;
 * }
 * ```
 */
enum Mp4Error mp4_file_muxer_advance_position(struct Mp4FileMuxer *muxer,
                                              uint64_t size);

/**
 * MP4 ファイルのマルチプレックス処理を完了する
 *
 * この関数は、それまでに追加されたすべてのサンプルの情報を用いて、
 * MP4 ファイルの再生に必要なメタデータ（moov ボックス）を構築し、
 * ファイルの最終的な形式を確定する
 *
 * マルチプレックス処理が完了すると、ファイルに書き込むべき最終的な出力データが
 * `mp4_file_muxer_next_output()` で取得できるようになる
 *
 * # 引数
 *
 * - `muxer`: `Mp4FileMuxer` インスタンスへのポインタ
 *   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
 *
 * # 戻り値
 *
 * - `MP4_ERROR_OK`: 正常にマルチプレックス処理が完了した
 * - `MP4_ERROR_NULL_POINTER`: `muxer` が NULL である
 * - `MP4_ERROR_INVALID_STATE`: マルチプレックスが初期化されていないか、既にファイナライズ済み
 * - `MP4_ERROR_OUTPUT_REQUIRED`: 前回の呼び出しで生成された出力データが未処理（`mp4_file_muxer_next_output()` で取得されていない）
 * - その他のエラー: マルチプレックス処理の完了に失敗した場合
 *
 * エラーが発生した場合は、`mp4_file_muxer_get_last_error()` でエラーメッセージを取得できる
 *
 * # 使用例
 *
 * ```c
 * // マルチプレックス処理を完了
 * Mp4Error ret = mp4_file_muxer_finalize(muxer);
 * if (ret != MP4_ERROR_OK) {
 *     fprintf(stderr, "ファイナライズ失敗: %s\n",
 *             mp4_file_muxer_get_last_error(muxer));
 *     return 1;
 * }
 *
 * // ファイナライズ後のデータをファイルに書き込む
 * uint64_t offset;
 * uint32_t size;
 * const uint8_t *data;
 * while (mp4_file_muxer_next_output(muxer, &offset, &size, &data) == MP4_ERROR_OK) {
 *     if (size == 0) break;
 *     fseek(fp, offset, SEEK_SET);
 *     fwrite(data, 1, size, fp);
 * }
 * ```
 */
enum Mp4Error mp4_file_muxer_finalize(struct Mp4FileMuxer *muxer);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* SHIGUREDO_MP4_H */
