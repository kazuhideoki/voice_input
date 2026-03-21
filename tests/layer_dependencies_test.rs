//! レイヤ間の依存方向を `cargo test` の既存フロー内で継続検証するためのテスト。
//! 新しい依存制約を追加したい場合は、このファイルへ禁止ルールを足していく。

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_files(root, &mut files);
    files.sort();
    files
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }

    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn assert_forbidden_dependency_absent(layer_dir: &str, forbidden_path: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(layer_dir);
    let mut violations = Vec::new();

    for path in rust_files_under(&root) {
        let content = fs::read_to_string(&path).unwrap();
        if contains_forbidden_dependency(&content, forbidden_path) {
            violations.push(
                path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
                    .unwrap()
                    .display()
                    .to_string(),
            );
        }
    }

    assert!(
        violations.is_empty(),
        "layer dependency violation: {layer_dir} must not depend on {forbidden_path}\n{}",
        violations.join("\n")
    );
}

fn contains_forbidden_dependency(content: &str, forbidden_path: &str) -> bool {
    let normalized = content
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    let forbidden_module = forbidden_path.trim_start_matches("crate::");

    [
        format!("crate::{forbidden_module}"),
        format!("crate::{{{forbidden_module}"),
        format!(",{forbidden_module}::"),
        format!(",{forbidden_module}::{{"),
        format!("{{{forbidden_module}::"),
        format!("{{{forbidden_module}::{{"),
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
}

/// domain層はapplication層とinfrastructure層へ依存しない
#[test]
fn domain_does_not_depend_on_outer_layers() {
    assert_forbidden_dependency_absent("src/domain", "crate::application");
    assert_forbidden_dependency_absent("src/domain", "crate::infrastructure");
}

/// application層はinfrastructure層へ依存しない
#[test]
fn application_does_not_depend_on_infrastructure() {
    assert_forbidden_dependency_absent("src/application", "crate::infrastructure");
}
