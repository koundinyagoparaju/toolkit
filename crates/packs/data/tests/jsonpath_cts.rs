//! Runs json-query against the official JSONPath compliance test suite
//! (jsonpath-standard/jsonpath-compliance-test-suite, BSD-2 — see
//! cts.LICENSE). Every case either selects the expected nodelist (any of
//! the permitted orderings) or is rejected as an invalid selector.

use serde_json::Value;
use toolkit_core::{run_tool, DataValue, Inputs, Options};

#[test]
fn jsonpath_compliance_suite() {
    let suite: Value = serde_json::from_str(include_str!("cts.json")).expect("suite parses");
    let registry = toolkit_pack_data::registry();
    let tool = registry.find("json-query").expect("tool exists");

    let mut failures = Vec::new();
    let mut ran = 0;
    for case in suite["tests"].as_array().expect("tests array") {
        let name = case["name"].as_str().unwrap_or("?");
        let selector = case["selector"].as_str().expect("selector");
        let invalid = case["invalid_selector"].as_bool().unwrap_or(false);
        ran += 1;

        let mut options = Options::new();
        options.insert("query".into(), selector.into());
        let mut inputs = Inputs::new();
        let document = if invalid {
            Value::Null
        } else {
            case["document"].clone()
        };
        inputs.insert("input".into(), vec![DataValue::Json(document)]);
        let outcome = run_tool(tool, inputs, &options);

        if invalid {
            if outcome.is_ok() {
                failures.push(format!("{name}: accepted invalid selector {selector:?}"));
            }
            continue;
        }
        let got = match outcome {
            Ok(DataValue::Json(v)) => v,
            Ok(_) => unreachable!(),
            Err(e) => {
                failures.push(format!("{name}: rejected valid selector {selector:?}: {e}"));
                continue;
            }
        };
        let ok = if let Some(expected) = case.get("result") {
            got == *expected
        } else if let Some(alternatives) = case["results"].as_array() {
            alternatives.contains(&got)
        } else {
            true // some cases only assert validity
        };
        if !ok {
            failures.push(format!(
                "{name}: {selector:?} gave {got} wanted {}",
                case.get("result").unwrap_or(&case["results"])
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {ran} compliance cases failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
