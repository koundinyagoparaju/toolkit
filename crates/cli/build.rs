//! Embed the chain library into the binary. Chains are pure data (~1 KB
//! of JSON each), so shipping them inside the CLI means `toolkit chains`
//! works anywhere, versioned with the release, with nothing to install
//! or drift. Generates OUT_DIR/builtin_chains.rs with one include_str!
//! per chains/*.json.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let chains_dir = Path::new(&manifest_dir).join("../../chains");
    println!("cargo::rerun-if-changed={}", chains_dir.display());

    let mut entries: Vec<(String, String)> = std::fs::read_dir(&chains_dir)
        .expect("chains/ directory exists in the repo")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_stem()?.to_str()?.to_string();
            (path.extension()? == "json" && name != "index")
                .then(|| (name, path.canonicalize().unwrap().display().to_string()))
        })
        .collect();
    entries.sort();

    let mut code = String::from(
        "/// The chain library shipped inside the binary: (name, json).\n\
         pub static BUILTIN_CHAINS: &[(&str, &str)] = &[\n",
    );
    for (name, path) in &entries {
        writeln!(code, "    ({name:?}, include_str!({path:?})),").unwrap();
    }
    code.push_str("];\n");

    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("builtin_chains.rs");
    std::fs::write(out, code).unwrap();
}
