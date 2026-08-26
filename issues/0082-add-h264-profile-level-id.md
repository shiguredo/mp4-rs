# H.264 の profile-level-id をパース・正規化する API を追加する

- Created: 2026-08-26
- Completed: {YYYY-MM-DD}
- Branch: feature/add-h264-profile-level-id
- Polished: 2026-08-26

## 目的

RFC 6184 Section 8.1 で定義された H.264 の profile-level-id をパースし、sub-profile と level へ正規化する API を `bitstream::h264` に追加する。
`parse_sps` が返す `H264Sps` の `profile_idc` / `constraint_set_flags` / `level_idc` と SDP の profile-level-id 表現を橋渡しし、利用側ごとの重複実装をなくす。

## 現状

- `src/bitstream/h264.rs` の `parse_sps` は SPS から `profile_idc` / `constraint_set_flags` / `level_idc` を抽出するが、これを RFC 6184 Section 8.1 の profile-level-id として解釈・正規化する API はない
- RFC 6184 Section 8.1 の Table 5 に基づく sub-profile 正規化（同じ sub-profile を表す profile_idc / profile-iop の複数表現の統合）と、level の正規化（Level 1b を含む）は利用側で自前実装が必要になる
- 利用側は SDP の H.264 capability 広告などで profile-level-id の正規化が必要になる。WebRTC 実装固有の profile 集合（Table 5 の部分集合や Constrained High）を選ぶ処理は利用側の責務であり、本 crate の契約にはしない

参照仕様は RFC 6184 Section 8.1 (Table 5) および Section 8.2.2 の Level 1b 記述、ITU-T H.264 (06/2026) の 7.4.2.1.1 と Annex A の profile / level 定義とする。

## 設計方針

### 正規化 API

中核 API は 3 引数の `u8`（profile_idc / profile-iop / level_idc）から `H264ProfileLevelId` へ正規化する関数とする。
`H264Sps` の `profile_idc` / `constraint_set_flags` / `level_idc` はいずれも `u8` であり、`constraint_set_flags` は RFC 6184 の profile-iop と同じ 1 バイト全体（`constraint_set0_flag` から `constraint_set5_flag` と `reserved_zero_2bits`）なので、3 フィールドをそのまま渡せる。
SDP 文字列（ちょうど 6 桁の RFC 4648 base16。`A-F` と `a-f` を受理する）のパースは、3 byte にデコードして同じ関数へ渡す薄いラッパーとする。

公開型・関数の骨格は次のとおり。モジュールは既に `bitstream::h264` なので、関数名に `h264_` は付けない。

```text
pub enum H264Profile {
    ConstrainedBaseline,
    Baseline,
    Main,
    Extended,
    High,
    High10,
    High42,
    High44,
    High10Intra,
    High42Intra,
    High44Intra,
    Cavlc444Intra,
}

pub enum H264Level {
    Level1,
    Level1b,
    Level1_1,
    Level1_2,
    Level1_3,
    Level2,
    Level2_1,
    Level2_2,
    Level3,
    Level3_1,
    Level3_2,
    Level4,
    Level4_1,
    Level4_2,
    Level5,
    Level5_1,
    Level5_2,
    Level6,
    Level6_1,
    Level6_2,
}

pub struct H264ProfileLevelId {
    pub profile: H264Profile,
    pub level: H264Level,
}

pub fn parse_profile_level_id(
    profile_idc: u8,
    profile_iop: u8,
    level_idc: u8,
) -> Result<H264ProfileLevelId>;

pub fn parse_profile_level_id_hex(hex: &str) -> Result<H264ProfileLevelId>;
```

正規化結果は元の 3 byte ではなく、上記の 2 つの enum だけを持つ。Level 1b と Level 1.1 はどちらも `level_idc == 11` になり得るため、`level_idc` の生値のままでは区別できない。

### sub-profile の正規化

RFC 6184 Section 8.1 Table 5 の 12 profile を `H264Profile` で表現する。

- ConstrainedBaseline / Baseline / Main / Extended / High / High10 / High42 / High44 / High10Intra / High42Intra / High44Intra / Cavlc444Intra

Table 5 の (profile_idc, profile-iop) パターンを mask / value で判定し、同じ sub-profile を表す複数表現を正規化する。
Table 5 に載っていない profile_idc / profile-iop の組み合わせは認識しない（エラー）。
Table 5 の全パターンが profile-iop の下位 2 bit（`reserved_zero_2bits`）に 0 を要求するため、非 0 の組み合わせも自然に拒否される。

### level の正規化

`level_idc` を ITU-T H.264 (06/2026) Annex A Table A-1 が定義する `H264Level` へ変換する。既知の `level_idc` は次に限る。これ以外はエラーとする。

- 10 → `Level1`
- 11 → `Level1_1`（下記の Level 1b を除く）
- 12 → `Level1_2`
- 13 → `Level1_3`
- 20 → `Level2`
- 21 → `Level2_1`
- 22 → `Level2_2`
- 30 → `Level3`
- 31 → `Level3_1`
- 32 → `Level3_2`
- 40 → `Level4`
- 41 → `Level4_1`
- 42 → `Level4_2`
- 50 → `Level5`
- 51 → `Level5_1`
- 52 → `Level5_2`
- 60 → `Level6`
- 61 → `Level6_1`
- 62 → `Level6_2`

Level 1b の合図は、先に Table 5 で sub-profile を確定したうえで次の 2 系統とする。RFC 6184 Section 8.1 と Section 8.2.2 の informative note、および ITU-T H.264 7.4.2.1.1 に従う。

- profile_idc が 66 / 77 / 88 のとき: `level_idc == 11` かつ `constraint_set3_flag == 1` なら `Level1b`。`level_idc == 11` かつ `constraint_set3_flag == 0` なら `Level1_1`。`level_idc == 9` はこの 3 つの profile では Level 1b の合図ではないためエラー
- それ以外の Table 5 profile のとき: `level_idc == 9` なら `Level1b`。`level_idc == 11` なら `Level1_1`。この系統では `constraint_set3_flag` は sub-profile 判定に使い、Level 1b 判定には使わない

### エラーと契約

- エラーは既存の `crate::Error` / `ErrorKind` に統合し、新しい公開エラー体系を増やさない。既存の `bitstream::h264` と同様に `Error::invalid_input` を使う
- `no_std` を維持し、crate 本体に新しい外部依存を追加しない
- 公開 API の rustdoc に 3 byte の解釈、正規化規則、エラー条件を記載する

### 対象外

- RFC 6184 Table 5 に無い sub-profile（Constrained High は ITU-T H.264 A.2.4.2 の Annex A profile だが Table 5 に行が無い）。libwebrtc の `kProfilePatterns` のような部分集合選択もしない
- SDP の negotiation ロジック（level の大小比較、互換判定、`max-recv-level` の解釈）
- C API / WASM バインディング。利用要件が明確になった時点で別 issue とする
- 他コーデックの profile / level 正規化

## 完了条件

- `parse_profile_level_id` と `parse_profile_level_id_hex` が `bitstream::h264` に追加され、`H264Sps` の `profile_idc` / `constraint_set_flags` / `level_idc` を直接渡せる
- 正規化結果は `H264ProfileLevelId { profile: H264Profile, level: H264Level }` である
- Table 5 の 12 sub-profile すべてが正しく正規化される
- 同じ sub-profile を表す複数の profile_idc / profile-iop 組み合わせが同じ sub-profile へ正規化される
- Level 1b が次の 2 系統で判別される
  - profile_idc 66 / 77 / 88 + `level_idc == 11` + `constraint_set3_flag == 1`
  - それ以外の Table 5 profile + `level_idc == 9`
- 不正な profile-level-id（桁数、base16 でない、Table 5 非該当、既知集合外の `level_idc`、66 / 77 / 88 に対する `level_idc == 9`）がエラーになる
- 決定的テスト（`tests/` 配下）が追加され、mock / stub、sleep、`#[ignore]`、外部 command、ネットワークを使用しない
- `CHANGES.md` の develop に `[ADD]` として記載される
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る
