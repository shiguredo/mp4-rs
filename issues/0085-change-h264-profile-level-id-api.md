# H.264 の profile-level-id API を可逆変換に限定する

- Created: 2026-08-27
- Completed: {YYYY-MM-DD}
- Branch: feature/change-h264-profile-level-id-api
- Polished: {YYYY-MM-DD}

## 目的

`bitstream::h264` の profile-level-id API を、元の 3 バイトを失わない表現と可逆な hex 変換に限定する。
特定の通信実装や negotiation 方針に依存する正規化を汎用 API から除き、利用側が生の `H264ProfileLevelId` に対して必要な互換性判定を適用できるようにする。

## 現状

- `src/bitstream/h264.rs` の `H264ProfileLevelId` は SPS 先頭の `profile_idc` / `profile_iop` / `level_idc` を検証せず保持し、`H264Sps::profile_level_id` でも利用されている
- `parse_profile_level_id_hex` は 6 桁の base16 文字列を `H264ProfileLevelId` へ可逆にデコードする
- `H264ProfileLevelId::normalize` は RFC 6184 Section 8.1 Table 5 の sub-profile と ITU-T H.264 Annex A の level を `H264ProfileLevel` へ正規化する
- 正規化は元の 3 バイトを失うため、bitstream に対応する profile-level-id の生成には利用できない
- Table 5 外の profile や level をどこまで受理するかは通信実装ごとに異なるため、正規化成功は実際の互換性を意味せず、正規化失敗も元の SPS や 3 バイトが不正であることを意味しない
- `H264ProfileLevelId` から 6 桁 hex へ戻す公開 API がなく、利用側が桁数と英字の大小を含む書式を個別に実装する必要がある

## 設計方針

### 可逆な API

次の API は維持する。

- `H264ProfileLevelId`
- `H264Sps::profile_level_id`

`parse_profile_level_id_hex` は削除し、`H264ProfileLevelId` に次の関連関数とメソッドを追加する。

```rust
pub fn from_hex(hex: &str) -> Result<Self>;
pub fn to_hex(self) -> String;
```

`from_hex` は、ちょうど 6 桁の RFC 4648 base16 文字列を `H264ProfileLevelId` へデコードする。
従来の `parse_profile_level_id_hex` と同様に `A-F` と `a-f` の両方を受理し、桁数が異なる入力や base16 でない文字を含む入力は `crate::Error` で拒否する。

`to_hex` は `profile_idc` / `profile_iop` / `level_idc` をこの順に並べた、ちょうど 6 桁の小文字 hex 文字列を返す。
各バイトはゼロ埋めした 2 桁とし、profile や level の意味検証は行わない。

任意の `H264ProfileLevelId` について、`H264ProfileLevelId::from_hex(&id.to_hex())` が元の値を返すことを契約とする。

### 正規化 API の削除

次の公開 API を削除する。

- `H264ProfileLevelId::normalize`
- `H264Profile`
- `H264Level`
- `H264ProfileLevel`

あわせて、RFC 6184 Table 5 の正規化にだけ使われる private な pattern 型、定数、profile / level 変換関数を削除する。

汎用的な profile / level 正規化 API や、特定の通信実装との互換性判定 API は代替として追加しない。
利用側は `H264ProfileLevelId` の生値を保持し、必要な仕様や対象実装に応じた判定を行う。

### テスト

- `tests/test_bitstream_h264.rs` から正規化 API 固有のテストを削除する
- `H264ProfileLevelId::from_hex` が桁数の異なる入力と base16 でない文字を拒否する決定的テストを追加する
- 先頭ゼロと `a` から `f` を含む値について、`H264ProfileLevelId::to_hex` が 6 桁の小文字を返す決定的テストを追加する
- `pbt/tests/prop_bitstream_h264.rs` に任意の 3 バイトについて `from_hex` と `to_hex` のラウンドトリップを確認する PBT を追加する

### 変更履歴

`CHANGES.md` の develop にある既存の profile-level-id `[ADD]` エントリーを、最終的に残る `H264ProfileLevelId`、`from_hex`、`to_hex` の説明へ更新する。
開発中に追加して削除した正規化 API は変更履歴に残さない。

## 完了条件

- `H264ProfileLevelId::from_hex` が大文字と小文字の 6 桁 base16 文字列を受理する
- `H264ProfileLevelId::from_hex` が桁数の異なる入力と base16 でない文字を `crate::Error` で拒否する
- `H264ProfileLevelId::to_hex` が 3 バイトを先頭ゼロを省略しない 6 桁の小文字 hex へ変換する
- 任意の `H264ProfileLevelId` について `H264ProfileLevelId::from_hex(&id.to_hex())` が元の値を返す PBT がある
- `parse_profile_level_id_hex`、`H264ProfileLevelId::normalize`、`H264Profile`、`H264Level`、`H264ProfileLevel` と正規化専用の private 実装が削除される
- `H264Sps::profile_level_id` と `parse_sps` の生値保持は維持される
- `CHANGES.md` の既存 `[ADD]` エントリーが最終的な公開 API に一致する
- mock / stub、sleep、`#[ignore]`、外部 command、ネットワークをテストで使用しない
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`cargo test --workspace`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通る
