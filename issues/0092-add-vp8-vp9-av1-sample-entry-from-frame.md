# VP8 / VP9 / AV1 のフレームから sample entry を構築する高レベル API を追加する

- Created: 2026-08-28
- Completed: {YYYY-MM-DD}
- Branch: feature/add-vp8-vp9-av1-sample-entry-from-frame
- Polished: {YYYY-MM-DD}

## 目的

1 フレーム分のエレメンタリーストリームから `Vp08Box` / `Vp09Box` / `Av01Box` を組み立てる公開 API を追加する。H.264 / H.265 にある `build_*_from_annexb` と同様に、ビットストリーム解析と box 構築を 1 回の呼び出しにまとめ、利用側のボイラープレートを減らす。

## 現状

- H.264 / H.265 には `build_avc1_box_from_annexb` / `build_hvc1_box_from_annexb` がある
- VP8 / VP9 / AV1 は低レベル API のみ
  - VP8: `parse_frame_header` + `build_vp08_box`（寸法・色は `Vp8SampleEntryConfig`）
  - VP9: `parse_frame_header` + `build_vp09_box`（vpcC の一部は header、visual 寸法・色・level は `Vp9SampleEntryConfig`）
  - AV1: `parse_obus` + `parse_sequence_header` + `parse_frame_header_prefix` + `build_av01_box`（または `build_av01_box_from_config_obus`）
- 利用側は「キーフレーム判定」「寸法の取り出し」「RAP 判定」「configOBUs の用意」を毎回手でつなぐ必要がある
- 既存の低レベル API は、トラック全体の最大寸法を呼び出し側が集約する用途向けであり、その設計自体は維持する

## 設計方針

各コーデックに「1 フレームのバイト列から box を返す」公開関数を追加する。命名は既存の `_from_annexb` に合わせて `_from_frame` とする。

候補:

- `bitstream::vp8::build_vp08_box_from_frame(data, color_config) -> Result<Vp08Box>`
- `bitstream::vp9::build_vp09_box_from_frame(data, color_and_level_config) -> Result<Vp09Box>`
- `bitstream::av1::build_av01_box_from_frame(data, config) -> Result<Av01Box>`

方針の詳細:

- ビットストリーム解析は既存の `parse_*` / `build_*` を再利用する。自前パーサーを増やさない
- sample entry を組めないフレームは `ErrorKind::InvalidInput` とする
  - VP8: キーフレーム以外
  - VP9: `build_vp09_box` が受理しない header（key / `intra_only` 以外）、または `frame_size` が `Resolved` でない場合
  - AV1: Sequence Header が無い、または `parse_frame_header_prefix` の結果が RAP でない（Key かつ `show_frame = 1` でない）場合
- visual 寸法は当該フレーム由来とする（VP8 は keyframe の width / height、VP9 は `Vp9FrameSize::Resolved`、AV1 は Sequence Header の max frame size）。トラック全体の最大寸法を渡す既存経路（`build_vp08_box` / `build_vp09_box` + Config）は残す
- 色特性・level・`initial_presentation_delay_minus_one` などストリームから一意に決まらない値は、既存 Config 型で受け取る
- AV1 の `av1C.configOBUs` には Sequence Header OBU だけを入れる。Sample 文脈から取った OBU を ConfigObus 規則へ載せる必要がある場合は、正規化ヘルパー（別 issue）があればそれを使う
- 戻り値は既存の `build_*_box` と同様に box 型とし、`SampleEntry` への包装は呼び出し側に任せる

## 完了条件

- VP8 / VP9 / AV1 それぞれに `_from_frame` API が公開されている
- 組めるフレームでは期待どおりの寸法・codec config を持つ box が返る
- 組めないフレームではエラーになる
- 既存の低レベル `build_*_box` / `build_av01_box_from_config_obus` の挙動と公開面は壊していない
- ユニットテストで上記を検証している
- `cargo test` が pass する
