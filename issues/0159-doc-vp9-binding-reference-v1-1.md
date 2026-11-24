# VP Codec ISO Media File Format Binding の参照先を v1.1 にそろえる

- Created: 2026-11-24
- Completed: {YYYY-MM-DD}
- Branch: feature/fix-vp9-binding-reference
- Polished: {YYYY-MM-DD}

## 目的

リポジトリ内の VP Codec ISO Media File Format Binding への参照を、`build_vp09_box` のコメントが根拠にしている v1.1 にそろえる。

今は `src/bitstream/vp9.rs` のモジュール doc などが v1.0 のページを指している。一方、同じファイルの `build_vp09_box` のコメントは v1.1 を根拠にしているため、読み手が参照先を開いても引用元の版と一致しない。

## 現状

2 つのページの版を確認した。

- `https://www.webmproject.org/vp9/mp4/` のページは「v1.0, 2017-03-31」と表記している
- `https://github.com/webmproject/vp9-dash/blob/master/VPCodecISOMediaFileFormatBinding.md` は「v1.1 2023-11-01」と表記している
- どちらも "The authoritative source of this document resides in the vp9-dash GitHub repository." と書いている
- `build_vp09_box` が根拠にする VP Codec Configuration Box のプロファイルの表と、`VPCodecConfigurationRecord` の構文は、両方の版で同じ内容である。そのため今の実装の挙動には影響しない。それ以外の差分は確かめていない
- vp9-dash リポジトリのタグは `v1.0.0` だけで、v1.1 のタグはない。`VPCodecISOMediaFileFormatBinding.md` の `master` には、2023-11-01 より後のコミットもある（2023-12-21 の "clarify bitdepth/subsampling restrictions" と 2024-04-23 の誤字修正）。それでも版の表記は「v1.1 2023-11-01」のままである

v1.0 のページ（`https://www.webmproject.org/vp9/mp4/`）を指している箇所:

- `src/bitstream/vp9.rs` のモジュール doc（`//!`）
- `src/bitstream/vp8.rs` のモジュール doc（`//!`）。URL のパスに `vp9` を含むが、同じページで VP8（`vp08`）のサンプルエントリーも定めている、という補足が付いている
- `src/boxes_sample_entry.rs` の `Vp08Box` / `Vp09Box` / `VpccBox` の doc
- `README.md` / `README.en.md` の参照仕様の一覧
- `examples/transcode_wasm/src/transcode.rs` の `build_output_sample_entry` で、`vp09.00.10.08` を扱う箇所のコメント

版を書いている箇所:

- `src/bitstream/vp9.rs` の `build_vp09_box` 内のコメント（`//`）は、「VP Codec ISO Media File Format Binding v1.1 の VP Codec Configuration Box の semantics」「Binding v1.1 と VP9 spec に基づく」と版だけを書いており、URL と日付は書いていない
- 他の箇所は版を書いていない

## 設計方針

- 上に挙げた箇所の URL を、vp9-dash リポジトリの `master` にある `VPCodecISOMediaFileFormatBinding.md` に変え、版（v1.1）も書く
  - v1.1 にはタグがないため、`master` を指す。`master` は版の表記を変えずに更新されることがあるが、コードコメントは「将来の改訂で変わる可能性がある」と書く方針をとっているので、それに合わせる
- `src/bitstream/vp8.rs` のモジュール doc にある URL のパスについての補足は、URL が変わると前提がなくなる。v1.1 の文書も `vp08` と `vp09` の両方を定めている（"The 4CC codes currently defined by this spec are 'vp08' and 'vp09'."）ことに合わせて書き直す
- 実装とテストは変えない

## 完了条件

- `rg -n 'webmproject\.org/vp9/mp4' --glob '!issues/**' .` の結果が空になる（develop では上の 8 箇所が出る。内訳は `src/bitstream/vp9.rs` 1、`src/bitstream/vp8.rs` 1、`src/boxes_sample_entry.rs` 3、`README.md` 1、`README.en.md` 1、`examples/transcode_wasm/src/transcode.rs` 1）
- 書き換えた各参照に、版（v1.1）が書かれている
- `cargo doc --no-deps -p shiguredo_mp4` が成功する
