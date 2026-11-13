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

/// ルートの `Cargo.toml` の `[package]` セクションからバージョンを読み取る
fn read_root_version(path: &Path) -> String {
    let text = std::fs::read_to_string(path).expect("failed to read root Cargo.toml");
    text.lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("version") {
                trimmed
                    .split_once('=')
                    .map(|(_, value)| value.trim().trim_matches('"').to_string())
            } else {
                None
            }
        })
        .expect("version not found in root Cargo.toml")
}
