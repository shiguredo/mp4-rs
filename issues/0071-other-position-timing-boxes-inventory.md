# 位置・時刻調整系ボックス群の対応状況と方針を整理する

- Created: 2026-08-19
- Completed: {YYYY-MM-DD}
- Branch: feature/other-position-timing-boxes-inventory
- Polished: {YYYY-MM-DD}

## 目的

MP4 / fMP4 の「位置調整系」および「時刻調整系」に分類できるボックス群のうち、`shiguredo_mp4` で対応が薄い / 未対応のものを 1 か所に棚卸しし、それぞれの実用重要度と対応方針の目安を残す。

このアンブレラは「情報の網羅性を保つ」ことを目的とする。実装 issue は本 umbrella からは直接切らず、実運用で必要になった時点で個別 issue として `create-issue` で起票する。収録するボックスの中には「対応する予定なし」に分類されるものもあるが、後から「そういえばあのボックスの扱いはどうなっていた?」を判断するときに漏れないよう、情報としては記載する。

## 現状

2026-08-19 時点の対応状況。

| ボックス | parse/encode | mux 生成 | demux 反映 | 一次実装場所 (存在するもの) |
|---|---|---|---|---|
| `edts` / `elst` | 実装済み | 常に `None` | 反映しない | `src/boxes_moov_tree.rs` |
| `ctts` | 実装済み | 生成する | `Sample::composition_time_offset` として露出 | `src/boxes_moov_tree.rs` / `src/mux_mp4_file.rs` |
| `cslg` | 実装済み | 常に `None` | 反映しない | `src/boxes_moov_tree.rs` |
| `tfdt` | 実装済み | 生成する | `Fmp4SegmentDemuxer` で `base_media_decode_time` を反映 | `src/boxes_fmp4.rs` / `src/demux_fmp4_segment.rs` |
| `prft` | 未実装 | - | - | - |
| `saio` / `saiz` | 未実装 | - | - | - |
| `sbgp` / `sgpd` | 未実装 | - | - | - |
| `stps` | 未実装 | - | - | - |
| `stsh` | 未実装 | - | - | - |
| `stdp` | 未実装 | - | - | - |
| `padb` | 未実装 | - | - | - |
| `tref` | 未実装 | - | - | - |

fMP4 系のボックス (`prft` など) の対応は当面低優先度扱いとする。

## 設計方針

本 umbrella では実装 issue を持たない。実装が必要になった時点で、以下の実用重要度と方針を参照しつつ `create-issue` で個別 issue に切り出す。

### 実用重要度と対応方針

| 項目 | 実用重要度 | 対応方針の目安 |
|---|---|---|
| `edts` / `elst` の demux 反映 | 中 (B-frame ありのファイルで PTS が typically 1〜2 サンプル分ズレる。A/V 同期をシビアに測る用途や外部字幕マッチングでは実害あり) | 設計選択肢を別 issue `issues/0070-add-edit-list-demux-support.md` に残す。実害の実例が確認できた時点で採用選択肢を確定し、実装用の別 issue を切る |
| `edts` / `elst` の mux 生成 | 中〜低 (B-frame を含む映像を muxer で出力するときに書かないと再生互換性が落ちる。ただし `shiguredo_mp4` の現在の主用途では該当ケースが少ない可能性) | 要求が出た時点で個別 issue 化。demux 側 (`issues/0070-add-edit-list-demux-support.md`) の設計選択肢と歩調を合わせる |
| `cslg` の mux 生成 | 中〜低 (負値の `composition_time_offset` を書く場合の仕様上の推奨。mux 側で `edts` を書くようになるタイミングで併せて検討) | mux 側 `edts` 対応時に併せて判断 |
| `prft` (Producer Reference Time) の parse/encode | 低 (fMP4 特化。CMAF ライブ配信で NTP / UTC 絶対時刻をメディア時刻に対応付ける用途) | 要求が出るまで見送り。fMP4 系は当面低優先度扱い |
| `saio` / `saiz` (Sample Auxiliary Information Offset/Size) の parse/encode | 低 (実運用ではほぼ CENC 用途。`shiguredo_mp4` は暗号化 / DRM をスコープに入れていない) | 暗号化対応の議論が発生した時点で再検討 |
| `sbgp` / `sgpd` (Sample Group) の parse/encode | 中 (Opus の `roll` grouping = pre-roll サンプル数指示は仕様上の SHOULD。AAC / HEVC の open GOP ランダムアクセス精度にも効く) | Opus / AAC トラックのシーク精度改善要求が具体化した時点で個別 issue 化 |
| `stps` (Partial Sync Sample) の parse/encode | 低 (open GOP のランダムアクセス点指示。近年は `sap` sample group での代替が主流化。プレイヤー側もあまり参照しない) | 要求が出るまで見送り |
| `stsh` (Shadow Sync Sample) の parse/encode | 極低 (事実上デッド機能。生成する encoder も読む decoder もほぼない) | 対応する予定なし。情報として本 umbrella に記載のみ |
| `stdp` (Degradation Priority) の parse/encode | 極低 (帯域絞り時のサンプル破棄優先度ヒント。実装している encoder / decoder をほぼ見ない) | 対応する予定なし。情報として本 umbrella に記載のみ |
| `padb` (Padding Bits) の parse/encode | 極低 (古い MPEG-4 audio 向けのサンプル末尾パディングビット指示。現行 encoder は生成しない) | 対応する予定なし。情報として本 umbrella に記載のみ |
| `tref` (Track Reference) の parse/encode | 中 (トラック間参照。字幕→映像の `subt`、チャプター `chap`、依存関係 `vdep`、hint `hint` などを表す。同一 kind 複数トラック対応との関連あり) | `issues/pending/0049-add-multiple-tracks-per-kind.md` の設計判断が固まるタイミングで、必要になれば個別 issue 化 |

### 対象外

- 個別ボックスの実装 issue の起票 (本 umbrella では起票しない。要求が出た時点で `create-issue` で切り出す)
- CHANGES.md への記録 (本 umbrella 自体はコード変更を伴わないため現時点では記載しない。個別 issue 化した時点で当該 issue 側で記載する)
- `docs::hybrid_mp4` や `docs::subtitle` のような crate 内 doc モジュールへの反映 (必要になった時点で別 issue 化)

## 完了条件

収録した全ボックスについて、以下のいずれかが成立した状態を目指す。両方が満たされた時点で本 umbrella を close する。

- 対応が確定 (実装完了、または「対応する予定なし」で確定) している
- 対応する個別 issue が起票されており、追跡がそちら側に移っている

対応状況の変化や実用重要度の判断の変化があった場合は、本 issue を更新する。

## 関連 issue

- `issues/0070-add-edit-list-demux-support.md` (`edts` / `elst` の demux 反映方式の設計選択肢)
- `issues/pending/0049-add-multiple-tracks-per-kind.md` (同一 `TrackKind` の複数トラック対応。`tref` と関連)
