use std::path::PathBuf;
use std::process::Command;

#[test]
fn test_c_examples_compile() {
    let project_root = get_project_root();
    let lib_path = project_root.join("target/debug/libmp4.a");

    // ライブラリファイルが存在することを確認
    assert!(
        lib_path.exists(),
        "libmp4.a が {} に見つからない。先に `cargo build` を実行すること",
        lib_path.display()
    );

    // examples ディレクトリから全ての .c ファイルを検索
    let c_files: Vec<_> = std::fs::read_dir(project_root.join("crates/c-api/examples/"))
        .expect("examples ディレクトリの読み取りに失敗した")
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
        "examples ディレクトリに .c ファイルが見つからない"
    );

    // 各 C ファイルをコンパイルする
    for c_file in c_files {
        let example_name = c_file
            .file_stem()
            .expect("ファイル stem の取得に失敗した")
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

        let status = cmd.status().expect("cc コマンドの実行に失敗した");

        assert!(
            status.success(),
            "example のコンパイルに失敗した: {example_name}"
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
        "libmp4.a が {} に見つからない。先に `cargo build` を実行すること",
        lib_path.display()
    );

    let c_file = project_root.join("crates/c-api/tests/simple_mux_demux.c");
    assert!(
        c_file.exists(),
        "simple_mux_demux.c が {} に見つからない",
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

    let status = cmd
        .status()
        .expect("simple_mux_demux.c のコンパイルに失敗した");

    assert!(
        status.success(),
        "simple_mux_demux.c のコンパイルに失敗した"
    );

    // コンパイルされた実行ファイルを実行
    let status = Command::new(&output_path)
        .status()
        .expect("simple_mux_demux の実行に失敗した");

    assert!(status.success(), "simple_mux_demux の実行が失敗した");
}

fn get_project_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("プロジェクトルートの特定に失敗した")
        .to_path_buf()
}
