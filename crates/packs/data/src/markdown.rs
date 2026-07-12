use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct MarkdownToHtml;

impl Tool for MarkdownToHtml {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "markdown-to-html".into(),
            label: "Markdown → HTML".into(),
            description: "Render Markdown (CommonMark + tables, strikethrough, footnotes) to HTML."
                .into(),
            keywords: ["markdown", "html", "render", "commonmark", "convert"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Text,
                "# Hello\n\nSome *emphasis* and a [link](https://example.com).",
            ),
            output: DataType::Text,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let options = pulldown_cmark::Options::ENABLE_TABLES
            | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
            | pulldown_cmark::Options::ENABLE_FOOTNOTES
            | pulldown_cmark::Options::ENABLE_TASKLISTS;
        let parser = pulldown_cmark::Parser::new_ext(&text, options);
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, parser);
        Ok(DataValue::Text(html))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn renders_extensions() {
        let md = "# Hi\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n~~gone~~";
        let out = run_single(&MarkdownToHtml, DataValue::Text(md.into()), &Options::new()).unwrap();
        let DataValue::Text(html) = out else { panic!() };
        assert!(html.contains("<h1>Hi</h1>"));
        assert!(html.contains("<table>"));
        assert!(html.contains("<del>gone</del>"));
    }
}
