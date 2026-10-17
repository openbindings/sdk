//! Embeds digests of the instrument and SDK sources this binary is compiled
//! from. The build driver recomputes them from the trees it claims to build and
//! refuses a binary that reports others, such as one Cargo reused from another
//! tree. FNV-1a (64-bit) runs over every regular file, sorted by relative path:
//! the path's UTF-8 bytes, a zero byte, the length as eight little-endian bytes,
//! then the contents. Symbolic links and `target` directories are skipped.
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const INSTRUMENT: [&str; 4] = ["src", "Cargo.toml", "Cargo.lock", "build.rs"];
const SDK: [&str; 2] = ["crates", "vendor"];

fn collect(root: &Path, path: &Path, files: &mut Vec<(String, PathBuf)>) {
    let kind = fs::symlink_metadata(path).unwrap().file_type();
    if kind.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() != "target" {
                collect(root, &entry.path(), files);
            }
        }
    } else if kind.is_file() {
        let relative = path.strip_prefix(root).unwrap();
        let parts: Vec<_> = relative.iter().map(|p| p.to_str().unwrap()).collect();
        files.push((parts.join("/"), path.to_path_buf()));
    }
}

fn digest(root: &Path, entries: &[&str]) -> String {
    let mut files = Vec::new();
    for entry in entries {
        let path = root.join(entry);
        if path.exists() {
            collect(root, &path, &mut files);
        }
    }
    files.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut update = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for (relative, path) in files {
        let contents = fs::read(path).unwrap();
        update(relative.as_bytes());
        update(&[0]);
        update(&(contents.len() as u64).to_le_bytes());
        update(&contents);
    }
    format!("{hash:016x}")
}

fn main() {
    let instrument = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source = instrument.join("../..");
    for entry in INSTRUMENT {
        println!("cargo::rerun-if-changed={entry}");
    }
    for entry in SDK {
        println!("cargo::rerun-if-changed={}", source.join(entry).display());
    }
    println!(
        "cargo::rustc-env=EVALUATOR_BENCH_INSTRUMENT_DIGEST={}",
        digest(&instrument, &INSTRUMENT)
    );
    println!(
        "cargo::rustc-env=EVALUATOR_BENCH_SDK_DIGEST={}",
        digest(&source, &SDK)
    );
}
