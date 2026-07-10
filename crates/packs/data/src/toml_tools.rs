use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct TomlToJson;

impl Tool for TomlToJson {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "toml-to-json".into(),
            label: "TOML → JSON".into(),
            description: "Convert TOML to JSON.".into(),
            keywords: ["toml", "json", "convert", "config", "cargo"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let value: toml::Value =
            toml::from_str(&text).map_err(|e| ToolError::new(format!("invalid TOML: {e}")))?;
        serde_json::to_value(value)
            .map(DataValue::Json)
            .map_err(|e| ToolError::new(e.to_string()))
    }
}

pub struct JsonToToml;

impl Tool for JsonToToml {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "json-to-toml".into(),
            label: "JSON → TOML".into(),
            description: "Convert JSON to TOML (the top level must be an object).".into(),
            keywords: ["json", "toml", "convert", "config"]
                .map(String::from)
                .to_vec(),
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
        toml::to_string_pretty(&value)
            .map(DataValue::Text)
            .map_err(|e| ToolError::new(format!("cannot represent as TOML: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn round_trip() {
        let toml_text = DataValue::Text("[package]\nname = \"x\"\nversion = \"1.0\"".into());
        let json = run_single(&TomlToJson, toml_text, &Options::new()).unwrap();
        assert_eq!(
            json,
            DataValue::Json(serde_json::json!({"package": {"name": "x", "version": "1.0"}}))
        );
        let back = run_single(&JsonToToml, json, &Options::new()).unwrap();
        let DataValue::Text(t) = back else { panic!() };
        assert!(t.contains("[package]"));
    }
}
