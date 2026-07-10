use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct JsonFormat;

impl Tool for JsonFormat {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "json-format".into(),
            label: "JSON Format".into(),
            description: "Pretty-print JSON with configurable indentation.".into(),
            keywords: ["json", "format", "pretty", "beautify", "indent"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Json),
            output: DataType::Text,
            streaming: false,
            options: vec![OptionSpec::integer(
                "indent",
                "Indent width",
                "Number of spaces per indentation level.",
                Some(0),
                Some(8),
            )
            .default_value(2.into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Json(value) = inputs.sole() else {
            unreachable!()
        };
        let indent = options.i64_opt("indent").unwrap_or(2) as usize;
        let indent_bytes = b" ".repeat(indent);
        let formatter = serde_json::ser::PrettyFormatter::with_indent(&indent_bytes);
        let mut out = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
        serde::Serialize::serialize(&value, &mut ser)
            .map_err(|e| ToolError::new(format!("failed to format JSON: {e}")))?;
        Ok(DataValue::Text(
            String::from_utf8(out).expect("serde_json emits UTF-8"),
        ))
    }
}

pub struct JsonMinify;

impl Tool for JsonMinify {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "json-minify".into(),
            label: "JSON Minify".into(),
            description: "Remove all insignificant whitespace from JSON.".into(),
            keywords: ["json", "minify", "compact"].map(String::from).to_vec(),
            inputs: InputSpec::sole(DataType::Json),
            output: DataType::Text,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Json(value) = inputs.sole() else {
            unreachable!()
        };
        Ok(DataValue::Text(
            serde_json::to_string(&value).expect("JSON value serializes"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn format_respects_indent_option() {
        let input = DataValue::Json(serde_json::json!({"a": [1, 2]}));
        let out = run_single(
            &JsonFormat,
            input,
            serde_json::json!({"indent": 4}).as_object().unwrap(),
        )
        .unwrap();
        let DataValue::Text(s) = out else { panic!() };
        assert!(s.contains("\n    \"a\""), "got: {s}");
    }

    #[test]
    fn format_coerces_text_input() {
        // Callers can feed Text; run_single coerces it to Json per the matrix.
        let out = run_single(
            &JsonFormat,
            DataValue::Text("{\"a\":1}".into()),
            &Options::new(),
        )
        .unwrap();
        let DataValue::Text(s) = out else { panic!() };
        assert_eq!(s, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn minify_strips_whitespace() {
        let out = run_single(
            &JsonMinify,
            DataValue::Text("{ \"a\" : [ 1 , 2 ] }".into()),
            &Options::new(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Text("{\"a\":[1,2]}".into()));
    }

    #[test]
    fn invalid_json_text_fails_coercion() {
        assert!(run_single(
            &JsonFormat,
            DataValue::Text("not json".into()),
            &Options::new()
        )
        .is_err());
    }
}
