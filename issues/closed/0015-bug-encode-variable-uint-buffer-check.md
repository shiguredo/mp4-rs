# boxes_fmp4.rs の encode_variable_uint がバッファ長を検査せず panic する

- Priority: Medium
- Created: 2026-07-15
- Completed: 2026-07-29
- Model: opencode-go glm-5.2
- Branch: feature/fix-encode-variable-uint-buffer-check
- Polished: 2026-07-28

## 目的

`encode_variable_uint` が `byte_count` 1〜3 のときバッファ長を検査せず `buf[0]` 等に直接書き込むため、短いバッファで panic する問題を修正する。`Encode` トレイトの契約どおり `InsufficientBuffer` を返すようにする。

## 優先度根拠

`Encode` トレイトの doc コメント（`src/codec.rs:120-125`）には「もし `buf` のサイズが不足している場合には `ErrorKind::InsufficientBuffer` エラーが返される」と明記されており、panic は契約違反。

到達経路は公開 API 経由で存在する。`TfraBox` は `pub use crate::boxes_fmp4::{... TfraBox, ...}`（`src/boxes.rs:6-9`）でクレートルートから再公開され、`Encode` トレイト（`pub use codec::{Decode, Encode, ...}`、`src/lib.rs:29`）も公開なので、外部の任意コードが `TfraBox::encode(&mut small_buf)` を直接呼び、内部の `encode_variable_uint`（`src/boxes_fmp4.rs:1193` / `:1198` / `:1203` の 3 箇所から呼ばれる）で panic する。debug ビルドの範囲を超え、release ビルドでも `buf[0] = ...` は panic する（インデックスの境界検査はビルドプロファイルに依存しない）。

Muxer 経路（`Fmp4SegmentMuxer::mfra_bytes()`、`src/mux_fmp4_segment.rs:479-546`）は最終的に `encode_to_vec` を呼び、`InsufficientBuffer` エラーを吸収してバッファを 2 倍化して再試行する（`src/codec.rs:127-143`）。ただし panic は `encode_to_vec` の `match` では吸収されない。`TfraBox::encode`（`src/boxes_fmp4.rs:1188-1207` の version=0 パス）は 1 エントリごとに `t.encode` → `mo.encode` → `encode_variable_uint(traf_number)` → `encode_variable_uint(trun_number)` → `encode_variable_uint(sample_number)` の順で書き出す。`t.encode` / `mo.encode` はいずれも `u32::encode`（`src/codec.rs:164-171`）で先頭に `Error::check_buffer_size(4, buf)?` が入っているため、両者の直後に残バイトが 4 未満なら次の `u32::encode` の側で `InsufficientBuffer` が返って panic には至らない。panic に到達するのは「`mo.encode` の直後、または前エントリの `sample_number` の直後」で残バイトが `byte_count` 未満になり、続く `encode_variable_uint(byte_count=1..=3)` に空〜(byte_count-1) バイトのスライスが渡るケースである（byte_count=1 なら残 0 バイト、byte_count=3 なら残 0〜2 バイト）。したがって Muxer 経路でも入力とバッファ拡張タイミングの組み合わせによっては到達し得る。

優先度は Medium。境界検査の抜けは重大だが、`Fmp4SegmentMuxer::mfra_bytes()` の通常経路では初期 256 バイトから 2 倍化するバッファ拡張パターンにより panic に至る条件が限定的で、公開 API を直接叩く経路も MP4 ライブラリ利用者にとっての標準ワークフローではない。既に Polished 済みの `issues/0013-bug-fullboxflags-shift-overflow.md`（同じく公開 API のシフトオーバーフロー）と同水準。

## 現状

```rust
// src/boxes_fmp4.rs:1407-1428
fn encode_variable_uint(value: u32, byte_count: u8, buf: &mut [u8]) -> Result<usize> {
    match byte_count {
        1 => {
            buf[0] = value as u8;
            Ok(1)
        }
        2 => {
            buf[0] = (value >> 8) as u8;
            buf[1] = value as u8;
            Ok(2)
        }
        3 => {
            buf[0] = (value >> 16) as u8;
            buf[1] = (value >> 8) as u8;
            buf[2] = value as u8;
            Ok(3)
        }
        4 => value.encode(buf),
        _ => Err(crate::Error::invalid_data(
            "Invalid byte count for variable uint",
        )),
    }
}
```

`byte_count` 1〜3 は `buf.len()` を検査せず `buf[0]` / `buf[1]` / `buf[2]` へ直接代入するため、`buf` の長さが不足するとインデックス範囲外で panic する。`byte_count == 4` は `value.encode(buf)`（`Encode for u32`、`src/codec.rs:164-171`）に委譲しており、その先頭で `Error::check_buffer_size(4, buf)?`（`src/codec.rs:83-90`）が走るので `InsufficientBuffer` が返る。

呼び出し元は `TfraBox::encode` の `src/boxes_fmp4.rs:1193-1207` の 3 箇所（`traf_number` / `trun_number` / `sample_number` それぞれ 1〜4 バイト）。`Encode for TfraBox` は先行して `header` / `FullBoxHeader` / `track_id` / `lengths` / `number_of_entry` / エントリごとの `time` / `moof_offset` をすべて `Encode` 経由で書くため、これらは `buf` が不足すれば正しく `InsufficientBuffer` を返す。バグは `encode_variable_uint` の 3 アームだけに閉じている。

### 対の decode 側との差

`decode_variable_uint`（`src/boxes_fmp4.rs:1432-1435`）は `match` の外で 1 回だけ `if *offset + byte_count as usize > buf.len() { return Err(crate::Error::invalid_data("Unexpected end of data")); }` として境界検査している。ただし返しているエラー種別は `ErrorKind::InvalidData` であり、`Encode` トレイト契約が要求する `ErrorKind::InsufficientBuffer` とは別種である（`ErrorKind`: `src/codec.rs:15-27`）。本 issue の修正は「decode と同じ形」ではなく、`Encode for u32` が採用している `Error::check_buffer_size` パターンに揃えるのが正しい。

## 設計方針

`encode_variable_uint` の `match` の直前に `Error::check_buffer_size(byte_count as usize, buf)?` を 1 回だけ置く。`byte_count == 4` の分岐は `value.encode(buf)` 経由で `check_buffer_size(4, buf)?` が走るため二重検査になるが、代入と検査を切り離すことで各アームで検査を書き忘れる余地を無くし、実装を `Encode for u32` 等の他の `Encode` 実装（`src/codec.rs:146-216`）と同じ形（先頭で `check_buffer_size` → データ書き込み）に揃えることを優先する。二重検査は `check_buffer_size` の分岐 1 回で、性能上の意味はない。

エラー種別は `ErrorKind::InsufficientBuffer`。`decode_variable_uint` は `ErrorKind::InvalidData` を返すが、これは decode 側の別問題（本 issue のスコープ外）であり、encode 側は `Encode` トレイト契約と `Encode for u32` の先例に従う。

`_ => Err(crate::Error::invalid_data("Invalid byte count for variable uint"))` の分岐は変更しない（バッファ不足ではなく引数の不正であり、`InvalidData` のままが妥当）。ここへ到達しないように `check_buffer_size` を先に呼ぶと、不正な `byte_count`（5 以上）でバッファがそれより小さい場合に `InvalidData` ではなく `InsufficientBuffer` が返るケースが生じる。`TfraBox::decode` は `& 0x3` でマスクするため decode 由来の値は 0..=3 に収まる（`src/boxes_fmp4.rs:1227-1229`、`byte_count = length_size_of_* + 1` で 1..=4）が、`pub length_size_of_traf_num: u8` / `pub length_size_of_trun_num: u8` / `pub length_size_of_sample_num: u8`（`src/boxes_fmp4.rs:1141` / `:1143` / `:1145`）は公開フィールドで、外部コードが任意の `u8` を代入したうえで `TfraBox::encode` を呼ぶことは可能である。不正な `byte_count` でどちらのエラー種別を返すかは実装者判断だが、境界検査を先に置く形をとると先に `InsufficientBuffer` を返し得る点は挙動として認めることになる。本 issue では、`_` アームの `InvalidData` は不正 `byte_count` かつバッファが十分な場合にのみ返り、バッファが不足しているならまず `InsufficientBuffer` が返る、という順序を仕様とする（`Encode` トレイト契約はバッファ不足時に `InsufficientBuffer` を返すことを求めており、他方 `byte_count` の値域は本モジュール内部の実装事情なので、契約側を優先する）。

## 完了条件

- `encode_variable_uint`（`src/boxes_fmp4.rs:1407-1428`）が `match` の直前で `Error::check_buffer_size(byte_count as usize, buf)?` を呼び、`byte_count` 1〜3 のアームで `buf[0]` / `buf[1]` / `buf[2]` への代入前に境界検査が済んでいること
- 短いバッファで panic せず `ErrorKind::InsufficientBuffer` の `Error` が返ること（`byte_count` 1〜3 それぞれについて）
- 十分なバッファでは従来と同じバイト列を書き出し、返り値も同じであること
- `tests/test_boxes_fmp4.rs` を新設し、以下のケースを公開 API `TfraBox::encode` 経由で検証する回帰テストを追加していること（テスト関数名は英語、コメントとアサーションメッセージは日本語）
  - `encode_variable_uint` の 3 呼び出し位置（`traf_number` / `trun_number` / `sample_number`）それぞれについて、`byte_count = 1` / `2` / `3` の全 9 通り（3 × 3 = 9）で、その呼び出しの直前まで書き込めるだけの長さを持ち、直前で残バイトが `byte_count - 1` になるように切り詰めたバッファを渡して `TfraBox::encode` を呼び、`Err` の `kind` が `ErrorKind::InsufficientBuffer` であること
  - 上記 9 ケースとは別に、`byte_count = 4` を 1 ケース（`length_size_of_traf_num = 3` かつ `traf_number` 直前で残 3 バイト）加え、`Encode for u32` の既存の境界検査が期待通り `InsufficientBuffer` を返すことのサニティチェックを行うこと（合計 10 ケース。`byte_count = 4` は本 issue の修正対象ではないが、`encode_variable_uint` 全体の外形的な回帰として同じテストファイル内で押さえておく）
- `pbt/tests/prop_fmp4_boxes.rs` に `arb_tfra_box` と `tfra_box_roundtrip` PBT を追加し、既存の `traf_box_roundtrip` / `moof_box_roundtrip` 群（`pbt/tests/prop_fmp4_boxes.rs:314` 以降の `proptest!` ブロック）に並べていること。`arb_tfra_box` は次のように組み立てる:
  - `version` は `0` / `1` から選ぶ（`arb_tfhd_box` の `bool` 相当と同じ書式）
  - `length_size_of_traf_num` / `length_size_of_trun_num` / `length_size_of_sample_num` を `0..=3u8` の範囲で先に決めたうえで `prop_flat_map` に入り、`TfraEntry::traf_number` / `trun_number` / `sample_number` を対応する `length_size` に応じた `byte_count` バイトに収まる上限（`length_size = 0` なら上限 `0xFF`、`length_size = 3` なら上限 `u32::MAX`）で生成する。これは `encode_variable_uint` の 1〜3 バイトアーム（`src/boxes_fmp4.rs:1409-1423`）が上位バイトを silently 捨てる仕様（本 issue のスコープ外、後述）を回避してラウンドトリップを成立させるため（具体的な計算式は実装時に決める）
  - `version = 0` のときは `time` / `moof_offset` を `u32` 範囲、`version = 1` のときは `u64` 全域とする（`TfraBox::full_box_version` は `time` / `moof_offset` の実値が `u32::MAX` を超えるかで version を切り替える。`src/boxes_fmp4.rs:1305-1318`）
  - `entries` は上で生成した `TfraEntry` の `Vec`（0..3 個）
  - `shiguredo-rust` の「PBT: ... ラウンドトリップ等」「PBT でカバーできるものを単体テストで書かないこと」に従い、ラウンドトリップは PBT 側に置く（正常系の単体テストは重複するので書かない）
- `CHANGES.md` の `## develop` セクションに `[FIX]` エントリと担当者行が追記されていること（種別は `shiguredo-changelog` の判断規則に従い、`Encode` トレイト契約違反の修正で公開 API シグネチャは変わらないため `[FIX]`）
- `make test`（`cargo test --workspace --exclude c-api` と `cargo test -p c-api --lib`）が通ること
- `cargo clippy --workspace --all-targets -- -D warnings` と `cargo fmt --all -- --check` が通ること
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` が通ること（`Makefile` にも `prek.toml` にも doc のターゲットはなく、CI だけが回している）

## 解決方法

`feature/fix-encode-variable-uint-buffer-check` ブランチで対応した。

### 実施内容

- `src/boxes_fmp4.rs` の `encode_variable_uint` を、`match byte_count` の直前で `Error::check_buffer_size(byte_count as usize, buf)?` を呼ぶ形に書き換えた。`byte_count == 4` 側の `value.encode(buf)` はそのまま残し、二重検査は許容する
- `tests/test_boxes_fmp4.rs` を新設し、3 呼び出し位置（`traf_number` / `trun_number` / `sample_number`）× `byte_count` 1〜3 の 9 ケースと `byte_count = 4` のサニティ 1 ケースの計 10 ケースで `TfraBox::encode` に短いバッファを渡した際の `ErrorKind::InsufficientBuffer` 返却を検証する
- `pbt/tests/prop_fmp4_boxes.rs` に `arb_tfra_entry` / `arb_tfra_box` / `tfra_box_roundtrip` PBT を追加した。`arb_tfra_box` は `prop_flat_map` で `length_size_of_*` を先に決めてから各可変長フィールドの上限を絞り、`encode_variable_uint` の 3 バイトアームの truncation を回避してラウンドトリップを成立させる。version と `time` / `moof_offset` の対応も同様に整合させる
- `CHANGES.md` の `## develop` の `[FIX]` 群の末尾にエントリと担当者行を追記した

### 計画から外れた点

- `encode_variable_uint` に追加したコメントから「4 バイトアームの二重検査を許容する」旨の弁明を後で削除した。`value.encode` 側の実装に依存した記述で、将来 `Encode for u32` が変わるとコメントが古くなるため

### レビューを受けて追加で対応した内容

- `arb_tfra_box` 内の `max_time` / `max_moof_offset` の同一分岐重複を 1 変数 `max_time_and_moof_offset` に統合した
- `arb_tfra_box` の `max_of` クロージャを `max_value_for_length_size` に、引数名 `l` を `length_size` に改名した
- `arb_tfra_box` の `version` 生成を `any::<bool>().prop_map(|b| b as u8)` から `0u8..=1u8` に変え、`arb_tfdt_box` の書式に揃えた
- `pbt/tests/prop_fmp4_boxes.rs` の `boundary_tests` モジュールに `TfraBox` の境界テスト 4 件（version=0 の上限値ラウンドトリップ、`self.version = 0` からの version 自動昇格、`length_size_of_* = 0` の最小構成、`entries` が空）を追加した。既存 PBT の 100 ケースでは version 自動昇格等の端点が確率的に届かないため
- `assert_insufficient_buffer_err` のパニックメッセージ内の `InsufficientBuffer` を backtick 引用に揃えた
- テスト・PBT の doc コメントから冗長な記述を削除し、`shiguredo-rust` 規約引用や `PartialEq` の説明などファイル名から自明な内容を落とした
- `tests/test_boxes_fmp4.rs` のテストコメントから「本 issue」への言及を除去した（`shiguredo-issues` の「ソースコード本体に issue 番号や issue への言及を書かない」規約に従う）
- `pbt/tests/prop_fmp4_boxes.rs` の `arb_tfra_entry` の doc から日本語文中の「silently」を除去した（`shiguredo-rust` の日本語訳語規約に従う）

### テストの配置

`shiguredo-rust` の「PBT: ... ラウンドトリップ等」「PBT でカバーできるものを単体テストで書かないこと」「unittest は pbt で実現できないものだけを書くこと」に従い、テストを 2 系統に分ける:

- **PBT（`pbt/tests/prop_fmp4_boxes.rs`）**: `TfraBox` の encode/decode ラウンドトリップ。既存の `traf_box_roundtrip` / `moof_box_roundtrip` / `sidx_box_roundtrip` 等（`pbt/tests/prop_fmp4_boxes.rs:314` 以降の `proptest!` ブロック内）と同じ書式で `arb_tfra_box` と `tfra_box_roundtrip` を追加する。`TfraBox` は既存 PBT の網から漏れており、本 issue で修正する `encode_variable_uint` の byte_count 1〜3 の正常経路も、この PBT でカバーされる
- **単体テスト（`tests/test_boxes_fmp4.rs`、新設）**: 特定のバッファ長で `InsufficientBuffer` を返す境界値の回帰テスト。`Strategy` で任意入力を生成する PBT では狙った境界（残バイト = `byte_count - 1`）を安定して当てにくく、目的（エラーパスの検証）とも合わないため、単体テストとして置く。`shiguredo-rust` の「単体テストのファイル名は `tests/test_<module>.rs` とし、`src/<module>.rs` に対応させること」に従い `test_boxes_fmp4.rs` とする

既存 `fuzz/fuzz_targets/fuzz_tfra_box.rs` は `TfraBox::decode` → `encode_to_vec` のみで任意 buf での encode を叩けておらず、fuzz の追加は本 issue のスコープ外（追加の要否は担当者判断とする）。

`tests/` には現状 `decode_encode_test.rs` と `test_auxiliary.rs`（`issues/closed/0009-bug-sample-table-accessor-overflow.md` で新設された）しか置かれていない。`test_boxes_fmp4.rs` の新設は 3 件目になる。

### スコープ外

- `decode_variable_uint`（`src/boxes_fmp4.rs:1432-1435`）が `ErrorKind::InvalidData` を返している件は別問題（本 issue は encode 側のみ扱う）
- `encode_variable_uint` の 3 バイトアーム（`src/boxes_fmp4.rs:1418-1423`）が `value` の上位バイト（bit 24-31）を silently 捨てる件は境界検査とは別の入力検証の話であり、本 issue では扱わない
- `TfraBox::encode` 内の `self.length_size_of_traf_num + 1` 等（`src/boxes_fmp4.rs:1193-1207`）が `length_size_of_* == 255` で `u8` 加算の overflow を起こし debug で panic する件も別問題。本 issue の修正で `encode_variable_uint` の入り口までは正しく届くが、その手前の加算はそのまま残る（起票の要否は担当者判断とする）
- `fuzz/fuzz_targets/fuzz_tfra_box.rs` への encode 側 fuzz 追加

## 後方互換

- 公開 API のシグネチャ変更なし（`TfraBox` / `Encode` トレイトともに変化なし）
- 十分なバッファに対する挙動は不変
- これまで panic していたケースが `Err(InsufficientBuffer)` を返すようになるが、`Encode` トレイトの契約（`src/codec.rs:120-125`）に沿った修正であり、契約を前提とした呼び出し側には影響しない

ブランチ prefix は `Branch:` のとおり `feature/fix-` とする（`create-issue` スキルのカテゴリ表: bug → `feature/fix-`）。

## CHANGES.md

`[FIX]` で記載する。`shiguredo-changelog` の種別区分は後方互換性の有無で決まり、本件は公開 API シグネチャに変更がないため `[CHANGE]` ではなく `[FIX]`。同じ crate の `## develop` には既に `[FIX]` 群が積まれており（`sidx` / `mfra` の `moof_offset` 修正、`fmp4_segment_muxer_write_media_segment*` のサンプル変換エラー種別修正、`c-api` の Avc1/Hev1/Hvc1 null チェック 等）、その末尾に追記する。

```markdown
- [FIX] `TfraBox` のエンコードでバッファ長を検査せずにパニックする問題を修正する
  - `encode_variable_uint` の `byte_count` 1〜3 のアームが `buf.len()` を検査しておらず、短いバッファで `TfraBox::encode` を呼ぶと `Encode` トレイト契約に反してパニックしていた
  - `match` の直前で `Error::check_buffer_size` を呼び、バッファ不足時は `InsufficientBuffer` を返すようにする
  - @<担当者>
```

`CHANGES.md` は `## develop` にエントリが積まれるたびに以降の行がずれるため、行番号ではなくエントリ内容で参照している。

## 他 issue との依存関係

- `issues/0027-test-fmp4-error-path-tests.md`: fMP4 の主要エラーパス（`EmptyTracks` / `EmptySamples` / `MixedSampleEntries` / `InvalidState`）のテスト追加。本 issue が対象とする `TfraBox::encode` の `InsufficientBuffer` は 0027 の対象外なので、テストの重複は生じない。0027 が指定する配置先は `pbt/tests/prop_fmp4_segment_mux_demux.rs` または `pbt/tests/prop_error_paths.rs` で、本 issue の `tests/test_boxes_fmp4.rs` とは別のディレクトリ・別の観点であり干渉しない
- `issues/0018-bug-stts-from-sample-deltas-overflow.md`: `stts` オーバーフロー修正。修正対象ファイル（`src/boxes_moov_tree.rs` 想定）が本 issue（`src/boxes_fmp4.rs`）と別で、干渉しない
