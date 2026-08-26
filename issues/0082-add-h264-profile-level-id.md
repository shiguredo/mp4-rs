# H.264 の profile-level-id をパース・正規化する API を追加する

- Created: 2026-08-26
- Completed: {YYYY-MM-DD}
- Branch: feature/add-h264-profile-level-id
- Polished: {YYYY-MM-DD}

## 目的

RFC 6184 Section 8.1 で定義された H.264 の profile-level-id をパースし、sub-profile と level へ正規化する API を `bitstream::h264` に追加する。
`parse_sps` が返す `H264Sps` の `profile_idc` / `constraint_set_flags` / `level_idc` と SDP の profile-level-id 表現を橋渡しし、利用側ごとの重複実装をなくす。

## 現状

- `src/bitstream/h264.rs` の `parse_sps` は SPS から `profile_idc` / `constraint_set_flags` / `level_idc` を抽出するが、これを RFC 6184 Section 8.1 の profile-level-id として解釈・正規化する API はない
- RFC 6184 Section 8.1 の Table 5 に基づく sub-profile 正規化（同じ sub-profile を表す profile_idc / profile-iop の複数表現の統合）と、level の正規化（level_idc と constraint_set3_flag による Level 1b の判別）は利用側で自前実装が必要になる
- `shiguredo/sora-rust-sdk` は SDP の H.264 capability を広告するために profile-level-id の正規化を自前実装している（WebRTC 固有の profile 集合を選ぶ形）

参照仕様は RFC 6184 Section 8.1 (Table 5)、および ITU-T H.264 Annex A の profile / level 定義とする。

## 設計方針

### 正規化 API

中核 API は 3 byte（profile_idc / profile-iop / level_idc）から `H264ProfileLevelId` へ正規化するバイト列ベースの関数とし、SDP 文字列（6 桁 base16）のパースは薄いラッパーとして追加する。
`H264Sps` の `profile_idc` / `constraint_set_flags` / `level_idc` をそのまま渡せる形にする。

### sub-profile の正規化

RFC 6184 Section 8.1 Table 5 の 12 profile を enum で表現する。

- ConstrainedBaseline / Baseline / Main / Extended / High / High10 / High42 / High44 / High10Intra / High42Intra / High44Intra / Cavlc444Intra

Table 5 の (profile_idc, profile-iop) パターンを mask / value で判定し、同じ sub-profile を表す複数表現を正規化する。
Table 5 に載っていない profile_idc / profile-iop の組み合わせは認識しない（エラー）。
Table 5 の全パターンが profile-iop の下位 2 bit（reserved_zero_2bits）に 0 を要求するため、非 0 の組み合わせも自然に拒否される。

### level の正規化

level_idc を ITU-T H.264 Annex A が定義する level へ変換する。
Level 1b は RFC 6184 Section 8.1 どおり、profile_idc が 66 / 77 / 88 かつ level_idc == 11 かつ constraint_set3_flag == 1 で判別する。
未知の level_idc は単純な整数比較で受理せずエラーにする。

### エラーと契約

- エラーは既存の `crate::Error` / `ErrorKind` に統合し、新しい公開エラー体系を増やさない
- `no_std` を維持し、crate 本体に新しい外部依存を追加しない
- 公開 API の rustdoc に 3 byte の解釈、正規化規則、エラー条件を記載する

### 対象外

- WebRTC 実装固有の profile 集合の扱い（libwebrtc の `kProfilePatterns` の部分集合選択、Constrained High、reserved_zero_2bits 非 0 の積極的拒否など）。RFC 6184 に忠実に実装する
- SDP の negotiation ロジック（level の大小比較、互換判定、`max-recv-level` の解釈）
- C API / WASM バインディング。利用要件が明確になった時点で別 issue とする
- 他コーデックの profile / level 正規化

## 完了条件

- 3 byte からの正規化 API と 6 桁 base16 文字列のパース API が `bitstream::h264` に追加され、`H264Sps` の `profile_idc` / `constraint_set_flags` / `level_idc` を直接渡せる
- Table 5 の 12 sub-profile すべてが正しく正規化される
- 同じ sub-profile を表す複数の profile_idc / profile-iop 組み合わせが同じ sub-profile へ正規化される
- Level 1b が profile_idc 66 / 77 / 88 + level_idc 11 + constraint_set3_flag で判別される
- 不正な profile-level-id（桁数、base16 でない、Table 5 非該当、未知の level_idc）がエラーになる
- 決定的テスト（`tests/` 配下）が追加され、mock / stub、sleep、`#[ignore]`、外部 command、ネットワークを使用しない
- `CHANGES.md` の develop に `[ADD]` として記載される
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る
