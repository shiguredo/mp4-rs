//! wasm ビルド時にルートのパッケージバージョンを埋め込むビルドスクリプト

use std::path::Path;

fn main() {
    // ルートの Cargo.toml が変わったら再ビルドされるようにする
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set");
    let root_manifest = Path::new(&manifest_dir).join("../../Cargo.toml");
    println!("cargo::rerun-if-changed={}", root_manifest.display());

    let version = read_root_version(&root_manifest);
    println!("cargo::rustc-env=SHIGUREDO_MP4_VERSION={version}");
}

/// ルートの `Cargo.toml` からパッケージのバージョンを読み取る
///
/// `version.workspace = true` のような継承指定や行末コメントは扱わない。
fn read_root_version(path: &Path) -> String {
    let text = std::fs::read_to_string(path).expect("failed to read root Cargo.toml");
    text.lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix("version")?;
            let value = rest.trim_start().strip_prefix('=')?;
            let value = value.split('#').next()?.trim();
            value
                .strip_prefix('"')?
                .strip_suffix('"')
                .map(str::to_string)
        })
        .expect("version not found in root Cargo.toml")
}
