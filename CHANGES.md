# 変更履歴

- UPDATE
  - 後方互換がある変更
- ADD
  - 後方互換がある追加
- CHANGE
  - 後方互換のない変更
- FIX
  - バグ修正

## develop

- [ADD] `Mp4FileMuxer::advance_position()` メソッドを追加する
  - OBS の Hybrid MP4 のように、サンプルデータ間に非サンプルデータが挿入される場合に対応する
  - @sile

## 2026.2.0

- [UPDATE] マルチプレックス・デマルチプレックス関連の構造体やエラー型に `Clone` トレイトを実装する
  - @sile
- [ADD] fMP4 のマルチプレックス機能 (`Fmp4SegmentMuxer`) を追加する
  - 複数のメディアトラックからのサンプルを初期化セグメントとメディアセグメントに分けて生成する
  - `sidx` ボックス付きメディアセグメントの生成に対応する
  - moof サイズ計算には 2 パスエンコード方式を採用し、手動でのサイズ同期を不要にする
  - @voluntas
- [ADD] fMP4 のデマルチプレックス機能 (`Fmp4SegmentDemuxer`) を追加する
  - 初期化セグメントとメディアセグメントを逐次処理してサンプルを取り出す
  - `default_base_is_moof=false` の traf に対応する
  - @voluntas
- [ADD] fMP4 のファイルベースデマルチプレックス機能 (`Fmp4FileDemuxer`) を追加する
  - `required_input()` / `handle_input()` ベースで完全な fMP4 ファイルを段階的に読み進める
  - 返り値のサンプル型には既存の `Sample` を再利用する
  - `tfhd` に絶対オフセット形式の `base_data_offset` が含まれる場合はエラーを返す
  - @voluntas
- [ADD] MP4 / fMP4 のファイル種別判定機能 (`Mp4FileKindDetector`) を追加する
  - `required_input()` / `handle_input()` ベースで巨大ファイルや non-faststart なファイルも段階的に判定できる
  - `moov` 内の `mvex` の有無に基づいて `Mp4` / `FragmentedMp4` を判定する
  - @voluntas
- [ADD] `mux::Sample` に `composition_time_offset: Option<i64>` フィールドを追加する
  - `Fmp4SegmentMuxer` は `trun` ボックスの `sample_composition_time_offset` 生成にこの値を使う
  - `Mp4FileMuxer` は `ctts` ボックスの生成にこの値を使う
  - `create_media_segment_metadata()` / `create_media_segment_metadata_with_sidx()` は `mdat` payload 自体を含まず、前方のメタデータ (`moof + mdat header`) を返す
  - C API の `Mp4MuxSample` にも `has_composition_time_offset` / `composition_time_offset` フィールドを追加する
  - C API の `Mp4DemuxSample` にも `has_composition_time_offset` / `composition_time_offset` フィールドを追加する
  - WASM API の JSON 入力にも `composition_time_offset` フィールドを追加する
  - WASM API の JSON 出力にも `composition_time_offset` フィールドを追加する
  - @voluntas
- [ADD] `Mp4FileDemuxer` で `ctts` ボックスをサポートする
  - `SampleAccessor::composition_time_offset()` メソッドを追加する
  - `Sample` 構造体に `composition_time_offset: Option<i64>` フィールドを追加する
  - `ctts` を含むトラック（H.265 など B フレームを持つコーデック）も正常にデマルチプレックスできるようになる
  - @voluntas
- [ADD] WebAssembly API (crates/wasm) を追加する
  - demux / mux の機能を wasm32-unknown-unknown ターゲットで利用可能にする
  - 基本的には C API が提供している関数群をそのまま wasm 向けにも提供している
  - ただし wasm ではホスト側とメモリ空間が分かれており、構造体などの複雑なデータ構造を直接やりとりすることもできないため、以下の補助関数群を追加する:
    - `mp4_alloc`: wasm メモリ空間にバイト列を確保
    - `mp4_free`: 確保したメモリを解放
    - `mp4_vec_ptr`: Vec<u8> のポインタ取得
    - `mp4_vec_len`: Vec<u8> の長さ取得
    - `mp4_vec_free`: Vec<u8> の解放
    - `mp4_mux_sample_from_json`: JSON から Mp4MuxSample 構造体を生成
    - `mp4_mux_sample_free`: mp4_mux_sample_from_json で確保したメモリの解放
    - `mp4_demux_track_info_to_json`: トラック情報を JSON に変換
    - `mp4_demux_sample_to_json`: デマルチプレックスしたサンプルを JSON に変換
  - JSON 処理には nojson を使用:
    - サンプルエントリー内のバイナリデータ (SPS/PPS/NALU 等) は数値配列で表現する
  - @voluntas
- [ADD] fMP4 の C API を追加する
  - `fmp4_segment_muxer_*` 関数群で fMP4 のマルチプレックスが可能になる
  - `fmp4_segment_demuxer_*` 関数群で fMP4 のデマルチプレックスが可能になる
  - @voluntas
- [ADD] Fragmented MP4 (fMP4) 関連のボックスを追加する
  - フラグメント用ボックス:
    - MoofBox (Movie Fragment Box)
    - MfhdBox (Movie Fragment Header Box)
    - TrafBox (Track Fragment Box)
    - TfhdBox (Track Fragment Header Box)
    - TrunBox (Track Fragment Run Box)
    - TfdtBox (Track Fragment Base Media Decode Time Box)
  - セグメントインデックス用ボックス:
    - SidxBox (Segment Index Box)
  - ランダムアクセス用ボックス:
    - MfraBox (Movie Fragment Random Access Box)
    - TfraBox (Track Fragment Random Access Box)
    - MfroBox (Movie Fragment Random Access Offset Box)
  - moov 内の拡張用ボックス:
    - MvexBox (Movie Extends Box)
    - MehdBox (Movie Extends Header Box)
    - TrexBox (Track Extends Box)
  - RootBox に Moof, Mfra, Sidx バリアントを追加する
  - @voluntas
- [ADD] SampleFlags 型を追加する
  - fMP4 で使用されるサンプルフラグを表現する型
  - @voluntas
- [ADD] Mp4FileDemuxer に prev_sample() を追加する
  - 時系列順に前のサンプルを取得できるようにする
  - C API に mp4_file_demuxer_prev_sample() を追加する
  - @sile
- [ADD] Mp4FileDemuxer に seek() を追加する
  - 指定した時刻にシークできるようにする
  - C API に mp4_file_demuxer_seek() を追加する
  - @sile
- [ADD] B フレーム関連 box (`ctts` / `cslg` / `sdtp`) の構造体対応を追加する
  - `StblBox` で `ctts` / `cslg` / `sdtp` を decode / encode できるようにする
  - `UnknownBox` 扱いだった `ctts` / `cslg` / `sdtp` を通常 box として扱うようにする
  - @sile
- [CHANGE] C API の `mp4_file_muxer_set_reserved_moov_box_size()` の `size` 引数の型を `u64` から `u32` に変更する
  - 理由:
    - `mp4_estimate_maximum_moov_box_size()` の返り値は `u32` なので一貫性がない
    - moov ボックスのサイズが 4GB に収まらないことは現実的にはあり得ないので `u64` は過剰
    - `u64` だと wasm にして JavaScript から呼ぶ場合に、通常の数値型ではなく BigInt を使う必要があって少し煩雑になる
  - @sile
- [CHANGE] MoovBox に mvex_box フィールドを追加する
  - fMP4 対応のため `mvex_box: Option<MvexBox>` フィールドを追加した
  - MoovBox を直接構築しているコードは `mvex_box: None` を追加する必要がある
  - @voluntas
- [FIX] EsDescriptor のエンコード時に URL 文字列長が 255 バイトを超える場合にエラーを返すようにする
  - URL 文字列長は u8 で表現されるため、255 バイトまでしか扱えない
  - これまでは長すぎる URL 文字列を与えた場合に暗黙的に切り捨てられていたが、明示的にエラーを返すようにした
  - @sile
- [FIX] AvccBox のエンコード時に SPS/PPS の数が仕様の上限を超える場合にエラーを返すようにする
  - ISO/IEC 14496-15 の仕様では SPS と PPS の数はそれぞれ最大 31 個までと定められている
  - これまでは 32 個以上の SPS/PPS を与えた場合に暗黙的に切り捨てられる可能性があったが、明示的にエラーを返すようにした
  - エラーメッセージも "Too many SPSs" から "Too many SPSs (max 31)" に改善し、上限値を明示するようにした
  - @sile
- [FIX] Mp4FileMuxer が使用した SampleEntry に応じて ftyp の compatible brands を更新する
  - 初期化時の ftyp は最小構成 (`isom` / `iso2` / `mp41`) にして、finalize 時に実際に使われた映像 SampleEntry (`avc1` / `hev1` / `hvc1` / `av01`) を反映する
  - ftyp 更新用の予約領域と faststart 用 moov 予約領域を単一 free ボックスで共有し、finalize 時に先頭領域を一括更新する
  - @sile
- [FIX] rustdoc の警告を解消する
  - 構造体やメソッドの参照を明示し、モジュールをまたぐ参照は `crate::` のパス付きリンクにする
  - @sile

### misc

- [UPDATE] CI で cargo doc の警告をエラー扱いにする
  - rustdoc の警告を検出した場合に CI が失敗するようにする
  - @sile
- [ADD] CI で wasm ビルドを artifact としてアップロードする
  - release-wasm プロファイルと wasm-opt で最適化した mp4_wasm.wasm を含む
  - @voluntas
- [ADD] Release で wasm バイナリを公開する
  - release-wasm プロファイルと wasm-opt で最適化した mp4_wasm.wasm を含む
  - @voluntas
- [ADD] examples/fmp4.rs を追加する
  - @voluntas
- [ADD] pbt/ 以下に PBT テストを追加する
  - MP4 および fMP4 の PBT テストを含む
  - make pbt で実行できる
  - make pbt-with-cover で llvm-cov を利用したカバレッジ計測付きで実行できる
  - @voluntas
- [ADD] fuzz/ 以下に cargo-fuzz 用のターゲットを追加する
  - MP4 および fMP4 (`fuzz_fmp4_segment_demux`, `fuzz_fmp4_file_demux`) のファジングターゲットを含む
  - make fuzzing で全ターゲットを 30 秒ずつ実行できる
  - @voluntas
- [CHANGE] Slack 通知を shiguredo/github-actions の slack-notify に移行する
  - rtCamp/action-slack-notify から Docker レスの Composite Action に変更
  - failure_and_fixed モードで failure と fixed のみ通知する
  - @voluntas

## 2026.1.0

- [UPDATE] `Mp4FileDemuxer::handle_input()` に要求を満たさない入力データが渡された場合はエラー扱いにする
  - 今まではエラー扱いではなく、単に無視していた
  - ただし、これでは利用側のコード次第では `required_input()` と `handle_input()` の無限ループが発生するリスクがあった
  - 例えば、以下のようなケースがこれに該当する:
    1. `required_input()` を呼び出す
    2. 1 の要求よりも小さいサイズの入力を指定して `handle_input()` を呼び出す
    3. 要求が満たされていないので demuxer の内部状態は変わらない
    4. 1 に戻る
  - これは呼び出し元で 2 の時点で確実に要求を満たした入力を渡すようにすれば発生しない問題ではあるが、バグの温床にはなり得る
  - そのため 3 の時点で「要求が満たされていない場合」をエラー扱いにするようにする（異常系で、よりデフェンシブな挙動にする）
    - 要求が満たされていない入力を受け取った demuxer はエラー状態に遷移する
    - エラー状態に遷移した場合は、その次の `required_input()` は `None` を返すため、ループはここで終了する
    - そして、その後に `Result` を返すメソッドを呼び出した時点でエラーが返されるようになる
  - @sile
- [ADD] hvc1 ボックス対応を追加する
  - H.265 ストリームを表現するために hev1 に加えて hvc1 を使えるようにする
  - hev1 と hvc1 はボックス自体の仕様は同じだが、後者の場合は映像サンプルデータの先頭に SPS などの NALU の配置が必須となる（hev1 の場合は任意）
  - なお Apple 系のプレイヤーでは hvc1 の再生しかサポートされていない
  - @sile
- [CHANGE] `std` feature フラグを削除する
  - sans I/O 対応に伴いほぼ不要となったので削除する
  - 今後は no_std 前提の crate とする
  - これによって、`std` 有効時に使えていた以下の機能は廃止となる:
    - `Error` 構造体の `backtrace` フィールド（`std::backtrace::Backtrace`):
      - そもそもほぼ使うことがなかったのと、以下理由による削除は問題ないと判断:
        - mp4-rs はその性質上、問題が発生しても、同じ入力を与えて関数を実行すれば確実に再現できる
        - 問題が発生した MP4 ボックスの名前やファイルの情報は常にエラーに含まれるので、通常はそれで十分
    - `Mp4FileMuxerOptions` 構造体の `creation_timestamp` フィールドのデフォルト値:
      - `std` 有効時には現在時刻を使っていたのが、UNIX エポック時刻固定となる
      - ただし、そもそも現在時刻を参照するのは、sans I/O としての設計に合わないので、必要なら利用側が指定するのが正しい（ので削除しても問題はないと判断）
  - @sile
- [FIX] 不正なボックス構造を与えると `SampleTableAccessor::new()` がパニックする可能性のある問題を修正する
  - 「チャンクが存在するのに stsc ボックスにエントリーが存在しない」ケースで減算がアンダーフローすることがあった
  - このケースに対するチェックを追加して、明示的にエラーを返すようにする
  - @sile
- [FIX] 入力データが破損してる場合に descriptors モジュール内の構造体のデコード時に算術オーバーフローが発生する可能性がある問題を修正する
  - 入力データからデコードしたサイズ値が極端に大きい場合に加算がオーバーフローしてパニックする可能性があった
  - @sile
- [FIX] 入力データが破損している場合などに、デコード時に大量のメモリを消費する可能性がある問題を修正する
  - `Vec::with_capacity()` を使っている箇所を `Vec::new()` に置換する
  - `Vec::with_capacity()` は実際には不要だとしても、指定のサイズ分のメモリを事前に確保しようとするため、入力データが破損していると大量のメモリが必要になる可能性があった
  - mp4-rs のユースケースでは `with_capacity()` と `new()` で、性能に有意な差が出ることも考えにくいため、今後は常に `new()` を使うようにする
  - @sile
- [FIX] HvccBox, VpccBox, EsDescriptor 関連のデコード時にスライス境界チェックを追加する
  - 破損データでサイズが巨大な値になると、ペイロード長を超えて範囲外アクセスが発生し panic するのを防ぐ
  - @voluntas

### misc

- [UPDATE] boxes.rs を boxes_moov_tree.rs と boxes_sample_entry.rs に分割する
  - 行数が大きくなりすぎていたので、二つに分割する
  - @sile

## 2025.4.0

- [UPDATE] avcC ボックスのデコード時にペイロード境界チェックを追加する
  - SPS/PPS/SPS EXT データ読み込み時にオフセットがペイロード範囲内にあるかをチェックし、範囲外ならエラーにする
  - @sile
- [ADD] FLAC 対応を追加する
  - FLAC を扱うのに必要な以下のボックスへの対応を追加する:
    - fLaC
    - dfLa
  - @sile
- [CHANGE] MinfBox 構造体の smhd_or_vmhd_box フィールドを Option 型に変更する
  - メディアトラック以外を含む MP4 ファイルの読み込みに対応するための変更
  - @sile
- [FIX] メディアトラック以外を含む MP4 ファイルの読み込みに失敗する問題を修正する
  - minf ボックスの中には smhd ボックス（音声）ないし vmhd ボックス（映像）が必ず存在する想定の実装となっていたため、そうではない場合にエラーになっていた
  - メディアトラック以外の場合には、minf ボックスがこれらを含まないため、それを許容するようにする
  - @sile
- [FIX] vmhd ボックスのデコード時にフラグの値チェックを緩和する
  - ISO/IEC 14496-12 の仕様では vmhd ボックスのフラグの値は 1 になると記載されているが、実装によっては 0 となるファイルも存在するため、このチェックは行わないようにする
  - @sile
- [FIX] AAC の MP4 読み込み時に DecoderConfigDescriptor が存在しないとエラーになる問題を修正する
  - ISO/IEC 14496-1 の仕様としては DecoderConfigDescriptor はオプショナルだが、実装が必須扱いになっていたのでエラーとなっていた
  - 仕様に合わせてオプショナル扱いとするように実装を修正する
  - @sile
- [FIX] H.264 の MP4 ファイルの読み込み時に、仕様に準拠しない avcC ボックスのデコードでエラーになる問題を修正する
  - H.264 のプロファイルが 66 | 77 | 88 以外の場合、ISO/IEC 14496-15 の仕様では avcC ボックスの末尾に追加フィールドが存在することになっている
  - しかし MP4 ファイル作成ライブラリやツールによっては、これを省略する実装が存在するため、ボックスのペイロード終端に達した場合は追加フィールドの処理をスキップするようにした
  - @sile
- [FIX] sans-I/O 対応の際にエラーメッセージに載る情報が不十分になっていたのを修正する
  - デコードエラー時に「どのボックスでエラーが発生したか」の情報を載せるようにする
  - エラー発生行番号が、新規に追加した共通関数（`Decode::decode_at()`）の中の位置ではなく、その呼び出し元の位置になるようにする
  - @sile

### misc

- [UPDATE] C API の不透明型を cbindgen の forward declaration 機能で実装する
  - `#[repr(C)]` を外すことで cbindgen が forward declaration のみを出力するようにした
  - ダミー構造体を削除し、実装構造体を直接使用するようにコードを簡素化した
  - @voluntas
- [ADD] C API の C++ 互換性を追加する
  - `extern "C"` ブロックを追加し、C++ から利用可能にする
  - @voluntas
- [ADD] CI で C API ライブラリを artifact としてアップロードする
  - @voluntas
- [ADD] Windows 用の C API ライブラリリリースを追加する
  - @voluntas
- [ADD] examples/demux.rs を追加する
  - @sile
- [ADD] Windows と macOS を CI 対象に追加する
  - @sile

## 2025.3.0

- [ADD] C 言語バインディングを追加する
  - MP4 ファイルのマルチプレックス・デマルチプレックス機能を C 言語から利用するための API を提供する `crates/c-api/` を追加した
    - このクレートは別の Rust ライブラリ から利用されることを想定していないため、crates-io には登録しない
  - `mp4_file_demuxer_*` 関数群により、MP4 ファイルの読み込みと時系列順のサンプル抽出が可能になった
  - `mp4_file_muxer_*` 関数群により、複数のメディアトラックからサンプルを統合して MP4 ファイルを構築できるようになった
  - サンプルプログラム（`examples/demux.c`, `examples/remux.c`）とテストプログラム（`tests/simple_mux_demux.c`）を追加した
  - @sile
- [ADD] MP4 ファイルのマルチプレックス機能を追加する
  - 複数のメディアトラック（音声・映像）からのサンプルを時系列順に統合して、MP4 ファイルを構築するための `mux` モジュールを追加した
  - 新しく追加された `Mp4FileMuxer` 構造体により、段階的にサンプルを追加して MP4 ファイルを構築できる
  - I/O 操作に依存しない設計で、ファイル書き込みは利用側で実施する
  - @sile
- [ADD] MP4 ファイルのデマルチプレックス機能を追加する
  - MP4 ファイルから複数のメディアトラック（音声・映像）内のサンプル群を時系列順に分離して抽出するための `demux` モジュールを追加した
  - 新しく追加された `Mp4FileDemuxer` 構造体により、段階的にファイルデータを処理し、サンプルを順序付けて取得できる
  - I/O 操作に依存しない設計で、ファイル読み込みは利用側で実施する
  - @sile
- [ADD] no_std 環境のサポートを追加する
  - `default-features = false` を指定することで no_std 環境でも利用可能になった
  - std 環境がデフォルトなので、既存のコードへの影響はない
  - @voluntas
- [CHANGE] `MdatBox::is_variable_size` フィールドを削除する
  - 4 GB までのペイロードしか扱えず中途半端だったので、`MdatBox` 構造体から `is_variable_size` フィールドを削除した
  - 今後は可変長ペイロードを表現する場合は、`MdatBox` ではなく [`BoxHeader`] を直接使用する必要がある
  - @sile
- [CHANGE] IgnoredBox 構造体を削除する
  - この構造体は Decode トレイトの古い設計前提であったので、設計変更に伴い不要となった
  - @sile
- [CHANGE] Error 構造体の std::io::Error への依存をなくす（sans-I/O 対応）
  - std::io モジュールへの依存をなくしたのに伴い、独自の ErrorKind enum を定義し、使用するようにした
  - @sile
- [CHANGE] Encode および Decode トレイトを I/O に依存しない設計に変更する（sans-I/O 対応）
  - モチベーション: no_std / wasm / C API に対応する際に、I/O と密結合になっていると取り回しが難しいので、mp4-rs レイヤーでは I/O に依存しないようにする
  - std::io::{Read, Write} に対してではなく、バッファ（&[u8]）に対して操作を行うように変更した
  - @sile

## 2025.2.0

- [CHANGE] 最小サポート Rust バージョンを 1.88 に設定する
  - `let-else` 構文を使い始めたため
  - @sile
- [FIX] Windows でリポジトリの clone に失敗する問題を修正する
  - Windows での予約ファイル名に衝突する `aux.rs` がリポジトリに含まれていたのが原因だった
  - ファイル名を `auxiliary.rs` に変更した上で、その中身を `lib.rs` の中でインラインで定義された `aux` モジュールに再エクスポートすることで対応した
    - 外部インターフェースへの変更は発生しないので、以前のバージョンとの互換性は維持されている
  - @sile

### misc

- [UPDATE] GitHub Actions の ci.yml で使用する Ubuntu のバージョンを 24.04 に固定する
  - @voluntas
- [UPDATE] clippy 0.1.89 に対応する
  - @sile
- [UPDATE] clippy 0.1.88 に対応する
  - @sile
- [UPDATE] actions/checkout を v5 に上げる
  - @miosakuma
- [ADD] GitHub Actions の ci.yml を平日 10:00 (JST) に実行するようにする
  - @voluntas

## 2025.1.0

- [ADD] AAC 関連の定数を追加する
  - MP4 に AAC ストリームを格納する際に、典型的に使用される値を以下の定数として定義した:
    - `EsDescriptor::MIN_ES_ID`
    - `EsDescriptor::LOWEST_STREAM_PRIORITY`
    - `DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3`
    - `DecoderConfigDescriptor::STREAM_TYPE_AUDIO`
    - `DecoderConfigDescriptor::UP_STREAM_FALSE`
  - @sile
- [ADD] `AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX` 定数を追加する
  - 通常の用途では `data_reference_index` には常にこの定数値が設定されることになる
  - @sile
- [CHANGE] `AudioSampleEntryFields.data_reference_index` の型を `u16` から `NonZeroU16` に変更する
  - 値が 0 になることはないため non zero 版に変更した
    - `VisualSampleEntryFields.data_reference_index` は元々 `NonZeroU16` だったので、両者の齟齬の解消も兼ねている
  - @sile
- [CHANGE] Rust のエディションを 2021 から 2024 に上げる
  - @sile
- [FIX] ディスクリプターのサイズがリトルエンディアンでエンコードされていたのを修正する
  - @sile

## 2024.4.0

- [UPDATE] `ChunkAccessor` と `SampleAccessor` の一部のメソッドのライフタイム制約が必要以上に厳しかったのを緩くする
  - @sile
- [ADD] AAC 用のボックスを追加する
  - @sile

## 2024.3.0

- [UPDATE] `SampleTableAccessor` が borrowed / owned の両方に対応できるようにする
  - @sile
- [UPDATE] 共通関数でエラーが発生した場合のファイル名・行番号表示を改善する
  - 今までは共通関数のエラー位置が `Error` に含まれていたが、それでは情報量が少ないので、その一つ上の呼び出し元の位置を使うように変更した
  - @sile
- [ADD] デコード時にペイロードデータを保持しない `IgnoredBox` を追加する
  - @sile
- [ADD] `SampleTableAccessor::get_sample_by_timestamp()` を追加する
  - @sile
- [ADD] `SampleAccessor::timestamp()` を追加する
  - @sile
- [ADD] `SampleAccessor::sync_sample()` を追加する
  - @sile
- [CHANGE] `Encode::encode()` が `writer: &mut W` ではなく `writer: W` を引数に取るように変更する
  - @sile
- [CHANGE] `Decode::decode()` が `reader: &mut R` ではなく `reader: R` を引数に取るように変更する
  - @sile
- [CHANGE] `SampleTableAccessor::new()` で stco ボックスと stsc ボックスの不整合をチェックするようにする
  - @sile

## 2024.2.0

- [UPDATE] ボックスに `Hash` を実装する
  - @sile
- [UPDATE] `Error` 構造体にエラー発生箇所特定用のフィールドを追加する
  - エラー発生時のボック種別、および、エラー発生ファイルと行番号、の情報を取得できるようにした
  - @sile
- [ADD] WebCodecs を使ってローカルで MP4 ファイルを変換するサンプルを追加する
  - @sile
- [ADD] `StblBox` の情報へのアクセスを簡単かつ安全にするための `SampleTableAccessor` 構造体を追加する
  - @sile
- [ADD] `SttsBox::from_sample_deltas()` 関数を追加する
  - @sile
- [ADD] `Utf8String::into_null_terminated_bytes()` メソッドを追加する
  - @sile
- [CHANGE] 仕様上 0 を取らないフィールドの型は `NonZeroXXX` にする
  - @sile
- [CHANGE] `BoxHeader` 書き込み時に large size にするかどうかの自動判定は行わないようにする
  - `BoxSize` 自体はどちらを使うべきかの情報を有しているので、それをそのまま反映するようにした
  - @sile
- [FIX] hdlr ボックスの name フィールドは単なるバイト列として扱うようにする
  - ISO の仕様上は、このフィールドは null 終端の UTF-8 文字列であるべきだが、それに準拠しない MP4 ファイルを生成する実装が普通に存在するため、中身を厳密にチェックしないようにした
  - @sile
- [FIX] 64 bit のボックスサイズが使われていた場合にペイロードのデコードに失敗する問題を修正する
  - @sile

## 2024.1.0

**公開**
