//! Fuzz the chain schema end to end: pipe-syntax parsing, JSON parsing,
//! validation, and — when validation accepts — execution. The engine
//! trusts validate() (expect("validated"), unreachable!), so any hole in
//! validation that lets a malformed DAG through becomes a panic here
//! rather than in a user's `run-chain -f file.json`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;
use toolkit_core::{Chain, DataValue, Registry};

static REGISTRY: OnceLock<Registry> = OnceLock::new();

fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| {
        Registry::merge([
            toolkit_pack_text::registry(),
            toolkit_pack_image::registry(),
            toolkit_pack_crypto::registry(),
            toolkit_pack_data::registry(),
        ])
    })
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let registry = registry();

    if let Ok(chain) = Chain::from_pipe_syntax(text) {
        if chain.validate(registry).is_ok() {
            let _ = chain.execute(registry, DataValue::Text("x".into()));
        }
    }
    if let Ok(chain) = serde_json::from_str::<Chain>(text) {
        if chain.validate(registry).is_ok() {
            let _ = chain.execute(registry, DataValue::Text("x".into()));
        }
    }
});
