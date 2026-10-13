use std::path::{Path, PathBuf};
use std::fmt::Write as _;

fn generate_default_styles() {
    let styles_dir = Path::new("styles");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR environment variable not set");
    let dest_path = Path::new(&out_dir).join("default_styles.rs");

    let mut files = Vec::new();
    walk_scss_files(styles_dir, styles_dir, &mut files);

    let mut code = String::from("pub const DEFAULT_STYLES: &[(&str, &str)] = &[\n");
    for (rel_path, abs_path) in &files {
        let _ = writeln!(
            code,
            "    (\"{}\", include_str!(\"{}\")),",
            rel_path.display(),
            abs_path.display()
        );
    }
    let _ = writeln!(code, "];");

    std::fs::write(&dest_path, code).unwrap();
}

fn walk_scss_files(root: &Path, current: &Path, entries: &mut Vec<(PathBuf, PathBuf)>) {
    if let Ok(read_dir) = std::fs::read_dir(current) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_scss_files(root, &path, entries);
            } else if path.extension().is_some_and(|e| e == "scss") {
                let rel_path = path.strip_prefix(root).unwrap().to_path_buf();
                let abs_path = std::fs::canonicalize(&path).unwrap();
                entries.push((rel_path, abs_path));
            }
        }
    }
}

fn main() {
    // Bundle styles into binary
    generate_default_styles();
    println!("cargo:rerun-if-changed=styles");
}