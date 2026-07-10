use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// Selects a piece of a JSON document by path. This is the composition
/// answer to "multi-output" tools: a tool emits one structured value, and
/// fan-out + json-pick extracts the parts (e.g. jwt-decode -> header and
/// payload in parallel branches).
pub struct JsonPick;

impl Tool for JsonPick {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "json-pick".into(),
            label: "JSON Pick".into(),
            description:
                "Extract a value from JSON by path, e.g. \"payload.name\" or \"items.0.id\".".into(),
            keywords: ["json", "pick", "extract", "path", "select", "query"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Json),
            output: DataType::Json,
            streaming: false,
            options: vec![OptionSpec::string(
                "path",
                "Path",
                "Dot-separated keys; numbers index into arrays.",
            )
            .required()],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Json(mut value) = inputs.sole() else {
            unreachable!()
        };
        let path = options.str_opt("path").expect("required");
        for (i, segment) in path.split('.').enumerate() {
            let at = || path.split('.').take(i + 1).collect::<Vec<_>>().join(".");
            value = match value {
                serde_json::Value::Object(mut map) => map.remove(segment).ok_or_else(|| {
                    ToolError::new(format!("no key \"{}\" at \"{}\"", segment, at()))
                })?,
                serde_json::Value::Array(mut items) => {
                    let index: usize = segment.parse().map_err(|_| {
                        ToolError::new(format!(
                            "\"{}\" is an array; \"{segment}\" is not an index",
                            at()
                        ))
                    })?;
                    if index >= items.len() {
                        return Err(ToolError::new(format!(
                            "index {index} out of bounds at \"{}\" (length {})",
                            at(),
                            items.len()
                        )));
                    }
                    items.swap_remove(index)
                }
                other => {
                    return Err(ToolError::new(format!(
                        "cannot descend into {} at \"{}\"",
                        type_name(&other),
                        at()
                    )));
                }
            };
        }
        Ok(DataValue::Json(value))
    }
}

fn type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::run_single;

    fn pick(doc: serde_json::Value, path: &str) -> Result<DataValue, ToolError> {
        run_single(
            &JsonPick,
            DataValue::Json(doc),
            json!({ "path": path }).as_object().unwrap(),
        )
    }

    #[test]
    fn picks_nested_keys_and_array_indices() {
        let doc = json!({"payload": {"items": [{"id": 7}, {"id": 9}]}});
        assert_eq!(
            pick(doc.clone(), "payload.items.1.id").unwrap(),
            DataValue::Json(json!(9))
        );
        assert_eq!(
            pick(doc, "payload.items").unwrap(),
            DataValue::Json(json!([{"id": 7}, {"id": 9}]))
        );
    }

    #[test]
    fn helpful_errors_name_the_failing_path_prefix() {
        let doc = json!({"a": {"b": 1}});
        let err = pick(doc.clone(), "a.nope").unwrap_err();
        assert!(err.message.contains("a.nope"), "{err}");
        let err = pick(doc, "a.b.c").unwrap_err();
        assert!(err.message.contains("a number"), "{err}");
    }
}
