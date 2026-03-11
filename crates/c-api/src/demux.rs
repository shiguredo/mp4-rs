//! ../../../src/demux.rs の C API を定義するためのモジュール
use std::ffi::{CString, c_char};

use shiguredo_mp4::BaseBox;

use crate::{
    basic_types::Mp4TrackKind,
    boxes::{Mp4SampleEntry, Mp4SampleEntryOwned},
    error::Mp4Error,
};

/// MP4 デマルチプレックス処理中に抽出されたメディアトラックの情報を表す構造体
#[repr(C)]
pub struct Mp4DemuxTrackInfo {
    /// このトラックを識別するための ID
    pub track_id: u32,

    /// トラックの種類（音声または映像）
    pub kind: Mp4TrackKind,

    /// トラックの尺（タイムスケール単位で表現）
    ///
    /// 実際の時間（秒単位）を得るには、この値を `timescale` で除算すること。
    ///
    /// fMP4 の場合は init segment 由来の値であり、実際には 0 になることが多い。
    /// その場合は「未確定ないし実質不明相当」とみなしてよい。
    pub duration: u64,

    /// このトラック内で使用されているタイムスケール
    ///
    /// タイムスタンプと尺の単位を定義する値で、1 秒間の単位数を表す
    /// 例えば `timescale` が 1000 の場合、タイムスタンプは 1 ms 単位で表現される
    pub timescale: u32,
}

impl From<shiguredo_mp4::demux::TrackInfo> for Mp4DemuxTrackInfo {
    fn from(track_info: shiguredo_mp4::demux::TrackInfo) -> Self {
        Self {
            track_id: track_info.track_id,
            kind: track_info.kind.into(),
            duration: track_info.duration,
            timescale: track_info.timescale.get(),
        }
    }
}

/// MP4 デマルチプレックス処理によって抽出されたメディアサンプルを表す構造体
///
/// MP4 ファイル内の各サンプル（フレーム単位の音声または映像データ）のメタデータと
/// ファイル内の位置情報を保持する
///
/// この構造体が参照しているポインタのメモリ管理は各 demuxer が行っており、
/// 対応する demuxer インスタンスが破棄されるまでは安全に参照可能である
#[repr(C)]
pub struct Mp4DemuxSample {
    /// サンプルが属するトラックの情報へのポインタ
    ///
    /// このポインタの参照先には対応する demuxer インスタンスが有効な間のみアクセス可能である
    pub track: *const Mp4DemuxTrackInfo,

    /// サンプルの詳細情報（コーデック設定など）へのポインタ
    ///
    /// 値が NULL の場合は「サンプルエントリーの内容が前のサンプルと同じ」であることを意味する
    ///
    /// このポインタの参照先には対応する demuxer インスタンスが有効な間のみアクセス可能である
    pub sample_entry: *const Mp4SampleEntry,

    /// このサンプルがキーフレームであるかの判定
    ///
    /// `true` の場合、このサンプルはキーフレームであり、このポイントから復号を開始できる
    ///
    /// 音声の場合には、通常はすべてのサンプルがキーフレーム扱いとなる
    pub keyframe: bool,

    /// サンプルのタイムスタンプ（タイムスケール単位）
    ///
    /// 実際の時間（秒単位）を得るには、この値を対応する `Mp4DemuxTrackInfo` の
    /// `timescale` で除算すること
    pub timestamp: u64,

    /// サンプルの尺（タイムスケール単位）
    ///
    /// 実際の時間（秒単位）を得るには、この値を対応する `Mp4DemuxTrackInfo` の
    /// `timescale` で除算すること
    pub duration: u32,

    /// コンポジション時間オフセットが存在するかどうか
    pub has_composition_time_offset: bool,

    /// コンポジション時間オフセット（タイムスケール単位）
    ///
    /// `has_composition_time_offset` が true の場合のみ有効。
    /// PTS = timestamp + composition_time_offset で計算できる。
    ///
    /// C API では通常 MP4 の `ctts` と fMP4 の `trun` の両方を共通の sample 型で扱うため、
    /// Rust 側の `Sample` に合わせて `i64` に widening している。
    /// 仕様上すべての入力が 64 bit 必須という意味ではない。
    pub composition_time_offset: i64,

    /// ファイル内におけるサンプルデータの開始位置（バイト単位）
    ///
    /// file demuxer ではファイル先頭からの絶対位置、
    /// segment demuxer では `fmp4_segment_demuxer_handle_media_segment()` に渡した
    /// 入力バッファ先頭からの相対位置を表す。
    pub data_offset: u64,

    /// サンプルデータのサイズ（バイト単位）
    ///
    /// `data_offset` から `data_offset + data_size` までの範囲がサンプルデータとなる
    pub data_size: usize,
}

impl Mp4DemuxSample {
    pub fn new(
        sample: shiguredo_mp4::demux::Sample<'_>,
        track: &Mp4DemuxTrackInfo,
        sample_entry: Option<&Mp4SampleEntry>,
    ) -> Self {
        Self {
            track,
            sample_entry: sample_entry
                .map(|x| x as *const _)
                .unwrap_or(std::ptr::null()),
            keyframe: sample.keyframe,
            timestamp: sample.timestamp,
            duration: sample.duration,
            has_composition_time_offset: sample.composition_time_offset.is_some(),
            composition_time_offset: sample.composition_time_offset.unwrap_or(0),
            data_offset: sample.data_offset,
            data_size: sample.data_size,
        }
    }
}

/// MP4 ファイルをデマルチプレックスして、メディアサンプルを時系列順に取得するための構造体
///
/// # 関連関数
///
/// この構造体は、以下の関数を通して操作する必要がある:
/// - `mp4_file_demuxer_new()`: `Mp4FileDemuxer` インスタンスを生成する
/// - `mp4_file_demuxer_free()`: リソースを解放する
/// - `mp4_file_demuxer_get_required_input()`: 次の処理に必要な入力データの位置とサイズを取得する
/// - `mp4_file_demuxer_handle_input()`: ファイルデータを入力として受け取る
/// - `mp4_file_demuxer_get_tracks()`: MP4 ファイル内のすべてのメディアトラック情報を取得する
/// - `mp4_file_demuxer_next_sample()`: 時系列順に次のサンプルを取得する
/// - `mp4_file_demuxer_prev_sample()`: 時系列順に前のサンプルを取得する
/// - `mp4_file_demuxer_seek()`: 指定した時刻へシークする
/// - `mp4_file_demuxer_get_last_error()`: 最後に発生したエラーのメッセージを取得する
///
/// # Examples
///
/// ```c
/// // Mp4FileDemuxer インスタンスを生成
/// Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
///
/// // 入力ファイルデータを供給して初期化
/// while (true) {
///     uint64_t required_pos;
///     int32_t required_size;
///     mp4_file_demuxer_get_required_input(demuxer, &required_pos, &required_size);
///     if (required_size == 0) break;
///
///     // NOTE: 実際には `required_size == -1` の場合には、ファイル末尾までを読み込む必要がある
///     uint8_t buffer[required_size];
///     size_t bytes_read = read_file_data(required_pos, buffer, sizeof(required_size));
///     mp4_file_demuxer_handle_input(demuxer, required_pos, buffer, bytes_read);
/// }
///
/// // トラック情報を取得
/// const Mp4DemuxTrackInfo *tracks;
/// uint32_t track_count;
/// Mp4Error ret = mp4_file_demuxer_get_tracks(demuxer, &tracks, &track_count);
/// if (ret == MP4_ERROR_OK) {
///     // トラック情報を処理...
/// }
///
/// // サンプルを取得
/// Mp4DemuxSample sample;
/// while (mp4_file_demuxer_next_sample(demuxer, &sample) == MP4_ERROR_OK) {
///     // サンプルを処理...
/// }
///
/// // 前のサンプルを取得
/// while (mp4_file_demuxer_prev_sample(demuxer, &sample) == MP4_ERROR_OK) {
///     // サンプルを処理...
/// }
///
/// // timestamp=1500, timescale=1000 にシーク
/// mp4_file_demuxer_seek(demuxer, 1500, 1000);
///
/// // リソース解放
/// mp4_file_demuxer_free(demuxer);
/// ```
pub struct Mp4FileDemuxer {
    inner: shiguredo_mp4::demux::Mp4FileDemuxer,
    tracks: Vec<Mp4DemuxTrackInfo>,
    sample_entries: Vec<(
        shiguredo_mp4::boxes::SampleEntry,
        Mp4SampleEntryOwned,
        // [NOTE]
        // tracks とは異なり sample_entries は途中でサイズが変わる可能性があるので、
        // その際に C 側で保持されているポインタが無効にならないように Box でラップしておく
        Box<Mp4SampleEntry>,
    )>,
    last_error_string: Option<CString>,
}

impl Mp4FileDemuxer {
    fn set_last_error(&mut self, message: &str) {
        self.last_error_string = CString::new(message).ok();
    }
}

/// 新しい `Mp4FileDemuxer` インスタンスを作成して、それへのポインタを返す
///
/// この関数が返したポインタは、使用後に `mp4_file_demuxer_free()` で破棄する必要がある
///
/// # 使用例
///
/// ```c
/// // Mp4FileDemuxer インスタンスを生成
/// Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
/// if (demuxer == NULL) {
///     fprintf(stderr, "Failed to create demuxer\n");
///     return;
/// }
///
/// // 処理を実行...
///
/// // リソース解放
/// mp4_file_demuxer_free(demuxer);
/// ```
#[unsafe(no_mangle)]
pub extern "C" fn mp4_file_demuxer_new() -> *mut Mp4FileDemuxer {
    let demuxer = Box::new(Mp4FileDemuxer {
        inner: shiguredo_mp4::demux::Mp4FileDemuxer::new(),
        tracks: Vec::new(),
        sample_entries: Vec::new(),
        last_error_string: None,
    });
    Box::into_raw(demuxer)
}

/// `Mp4FileDemuxer` インスタンスを破棄して、割り当てられたリソースを解放する
///
/// この関数は、`mp4_file_demuxer_new()` で作成された `Mp4FileDemuxer` インスタンスを破棄し、
/// その内部で割り当てられたすべてのメモリを解放する。
///
/// # 引数
///
/// - `demuxer`: 破棄する `Mp4FileDemuxer` インスタンスへのポインタ
///   - NULL ポインタが渡された場合、この関数は何もしない
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_demuxer_free(demuxer: *mut Mp4FileDemuxer) {
    if !demuxer.is_null() {
        let _ = unsafe { Box::from_raw(demuxer) };
    }
}

/// `Mp4FileDemuxer` で最後に発生したエラーのメッセージを取得する
///
/// この関数は、デマルチプレックス処理中に発生した最後のエラーのメッセージ（NULL 終端）を返す
///
/// エラーが発生していない場合は、空文字列へのポインタを返す
///
/// # 引数
///
/// - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
///
/// # 戻り値
///
///
/// - メッセージが存在する場合: NULL 終端のエラーメッセージへのポインタ
/// - メッセージが存在しない場合: NULL 終端の空文字列へのポインタ
/// - `demuxer` 引数が NULL の場合: NULL 終端の空文字列へのポインタ
///
/// # 使用例
///
/// ```c
/// Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
///
/// Mp4Error ret = // なんらかの処理;
///
/// // エラーが発生した場合、メッセージを取得
/// if (ret != MP4_ERROR_OK) {
///     const char *error_msg = mp4_file_demuxer_get_last_error(demuxer);
///     fprintf(stderr, "エラー: %s\n", error_msg);
/// }
/// ```
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_demuxer_get_last_error(
    demuxer: *const Mp4FileDemuxer,
) -> *const c_char {
    if demuxer.is_null() {
        return c"".as_ptr();
    }

    let demuxer = unsafe { &*demuxer };
    let Some(e) = &demuxer.last_error_string else {
        return c"".as_ptr();
    };
    e.as_ptr()
}

/// `Mp4FileDemuxer` で次の処理を進めるために必要な I/O の位置とサイズを取得する
///
/// この関数は、処理を進めるために必要な I/O がない場合には `out_required_input_size` に 0 を設定して返し、
/// それ以外の場合は、ファイルから読み込む必要があるデータの位置とサイズを出力引数に設定して返す
///
/// この関数から取得した位置とサイズの情報をもとに、呼び出し元がファイルなどからデータを読み込み、
/// `mp4_file_demuxer_handle_input()` に渡す必要がある
///
/// なお、現在の `Mp4FileDemuxer` の実装は fragmented MP4 には対応していないため、
/// サンプルの取得に必要なメタデータ（moovボックス）の読み込み（初期化）が終わったら、
/// 以後はこの関数が追加の入力データを要求することはない
///
/// # 引数
///
/// - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///
/// - `out_required_input_position`: 必要なデータの開始位置（バイト単位）を受け取るポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///
/// - `out_required_input_size`: 必要なデータのサイズ（バイト単位）を受け取るポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///   - なお、ここに設定されるサイズはあくまでもヒントであり、厳密に一致したサイズのデータを提供する必要はない
///     - 通常は、より大きな範囲のデータを一度に渡した方が効率がいい
///   - 0 が設定された場合は、これ以上の入力データが不要であることを意味する
///   - -1 が設定された場合は、ファイルの末尾までのデータが必要であることを意味する
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常に処理された
/// - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
///
/// # 使用例
///
/// ```c
/// Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
/// FILE *fp = fopen("input.mp4", "rb");
///
/// // 初期化が完了するまでループ
/// while (true) {
///     uint64_t required_pos;
///     int32_t required_size;
///     mp4_file_demuxer_get_required_input(demuxer, &required_pos, &required_size);
///     if (required_size == 0) break; // 初期化完了
///    
///     // ファイルから必要なデータを読み込む
///     //
///     // NOTE: 実際には `required_size == -1` の場合には、ファイル末尾までを読み込む必要がある
///     uint8_t buffer[required_size];
///     fseek(fp, required_pos, SEEK_SET);
///     size_t bytes_read = fread(buffer, 1, required_size, fp);
///
///     // demuxer にデータを供給
///     mp4_file_demuxer_handle_input(demuxer, required_pos, buffer, bytes_read);
/// }
/// ```
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_demuxer_get_required_input(
    demuxer: *mut Mp4FileDemuxer,
    out_required_input_position: *mut u64,
    out_required_input_size: *mut i32,
) -> Mp4Error {
    if demuxer.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let demuxer = unsafe { &mut *demuxer };

    if out_required_input_position.is_null() {
        demuxer.set_last_error(
            "[mp4_file_demuxer_get_required_input] out_required_input_position is null",
        );
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    if out_required_input_size.is_null() {
        demuxer.set_last_error(
            "[mp4_file_demuxer_get_required_input] out_required_input_size is null",
        );
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }

    unsafe {
        if let Some(required) = demuxer.inner.required_input() {
            *out_required_input_position = required.position;
            *out_required_input_size = required.size.map(|n| n as i32).unwrap_or(-1);
        } else {
            *out_required_input_position = 0;
            *out_required_input_size = 0;
        }
    }

    Mp4Error::MP4_ERROR_OK
}

/// `Mp4FileDemuxer` にファイルデータを入力として供給し、デマルチプレックス処理を進める
///
/// この関数は、`mp4_file_demuxer_get_required_input()` で取得した位置に対応するファイルデータを
/// 受け取り、デマルチプレックス処理を進める
///
/// なお、この関数はデータの部分的な消費を行わないため、呼び出し元が必要なデータを一度に全て渡す必要がある
/// （固定長のバッファを使って複数回に分けてデータを供給することはできない）
///
/// # 引数
///
/// - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///
/// - `input_position`: 入力データがファイル内で始まる位置（バイト単位）
///   - `mp4_file_demuxer_get_required_input()` で取得した位置と一致していることが期待される
///
/// - `input_data`: ファイルデータのバッファへのポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///
/// - `input_data_size`: 入力データのサイズ（バイト単位）
///   - 0 以上の値を指定する必要がある
///   - `mp4_file_demuxer_get_required_input()` で取得したサイズより大きいサイズを指定することは問題ない
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常に入力データが受け取られた
///   - この場合でも `mp4_file_demuxer_get_required_input()` を使って、追加の入力が必要かどうかを確認する必要がある
/// - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
///
/// # エラー状態への遷移
///
/// 入力データの内容や範囲が不正な場合には `Mp4FileDemuxer` はエラー状態に遷移する。
///
/// これは以下のようなケースで発生する:
/// - `input_position` が `mp4_file_demuxer_get_required_input()` で取得した位置と異なる
/// - `input_data_size` が要求されたサイズより不足している
/// - 入力ファイルデータが MP4 形式として不正である（ボックスのデコード失敗など）
/// - サポートされていないコーデックが使用されている
///
/// エラー状態に遷移した後は、
/// - `mp4_file_demuxer_get_required_input()` は `out_required_input_size` に 0 を設定する
/// - `mp4_file_demuxer_get_tracks()` および `mp4_file_demuxer_next_sample()` の呼び出しはエラーを返す
/// - `mp4_file_demuxer_get_last_error()` でエラーメッセージを確認できる
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_demuxer_handle_input(
    demuxer: *mut Mp4FileDemuxer,
    input_position: u64,
    input_data: *const u8,
    input_data_size: u32,
) -> Mp4Error {
    if demuxer.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let demuxer = unsafe { &mut *demuxer };

    if input_data.is_null() {
        demuxer.set_last_error("[mp4_file_demuxer_handle_input] input_data is null");
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }

    let input_data = unsafe { std::slice::from_raw_parts(input_data, input_data_size as usize) };
    let input = shiguredo_mp4::demux::Input {
        position: input_position,
        data: input_data,
    };
    demuxer.inner.handle_input(input);

    Mp4Error::MP4_ERROR_OK
}

/// MP4 ファイル内に含まれるすべてのメディアトラック（音声および映像）の情報を取得する
///
/// # 引数
///
/// - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///
/// - `out_tracks`: 取得したトラック情報の配列へのポインタを受け取るポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///   - このポインタの参照先には `Mp4FileDemuxer` インスタンスが有効な間のみアクセス可能である
///
/// - `out_track_count`: トラック情報の個数を受け取るポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///   - MP4 ファイルにトラックが含まれていない場合は 0 が設定される
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常にトラック情報が取得された
/// - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
/// - `MP4_ERROR_INPUT_REQUIRED`: 初期化に必要な入力データが不足している
///   - `mp4_file_demuxer_get_required_input()` および `mp4_file_demuxer_handle_input()` のハンドリングが必要
/// - その他のエラー: 入力ファイルが破損していたり、未対応のコーデックを含んでいる場合
///
/// # 使用例
///
/// ```c
/// Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
///
/// // ファイルデータを供給（省略）...
///
/// // トラック情報を取得
/// const Mp4DemuxTrackInfo *tracks;
/// uint32_t track_count;
/// Mp4Error ret = mp4_file_demuxer_get_tracks(demuxer, &tracks, &track_count);
///
/// if (ret == MP4_ERROR_OK) {
///    printf("Found %u tracks\n", track_count);
///    for (uint32_t i = 0; i < track_count; i++) {
///        printf("Track %u: ID=%u, Kind=%d, Duration=%lu, Timescale=%u\n",
///               i, tracks[i].track_id, tracks[i].kind,
///               tracks[i].duration, tracks[i].timescale);
///    }
/// } else {
///    fprintf(stderr, "Error: %d - %s\n", ret, mp4_file_demuxer_get_last_error(demuxer));
/// }
/// ```
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_demuxer_get_tracks(
    demuxer: *mut Mp4FileDemuxer,
    out_tracks: *mut *const Mp4DemuxTrackInfo,
    out_track_count: *mut u32,
) -> Mp4Error {
    if demuxer.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let demuxer = unsafe { &mut *demuxer };

    if out_tracks.is_null() {
        demuxer.set_last_error("[mp4_file_demuxer_get_tracks] out_tracks is null");
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    if out_track_count.is_null() {
        demuxer.set_last_error("[mp4_file_demuxer_get_tracks] out_track_count is null");
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }

    if demuxer.tracks.is_empty() {
        match demuxer.inner.tracks() {
            Ok(tracks) => {
                demuxer.tracks = tracks.iter().map(|t| t.clone().into()).collect();
            }
            Err(e) => {
                demuxer.set_last_error(&format!("[mp4_file_demuxer_get_tracks] {e}"));
                return e.into();
            }
        }
    }

    unsafe {
        *out_tracks = demuxer.tracks.as_ptr();
        *out_track_count = demuxer.tracks.len() as u32;
    }
    Mp4Error::MP4_ERROR_OK
}

/// MP4 ファイルから時系列順に次のメディアサンプルを取得する
///
/// すべてのトラックから、まだ取得していないもののなかで、
/// 最も早いタイムスタンプを持つサンプルを返す
///
/// ファイルの先頭に達した場合は `MP4_ERROR_NO_MORE_SAMPLES` が返される
///
/// # サンプルデータの読み込みについて
///
/// この関数は、サンプルのメタデータ（タイムスタンプ、サイズ、ファイル内の位置など）のみを返すので、
/// 実際のサンプルデータ（音声フレームや映像フレーム）の読み込みは呼び出し元の責務となる
///
/// サンプルデータを処理する場合には、返された `Mp4DemuxSample` の `data_offset` と `data_size` フィールドを使用して、
/// 入力ファイルから直接サンプルデータを読み込む必要がある
///
/// # 引数
///
/// - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///
/// - `out_sample`: 取得したサンプル情報を受け取るポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常にサンプルが取得された
/// - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
/// - `MP4_ERROR_NO_MORE_SAMPLES`: すべてのサンプルを取得し終えた
/// - `MP4_ERROR_INPUT_REQUIRED`: 初期化に必要な入力データが不足している
///   - `mp4_file_demuxer_get_required_input()` および `mp4_file_demuxer_handle_input()` のハンドリングが必要
/// - その他のエラー: 入力ファイルが破損していたり、未対応のコーデックを含んでいる場合
///
/// # 使用例
///
/// ```c
/// FILE *fp = fopen("input.mp4", "rb");
/// Mp4FileDemuxer *demuxer = mp4_file_demuxer_new();
///
/// // ファイルデータを供給して初期化（省略）...
///
/// // 時系列順にサンプルを取得
/// Mp4DemuxSample sample;
/// while (mp4_file_demuxer_next_sample(demuxer, &sample) == MP4_ERROR_OK) {
///     printf("サンプル - トラックID: %u, タイムスタンプ: %lu, サイズ: %zu バイト\n",
///            sample.track->track_id, sample.timestamp, sample.data_size);
///
///     // サンプルデータを入力ファイルから読み込む
///     uint8_t sample_data[sample.data_size];
///     fseek(fp, sample.data_offset, SEEK_SET);
///     fread(sample_data, 1, sample.data_size, fp);
///
///     // サンプルを処理...
/// }
/// ```
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_demuxer_next_sample(
    demuxer: *mut Mp4FileDemuxer,
    out_sample: *mut Mp4DemuxSample,
) -> Mp4Error {
    // 最初に mp4_file_demuxer_get_tracks() を呼んで、demuxer.tracks が確実に初期化されているようにする
    let mut tracks_ptr: *const Mp4DemuxTrackInfo = std::ptr::null();
    let mut track_count: u32 = 0;
    let result = unsafe { mp4_file_demuxer_get_tracks(demuxer, &mut tracks_ptr, &mut track_count) };
    if !matches!(result, Mp4Error::MP4_ERROR_OK) {
        return result;
    }

    if demuxer.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let demuxer = unsafe { &mut *demuxer };

    if out_sample.is_null() {
        demuxer.set_last_error("[mp4_file_demuxer_next_sample] out_sample is null");
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }

    match demuxer.inner.next_sample() {
        Ok(Some(sample)) => {
            let Some(track_info) = demuxer
                .tracks
                .iter()
                .find(|t| t.track_id == sample.track.track_id)
            else {
                demuxer.set_last_error(
                    "[mp4_file_demuxer_next_sample] track info not found for sample",
                );
                return Mp4Error::MP4_ERROR_INVALID_STATE;
            };

            let sample_entry = if let Some(sample_entry) = sample.sample_entry {
                let sample_entry_box_type = sample_entry.box_type();
                if let Some(entry) = demuxer
                    .sample_entries
                    .iter()
                    .find_map(|entry| (entry.0 == *sample_entry).then_some(&entry.2))
                {
                    Some(&**entry)
                } else {
                    let Some(entry_owned) = Mp4SampleEntryOwned::new(sample_entry.clone()) else {
                        demuxer.set_last_error(&format!(
                        "[mp4_file_demuxer_next_sample] Unsupported sample entry box type: {sample_entry_box_type}",
                    ));
                        return Mp4Error::MP4_ERROR_UNSUPPORTED;
                    };
                    let entry = Box::new(entry_owned.to_mp4_sample_entry());
                    demuxer
                        .sample_entries
                        .push((sample_entry.clone(), entry_owned, entry));
                    demuxer.sample_entries.last().map(|entry| &*entry.2)
                }
            } else {
                None
            };

            unsafe {
                *out_sample = Mp4DemuxSample::new(sample, track_info, sample_entry);
            }

            Mp4Error::MP4_ERROR_OK
        }
        Ok(None) => Mp4Error::MP4_ERROR_NO_MORE_SAMPLES,
        Err(e) => {
            demuxer.set_last_error(&format!("[mp4_file_demuxer_next_sample] {e}"));
            e.into()
        }
    }
}

/// MP4 ファイルから時系列順に前のメディアサンプルを取得する
///
/// すべてのトラックのうち、現在位置より前にあるサンプルから、
/// 最も遅いタイムスタンプのものを返す
///
/// ファイルの先頭に達した場合は `MP4_ERROR_NO_MORE_SAMPLES` が返される
///
/// # サンプルデータの読み込みについて
///
/// この関数は、サンプルのメタデータ（タイムスタンプ、サイズ、ファイル内の位置など）のみを返すので、
/// 実際のサンプルデータ（音声フレームや映像フレーム）の読み込みは呼び出し元の責務となる
///
/// サンプルデータを処理する場合には、返された `Mp4DemuxSample` の `data_offset` と `data_size` フィールドを使用して、
/// 入力ファイルから直接サンプルデータを読み込む必要がある
///
/// # 引数
///
/// - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///
/// - `out_sample`: 取得したサンプル情報を受け取るポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常にサンプルが取得された
/// - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
/// - `MP4_ERROR_NO_MORE_SAMPLES`: ファイルの先頭に達した
/// - `MP4_ERROR_INPUT_REQUIRED`: 初期化に必要な入力データが不足している
///   - `mp4_file_demuxer_get_required_input()` および `mp4_file_demuxer_handle_input()` のハンドリングが必要
/// - その他のエラー: 入力ファイルが破損していたり、未対応のコーデックを含んでいる場合
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_demuxer_prev_sample(
    demuxer: *mut Mp4FileDemuxer,
    out_sample: *mut Mp4DemuxSample,
) -> Mp4Error {
    // 最初に mp4_file_demuxer_get_tracks() を呼んで、demuxer.tracks が確実に初期化されているようにする
    let mut tracks_ptr: *const Mp4DemuxTrackInfo = std::ptr::null();
    let mut track_count: u32 = 0;
    let result = unsafe { mp4_file_demuxer_get_tracks(demuxer, &mut tracks_ptr, &mut track_count) };
    if !matches!(result, Mp4Error::MP4_ERROR_OK) {
        return result;
    }

    if demuxer.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let demuxer = unsafe { &mut *demuxer };

    if out_sample.is_null() {
        demuxer.set_last_error("[mp4_file_demuxer_prev_sample] out_sample is null");
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }

    match demuxer.inner.prev_sample() {
        Ok(Some(sample)) => {
            let Some(track_info) = demuxer
                .tracks
                .iter()
                .find(|t| t.track_id == sample.track.track_id)
            else {
                demuxer.set_last_error(
                    "[mp4_file_demuxer_prev_sample] track info not found for sample",
                );
                return Mp4Error::MP4_ERROR_INVALID_STATE;
            };

            let sample_entry = if let Some(sample_entry) = sample.sample_entry {
                let sample_entry_box_type = sample_entry.box_type();
                if let Some(entry) = demuxer
                    .sample_entries
                    .iter()
                    .find_map(|entry| (entry.0 == *sample_entry).then_some(&entry.2))
                {
                    Some(&**entry)
                } else {
                    let Some(entry_owned) = Mp4SampleEntryOwned::new(sample_entry.clone()) else {
                        demuxer.set_last_error(&format!(
                            "[mp4_file_demuxer_prev_sample] Unsupported sample entry box type: {sample_entry_box_type}",
                        ));
                        return Mp4Error::MP4_ERROR_UNSUPPORTED;
                    };
                    let entry = Box::new(entry_owned.to_mp4_sample_entry());
                    demuxer
                        .sample_entries
                        .push((sample_entry.clone(), entry_owned, entry));
                    demuxer.sample_entries.last().map(|entry| &*entry.2)
                }
            } else {
                None
            };

            unsafe {
                *out_sample = Mp4DemuxSample::new(sample, track_info, sample_entry);
            }

            Mp4Error::MP4_ERROR_OK
        }
        Ok(None) => Mp4Error::MP4_ERROR_NO_MORE_SAMPLES,
        Err(e) => {
            demuxer.set_last_error(&format!("[mp4_file_demuxer_prev_sample] {e}"));
            e.into()
        }
    }
}

/// MP4 ファイルの指定時刻にシークする
///
/// 各トラックで指定時刻を含むサンプルを選び、次回の `mp4_file_demuxer_next_sample()` が
/// その位置から開始されるようにする
///
/// 同一タイムスタンプのサンプルが複数ある場合は、シーク後の `mp4_file_demuxer_next_sample()` の走査対象に含まれる
///
/// # 引数
///
/// - `demuxer`: `Mp4FileDemuxer` インスタンスへのポインタ
///   - NULL ポインタが渡された場合、`MP4_ERROR_NULL_POINTER` が返される
/// - `timestamp`: シーク先の時刻を表すタイムスタンプ値（単位は `timescale` で指定）
///   - 実際の秒数は `timestamp / timescale` で計算される
/// - `timescale`: タイムスケール（1 秒間の単位数）
///   - 0 の場合は `MP4_ERROR_INVALID_INPUT` が返される
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常にシークできた
/// - `MP4_ERROR_NULL_POINTER`: 引数として NULL ポインタが渡された
/// - `MP4_ERROR_INVALID_INPUT`: 引数の値が不正
/// - `MP4_ERROR_INPUT_REQUIRED`: 初期化に必要な入力データが不足している
///   - `mp4_file_demuxer_get_required_input()` および `mp4_file_demuxer_handle_input()` のハンドリングが必要
/// - その他のエラー: 入力ファイルが破損していたり、未対応のコーデックを含んでいる場合
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_demuxer_seek(
    demuxer: *mut Mp4FileDemuxer,
    timestamp: u64,
    timescale: u32,
) -> Mp4Error {
    if demuxer.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let demuxer = unsafe { &mut *demuxer };

    let position = std::time::Duration::from_secs(timestamp).checked_div(timescale);
    let Some(position) = position else {
        demuxer.set_last_error("[mp4_file_demuxer_seek] timescale must be non-zero");
        return Mp4Error::MP4_ERROR_INVALID_INPUT;
    };
    match demuxer.inner.seek(position) {
        Ok(()) => Mp4Error::MP4_ERROR_OK,
        Err(e) => {
            demuxer.set_last_error(&format!("[mp4_file_demuxer_seek] {e}"));
            e.into()
        }
    }
}
