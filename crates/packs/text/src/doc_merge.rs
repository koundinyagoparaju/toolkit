use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// The pack's first variable-arity tool: one `multi` port that accepts any
/// number of documents. Cardinality lives on the port — the type system
/// stays list-free.
pub struct DocMerge;

impl Tool for DocMerge {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "doc-merge".into(),
            label: "Document Merge".into(),
            description: "Concatenate any number of text documents into one, in connection order, with a configurable separator.".into(),
            keywords: ["text", "merge", "concat", "join", "documents", "combine"]
                .map(String::from)
                .to_vec(),
            inputs: vec![InputSpec::named("documents", DataType::Text).multi()],
            output: DataType::Text,
            options: vec![OptionSpec::string(
                "separator",
                "Separator",
                "Inserted between documents. Escapes like \\n are taken literally as typed.",
            )
            .default_value("\n".into())],
        }
    }

    fn run(&self, mut inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let documents = inputs.take_many("documents");
        let separator = options.str_opt("separator").unwrap_or("\n").to_string();
        let parts: Vec<String> = documents
            .into_iter()
            .map(|doc| {
                let DataValue::Text(text) = doc else {
                    unreachable!()
                };
                text
            })
            .collect();
        Ok(DataValue::Text(parts.join(&separator)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::{run_single, run_tool};

    fn docs(texts: &[&str]) -> Inputs {
        Inputs::from([(
            "documents".to_string(),
            texts
                .iter()
                .map(|t| DataValue::Text(t.to_string()))
                .collect(),
        )])
    }

    #[test]
    fn merges_in_order_with_default_separator() {
        let out = run_tool(&DocMerge, docs(&["a", "b", "c"]), &Options::new()).unwrap();
        assert_eq!(out, DataValue::Text("a\nb\nc".into()));
    }

    #[test]
    fn custom_separator() {
        let out = run_tool(
            &DocMerge,
            docs(&["x", "y"]),
            json!({"separator": " --- "}).as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Text("x --- y".into()));
    }

    #[test]
    fn single_document_passes_through() {
        // A multi port with one value is fine — and run_single works since
        // doc-merge has a sole (multi) port.
        let out = run_single(&DocMerge, DataValue::Text("solo".into()), &Options::new()).unwrap();
        assert_eq!(out, DataValue::Text("solo".into()));
    }

    #[test]
    fn coerces_each_element() {
        let inputs = Inputs::from([(
            "documents".to_string(),
            vec![
                DataValue::Bytes(b"one".to_vec()),
                DataValue::Text("two".into()),
            ],
        )]);
        let out = run_tool(&DocMerge, inputs, &Options::new()).unwrap();
        assert_eq!(out, DataValue::Text("one\ntwo".into()));

        let bad = Inputs::from([(
            "documents".to_string(),
            vec![DataValue::Bytes(vec![0xff, 0xfe])],
        )]);
        assert!(run_tool(&DocMerge, bad, &Options::new()).is_err());
    }

    #[test]
    fn empty_port_is_an_error() {
        assert!(run_tool(&DocMerge, Inputs::new(), &Options::new()).is_err());
    }
}
