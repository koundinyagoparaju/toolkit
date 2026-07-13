use serde_json::{json, Map, Value};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Structural diff of two JSON values: a flat, path-addressed list of what
/// was added, removed, or changed — for comparing API responses, config
/// versions, or test fixtures without a website.
pub struct JsonDiff;

impl Tool for JsonDiff {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "json-diff".into(),
            label: "JSON Diff".into(),
            description: "Compare two JSON values and list the added, removed, and changed paths (as JSON Pointers).".into(),
            keywords: ["json", "diff", "compare", "changes", "delta"]
                .map(String::from)
                .to_vec(),
            inputs: vec![
                InputSpec::named("left", DataType::Json)
                    .describe("The baseline JSON value.")
                    .example(r#"{"name":"Ada","role":"admin"}"#),
                InputSpec::named("right", DataType::Json)
                    .describe("The JSON value to compare against the baseline.")
                    .example(r#"{"name":"Ada","role":"owner"}"#),
            ],
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, mut inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Json(left) = inputs.take("left") else {
            unreachable!()
        };
        let DataValue::Json(right) = inputs.take("right") else {
            unreachable!()
        };
        let mut changes = Vec::new();
        diff(&mut String::new(), &left, &right, &mut changes);
        Ok(DataValue::Json(json!({
            "changed": changes.len(),
            "changes": changes,
        })))
    }
}

fn diff(path: &mut String, left: &Value, right: &Value, out: &mut Vec<Value>) {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => diff_objects(path, a, b, out),
        (Value::Array(a), Value::Array(b)) => diff_arrays(path, a, b, out),
        _ if left == right => {}
        _ => out.push(json!({
            "op": "changed", "path": here(path), "from": left, "to": right,
        })),
    }
}

fn diff_objects(
    path: &mut String,
    a: &Map<String, Value>,
    b: &Map<String, Value>,
    out: &mut Vec<Value>,
) {
    // Keys in sorted order so output is deterministic across the two
    // preserve-order maps.
    let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
    keys.sort();
    keys.dedup();
    for key in keys {
        let len = path.len();
        push_segment(path, &escape_token(key));
        match (a.get(key), b.get(key)) {
            (Some(l), Some(r)) => diff(path, l, r, out),
            (Some(l), None) => out.push(json!({ "op": "removed", "path": here(path), "value": l })),
            (None, Some(r)) => out.push(json!({ "op": "added", "path": here(path), "value": r })),
            (None, None) => {}
        }
        path.truncate(len);
    }
}

fn diff_arrays(path: &mut String, a: &[Value], b: &[Value], out: &mut Vec<Value>) {
    for i in 0..a.len().max(b.len()) {
        let len = path.len();
        push_segment(path, &i.to_string());
        match (a.get(i), b.get(i)) {
            (Some(l), Some(r)) => diff(path, l, r, out),
            (Some(l), None) => out.push(json!({ "op": "removed", "path": here(path), "value": l })),
            (None, Some(r)) => out.push(json!({ "op": "added", "path": here(path), "value": r })),
            (None, None) => {}
        }
        path.truncate(len);
    }
}

fn push_segment(path: &mut String, seg: &str) {
    path.push('/');
    path.push_str(seg);
}

fn here(path: &str) -> String {
    if path.is_empty() {
        "".into()
    } else {
        path.to_string()
    }
}

/// JSON Pointer token escaping (RFC 6901): ~ -> ~0, / -> ~1.
fn escape_token(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_tool;

    fn diff_of(left: Value, right: Value) -> Value {
        let mut inputs = Inputs::new();
        inputs.insert("left".into(), vec![DataValue::Json(left)]);
        inputs.insert("right".into(), vec![DataValue::Json(right)]);
        let DataValue::Json(v) = run_tool(&JsonDiff, inputs, &Options::new()).unwrap() else {
            unreachable!()
        };
        v
    }

    #[test]
    fn added_removed_changed() {
        let v = diff_of(
            json!({"a": 1, "b": 2, "nested": {"x": 1}}),
            json!({"a": 1, "b": 3, "c": 4, "nested": {"x": 2}}),
        );
        assert_eq!(v["changed"], 3);
        let changes = v["changes"].as_array().unwrap();
        assert!(changes.contains(&json!({"op": "changed", "path": "/b", "from": 2, "to": 3})));
        assert!(changes.contains(&json!({"op": "added", "path": "/c", "value": 4})));
        assert!(
            changes.contains(&json!({"op": "changed", "path": "/nested/x", "from": 1, "to": 2}))
        );
    }

    #[test]
    fn identical_is_empty() {
        let v = diff_of(json!({"a": [1, 2, 3]}), json!({"a": [1, 2, 3]}));
        assert_eq!(v["changed"], 0);
    }

    #[test]
    fn array_length_change() {
        let v = diff_of(json!([1, 2]), json!([1, 2, 3]));
        let changes = v["changes"].as_array().unwrap();
        assert_eq!(changes, &[json!({"op": "added", "path": "/2", "value": 3})]);
    }

    #[test]
    fn escapes_pointer_tokens() {
        let v = diff_of(json!({"a/b": 1}), json!({"a/b": 2}));
        assert_eq!(v["changes"][0]["path"], "/a~1b");
    }
}
