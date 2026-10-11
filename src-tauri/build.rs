use std::fmt::Write as _;
use std::path::Path;

fn main() {
    embed_builtin_blocks();
    embed_external_blocks();
    tauri_build::build()
}

/// Embeds `catalog/external/<source>/*.md` (third-party blocks with their
/// provenance in frontmatter) as `(source, stem, text)`.
fn embed_external_blocks() {
    let dir = Path::new("catalog/external");
    println!("cargo:rerun-if-changed={}", dir.display());
    let root = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).to_path_buf();

    let mut sources: Vec<_> = std::fs::read_dir(dir)
        .expect("catalog/external exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    sources.sort();

    let mut out = String::from("&[\n");
    for source in sources {
        println!("cargo:rerun-if-changed={}", source.display());
        let name = source.file_name().unwrap().to_string_lossy().into_owned();
        let mut files: Vec<_> = std::fs::read_dir(&source)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "md"))
            .collect();
        files.sort();
        for path in files {
            let stem = path.file_stem().unwrap().to_string_lossy();
            let abs = root.join(&path);
            writeln!(
                out,
                "    ({name:?}, {stem:?}, include_str!({:?})),",
                abs.display().to_string()
            )
            .unwrap();
        }
    }
    out.push(']');

    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("external_blocks.rs");
    std::fs::write(dest, out).unwrap();
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
