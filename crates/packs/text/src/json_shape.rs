use serde_json::{json, Map, Value};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// The shape of a JSON document without the document: types, key lists,
/// array lengths, and truncated samples. Built for the "what does this
/// 5 MB API response look like?" question — an agent (or a human) learns
/// the structure for a few hundred bytes instead of ingesting the lot,
/// then pulls exactly what it needs with json-query.
pub struct JsonShape;

const KEY_CAP: usize = 24;
const SAMPLE_CAP: usize = 40;

impl Tool for JsonShape {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "json-shape".into(),
            label: "JSON Shape".into(),
            description: "Summarize a JSON document's structure: nested types, keys, array \
                          lengths, and truncated sample values — the shape of megabytes in a \
                          few hundred bytes. Pair with json-query to extract what you find."
                .into(),
            keywords: [
                "json",
                "shape",
                "structure",
                "summary",
                "schema",
                "explore",
                "inspect",
                "types",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Json,
                r#"{"users":[{"name":"Ada","admin":true},{"name":"Alan","admin":false}],"total":2}"#,
            ),
            output: DataType::Json,
            streaming: false,
            options: vec![OptionSpec::integer(
                "depth",
                "Depth",
                "How many levels to descend before eliding.",
                Some(1),
                Some(10),
            )
            .default_value(4.into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Json(root) = inputs.sole() else {
            unreachable!()
        };
        let depth = options.u32_opt("depth").unwrap_or(4) as usize;
        Ok(DataValue::Json(shape(&root, depth)))
    }
}

fn shape(value: &Value, depth: usize) -> Value {
    match value {
        Value::Null => json!({"type": "null"}),
        Value::Bool(b) => json!({"type": "boolean", "sample": b}),
        Value::Number(n) => json!({"type": "number", "sample": n}),
        Value::String(s) => {
            let sample: String = s.chars().take(SAMPLE_CAP).collect();
            let mut out = json!({"type": "string", "sample": sample});
            if s.chars().count() > SAMPLE_CAP {
                out["truncated"] = json!(true);
                out["length"] = json!(s.chars().count());
            }
            out
        }
        Value::Array(items) => {
            let mut out = json!({"type": "array", "length": items.len()});
            if depth > 0 {
                if let Some(first) = items.first() {
                    // The first element stands in for all of them; arrays
                    // with mixed shapes show it via a note.
                    out["items"] = shape(first, depth - 1);
                    let first_kind = kind(first);
                    if items.iter().any(|v| kind(v) != first_kind) {
                        out["mixed_types"] = json!(true);
                    }
                }
            }
            out
        }
        Value::Object(map) => {
            let mut out = json!({"type": "object", "size": map.len()});
            if depth > 0 {
                let mut keys = Map::new();
                for (k, v) in map.iter().take(KEY_CAP) {
                    keys.insert(k.clone(), shape(v, depth - 1));
                }
                out["keys"] = Value::Object(keys);
                if map.len() > KEY_CAP {
                    out["more_keys"] = json!(map.len() - KEY_CAP);
                }
            }
            out
        }
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn shape_of(doc: Value, depth: i64) -> Value {
        let mut opts = Options::new();
        opts.insert("depth".into(), depth.into());
        let DataValue::Json(v) = run_single(&JsonShape, DataValue::Json(doc), &opts).unwrap()
        else {
            unreachable!()
        };
        v
    }

    #[test]
    fn summarizes_structure_without_content() {
        let doc = json!({"users": [{"name": "Ada", "admin": true}], "total": 1});
        let v = shape_of(doc, 4);
        assert_eq!(v["type"], "object");
        assert_eq!(v["keys"]["users"]["type"], "array");
        assert_eq!(v["keys"]["users"]["length"], 1);
        assert_eq!(v["keys"]["users"]["items"]["keys"]["name"]["sample"], "Ada");
        assert_eq!(v["keys"]["total"]["sample"], 1);
    }

    #[test]
    fn truncates_long_strings_and_respects_depth() {
        let long = "x".repeat(100);
        let v = shape_of(json!({"blob": long}), 4);
        assert_eq!(v["keys"]["blob"]["truncated"], true);
        assert_eq!(v["keys"]["blob"]["length"], 100);
        assert_eq!(v["keys"]["blob"]["sample"].as_str().unwrap().len(), 40);

        let deep = json!({"a": {"b": {"c": 1}}});
        let v = shape_of(deep, 1);
        assert_eq!(v["keys"]["a"]["type"], "object");
        assert!(v["keys"]["a"].get("keys").is_none(), "depth elides");
    }

    #[test]
    fn key_cap_and_mixed_arrays() {
        let mut obj = serde_json::Map::new();
        for i in 0..30 {
            obj.insert(format!("k{i:02}"), json!(i));
        }
        let v = shape_of(Value::Object(obj), 2);
        assert_eq!(v["more_keys"], 6);

        let v = shape_of(json!([1, "two", 3]), 2);
        assert_eq!(v["mixed_types"], true);
    }
}
