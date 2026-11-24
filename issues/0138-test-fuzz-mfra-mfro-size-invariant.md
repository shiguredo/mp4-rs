# fuzz ターゲット `fuzz_mfra_box` で、エンコード結果の `mfro.size` が出力長と一致することを検査する

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/add-fuzz-mfra-mfro-size-invariant
- Polished: {YYYY-MM-DD}

## 目的

`fuzz/fuzz_targets/fuzz_mfra_box.rs` で、decode できた任意の `mfra` について、エンコードに成功したら出力の末尾 4 バイト（`mfro.size`）が出力のバイト数と一致することを assert する。

この性質は issue 0125 で直した不具合そのものである。`MfraBox::encode` は、`mfro` を最後に書き、末尾 4 バイトを `mfra` 全体のバイト数で上書きする。`MfraBox::encode` のコメントは、ISO/IEC 14496-12:2015 の 8.8.11.3 で `mfro.size` が親の `mfra` 全体のバイト数を表すことを根拠にしている。

同じ性質は PBT でも検証しているが、PBT は決まった形の入力しか作らない。fuzz では、PBT が作らない入力でもこの性質が成り立つことを確かめる。

## 現状

### fuzz ターゲット

`fuzz/fuzz_targets/fuzz_mfra_box.rs` は、`MfraBox::decode` に成功したら `encode_to_vec` を呼び、結果を捨てる。そのため、検出できるのはパニックだけである。出力の `mfro.size` が実サイズと食い違っても検出できない。

`fuzz/fuzz_targets/` 配下のターゲットには、出力の性質を assert しているものがまだない。shiguredo-rust のテストの役割分担では、fuzzing の役割を任意入力に対するクラッシュ耐性（パニック安全性）としている。本 issue の assert は、性質が破れたことをパニックとして fuzzer に検出させるもので、このリポジトリで初めての使い方になる。

### PBT

`pbt/tests/prop_fmp4_boxes.rs` の `mfra_box_decode_encode_rewrites_mfro_size` が同じ性質を検証している。入力は次の要素だけで組み立てる。

- 32 ビットの size を持つ通常ヘッダー
- `arb_tfra_box` で作った `tfra` を 0〜2 個
- 任意の位置に挟んだ `free` を 0 個か 1 個
- 末尾の `mfro`

この PBT が作らない入力のうち、`MfraBox::decode` が受け付けて encode の結果に影響するものは、たとえば次のとおり。

- 64 ビットサイズ（largesize）のヘッダーを持つ `mfra`。encode では 32 ビットのヘッダーで書き直すため、出力が短くなる
- largesize のヘッダーを持つ子ボックス
- 対応しないバージョンの `tfra`。decode で読み飛ばされる
  - `mfra_box_skips_unsupported_version_tfra` は decode の結果だけを確かめ、encode しない
- `mfro` の後ろにある子ボックス
- 複数の `mfro`
- 32 ビットの size=0 の子ボックス

## 設計方針

- `fuzz/fuzz_targets/fuzz_mfra_box.rs` で、`MfraBox::decode` に成功し、かつ `encode_to_vec` に成功した場合に、次のことを assert する
  - 出力の末尾 4 バイトを big-endian の `u32` として読んだ値が、出力のバイト数と一致する
  - assert のメッセージは日本語で書く（`fuzz/` はテストコードとして扱う）
- `encode_to_vec` が失敗した場合は、今と同じく何もしない
  - たとえば、`mfra` 全体のサイズが通常ヘッダーの上限を超える場合に失敗する
- この assert はこのリポジトリの fuzz ターゲットで初めての性質の検査になるので、ターゲットのコメントに次のことを書く
  - 何を検査するか
  - PBT ではなく fuzz で検査する理由（PBT が作らない形の入力でも性質が成り立つことを確かめるため）
- 対象は `fuzz_mfra_box` だけとし、ほかの fuzz ターゲットには assert を追加しない
- `MfraBox` のフィールドは参照しない。issue 0136（`mfro_box` フィールドを外す）や issue 0137（未知の子ボックスを保持する）の前後どちらでも、そのまま成り立つ

## 完了条件

- `fuzz/fuzz_targets/fuzz_mfra_box.rs` が、エンコードに成功した場合に出力の末尾 4 バイトと出力のバイト数の一致を assert している
- `fuzz/` で、CI と同じく `cargo fmt --check`、`cargo check --all-targets`、`cargo clippy --all-targets -- -D warnings` が成功する
- 有効な `mfra` を種入力として `fuzz/corpus/fuzz_mfra_box/` に置き、`cargo +nightly fuzz run fuzz_mfra_box -- -max_total_time=30` がクラッシュを報告せずに終わる
  - 種入力は、`Fmp4SegmentMuxer::mfra_bytes` の出力か、手で組み立てた `mfra` とする
  - `fuzz/` にはコミット済みのコーパスがない。空のコーパスから始めると、30 秒の間に decode できる `mfra` に届かず、assert まで到達しないおそれがあるため、種入力を置く
  - 実行時間は `Makefile` の `fuzzing` と同じにする
  - `fuzz/.gitignore` が `corpus/` を除外しているので、種入力は作業中だけ使い、コミットしない
- assert が実際に破れを検出できることを、作業中に確認する
  - 一時的に `MfraBox::encode` を、`mfro.size` に実サイズ以外の値を書くように変える
  - 上と同じ種入力を置いた状態で `cargo +nightly fuzz run fuzz_mfra_box -- -max_total_time=30` を実行し、その時間内にクラッシュが報告されることを確かめる
  - 確認後は変更を戻す
