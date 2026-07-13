use serde_json::Value;
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// JSONPath-style queries over a document, returning every match as a
/// JSON array. The multi-match upgrade from json-pick's single dot-path:
/// wildcards, recursive descent, and quoted keys.
pub struct JsonQuery;

const QUERY_HELP: &str = "JSONPath subset: `$` root, `.key`, `[0]` index, `[-1]` from the end, \
                          `.*` / `[*]` every child, `..key` recursive descent, `[\"a.b\"]` quoted \
                          keys. Returns a JSON array of every match.";

impl Tool for JsonQuery {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "json-query".into(),
            label: "JSON Query".into(),
            description: "Query JSON with a JSONPath subset and get every match as a JSON array \
                          — `$.users[*].name` pulls one field out of a whole list. Wildcards, \
                          negative indexes, and `..key` recursive descent."
                .into(),
            keywords: [
                "json", "query", "jsonpath", "path", "extract", "filter", "select", "pick",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Json,
                r#"{"users":[{"name":"Ada","admin":true},{"name":"Alan","admin":false}]}"#,
            ),
            output: DataType::Json,
            streaming: false,
            options: vec![
                OptionSpec::string("query", "Query", QUERY_HELP).default_value("$..name".into())
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Json(root) = inputs.sole() else {
            unreachable!()
        };
        let query = options.str_opt("query").unwrap_or("$");
        let segments = parse_query(query)?;
        let mut nodes = vec![&root];
        for segment in &segments {
            let mut next = Vec::new();
            for node in nodes {
                segment.select(node, &mut next);
            }
            nodes = next;
        }
        Ok(DataValue::Json(Value::Array(
            nodes.into_iter().cloned().collect(),
        )))
    }
}

enum Segment {
    Key(String),
    Index(i64),
    Wildcard,
    /// `..key`: this key anywhere beneath the current nodes.
    Recursive(String),
}

impl Segment {
    fn select<'a>(&self, node: &'a Value, out: &mut Vec<&'a Value>) {
        match self {
            Segment::Key(k) => {
                if let Value::Object(map) = node {
                    if let Some(v) = map.get(k) {
                        out.push(v);
                    }
                }
            }
            Segment::Index(i) => {
                if let Value::Array(items) = node {
                    let index = if *i < 0 { items.len() as i64 + i } else { *i };
                    if let Ok(index) = usize::try_from(index) {
                        if let Some(v) = items.get(index) {
                            out.push(v);
                        }
                    }
                }
            }
            Segment::Wildcard => match node {
                Value::Object(map) => out.extend(map.values()),
                Value::Array(items) => out.extend(items.iter()),
                _ => {}
            },
            Segment::Recursive(k) => descend(node, k, out),
        }
    }
}

/// Depth-first: every value under `node` (including it) whose key is `k`.
fn descend<'a>(node: &'a Value, k: &str, out: &mut Vec<&'a Value>) {
    match node {
        Value::Object(map) => {
            for (key, v) in map {
                if key == k {
                    out.push(v);
                }
                descend(v, k, out);
            }
        }
        Value::Array(items) => {
            for v in items {
                descend(v, k, out);
            }
        }
        _ => {}
    }
}

fn parse_query(query: &str) -> Result<Vec<Segment>, ToolError> {
    let err = |msg: &str| ToolError::new(format!("bad query: {msg} ({QUERY_HELP})"));
    let mut rest = query.trim().strip_prefix('$').unwrap_or(query.trim());
    let mut segments = Vec::new();
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("..") {
            let end = r.find(['.', '[']).unwrap_or(r.len());
            if end == 0 {
                return Err(err("`..` needs a key name after it"));
            }
            segments.push(Segment::Recursive(r[..end].to_string()));
            rest = &r[end..];
        } else if let Some(r) = rest.strip_prefix('.') {
            if let Some(r) = r.strip_prefix('*') {
                segments.push(Segment::Wildcard);
                rest = r;
            } else {
                let end = r.find(['.', '[']).unwrap_or(r.len());
                if end == 0 {
                    return Err(err("`.` needs a key name after it"));
                }
                segments.push(Segment::Key(r[..end].to_string()));
                rest = &r[end..];
            }
        } else if let Some(r) = rest.strip_prefix('[') {
            let close = r.find(']').ok_or_else(|| err("unclosed `[`"))?;
            let inner = r[..close].trim();
            if inner == "*" {
                segments.push(Segment::Wildcard);
            } else if (inner.starts_with('"') && inner.ends_with('"') && inner.len() >= 2)
                || (inner.starts_with('\'') && inner.ends_with('\'') && inner.len() >= 2)
            {
                segments.push(Segment::Key(inner[1..inner.len() - 1].to_string()));
            } else {
                let index: i64 = inner.parse().map_err(|_| {
                    err(&format!(
                        "`[{inner}]` is not an index, `*`, or a quoted key"
                    ))
                })?;
                segments.push(Segment::Index(index));
            }
            rest = &r[close + 1..];
        } else {
            // Bare leading key, e.g. `users[0]` without `$.`.
            let end = rest.find(['.', '[']).unwrap_or(rest.len());
            segments.push(Segment::Key(rest[..end].to_string()));
            rest = &rest[end..];
        }
    }
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::run_single;

    fn query(doc: Value, q: &str) -> Value {
        let mut opts = Options::new();
        opts.insert("query".into(), q.into());
        let DataValue::Json(v) = run_single(&JsonQuery, DataValue::Json(doc), &opts).unwrap()
        else {
            unreachable!()
        };
        v
    }

    fn doc() -> Value {
        json!({"users":[{"name":"Ada","admin":true},{"name":"Alan","admin":false}],
               "meta":{"name":"directory","a.b":7}})
    }

    #[test]
    fn paths_wildcards_and_indexes() {
        assert_eq!(query(doc(), "$.users[*].name"), json!(["Ada", "Alan"]));
        assert_eq!(query(doc(), "$.users[0].name"), json!(["Ada"]));
        assert_eq!(query(doc(), "$.users[-1].name"), json!(["Alan"]));
        assert_eq!(query(doc(), "users[1].admin"), json!([false]));
        assert_eq!(query(doc(), "$"), json!([doc()]));
    }

    #[test]
    fn recursive_descent_and_quoted_keys() {
        assert_eq!(query(doc(), "$..name"), json!(["Ada", "Alan", "directory"]));
        assert_eq!(query(doc(), r#"$.meta["a.b"]"#), json!([7]));
        assert_eq!(query(doc(), "$..admin[0]"), json!([]));
    }

    #[test]
    fn misses_are_empty_not_errors() {
        assert_eq!(query(doc(), "$.nope.deeper"), json!([]));
        assert_eq!(query(doc(), "$.users[99]"), json!([]));
    }

    #[test]
    fn bad_queries_error() {
        assert!(run_single(&JsonQuery, DataValue::Json(doc()), &{
            let mut o = Options::new();
            o.insert("query".into(), "$.users[".into());
            o
        })
        .is_err());
    }
}
