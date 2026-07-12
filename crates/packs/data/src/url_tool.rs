use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct UrlParse;

impl Tool for UrlParse {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "url-parse".into(),
            label: "URL Parse".into(),
            description:
                "Break a URL into its components, with query parameters decoded into JSON.".into(),
            keywords: ["url", "parse", "query", "params", "uri", "inspect"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Text,
                "https://example.com/search?q=toolkit&lang=en#results",
            ),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let url = url::Url::parse(text.trim())
            .map_err(|e| ToolError::new(format!("invalid URL: {e}")))?;

        // Repeated keys become arrays; single keys stay scalar.
        let mut query = serde_json::Map::new();
        for (key, value) in url.query_pairs() {
            match query.get_mut(key.as_ref()) {
                None => {
                    query.insert(
                        key.into_owned(),
                        serde_json::Value::String(value.into_owned()),
                    );
                }
                Some(serde_json::Value::Array(items)) => {
                    items.push(serde_json::Value::String(value.into_owned()));
                }
                Some(existing) => {
                    let first = existing.take();
                    *existing = serde_json::Value::Array(vec![
                        first,
                        serde_json::Value::String(value.into_owned()),
                    ]);
                }
            }
        }

        let mut out = serde_json::Map::new();
        out.insert("scheme".into(), url.scheme().into());
        if let Some(host) = url.host_str() {
            out.insert("host".into(), host.into());
        }
        if let Some(port) = url.port_or_known_default() {
            out.insert("port".into(), port.into());
        }
        if !url.username().is_empty() {
            out.insert("username".into(), url.username().into());
        }
        out.insert("path".into(), url.path().into());
        out.insert("query".into(), serde_json::Value::Object(query));
        if let Some(fragment) = url.fragment() {
            out.insert("fragment".into(), fragment.into());
        }
        Ok(DataValue::Json(serde_json::Value::Object(out)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn parses_components_and_query() {
        let out = run_single(
            &UrlParse,
            DataValue::Text(
                "https://api.example.com:8443/v1/users?tag=a&tag=b&q=hello%20world#top".into(),
            ),
            &Options::new(),
        )
        .unwrap();
        let DataValue::Json(v) = out else { panic!() };
        assert_eq!(v["scheme"], "https");
        assert_eq!(v["host"], "api.example.com");
        assert_eq!(v["port"], 8443);
        assert_eq!(v["path"], "/v1/users");
        assert_eq!(v["query"]["q"], "hello world");
        assert_eq!(v["query"]["tag"], serde_json::json!(["a", "b"]));
        assert_eq!(v["fragment"], "top");
    }

    #[test]
    fn invalid_url_errors() {
        assert!(run_single(
            &UrlParse,
            DataValue::Text("not a url".into()),
            &Options::new()
        )
        .is_err());
    }
}
