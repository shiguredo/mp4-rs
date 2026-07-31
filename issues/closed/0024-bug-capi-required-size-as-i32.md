# c-api の required_input_size が usize as i32 で切り捨てられ -1（EOF）と衝突する

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-31
- Model: opencode-go glm-5.2
- Branch: feature/fix-capi-required-size-as-i32
- Polished: 2026-07-31

## 目的

C API の `mp4_file_demuxer_get_required_input` / `mp4_file_kind_detector_get_required_input` で `required.size`（`Option<usize>`）を `as i32` で変換しており、`> i32::MAX` で負値になり `-1`（末尾までの意味）と衝突する問題を修正する。

## 優先度根拠

巨大 moov 等で要求サイズが `i32::MAX`（約 2 GiB）を超えると負値になり、API 上の `-1`（末尾まで必要）と誤認される。呼び出し側が不足バッファを供給して不正デコード・状態破壊に至る。`usize as i32` は黙って下位ビット切り捨てするため、コード上からは異常が見えず暗黙に破綻する点で危険度が高い。

一方で、実運用で > 2 GiB の moov が現れるケースは限定的なため Medium に留める。

## 現状

`crates/c-api/src/demux.rs` の `mp4_file_demuxer_get_required_input` 関数と、`crates/c-api/src/mp4_file_kind_detector.rs` の `mp4_file_kind_detector_get_required_input` 関数で、それぞれ以下の変換を行っている:

```rust
*out_required_input_size = required.size.map(|n| n as i32).unwrap_or(-1);
```

`RequiredInput.size` は `Option<usize>`（`src/demux_mp4_file.rs` の `RequiredInput` 型）。`n as i32` は `n > i32::MAX` で下位 32 ビットを符号付き再解釈するため、例えば `usize = 0xFFFF_FFFF` → `i32 = -1` となり `-1`（末尾まで必要）と衝突する。`> i32::MAX` の入力はどれも負値ないし過小な正値に化ける。

API 仕様上、`out_required_input_size` の値は以下のとおり定義されている（`crates/c-api/include/mp4.h` の `mp4_file_demuxer_get_required_input` と `mp4_file_kind_detector_get_required_input` のドキュメントコメント）:

- `0`: これ以上の入力が不要
- `-1`: ファイル末尾までのデータが必要
- 正値: そのサイズ以上の入力が必要

さらに `mp4_file_kind_detector_get_required_input` のドキュメントコメントには「大きなサイズを要求するのは実質的には `moov` ボックス本体であり、`mdat` のような巨大ペイロードを丸ごと要求することはない想定である。そのため、サイズ表現には `int32_t` を使っている」と明記されており、`int32_t` に収まらないサイズは API の対象外である旨が示唆されている。

`RequiredInput.size` に大きな値が入り得るのは `Phase::ReadFtypBox` の box_size と `Phase::ReadMoovBox` の box_size のみで、実質的には moov box_size が `i32::MAX`（約 2 GiB）を超えた場合が該当する。

## 設計方針

`i32::try_from(n)` で変換し、変換失敗時は `MP4_ERROR_UNSUPPORTED` を返して `last_error` に詳細を記録する。

- **飽和は採用しない**: `i32::MAX` にクランプすると、呼び出し側は提示サイズどおりの入力を渡したつもりでも、内部の `RequiredInput` は実サイズ（`> i32::MAX`）のままなので要求を満たせない。その結果の観測は次のとおりで、いずれも「サイズは提示どおり供給したのに別の失敗になった」という不透明な UX になる。エラーを即返した方が「> 2 GiB moov は非対応」と明確に伝わる。
  - **demuxer** (`Mp4FileDemuxer::handle_input`): `RequiredInput::is_satisfied_by()` が `false` になり、`DemuxError::DecodeError(Error::invalid_input(...))` を記録する。C API の `mp4_file_demuxer_handle_input` は常に `MP4_ERROR_OK` を返すため、直後の `mp4_file_demuxer_get_required_input` は `required_input() == None` により `out_required_input_size = 0`（完了に見える）を返し、表面化するのは後続 API（`get_tracks` 等）での `MP4_ERROR_INVALID_INPUT` になる。
  - **kind detector** (`Mp4FileKindDetector::handle_input`): 位置一致かつ短すぎる入力は `input_is_acceptable` で受理され、`available_bytes` が `Error::invalid_data` を返す。C API の `mp4_file_kind_detector_handle_input` はその場で `MP4_ERROR_INVALID_DATA` を返す。
- **`int64_t` への拡張は採用しない**: ABI 破壊になる。加えて `mp4.h` のドキュメントコメントで「`int32_t` を使う」と設計意図が明示されているため、その前提を維持する。
- **エラーコードに `MP4_ERROR_UNSUPPORTED` を選ぶ理由**: ヘッダで「`int32_t` を前提とし巨大ペイロードは想定しない」と設計上の制約を明示している以上、それを超えるサイズは「API がサポートしていない」に該当する。`MP4_ERROR_OTHER` は「上記以外」で意味が弱く、`MP4_ERROR_INVALID_DATA` はデータ破損を含意するため不適。
- **内部状態は変えない**: この関数はまだ入力を受け取っておらず状態遷移を伴わないため、demuxer / detector を error 状態にはせず、戻り値と `last_error` だけで通知する。
- **エラー時は両 out を更新しない**: `MP4_ERROR_UNSUPPORTED` を返すときは `out_required_input_position` / `out_required_input_size` を書き込まない。既存の NULL ポインタ経路（および kind detector のエラー状態経路）と同じ契約にし、位置だけ書いてから失敗する・切り捨て値を書いてから `UNSUPPORTED` を返す、といった再発経路を防ぐ。成功時（`MP4_ERROR_OK`）にのみ両 out を設定する。
- **変換ロジックの共通化**: 同一の変換が 2 関数に重複しているため、`RequiredInput.size`（`Option<usize>`）を `Result<i32, ...>` に変換する小さなヘルパー関数として切り出し、両呼び出し箇所から使う。切り出すことで純粋関数として単体テスト可能になる（CLAUDE.md でモック禁止のため、実 MP4 ではなくヘルパー単体でテストする方針を採る）。

## 完了条件

- `required.size > i32::MAX` の場合に `-1` と衝突せず、`MP4_ERROR_UNSUPPORTED` が返ること
- そのとき `out_required_input_position` / `out_required_input_size` が更新されていないこと（呼び出し前の値が残ること）
- `last_error` に「要求サイズが `i32::MAX` を超えた」旨の情報が記録されること
- `crates/c-api/include/mp4.h` の `mp4_file_demuxer_get_required_input` / `mp4_file_kind_detector_get_required_input` のドキュメントコメントに、「`MP4_ERROR_UNSUPPORTED` が返る条件」として本ケースが記載されていること
- 変換ヘルパー関数に対する単体テストが追加されていること（`None` / `0` / `1` / `i32::MAX as usize` / `i32::MAX as usize + 1` / `usize::MAX` を含む境界値）
- `cargo test` / `cargo clippy` が通ること

## 解決方法

設計方針どおり `i32::try_from` + `MP4_ERROR_UNSUPPORTED` を採用した。

- `crates/c-api/src/error.rs` に `required_input_size_to_i32` を追加し、`None` → `-1`、`Some(n)` は `i32::try_from`、超過時はエラーメッセージ付き `Err` とした
- `mp4_file_demuxer_get_required_input` / `mp4_file_kind_detector_get_required_input` でヘルパーを使い、`Ok` のときだけ両 out を設定、`Err` のときは out 未更新で `set_last_error` + `MP4_ERROR_UNSUPPORTED` を返すようにした
- 境界値単体テスト（`None` / `0` / `1` / `i32::MAX` / `i32::MAX+1` / `usize::MAX`）を追加した
- `mp4.h`（および rustdoc）に `MP4_ERROR_UNSUPPORTED` 条件と「非 OK 時は out を読まない」使用例を追記した
- `CHANGES.md` の `## develop` に `[FIX]` エントリを追加した
