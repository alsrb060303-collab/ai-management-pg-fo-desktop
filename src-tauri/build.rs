use std::fmt::Write as _;
use std::path::Path;

fn main() {
    embed_builtin_blocks();
    tauri_build::build()
}

/// Embeds every `catalog/builtin/*.md` as `(id, text)` so blocks can be added
/// by dropping in a file.
fn embed_builtin_blocks() {
    let dir = Path::new("catalog/builtin");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("catalog/builtin exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    files.sort();

    // An absolute path that is not `\\?\`-prefixed (canonicalize does that on Windows).
    let root = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).to_path_buf();
    let mut out = String::from("&[\n");
    for path in files {
        let id = path.file_stem().unwrap().to_string_lossy();
        let abs = root.join(&path);
        writeln!(
            out,
            "    ({id:?}, include_str!({:?})),",
            abs.display().to_string()
        )
        .unwrap();
    }
    out.push(']');

    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("builtin_blocks.rs");
    std::fs::write(dest, out).unwrap();
}
