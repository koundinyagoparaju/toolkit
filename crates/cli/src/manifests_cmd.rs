//! The `toolkit manifests` command: the machine-readable tool catalog the
//! web build consumes (written to web/public/wasm/manifests.json).
//!
//! Includes the coercion matrix so the web UI type-checks chain edges with
//! exactly the same rules as core — generated, not duplicated.

use toolkit_core::DataType;

pub fn catalog_json() -> String {
    let coercions: serde_json::Map<String, serde_json::Value> = DataType::ALL
        .iter()
        .map(|from| {
            let targets: Vec<&str> = DataType::ALL
                .iter()
                .filter(|to| DataType::can_coerce(*from, **to))
                .map(|to| to.name())
                .collect();
            (from.name().to_string(), targets.into())
        })
        .collect();

    let packs = serde_json::json!([
        {
            "name": "text",
            "module": "text.wasm",
            "tools": toolkit_pack_text::registry().manifests(),
        },
        {
            "name": "image",
            "module": "image.wasm",
            "tools": toolkit_pack_image::registry().manifests(),
        },
        {
            "name": "crypto",
            "module": "crypto.wasm",
            "tools": toolkit_pack_crypto::registry().manifests(),
        },
        {
            "name": "data",
            "module": "data.wasm",
            "tools": toolkit_pack_data::registry().manifests(),
        },
    ]);

    serde_json::to_string_pretty(&serde_json::json!({
        "version": 1,
        "coercions": coercions,
        "packs": packs,
    }))
    .expect("catalog serializes")
}
