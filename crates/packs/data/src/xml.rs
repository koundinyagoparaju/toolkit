use quick_xml::events::Event;
use quick_xml::{Reader, Writer};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct XmlFormat;

impl Tool for XmlFormat {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "xml-format".into(),
            label: "XML Format".into(),
            description:
                "Pretty-print XML with configurable indentation (or minify with indent 0).".into(),
            keywords: ["xml", "format", "pretty", "saml", "soap", "beautify"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::integer("indent", "Indent width", "", Some(0), Some(8))
                    .default_value(2.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let indent = options.i64_opt("indent").unwrap_or(2) as usize;
        let mut reader = Reader::from_str(&text);
        reader.config_mut().trim_text(true);
        let mut writer = if indent > 0 {
            Writer::new_with_indent(Vec::new(), b' ', indent)
        } else {
            Writer::new(Vec::new())
        };
        loop {
            match reader.read_event() {
                Ok(Event::Eof) => break,
                Ok(event) => writer
                    .write_event(event)
                    .map_err(|e| ToolError::new(e.to_string()))?,
                Err(e) => return Err(ToolError::new(format!("invalid XML: {e}"))),
            }
        }
        Ok(DataValue::Text(
            String::from_utf8(writer.into_inner()).expect("writer emits UTF-8"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn pretty_prints() {
        let input = DataValue::Text("<a><b attr=\"1\">hi</b><c/></a>".into());
        let out = run_single(&XmlFormat, input, &Options::new()).unwrap();
        let DataValue::Text(t) = out else { panic!() };
        assert!(t.contains("\n  <b attr=\"1\">hi</b>"), "{t}");
    }

    #[test]
    fn invalid_xml_errors() {
        assert!(run_single(
            &XmlFormat,
            DataValue::Text("<a><b></a>".into()),
            &Options::new()
        )
        .is_err());
    }
}
