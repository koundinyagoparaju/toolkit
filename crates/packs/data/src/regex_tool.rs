use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct RegexExtract;

impl Tool for RegexExtract {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "regex-extract".into(),
            label: "Regex Extract".into(),
            description: "Extract all matches of a regular expression as a JSON array (grep as a chain step).".into(),
            keywords: ["regex", "extract", "match", "grep", "pattern", "search"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Text,
                "Contact ada@example.com or alan@example.com for access.",
            ),
            output: DataType::Json,
            streaming: false,
            options: vec![
                OptionSpec::string("pattern", "Pattern", "Rust regex syntax.")
                    .default_value(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}".into()),
                OptionSpec::integer("group", "Capture group", "0 = whole match.", Some(0), Some(99))
                    .default_value(0.into()),
                OptionSpec::bool("ignore_case", "Ignore case", "").default_value(false.into()),
                OptionSpec::bool("multiline", "^ and $ match line boundaries", "")
                    .default_value(false.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let pattern = options.str_opt("pattern").expect("required");
        let regex = regex::RegexBuilder::new(pattern)
            .case_insensitive(options.bool_opt("ignore_case").unwrap_or(false))
            .multi_line(options.bool_opt("multiline").unwrap_or(false))
            .size_limit(1 << 22)
            .build()
            .map_err(|e| ToolError::new(format!("invalid pattern: {e}")))?;
        let group = options.u32_opt("group").unwrap_or(0) as usize;
        if group >= regex.captures_len() {
            return Err(ToolError::new(format!(
                "pattern has {} capture group(s); group {group} does not exist",
                regex.captures_len() - 1
            )));
        }
        let matches: Vec<serde_json::Value> = regex
            .captures_iter(&text)
            .filter_map(|c| c.get(group))
            .map(|m| serde_json::Value::String(m.as_str().to_string()))
            .collect();
        Ok(DataValue::Json(serde_json::Value::Array(matches)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::run_single;

    #[test]
    fn extracts_groups() {
        let out = run_single(
            &RegexExtract,
            DataValue::Text("a=1 b=22 c=333".into()),
            json!({"pattern": r"\w=(\d+)", "group": 1})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Json(json!(["1", "22", "333"])));
    }

    #[test]
    fn bad_pattern_and_bad_group_error() {
        let text = DataValue::Text("x".into());
        assert!(run_single(
            &RegexExtract,
            text.clone(),
            json!({"pattern": "("}).as_object().unwrap()
        )
        .is_err());
        assert!(run_single(
            &RegexExtract,
            text,
            json!({"pattern": "x", "group": 3}).as_object().unwrap()
        )
        .is_err());
    }
}
