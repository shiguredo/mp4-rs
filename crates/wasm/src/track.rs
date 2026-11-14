//! デマルチプレックス結果（トラック情報・サンプル情報・入力要求）の JSON 変換
//!
//! JavaScript 側に返す小さな構造体を JSON 文字列へ変換する。
//! マルチプレクサへ渡すサンプル JSON の解釈は `crate::mux_sample` が担当する。
//! スキーマは `crates/wasm/README.md` に記載している。
use shiguredo_mp4::{
    TrackKind,
    demux::{RequiredInput, Sample, TrackInfo},
};

use crate::boxes::fmt_json as fmt_json_sample_entry;

/// トラック種別を JavaScript 向けの文字列に変換する
pub(crate) fn track_kind_str(kind: TrackKind) -> &'static str {
    match kind {
        TrackKind::Audio => "audio",
        TrackKind::Video => "video",
        TrackKind::Subtitle => "subtitle",
    }
}

/// トラック情報の一覧を JSON の配列に変換する
pub(crate) fn fmt_track_info_array(tracks: &[TrackInfo]) -> String {
    nojson::json(|f| {
        f.array(|f| {
            for track in tracks {
                f.element(nojson::json(|f| fmt_track_info(f, track)))?;
            }
            Ok(())
        })
    })
    .to_string()
}

/// トラック情報を JSON に変換する
pub(crate) fn fmt_track_info(
    f: &mut nojson::JsonFormatter<'_, '_>,
    track: &TrackInfo,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("track_id", track.track_id)?;
        f.member("kind", track_kind_str(track.kind))?;
        f.member("duration", track.duration)?;
        f.member("timescale", track.timescale.get())
    })
}

/// デマルチプレックスしたサンプルを JSON に変換する
pub(crate) fn fmt_sample(
    f: &mut nojson::JsonFormatter<'_, '_>,
    sample: &Sample<'_>,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("track_id", sample.track.track_id)?;
        // サンプルエントリーは前のサンプルから変化がない場合に省略されるため、
        // その場合はキー自体を出力しない（利用側は直前の値を引き継ぐ）
        if let Some(sample_entry) = sample.sample_entry {
            f.member(
                "sample_entry",
                nojson::json(|f| fmt_json_sample_entry(f, sample_entry)),
            )?;
        }
        f.member("keyframe", sample.keyframe)?;
        f.member("timestamp", sample.timestamp)?;
        f.member("duration", sample.duration)?;
        f.member("composition_time_offset", sample.composition_time_offset)?;
        f.member("data_offset", sample.data_offset)?;
        f.member("data_size", sample.data_size)?;
        Ok(())
    })
}

/// 次に必要な入力データを JSON に変換する
///
/// `size` が `null` の場合はファイル末尾までのデータが必要であることを表す。
pub(crate) fn fmt_required_input(
    f: &mut nojson::JsonFormatter<'_, '_>,
    required: RequiredInput,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("position", required.position)?;
        f.member("size", required.size)?;
        Ok(())
    })
}
