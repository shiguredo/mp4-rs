# auxiliary.rs の SampleTableAccessor::new の加算が非 checked であり overflow 時に panic / wrap する

- Priority: High
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-sample-table-accessor-overflow
- Polished: 2026-07-27

## 目的

`SampleTableAccessor::new` は `Result` を返す API でありながら、内部の加算が `checked_add` を使っていない。debug ビルドでは panic し、release ビルドでは wrap して検証をすり抜けたアクセサを `Ok` で返す。対象の 3 箇所（`src/auxiliary.rs:31` / `:51` / `:155`）を `checked_add` に置き換え、overflow 時は `Err` を返す。

## 優先度根拠

外部ファイル由来の入力から到達する点が High の根拠である。`src/demux_mp4_file.rs:519` の `SampleTableAccessor::new(trak_box.mdia_box.minf_box.stbl_box)?` により、`aux` モジュールを直接使わない `Mp4FileDemuxer` の利用者（C API・WASM を含む）にも、細工された MP4 ファイルを渡すだけで到達する。`SttsBox::decode` / `CttsBox::decode`（`src/boxes_moov_tree.rs`）はエントリの値を検証せずそのまま読むため、`stts` ボックス自体は 32 バイト、`stbl` ボックス全体でも 108 バイトで足りる。

release ビルドでの被害のほうが大きい。`Cargo.toml` に `overflow-checks` の指定はないため wrap し、wrap 後の `sample_count` は `src/auxiliary.rs:35-44` の stsz 突き合わせと `:131` の stsc 突き合わせの両方でそのまま比較対象になる。入力側の `stsz` / `stsc` を wrap 後の値に合わせておけば既存の検証をすり抜け、誤った `sample_count()` を持つアクセサが `Ok` で返る。`[profile.release-wasm]` も overflow-checks 無効なので WASM ビルドでも同じである。C API では panic が `extern "C"` 境界に達した時点でプロセスが abort し（Rust 1.81 以降の定義済み挙動。`crates/` 配下に `catch_unwind` はない）、利用者側では復帰できない。

## 現状

### overflow する 3 箇所

`src/auxiliary.rs:29-33`（stts のサンプル数累計）:

```rust
        for entry in &stbl_box_ref.stts_box.entries {
            sample_durations.push((sample_count, entry.sample_delta, acc_duration));
            sample_count += entry.sample_count;
            acc_duration += entry.sample_delta as u64 * entry.sample_count as u64;
        }
```

`src/auxiliary.rs:49-52`（ctts のサンプル数累計）:

```rust
            for entry in &ctts_box.entries {
                sample_composition_offsets.push((ctts_sample_count, entry.sample_offset));
                ctts_sample_count += entry.sample_count;
            }
```

`src/auxiliary.rs:150-157`（サンプルデータオフセットの累計）:

```rust
        let mut sample_data_offsets = Vec::new();
        for chunk in this.chunks() {
            let mut offset = chunk.offset();
            for sample in chunk.samples() {
                sample_data_offsets.push(offset);
                offset += sample.data_size() as u64;
            }
        }
```

`SttsEntry.sample_count` / `CttsEntry.sample_count` はいずれも `u32` で、複数エントリの合計は容易に `u32::MAX` を超える。stts / ctts の整合性チェック（`:35-44` / `:53`）はどちらも該当する加算の後にあるため、既存の検証では防げない。

3 つ目は `Co64Box` 経由でのみ overflow する。`StcoBox` はオフセットが `u32`（`src/boxes_moov_tree.rs` の `chunk_offsets: Vec<u32>`）、`data_size()` も `u32`、サンプル数も `:31` を checked 化すれば `u32::MAX` 以下なので、上界は `(2^32 - 1) + (2^32 - 1)^2 = 18446744069414584320` で `u64::MAX` に収まるためである。この加算にはそもそも対応する検証が存在しない。

`SampleTableAccessorError` に overflow を表現できるバリアントも存在しない。

既存の `fuzz/fuzz_targets/fuzz_sample_table_accessor.rs` はこの経路を叩いているが、有効な `stbl` の合成が難しく `fuzz/corpus/` もディレクトリごと存在しないため、`issues/closed/0006-add-fuzz-sample-table-accessor.md` の 946 万回実行でも検出されていない。「fuzz が通っている」ことを修正済みの根拠にしてはならない。

### 再現条件

`StblBox` とその子ボックスのフィールドはすべて `pub` なので、以下はいずれも `StblBox` を組み立てて `SampleTableAccessor::new` に渡すだけで再現する。修正前は debug ビルドで該当行が panic する。

```rust
use std::num::NonZeroU32;

use shiguredo_mp4::aux::{SampleTableAccessor, SampleTableAccessorError};
use shiguredo_mp4::boxes::{
    Co64Box, CttsBox, CttsEntry, SampleEntry, StblBox, StcoBox, StscBox, StscEntry, StsdBox,
    StszBox, SttsBox, SttsEntry, UnknownBox,
};
use shiguredo_mp4::{BoxSize, BoxType, Either};
```

`stsd` は `SampleTableAccessor::new` から `entries.len()` しか参照されないため、中身は任意でよい。

```rust
fn stsd_box() -> StsdBox {
    StsdBox {
        entries: vec![SampleEntry::Unknown(UnknownBox {
            box_type: BoxType::Normal(*b"test"),
            box_size: BoxSize::U32(8),
            payload: Vec::new(),
        })],
    }
}
```

いずれのケースも `stsd_box: stsd_box()` とし、明示しないフィールド（`cslg_box` / `stss_box` / `sdtp_box`）は `None`、`unknown_boxes` は `Vec::new()` とする。

- **stts 側（`:31`）**: `:31` は他のすべての整合性チェックより前にあるので、`stts` 以外は空でよい
  - `stts_box`: `SttsEntry { sample_count: 0x8000_0000, sample_delta: 1 }` の 2 エントリ
  - `ctts_box: None`、`stsc_box: StscBox { entries: Vec::new() }`、`stsz_box: StszBox::Variable { entry_sizes: Vec::new() }`、`stco_or_co64_box: Either::A(StcoBox { chunk_offsets: Vec::new() })`
- **ctts 側（`:51`）**: `:35-44` の stsz 突き合わせを通す必要がある
  - `stts_box`: `SttsEntry { sample_count: 1, sample_delta: 1 }` の 1 エントリ
  - `ctts_box`: `Some(CttsBox { version: 0, entries: vec![CttsEntry { sample_count: 0x8000_0000, sample_offset: 0 }; 2] })`
  - `stsz_box: StszBox::Variable { entry_sizes: vec![1] }`、`stsc` / `stco` は stts 側と同じく空
- **データオフセット側（`:155`）**: この加算はすべての整合性チェックの後にあるため全ボックスを整合させる。以下は 1 チャンク 3 サンプルで、2 サンプル目の加算が overflow する（`sample_index` は 2 になる。3 サンプル目に格納されるはずだった値が失われるケース）

  ```rust
  StblBox {
      stsd_box: stsd_box(),
      stts_box: SttsBox {
          entries: vec![SttsEntry { sample_count: 3, sample_delta: 1 }],
      },
      ctts_box: None,
      cslg_box: None,
      stsc_box: StscBox {
          entries: vec![StscEntry {
              first_chunk: NonZeroU32::MIN,
              sample_per_chunk: 3,
              sample_description_index: NonZeroU32::MIN,
          }],
      },
      stsz_box: StszBox::Variable { entry_sizes: vec![1, 1, 1] },
      stco_or_co64_box: Either::B(Co64Box { chunk_offsets: vec![u64::MAX - 1] }),
      stss_box: None,
      sdtp_box: None,
      unknown_boxes: Vec::new(),
  }
  ```

  同じ構成を 1 チャンク 1 サンプル（`sample_count: 1`、`sample_per_chunk: 1`、`entry_sizes: vec![1]`、`chunk_offsets: vec![u64::MAX]`）にすると、格納される値はすべて正常なまま、捨てられる末尾の加算だけが overflow する（`sample_index` は 1）。この挙動も仕様として固定する（「## 設計方針」参照）

上記のケースはいずれも小さな `Vec` しか作らないが、`sample_count` を大きくしたまま先へ進ませる入力には注意が要る。`stsz` を `Variable` にすると `:35-44` を通すのに `entry_sizes` を `sample_count` 個（`u32::MAX` なら約 17 GB）用意することになり、`:150-157` まで到達させると `sample_data_offsets` が `sample_count` 個（約 34 GB）に膨らむ。後述の境界ケースは、`stsz` を `Fixed` にして前者を、`stsc` / `stco` を空にして `:131` で止めることで後者を回避している。

## 設計方針

3 箇所すべてを `checked_add` に置き換え、overflow 時は `Err` を返す。

`:155` については、チャンク末尾サンプルの加算結果は `sample_data_offsets` に格納されずに捨てられるが、そのケースも `Err` にする。この加算が overflow するということは、そのサンプルのデータ範囲が `u64` で表せるファイル内に収まらないということであり、拒否するのが正しいためである（ループを組み替えて末尾の加算自体を無くす案は採らない）。格納される値がすべて正常でも `Err` になる入力が存在することになるが、その入力は破損している。

### 追加するエラーバリアント

`SampleTableAccessorError` に以下の 2 バリアントを追加する。既存 6 バリアントのうち 5 つは診断用の具体値を持ち `Display` でそれを出力しているので、それに合わせて累計値と加算しようとした値を持たせる。どちらも overflow 地点で既知である。

```rust
    /// サンプル数の累計が [`u32`] の範囲を超えた
    SampleCountOverflow {
        /// overflow が発生したボックスの種別（`stts` ないし `ctts`）
        box_type: BoxType,

        /// overflow 直前までの累計サンプル数
        accumulated_sample_count: u32,

        /// 加算しようとしたエントリのサンプル数
        entry_sample_count: u32,
    },

    /// サンプルデータのバイト位置の累計が [`u64`] の範囲を超えた
    SampleDataOffsetOverflow {
        /// オフセットの累計が overflow した時点で処理していたサンプルのインデックス
        ///
        /// このサンプル自身の開始位置は正常に算出できており、overflow するのはその終端位置
        /// （同じチャンク内に後続サンプルがあれば、その開始位置になる値）の計算である。
        sample_index: NonZeroU32,

        /// overflow 直前までの累計バイト位置（このサンプルの開始位置）
        accumulated_offset: u64,

        /// 加算しようとしたサンプルのデータサイズ
        sample_data_size: u32,
    },
```

`Display` にもアームを追加する。既存の 3 メッセージには文法誤りがあり `issues/0040` が修正予定なので、新規分は最初から正しい英語にする。既存のフィールド 1 個のバリアント（`FirstChunkIndexIsNotOne` / `ChunksExistButNoSamples`）はブロック形式で書かれているので、それに揃える。

```rust
            SampleTableAccessorError::SampleCountOverflow {
                box_type,
                accumulated_sample_count,
                entry_sample_count,
            } => {
                write!(
                    f,
                    "Total sample count in `{box_type}` box overflows u32 (accumulated {accumulated_sample_count}, adding {entry_sample_count})"
                )
            }
            SampleTableAccessorError::SampleDataOffsetOverflow {
                sample_index,
                accumulated_offset,
                sample_data_size,
            } => {
                write!(
                    f,
                    "Sample data offset overflows u64 at sample {sample_index} (accumulated {accumulated_offset}, adding {sample_data_size})"
                )
            }
```

バリアントも `Display` のアームも既存の並び順を崩さないよう末尾に追加する。

既存の `InconsistentSampleCount` は流用しない。同バリアントは `stts_sample_count` / `other_box_type` / `other_sample_count` の 3 つの確定値を要求するが、overflow 時点では合計値そのものが算出できない。

### acc_duration は checked_add にしない

`src/auxiliary.rs:32` の `acc_duration` は `+=` のまま残し、次のコメントを添える。

```rust
            // acc_duration は checked_add にしない。
            // 直前の sample_count の加算を checked_add にして overflow で即 Err を返すため、
            // ここに到達した時点で常に Σ sample_count <= u32::MAX が保証される。
            // このとき acc_duration <= (2^32 - 1) * (2^32 - 1) = 18446744065119617025 < u64::MAX
            // となり、原理的に overflow しない。checked_add を入れると到達不能な Err 分岐になる。
            // この不変条件は sample_count の加算を acc_duration の加算より先に行うことに
            // 依存しているため、2 つの加算の順序自体が仕様である。
```

`checked_add` を入れると `shiguredo-rust` の「公開 API 経由で到達できないコードはデッドコードとして削除を検討すること」に反する。

`SampleAccessor::timestamp()`（`src/auxiliary.rs:382`）と `get_sample_by_timestamp()`（`:203`）の加算も触らない。`timestamp()` は `base_timestamp + duration * (index - 1 - base_index_minus_1)` の形だが、`base_timestamp <= (2^32 - 1) * base_index_minus_1` かつ `duration <= 2^32 - 1` なので、全体が `(2^32 - 1) * (index - 1) <= (2^32 - 1)^2 < u64::MAX` に収まる（`index` は `NonZeroU32`）。`:203` の `sample_timestamp + sample.duration() as u64` も同じ不等式に収まる。この論証は `stts` のエントリ内容にも `stss` の妥当性にも依存しない。

### スコープ外

以下は本 issue では扱わない。いずれも未起票であり、起票の要否とタイミングは担当者判断とする。

- `src/auxiliary.rs` の `NonZeroU32::saturating_add`（`:120` / `:128-129` / `:193` / `:227` / `:235` / `:449` / `:499`）。とくに `:128-129` が飽和すると `:131` の検査を `sample_count == u32::MAX - 1` で通過し、`:499` が算出するサンプルインデックスが `sample_count` を超えた時点で `get_sample()` が `None` を返し、`:502` の `.expect("unreachable")` で panic する（引き金は `:499` の飽和そのものではなくインデックス超過であり、加算が飽和する 1 つ手前の反復で既に panic に達する）。本 issue の完了後も `SampleTableAccessor::new` から panic が理論上は消えないが、到達には 34 GB 規模の確保が先行するため実質的には OOM abort が先に起きる。`issues/0032` は mux 側の `saturating_add` のみを扱っている
- `:150-157` の入力サイズに対するメモリ増幅（124 バイトの `stbl` から 160 MB を確保できる）。`shiguredo-rust` の「メモリ消費量のオーダーは実際の入力データのサイズから大きく乖離することはない」という意図に反する
- `sample_count` の実効上限が `u32::MAX` ではなく `u32::MAX - 1` になっている off-by-one（`:131` の `first_sample_index.get() - 1` の上限が `u32::MAX - 1` であるため）
- `StszBox::Fixed { sample_count }` が `stts` の合計と突き合わされていないこと（`:35-44` は `StszBox::Variable` のときしか検証しない）
- `stss_box.sample_numbers` が範囲もソート順も検証されていないこと（`SampleAccessor::sync_sample()` が生の値をそのままインデックスに使う）
- `src/auxiliary.rs:42` / `:66-67` の `len() as u32` による narrowing cast
- `SampleTableAccessorError` への `Copy` derive（規約上は derive すべきだが未対応。本 issue で追加するバリアントは `Copy` 可能な形にしてあるため将来の derive を妨げない）
- `pbt/tests/prop_auxiliary.rs` にある既存テスト群の `tests/` への移設（単体テストと PBT が同居しており単純な移設にはならない）
- `fuzz/corpus/` へのシード追加（`fuzz/.gitignore` が `corpus/` を除外しているため `.gitignore` の変更か `git add -f` が要る）

### テストの配置

`tests/test_auxiliary.rs` を新設する。`shiguredo-rust` の「pbt 以下に unittest を書かないこと」「単体テストのファイル名は `tests/test_<module>.rs` とし、`src/<module>.rs` に対応させること」「`tests/`・`pbt/`・`fuzz/` のテストは公開 API に対してだけ書くこと」「unittest は pbt で実現できないものだけを書くこと」に従うためで、今回のテストは固定入力の回帰テストなので PBT では代替できず、公開 API だけで書ける。

ただしこれは repo の現行実態から外れる点に注意する。`tests/` には `decode_encode_test.rs` の 1 本しかなく、`SampleTableAccessorError` の既存テストは `pbt/tests/prop_auxiliary.rs` と `src/auxiliary.rs` の `#[cfg(test)] mod tests` の 2 箇所に分散している（`ChunksExistButNoSamples` のケースは両方に重複して存在する）。open issue の方針も割れており、`issues/0027` / `issues/0029` は `pbt/` 側を、`issues/0030` は `tests/test_<module>.rs` の新規作成を指定している。本 issue は規約どおり後者を採る。既存テストの集約は「### スコープ外」のとおり扱わない。

## 完了条件

- `src/auxiliary.rs:31` / `:51` / `:155` の 3 箇所が `checked_add` に置き換えられ、overflow 時に `Err` が返ること
- `src/auxiliary.rs:32` の `acc_duration` は `+=` のまま残り、overflow しない根拠と 2 つの加算の順序が仕様である旨の日本語コメントが付いていること
- `SampleTableAccessorError` の末尾に `SampleCountOverflow` / `SampleDataOffsetOverflow` が日本語 doc コメント付きで追加され、`Display` の末尾にアームが追加されていること
- `tests/test_auxiliary.rs` が新規作成され、以下を検証していること。`SampleTableAccessorError` は `PartialEq` を derive していないので、`let Err(SampleTableAccessorError::SampleCountOverflow { .. }) = result else { panic!("...") };` の形で分解してから `assert_eq!` で各フィールドを照合する（失敗時に実際の値が出るようにするため。テスト関数名は英語、コメントとアサーションメッセージは日本語）。テスト数が少ないので `mod` で分けずフラットに並べる
  - stts 側の再現条件が `SampleCountOverflow { box_type: SttsBox::TYPE, accumulated_sample_count: 0x8000_0000, entry_sample_count: 0x8000_0000 }` を返すこと
  - ctts 側の再現条件が同じ値で `box_type: CttsBox::TYPE` を返すこと
  - データオフセット側の再現条件（1 チャンク 3 サンプル）が `SampleDataOffsetOverflow { sample_index: NonZeroU32::new(2).expect("bug"), accumulated_offset: u64::MAX, sample_data_size: 1 }` を返すこと（`sample_index` は `NonZeroU32` なので、`assert_eq!` には `u32` リテラルではなく `NonZeroU32` 値を渡す）
  - 1 チャンク 1 サンプル（捨てられる末尾の加算）でも `SampleDataOffsetOverflow` を返し、`sample_index` が 1 になること
  - サンプル数の合計がちょうど `u32::MAX` のとき `InconsistentSampleCount { stts_sample_count: u32::MAX, other_box_type: StscBox::TYPE, other_sample_count: 0 }` が返ること（`SampleCountOverflow` にはならない。`stts` を `SttsEntry { sample_count: 0x8000_0000, sample_delta: 1 }` と `SttsEntry { sample_count: 0x7fff_ffff, sample_delta: 1 }` の 2 エントリ、`stsz` を `StszBox::Fixed { sample_size: NonZeroU32::MIN, sample_count: u32::MAX }`、`stsc` / `stco` を空にする。`checked_add` を使う限り境界の取り違えは起きないが、将来これを手書きの上限比較に置き換えたときの回帰検出になる）
  - 新規 2 バリアントの `Display` 出力が「### 追加するエラーバリアント」の format と完全一致すること
- `CHANGES.md` の `## develop` の `[CHANGE]` 群の末尾に、エントリと担当者行が追記されていること
- `make test`（`cargo test --workspace --exclude c-api` と `cargo test -p c-api --lib`）が通ること
- `cargo clippy --workspace --all-targets -- -D warnings` と `cargo fmt --all -- --check` が通ること。`make clippy` も CI も `--all-targets` を付けないため新規テストファイルが lint されず、`prek.toml` の cargo-clippy フックだけがこれを検出する
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` が通ること（`Makefile` にも `prek.toml` にも doc のターゲットはなく、CI だけが回している。`fuzz` は workspace メンバではないので `warning: excluded package(s) 'fuzz' not found in workspace` が出るが、CI と同一コマンドなので問題ない）

## 解決方法

以下の行番号はすべて変更前のものである。手順を順に適用すると後続の行番号はずれるので、置き換え対象は併記した元コードで照合すること。

1. `src/auxiliary.rs:26` の `let mut sample_count = 0;` を `let mut sample_count: u32 = 0;` に、`:47` の `let mut ctts_sample_count = 0;` を `let mut ctts_sample_count: u32 = 0;` に変更する。現状これらは `+=` の右辺から型が推論されているだけで、`checked_add` に置き換えると `error[E0689]: can't call method 'checked_add' on ambiguous numeric type '{integer}'` になる（rustc は `i32` を提案してくるので従ってはならない）
2. `src/auxiliary.rs:5-8` の `use` に `SttsBox` を追加する（`BoxType` / `NonZeroU32` / `CttsBox` は import 済み）
3. `src/auxiliary.rs:31` の `sample_count += entry.sample_count;` を次に置き換える

   ```rust
   sample_count = sample_count.checked_add(entry.sample_count).ok_or(
       SampleTableAccessorError::SampleCountOverflow {
           box_type: SttsBox::TYPE,
           accumulated_sample_count: sample_count,
           entry_sample_count: entry.sample_count,
       },
   )?;
   ```

4. `src/auxiliary.rs:51` の `ctts_sample_count += entry.sample_count;` を同様に置き換える（`box_type: CttsBox::TYPE`）
5. `src/auxiliary.rs:155` の `offset += sample.data_size() as u64;` を次に置き換える

   ```rust
   offset = offset.checked_add(sample.data_size() as u64).ok_or(
       SampleTableAccessorError::SampleDataOffsetOverflow {
           sample_index: sample.index(),
           accumulated_offset: offset,
           sample_data_size: sample.data_size(),
       },
   )?;
   ```

6. `sample_count` の加算の直後にある `acc_duration` の `+=` はそのまま残し、「### acc_duration は checked_add にしない」のコメントを添える
7. `SampleTableAccessorError` の末尾に 2 バリアントを、`Display` の末尾にアームを追加する
8. `tests/test_auxiliary.rs` を新規作成し、完了条件のテストを追加する。`pbt/tests/prop_auxiliary.rs` にも `src/auxiliary.rs:518-653` の `#[cfg(test)] mod tests` にも追加しない
9. `CHANGES.md` にエントリを追記する
10. `make fmt` を実行する

## 波及範囲

`SampleTableAccessorError` を網羅 `match` している箇所は `src/auxiliary.rs` の `Display` だけであり、他のコード変更は不要である。参照しているのは `src/demux_mp4_file.rs`（enum ごとラップ）、`crates/c-api/src/error.rs:54-58`（バリアントを見ずに一律 `MP4_ERROR_INVALID_DATA`）、`examples/transcode_wasm`（`to_string()` のみ）、`pbt/tests/prop_auxiliary.rs` と `pbt/tests/prop_demux.rs`（部分パターンでの照合とバリアント構築）、`fuzz/`（`let Ok(...) else`）で、いずれもバリアント追加の影響を受けない。

`Mp4FileDemuxer` への伝播は `src/demux_mp4_file.rs:519` の `?` と `From<SampleTableAccessorError> for DemuxError` により型で保証されるため、専用のテストは追加しない。

## 後方互換

`SampleTableAccessorError` には `#[non_exhaustive]` が付いていないため、`src/lib.rs` から公開されている本 enum へのバリアント追加は、リポジトリ外の利用者の網羅 `match` を壊す後方互換のない変更である。

overflow しない正当な入力に対する挙動は不変で、API シグネチャの変更もない。

ブランチ prefix は `Branch:` のとおり `feature/fix-` とする。`shiguredo-git` は「バグ修正は prefix を `feature/fix-`」と「後方互換のない変更は prefix を `feature/change-`」の両方を定めていて本件は双方に当てはまるが、`create-issue` のカテゴリと prefix の対応表が bug に `feature/fix-` を割り当てているため、issue のカテゴリ（bug）側に揃える。次節の `CHANGES.md` の種別は `shiguredo-changelog` の定義に沿って後方互換性の有無だけで決まる別の判断軸であり、`[CHANGE]` になっても prefix は変わらない。

## CHANGES.md

`[CHANGE]` で記載する。`shiguredo-rust` は「`#[non_exhaustive]` を使わないこと」「将来 variant や field を追加するときは素直に破壊的変更として扱うこと」と定めており、種別は後方互換性の有無で決まる（バグ修正であっても後方互換がなければ `[CHANGE]`）。

`CHANGES.md` の `## 2026.1.0` には、同じ enum に同じ理由でバリアント（`ChunksExistButNoSamples`）を追加した変更を `[FIX]` とした先例（`SampleTableAccessor::new()` のパニックを修正したエントリ）があるが、上記の判断規則からは `[CHANGE]` が正しい。現行の `## develop` には、`#[non_exhaustive]` を持たない公開 enum へのバリアント追加を `[CHANGE]` とした先例が揃っている（`SampleEntry` への `Tx3g` / `Wvtt` / `Stpp` の追加、`TrackKind` への `Subtitle` の追加）。姉妹 issue の `issues/0030` / `issues/0032` が `[FIX]` を選んでいるのも、公開 enum にバリアントを足さないためである。

`CHANGES.md` は `## develop` にエントリが積まれるたびに以降の行がずれるため、ここでは行番号ではなくエントリ内容で参照している。

```markdown
- [CHANGE] `SampleTableAccessorError` に `SampleCountOverflow` と `SampleDataOffsetOverflow` を追加する
  - 破損した `stts` / `ctts` / `co64` を含む入力で、サンプル数やデータ位置の累計が overflow したときに panic せずエラーを返すようにする
  - release ビルドでは wrap して誤ったサンプル数のアクセサが返っていた
  - @<担当者>
```

## 他 issue との依存関係

以下は本 issue の作業には含まれない。追随はいずれも `develop` 上で `refresh-issue` として行う。

- `issues/0040-bug-sample-table-accessor-error-grammar.md`: 同じ `impl Display for SampleTableAccessorError` の 311 / 320 / 336 行目を修正する。本 issue はアームを末尾に追加するので 0040 が触る 3 アームとは競合しない。0040 が先なら文字列の書き換えだけで行数が変わらないため本 issue の行番号参照は有効なまま。本 issue が先だと `Display` 実装が数十行下にずれるので 0040 の行番号を更新する必要がある
- `issues/0020-bug-remove-non-exhaustive.md`: `ErrorKind` / `MuxError` / `DemuxError` から `#[non_exhaustive]` を削除する。0020 の「## 現状」は `SampleTableAccessorError` を「`#[non_exhaustive]` なし」の基準として参照しているが、本 issue はバリアントを追加するだけで `#[non_exhaustive]` の有無には触れないため、どちらを先に進めても干渉しない。コード上の競合もない
- `issues/0038-fmt-test-file-naming-convention.md`: `tests/` 配下の命名規則を `test_<module>.rs` に統一する。本 issue が新設する `tests/test_auxiliary.rs` はこの規則に沿うので干渉しないが、0038 の「## 現状」（`tests/` には `decode_encode_test.rs` のみ存在する）は本 issue の完了後に事実と食い違う
