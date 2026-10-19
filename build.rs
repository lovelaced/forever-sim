//! Embeds data/ and builds/ into the library so the web (WebAssembly)
//! version needs no file system: any change to the data files is picked up
//! by the next build. Also stamps the build with `FSIM_BUILD_ID`, a hash of
//! the engine's source and data, so saved results can tell which engine
//! made them.
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

fn list(name: &str, root: &Path, only: &[&str]) -> String {
    let mut files = Vec::new();
    if only.is_empty() {
        walk(root, root, &mut files);
    } else {
        for d in only {
            walk(root, &root.join(d), &mut files);
        }
    }
    let mut s = format!("pub static {name}: &[(&str, &str)] = &[\n");
    for (rel, abs) in files {
        s.push_str(&format!("    ({rel:?}, include_str!({:?})),\n", abs.to_string_lossy()));
    }
    s.push_str("];\n");
    s
}

// Every file under `dir` with one of `exts`, sorted, for the build id.
fn files(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            files(&p, exts, out);
        } else if p.extension().map_or(false, |x| exts.iter().any(|e| x == *e)) {
            out.push(p);
        }
    }
}

// FNV-1a over the engine's source and data (paths and contents).
fn build_id(root: &Path) -> String {
    let mut all = Vec::new();
    files(&root.join("src"), &["rs"], &mut all);
    files(&root.join("data"), &["toml"], &mut all);
    files(&root.join("builds"), &["toml"], &mut all);
    all.push(root.join("build.rs"));
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for p in all {
        let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        eat(rel.as_bytes());
        eat(&fs::read(&p).unwrap_or_default());
    }
    format!("{h:016x}")
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("embedded.rs");
    let mut src = String::new();
    // what the engine loads: the game data and its patches (not the audit's
    // reference notes or the what-if examples)
    src.push_str(&list("DATA_FILES", &manifest.join("data"), &["base", "patches"]));
    src.push_str(&list("BUILD_FILES", &manifest.join("builds"), &[]));
    fs::write(out, src).unwrap();
    println!("cargo:rustc-env=FSIM_BUILD_ID={}", build_id(&manifest));
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=data");
    println!("cargo:rerun-if-changed=builds");
    println!("cargo:rerun-if-changed=build.rs");
}
