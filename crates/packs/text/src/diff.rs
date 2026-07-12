use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct TextDiff;

impl Tool for TextDiff {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "text-diff".into(),
            label: "Text Diff".into(),
            description: "Compare two texts and produce a unified diff — without pasting either into someone's website.".into(),
            keywords: ["diff", "compare", "unified", "patch", "changes"]
                .map(String::from)
                .to_vec(),
            inputs: vec![
                InputSpec::named("old", DataType::Text).example("alpha\nbeta\ngamma\n"),
                InputSpec::named("new", DataType::Text).example("alpha\nbeta\ndelta\n"),
            ],
            output: DataType::Text,
            streaming: false,
            options: vec![OptionSpec::integer(
                "context",
                "Context lines",
                "",
                Some(0),
                Some(20),
            )
            .default_value(3.into())],
        }
    }

    fn run(&self, mut inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(old) = inputs.take("old") else {
            unreachable!()
        };
        let DataValue::Text(new) = inputs.take("new") else {
            unreachable!()
        };
        let context = options.u32_opt("context").unwrap_or(3) as usize;
        if old == new {
            return Ok(DataValue::Text("(no differences)".into()));
        }
        let diff = similar::TextDiff::from_lines(&old, &new);
        let out = diff
            .unified_diff()
            .context_radius(context)
            .header("old", "new")
            .to_string();
        Ok(DataValue::Text(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_tool;

    #[test]
    fn produces_unified_diff() {
        let inputs = Inputs::from([
            ("old".to_string(), vec![DataValue::Text("a\nb\nc\n".into())]),
            ("new".to_string(), vec![DataValue::Text("a\nB\nc\n".into())]),
        ]);
        let out = run_tool(&TextDiff, inputs, &Options::new()).unwrap();
        let DataValue::Text(d) = out else { panic!() };
        assert!(d.contains("-b"), "{d}");
        assert!(d.contains("+B"), "{d}");
        assert!(d.starts_with("--- old"), "{d}");
    }

    #[test]
    fn identical_inputs() {
        let inputs = Inputs::from([
            ("old".to_string(), vec![DataValue::Text("same".into())]),
            ("new".to_string(), vec![DataValue::Text("same".into())]),
        ]);
        let out = run_tool(&TextDiff, inputs, &Options::new()).unwrap();
        assert_eq!(out, DataValue::Text("(no differences)".into()));
    }
}
