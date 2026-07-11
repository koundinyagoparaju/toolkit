//! Fuzz every tool in the crypto pack. The first byte picks the tool, the
//! second seeds the chunk splitter, the rest is the input — so libFuzzer
//! controls all three and coverage feedback steers it toward whichever
//! tool/split/input combination reaches new code.
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;
use toolkit_core::Registry;

static REGISTRY: OnceLock<Registry> = OnceLock::new();

fuzz_target!(|data: &[u8]| {
    let [pick, seed, input @ ..] = data else { return };
    let registry = REGISTRY.get_or_init(toolkit_pack_crypto::registry);
    let manifests = registry.manifests();
    let tool = registry
        .find(&manifests[*pick as usize % manifests.len()].name)
        .unwrap();
    toolkit_core::exercise::exercise(tool, input, u64::from(*seed));
});
