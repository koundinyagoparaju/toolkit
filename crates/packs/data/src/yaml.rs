use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct JsonToYaml;

impl Tool for JsonToYaml {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "json-to-yaml".into(),
            label: "JSON → YAML".into(),
            description: "Convert JSON to YAML.".into(),
            keywords: ["json", "yaml", "convert", "config"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Json,
                r#"{"server":{"host":"localhost","port":8080}}"#,
            ),
            output: DataType::Text,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Json(value) = inputs.sole() else {
            unreachable!()
        };
        serde_yaml::to_string(&value)
            .map(DataValue::Text)
            .map_err(|e| ToolError::new(format!("cannot represent as YAML: {e}")))
    }
}

pub struct YamlToJson;

impl Tool for YamlToJson {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "yaml-to-json".into(),
            label: "YAML → JSON".into(),
            description: "Convert YAML to JSON.".into(),
            keywords: ["yaml", "json", "convert", "config"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Text,
                "server:\n  host: localhost\n  port: 8080\n",
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
        serde_yaml::from_str::<serde_json::Value>(&text)
            .map(DataValue::Json)
            .map_err(|e| ToolError::new(format!("invalid YAML: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn round_trip() {
        let json = DataValue::Json(serde_json::json!({"a": [1, 2], "b": {"c": true}}));
        let yaml = run_single(&JsonToYaml, json.clone(), &Options::new()).unwrap();
        let DataValue::Text(ref y) = yaml else {
            panic!()
        };
        assert!(y.contains("a:") && y.contains("- 1"));
        let back = run_single(&YamlToJson, yaml, &Options::new()).unwrap();
        assert_eq!(back, json);
    }

    #[test]
    fn invalid_yaml_errors() {
        assert!(run_single(
            &YamlToJson,
            DataValue::Text("a: [unclosed".into()),
            &Options::new()
        )
        .is_err());
    }
}
