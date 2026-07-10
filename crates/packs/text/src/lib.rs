//! Text & encoding tools.
//!
//! To add a tool: create a module with a unit struct implementing
//! [`toolkit_core::Tool`], then add it to [`registry`] below. That's the
//! whole integration — the CLI, the web catalog, forms, and chains pick it
//! up from the manifest.

mod base64_tools;
mod doc_merge;
mod hash;
mod hex;
mod json;
mod json_pick;
mod jwt;
mod url;

use toolkit_core::Registry;

pub fn registry() -> Registry {
    Registry::new(vec![
        Box::new(base64_tools::Base64Encode),
        Box::new(base64_tools::Base64Decode),
        Box::new(doc_merge::DocMerge),
        Box::new(url::UrlEncode),
        Box::new(url::UrlDecode),
        Box::new(hex::HexEncode),
        Box::new(hex::HexDecode),
        Box::new(jwt::JwtDecode),
        Box::new(json::JsonFormat),
        Box::new(json::JsonMinify),
        Box::new(json_pick::JsonPick),
        Box::new(hash::Hash),
    ])
}

toolkit_core::export_pack_abi!(crate::registry);

#[cfg(test)]
mod pack_tests {
    use toolkit_core::{validate_against_specs, Options};

    #[test]
    fn streaming_flag_matches_sessions() {
        let registry = super::registry();
        for m in registry.manifests() {
            let tool = registry.find(&m.name).unwrap();
            match validate_against_specs(&m.options, &Options::new(), &m.name) {
                Ok(opts) => {
                    let streams = tool.open_stream(&opts).unwrap().is_some();
                    assert_eq!(
                        streams, m.streaming,
                        "tool {} flag/session mismatch",
                        m.name
                    );
                }
                Err(_) => assert!(!m.streaming, "streaming tool {} needs defaults", m.name),
            }
        }
    }
}
