//! Embeds data/ and builds/ into the library so the web (WebAssembly)
//! version needs no file system: any change to the data files is picked up
//! by the next build.
use std::fs;
use std::path::{Path, PathBuf};

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(root, &p, out);
        } else if p.extension().map_or(false, |x| x == "toml") {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            out.push((rel, fs::canonicalize(&p).unwrap()));
        }
    }
}

fn list(name: &str, root: &Path) -> String {
    let mut files = Vec::new();
    walk(root, root, &mut files);
    let mut s = format!("pub static {name}: &[(&str, &str)] = &[\n");
    for (rel, abs) in files {
        s.push_str(&format!("    ({rel:?}, include_str!({:?})),\n", abs.to_string_lossy()));
    }
    s.push_str("];\n");
    s
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("embedded.rs");
    let mut src = String::new();
    src.push_str(&list("DATA_FILES", &manifest.join("data")));
    src.push_str(&list("BUILD_FILES", &manifest.join("builds")));
    fs::write(out, src).unwrap();
    println!("cargo:rerun-if-changed=data");
    println!("cargo:rerun-if-changed=builds");
    println!("cargo:rerun-if-changed=build.rs");
}
