//! Math tools: exact arithmetic, statistics, and number theory.

#![deny(unsafe_code)]
mod calc;
mod combinatorics;
mod number_factor;
mod number_stats;
mod percent;

use toolkit_core::Registry;

pub fn registry() -> Registry {
    Registry::new(vec![
        Box::new(calc::Calc),
        Box::new(number_stats::NumberStats),
        Box::new(number_factor::NumberFactor),
        Box::new(combinatorics::Combinatorics),
        Box::new(percent::PercentCalc),
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
