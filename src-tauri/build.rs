use std::fmt::Write as _;
use std::path::Path;

fn main() {
    embed_builtin_blocks();
    embed_external_blocks();
    embed_external_companions();
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

/// Embeds the Markdown files that go next to a bundled skill's SKILL.md
/// (`catalog/external/<source>/<stem>/<relative path>.md`) as
/// `(source, stem, relative path, text)`.
fn embed_external_companions() {
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "md") {
                out.push(path);
            }
        }
    }

    let dir = Path::new("catalog/external");
    let root = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).to_path_buf();
    let mut out = String::from("&[\n");
    let mut sources: Vec<_> = std::fs::read_dir(dir)
        .expect("catalog/external exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    sources.sort();
    for source in sources {
        let name = source.file_name().unwrap().to_string_lossy().into_owned();
        let mut stems: Vec<_> = std::fs::read_dir(&source)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        stems.sort();
        for stem_dir in stems {
            println!("cargo:rerun-if-changed={}", stem_dir.display());
            let stem = stem_dir.file_name().unwrap().to_string_lossy().into_owned();
            let mut files = Vec::new();
            walk(&stem_dir, &mut files);
            files.sort();
            for path in files {
                let rel = path
                    .strip_prefix(&stem_dir)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                let abs = root.join(&path);
                writeln!(
                    out,
                    "    ({name:?}, {stem:?}, {rel:?}, include_str!({:?})),",
                    abs.display().to_string()
                )
                .unwrap();
            }
        }
    }
    out.push(']');

    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("external_files.rs");
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
