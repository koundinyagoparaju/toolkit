//! Adversarial-input property test: every tool in this pack survives
//! arbitrary bytes without panicking, and streaming tools produce the
//! same outcome regardless of chunk boundaries. Deterministic (seeded),
//! so a failure reproduces; the cargo-fuzz targets in fuzz/ run the same
//! harness with coverage guidance.

use toolkit_core::exercise::{corpus_entry, exercise};

#[test]
fn tools_survive_arbitrary_inputs() {
    let registry = toolkit_pack_image::registry();
    for manifest in registry.manifests() {
        let tool = registry.find(&manifest.name).unwrap();
        for round in 0..40 {
            let seed = 0xC0FFEE ^ u64::from(round);
            let entry = corpus_entry(seed, round);
            // Cap the size: qr-generate happily encodes kilobytes of
            // arbitrary text, and 200 max-size QR codes take ~19s in
            // debug builds. A few dozen bytes walks the same code.
            let entry = &entry[..entry.len().min(64)];
            exercise(tool, entry, seed);
        }
    }
}
