# MP4 ライブラリ C API

MP4 ファイルの読み込み（デマルチプレックス）と書き込み（マルチプレックス）を行うための C 言語 API です。

C 言語用のヘッダファイルは [`include/mp4.h`](./include/mp4.h) にあり、
以下のサンプルプログラムに実際の使用例が記載されています:

- [`examples/demux.c`](./examples/demux.c): MP4 ファイルをデマルチプレックスして情報を表示する例
- [`examples/remux.c`](./examples/remux.c): MP4 ファイルをリマルチプレックス（読み込んで再度書き込み）する例

## サンプルプログラムのビルド方法

### ビルド済みライブラリを使用する方法

[mp4-rs の Releases ページ](https://github.com/shiguredo/mp4-rs/releases) から、お使いのプラットフォームに対応したアーカイブをダウンロードしてください。

```bash
# 例: macOS の場合
#
# NOTE: VERSION 環境は変数は実際に値に適宜置き換えてください
wget https://github.com/shiguredo/mp4-rs/releases/download/$VERSION/mp4-$VERSION_macos-26_arm64.tar.gz

# アーカイブを展開
tar zxvf mp4-$VERSION_macos-26_arm64.tar.gz
```

展開後のディレクトリ構造は以下の通りです:

```
mp4-$VERSION_macos-26_arm64/
├── include/
│   └── mp4.h
└── lib/
    ├── libmp4.a       （静的ライブラリ）
    └── libmp4.dylib   （動的ライブラリ）
```

ダウンロード済みの静的ライブラリを使用してサンプルプログラムをビルドします。

```bash
# demux.c をビルドおよび実行
cc -o demux \
   -I mp4-$VERSION_macos-26_arm64/include/ \
   mp4-$VERSION_macos-26_arm64/lib/libmp4.a \
   crates/c-api/examples/demux.c
./demux /path/to/sample.mp4

# remux.c をビルドおよび実行
cc -o remux \
   -I mp4-$VERSION_macos-26_arm64/include/ \
   mp4-$VERSION_macos-26_arm64/lib/libmp4.a \
   crates/c-api/examples/remux.c
./remux /path/to/input.mp4 /path/to/output.mp4
```

### ライブラリを自前でビルドする方法

```bash
# mp4-rs のプロジェクトルートでライブラリをビルド
cargo build --release

# サンプルプログラムのビルドに必要なファイルのパスは以下の通りです:
# - C ヘッダファイル: crates/c-api/include/mp4.h
# - ライブラリファイル:
#   - target/release/libmp4.a (静的ライブラリ)
#   - target/release/libmp4.so (動的ライブラリ)

# demux.c をビルドおよび実行
cc -o target/release/demux \
   -I crates/c-api/include/ \
   crates/c-api/examples/demux.c \
   target/release/libmp4.a
./target/release/demux /path/to/sample.mp4

# remux.c をビルドおよび実行
cc -o target/release/remux \
   -I crates/c-api/include/ \
   crates/c-api/examples/remux.c \
   target/release/libmp4.a
./target/release/remux /path/to/input.mp4 /path/to/output.mp4
```
