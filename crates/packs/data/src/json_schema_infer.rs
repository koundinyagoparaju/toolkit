use serde_json::{json, Map, Value};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Infer a JSON Schema (draft 2020-12) from a sample value — bootstrap an
/// API contract or give an agent a schema to validate against.
pub struct JsonSchemaInfer;

impl Tool for JsonSchemaInfer {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "json-schema-infer".into(),
            label: "JSON Schema Infer".into(),
            description: "Generate a JSON Schema from a sample JSON value. Object properties become `properties` with `required`; arrays infer a merged item schema.".into(),
            keywords: ["json", "schema", "infer", "generate", "contract", "validate"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Json),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Json(value) = inputs.sole() else {
            unreachable!()
        };
        let mut schema = infer(&value);
        if let Value::Object(map) = &mut schema {
            map.insert(
                "$schema".into(),
                json!("https://json-schema.org/draft/2020-12/schema"),
            );
        }
        Ok(DataValue::Json(schema))
    }
}

fn infer(value: &Value) -> Value {
    match value {
        Value::Null => json!({ "type": "null" }),
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Number(n) => {
            json!({ "type": if n.is_i64() || n.is_u64() { "integer" } else { "number" } })
        }
        Value::String(_) => json!({ "type": "string" }),
        Value::Object(map) => {
            let mut props = Map::new();
            for (k, v) in map {
                props.insert(k.clone(), infer(v));
            }
            let required: Vec<Value> = map.keys().map(|k| json!(k)).collect();
            let mut schema = json!({ "type": "object", "properties": props });
            if !required.is_empty() {
                schema["required"] = Value::Array(required);
            }
            schema
        }
        Value::Array(items) => {
            let mut schema = json!({ "type": "array" });
            if let Some(item) = items.iter().map(infer).reduce(merge) {
                schema["items"] = item;
            }
            schema
        }
    }
}

/// Combine two element schemas into one describing both: union object
/// properties (required = the intersection), merge array items, and fall
/// back to a type union when the base types differ.
fn merge(a: Value, b: Value) -> Value {
    if a == b {
        return a;
    }
    let (mut a, mut b) = match (a, b) {
        (Value::Object(a), Value::Object(b)) => (a, b),
        // Non-objects that already differ: keep the first.
        (a, _) => return a,
    };
    if a.get("type") != b.get("type") {
        let types: Vec<Value> = [a.get("type"), b.get("type")]
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        return json!({ "type": types });
    }
    // Same type: merge structure.
    if let (Some(Value::Object(pa)), Some(Value::Object(pb))) = (
        a.get("properties").cloned().as_mut(),
        b.remove("properties"),
    ) {
        let mut props = pa.clone();
        for (k, v) in pb {
            let merged = match props.remove(&k) {
                Some(existing) => merge(existing, v),
                None => v,
            };
            props.insert(k, merged);
        }
        a["properties"] = Value::Object(props);
        // required shrinks to keys present in both.
        let ra = required_set(a.get("required"));
        let rb = required_set(b.get("required"));
        let common: Vec<Value> = ra
            .iter()
            .filter(|k| rb.contains(*k))
            .map(|k| json!(k))
            .collect();
        if common.is_empty() {
            a.remove("required");
        } else {
            a.insert("required".into(), Value::Array(common));
        }
    }
    if let (Some(ia), Some(ib)) = (a.get("items").cloned(), b.remove("items")) {
        a["items"] = merge(ia, ib);
    }
    Value::Object(a)
}

fn required_set(v: Option<&Value>) -> Vec<String> {
    v.and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn schema(v: Value) -> Value {
        let DataValue::Json(s) =
            run_single(&JsonSchemaInfer, DataValue::Json(v), &Options::new()).unwrap()
        else {
            unreachable!()
        };
        s
    }

    #[test]
    fn object_types_and_required() {
        let s = schema(json!({"id": 1, "name": "x", "active": true, "score": 1.5}));
        assert_eq!(s["type"], "object");
        assert_eq!(s["properties"]["id"]["type"], "integer");
        assert_eq!(s["properties"]["name"]["type"], "string");
        assert_eq!(s["properties"]["active"]["type"], "boolean");
        assert_eq!(s["properties"]["score"]["type"], "number");
        let req = s["required"].as_array().unwrap();
        assert_eq!(req.len(), 4);
        assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
    }

    #[test]
    fn array_merges_heterogeneous_objects() {
        // Second object lacks "b" and adds "c": required becomes {a}.
        let s = schema(json!([{"a": 1, "b": 2}, {"a": 1, "c": 3}]));
        assert_eq!(s["type"], "array");
        assert_eq!(s["items"]["type"], "object");
        assert_eq!(s["items"]["required"], json!(["a"]));
        assert!(s["items"]["properties"].get("c").is_some());
    }

    #[test]
    fn mixed_scalar_array_uses_type_union() {
        let s = schema(json!([1, "two"]));
        let types = s["items"]["type"].as_array().unwrap();
        assert!(types.contains(&json!("integer")) && types.contains(&json!("string")));
    }
}
