use std::path::PathBuf;
use std::process::Command;

#[test]
fn test_c_examples_compile() {
    let project_root = get_project_root();
    let lib_path = project_root.join("target/debug/libmp4.a");

    // ライブラリファイルが存在することを確認
    assert!(
        lib_path.exists(),
        "libmp4.a not found at {}. Run `cargo build` first.",
        lib_path.display()
    );

    // examples ディレクトリから全ての .c ファイルを検索
    let c_files: Vec<_> = std::fs::read_dir(project_root.join("crates/c-api/examples/"))
        .expect("Failed to read examples directory")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "c") {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    assert!(
        !c_files.is_empty(),
        "No .c files found in examples directory"
    );

    // 各 C ファイルをコンパイルする
    for c_file in c_files {
        let example_name = c_file
            .file_stem()
            .expect("Failed to get file stem")
            .to_string_lossy();
        let output_path = project_root
            .join("target/debug")
            .join(format!("{}", example_name));

        // C コンパイラでビルド
        let mut cmd = Command::new("cc");
        cmd.arg(&c_file)
            .arg("-o")
            .arg(&output_path)
            .arg(&lib_path)
            .arg("-I")
            .arg(project_root.join("crates/c-api/include"));

        // Windows のみ追加のライブラリをリンク
        #[cfg(target_os = "windows")]
        cmd.arg("-lws2_32").arg("-lntdll").arg("-luserenv");

        let status = cmd.status().expect("Failed to execute cc command");

        assert!(
            status.success(),
            "Compilation failed for example: {example_name}"
        );
    }
}

#[test]
fn test_simple_mux_demux() {
    let project_root = get_project_root();
    let lib_path = project_root.join("target/debug/libmp4.a");

    // ライブラリファイルが存在することを確認
    assert!(
        lib_path.exists(),
        "libmp4.a not found at {}. Run `cargo build` first.",
        lib_path.display()
    );

    let c_file = project_root.join("crates/c-api/tests/simple_mux_demux.c");
    assert!(
        c_file.exists(),
        "simple_mux_demux.c not found at {}",
        c_file.display()
    );

    let output_path = project_root.join("target/debug").join("simple_mux_demux");

    // C ファイルをコンパイル
    let mut cmd = Command::new("cc");
    cmd.arg(&c_file)
        .arg("-o")
        .arg(&output_path)
        .arg(&lib_path)
        .arg("-I")
        .arg(project_root.join("crates/c-api/include"));

    // Windows のみ追加のライブラリをリンク
    #[cfg(target_os = "windows")]
    cmd.arg("-lws2_32").arg("-lntdll").arg("-luserenv");

    let status = cmd.status().expect("Failed to compile simple_mux_demux.c");

    assert!(
        status.success(),
        "Compilation failed for simple_mux_demux.c"
    );

    // コンパイルされた実行ファイルを実行
    let status = Command::new(&output_path)
        .status()
        .expect("Failed to execute simple_mux_demux");

    assert!(status.success(), "simple_mux_demux execution failed");
}

fn get_project_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to find project root")
        .to_path_buf()
}
