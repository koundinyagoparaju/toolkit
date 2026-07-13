//! Cryptographic and generator tools. Generators take their randomness
//! through an explicit entropy input port filled by the driver, so every
//! tool here remains a pure function of its inputs.

#![deny(unsafe_code)]
mod base32_tools;
mod base58_tools;
mod cert;
mod generators;
mod hmac_tool;
mod jwt_verify;

use toolkit_core::Registry;

pub fn registry() -> Registry {
    Registry::new(vec![
        Box::new(hmac_tool::Hmac),
        Box::new(jwt_verify::JwtVerify),
        Box::new(cert::CertDecode),
        Box::new(generators::Uuid),
        Box::new(generators::PasswordGen),
        Box::new(generators::RandomBytes),
        Box::new(base32_tools::Base32Encode),
        Box::new(base32_tools::Base32Decode),
        Box::new(base58_tools::Base58Encode),
        Box::new(base58_tools::Base58Decode),
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
                    assert_eq!(streams, m.streaming, "tool {} flag mismatch", m.name);
                }
                Err(_) => assert!(!m.streaming),
            }
        }
    }
}
