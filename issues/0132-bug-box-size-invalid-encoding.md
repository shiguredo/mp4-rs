# ボックスヘッダーのサイズと未知ボックスのペイロード長を検査する

- Created: 2026-11-22
- Completed: 2026-11-23
- Branch: feature/fix-box-size-invalid-encoding
- Polished: 2026-11-22

## 目的

`BoxHeader::encode` で符号化形式と両立しないサイズを拒否し、`UnknownBox::encode` では固定長の宣言と実際のペイロード長を一致させる。`BoxSize::with_payload_size` は、総サイズを表現できないとき、パニックや切り詰めではなくエラーを返す。

## 現状

`src/basic_types.rs` の `BoxHeader::encode` は、公開の `BoxSize` の値に合わせてサイズ欄を書き出すが、予約値と符号化形式の整合や、固定長の下限は検査しない。`BoxSize::with_payload_size` は、ペイロードに通常ヘッダー分を足す計算と、拡張サイズ欄の 8 バイトを後から足す計算を検査していない。

ISO/IEC 14496-12:2015 の 4.2 では、サイズはヘッダーを含むボックス全体のバイト数で、32 ビットのサイズ欄の 1 は拡張サイズ、0 はファイルの末尾までを表す。UUID 型は 16 バイトの `usertype` を追加する。通常型のヘッダー長は 32 ビット形式で 8 バイト、拡張形式で 16 バイト。UUID 型ではそれぞれ 24 バイトと 32 バイトになる。

`src/boxes.rs` の `UnknownBox::encode` は保持したサイズ欄とペイロードを書くだけで、両者が一致するかを検査しない。

## 設計方針

- `BoxHeader::encode` は書き込み前に、拡張サイズ欄を伴わない 32 ビットの拡張用予約値と、実際のヘッダー幅に満たない固定長を `ErrorKind::InvalidInput` として拒否する。種別とサイズ形式の両方から下限を求める。`BoxSize::VARIABLE_SIZE` と、`Mp4FileMuxer` が `mdat` 用に予約し後で確定する `BoxSize::LARGE_VARIABLE_SIZE` は、ヘッダー単体では種別を問わず従来どおり書き込めるようにする。後者を完成したボックスのサイズとしては扱わない
- `BoxSize::with_payload_size` の戻り値を `Result<Self>` に変更する。種別に応じた `U32` 形式のヘッダーを加えた総サイズが `u32` に収まる場合のみ `U32` を選び、収まらなければ拡張サイズ欄の分も足して `U64` を返す。どちらの加算も検査し、表現できない総サイズには `ErrorKind::InvalidInput` を返す。飽和・ラップは行わない。この戻り値の変更は公開 Rust API の破壊的変更となる。コアの `BoxHeader::finalize_box_size`、`FreeBox::encode`、`MdatBox::encode`、WASM バインディングとトランスコード例、およびテストの呼び出し箇所を更新する
- `UnknownBox::encode` は固定長の場合、保持する形式のヘッダー幅と `payload.len()` の検査付きの合計が宣言サイズに一致するときだけ書き込む。不一致、または完成済みボックスに `BoxSize::LARGE_VARIABLE_SIZE` を指定した場合は、書き込み前に `ErrorKind::InvalidInput` を返す。有効な固定長の形式を選び直さず、`BoxSize::VARIABLE_SIZE` によるトップレベル末尾の既存表現も保持する。デコード側と既存のボックス配置ルールは変更しない。UUID ヘッダー内のフィールド順序は 0095 の対象とする

## 完了条件

- 公開エンコードのエラーパスで、通常型と UUID 型のヘッダー下限、および未知ボックスの固定長とペイロード長の不一致を検査する
- ペイロードを確保せず、通常型と UUID 型のそれぞれについて、`U32` に収まる最大の総サイズ、`U64` への切り替え直後、表現可能な最大の総サイズをサイズ計算 API で確認する。二段の加算はそれぞれ表現不能な場合に `ErrorKind::InvalidInput` となることを確認する。大きなペイロードを生成する既存の正常系 PBT は、成功を期待する範囲を表現可能な値に限定する
- 正常系の PBT で、通常型と UUID 型の `U32`・`U64` ヘッダーが従来の幅で往復し、未知ボックスは短いペイロードに対しても `U32` と `U64` の両方を明示的に構成し、符号化されたヘッダーの幅と、デコード後のサイズ形式・サイズ値・ペイロードが元の指定と一致することを確認する。既存のトップレベル末尾の可変長表現と、マルチプレクサーの `mdat` ヘッダー予約・確定も維持する
- `cargo test --workspace` が成功する

## 解決方法

- `src/basic_types.rs`
  - `BoxSize::with_payload_size` の戻り値を `Result<Self>` に変えた。32-bit の size を使うヘッダー（通常型 8 バイト・UUID 型 24 バイト）の加算と、largesize の 8 バイトの加算を、どちらも `checked_add` で検査する。総サイズが `u64` に収まらなければ `ErrorKind::InvalidInput` を返す。総サイズが `u32` に収まる場合だけ `BoxSize::U32` を選ぶ点は変えていない
  - `BoxHeader::encode` は、書き込む前に `box_size != 0 && box_size < external_size()` を `ErrorKind::InvalidInput` として拒否する。32-bit の size の 1（`BoxSize::U32(1)`）もこの条件で拒否される。`BoxSize::VARIABLE_SIZE` と `BoxSize::LARGE_VARIABLE_SIZE` は、種別を問わずこれまでどおり書き込める
  - `BoxHeader::finalize_box_size` は `with_payload_size` の結果を `?` で伝える
  - `BoxHeader::box_size`、`BoxSize::LARGE_VARIABLE_SIZE`、`BoxSize::with_payload_size` の doc に、エラー条件と使いどころを書いた
- `src/boxes.rs`
  - `UnknownBox::encode` は、`BoxSize::VARIABLE_SIZE` 以外で、ヘッダーのバイト数と `payload.len()` の合計（`u64` で検査付きで計算する）が宣言サイズと一致しない場合に、書き込む前に `ErrorKind::InvalidInput` を返す。`BoxSize::LARGE_VARIABLE_SIZE` はこの検査で拒否される。形式（U32 / U64）は選び直さない
  - `UnknownBox::box_size` の doc に、一致の条件、`payload` を変えたときにサイズを設定し直すこと、`BoxSize::VARIABLE_SIZE` はファイル末尾のトップレベルのボックスに限って使うことを書いた
  - `FreeBox::encode` / `MdatBox::encode` は `with_payload_size` の結果を `?` で伝える
- 仕様の根拠（ISO/IEC 14496-12:2015 の 4.2）と、将来の改訂で変わる可能性があることは、各検査のコメントに残した
- 呼び出し元の更新: wasm の `unknownBoxes` の組み立て（エラーを JSON の `data` の値のエラーにする）、`examples/transcode_wasm` の `decode_description`、テスト
- テスト
  - `tests/test_basic_types.rs`
    - `box_header_encode_size_limits`: 通常型と UUID 型、32-bit の size と largesize の 4 通りについて、1 からヘッダー幅未満までの全値が、`encode_to_vec` と空のバッファへの `encode` の両方で `InvalidInput` になることを確認する
    - `box_size_with_payload_size_overflow`: 両種別について、2 段目の加算で溢れる範囲と 1 段目の加算で溢れる範囲の両端が `InvalidInput` になることを確認する
  - `tests/test_boxes.rs`
    - `unknown_box_encode_size_mismatch`: 宣言サイズが 1 だけずれた場合（4 通り）と `BoxSize::LARGE_VARIABLE_SIZE` が拒否されることを確認する
    - 既存の `root_box_unknown_size_zero` で、size=0 のトップレベルの未知ボックスが size=0 のまま同じバイト列に再エンコードされることも確認するようにした
  - `pbt/tests/prop_basic_types.rs`
    - `box_size_with_payload_size` を、両種別・両形式の範囲と境界値（ペイロード長 0、U32 に収まる最大の総サイズ、U64 への切り替え直後、表現可能な最大の総サイズ）で総サイズを確認する PBT に書き直した。成功を期待する範囲を表現可能な値に限り、これに含まれる `boundary_tests` の 2 件（`box_size_u64_boundary` / `box_size_large_payload`）は削除した
    - ヘッダー往復の PBT を 4 通り（UUID 型の U64 を追加）にし、ヘッダーの幅（8 / 24 / 16 / 32）をリテラルで確認する。サイズには 0、ヘッダー幅、最大値を境界値として混ぜる
  - `pbt/tests/prop_additional_boxes.rs`
    - `unknown_box_roundtrip` で、両種別の短いペイロードに対して U32 と U64 を明示的に指定し、書き出されたヘッダーの幅と、デコード後のサイズの形式・値・ペイロードが元と一致することを確認する
  - マルチプレクサーの `mdat` ヘッダーの予約と確定は、既存のテストで維持されることを確認した
- UUID ヘッダー内のフィールド順序は 0095 の対象として変更していない
- `CHANGES.md` の `## develop` に [CHANGE] を追記した。ブランチは issue の指定どおり `feature/fix-` だが、公開 API の破壊的変更を含むため [CHANGE] とした（0126 と同じ扱い）
