use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Everything except RFC 3986 unreserved characters (A-Z a-z 0-9 - _ . ~),
/// matching JavaScript's encodeURIComponent behavior for the common cases.
const COMPONENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}')
    .add(b'!');

pub struct UrlEncode;

impl Tool for UrlEncode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "url-encode".into(),
            label: "URL Encode".into(),
            description: "Percent-encode text for safe use in a URL component.".into(),
            keywords: ["url", "encode", "percent", "uri", "escape"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        Ok(DataValue::Text(
            utf8_percent_encode(&text, COMPONENT).to_string(),
        ))
    }
}

pub struct UrlDecode;

impl Tool for UrlDecode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "url-decode".into(),
            label: "URL Decode".into(),
            description: "Decode percent-encoded text (e.g. %20 becomes a space).".into(),
            keywords: ["url", "decode", "percent", "uri", "unescape"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        percent_decode_str(&text)
            .decode_utf8()
            .map(|s| DataValue::Text(s.into_owned()))
            .map_err(|_| ToolError::new("decoded bytes are not valid UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn round_trip() {
        let original = DataValue::Text("a b&c=d?e/f#g%h+ü".into());
        let encoded = run_single(&UrlEncode, original.clone(), &Options::new()).unwrap();
        let DataValue::Text(ref e) = encoded else {
            panic!()
        };
        assert!(!e.contains(' ') && !e.contains('&') && !e.contains('#'));
        assert_eq!(
            run_single(&UrlDecode, encoded, &Options::new()).unwrap(),
            original
        );
    }

    #[test]
    fn decode_plain_percent_sequences() {
        let out = run_single(
            &UrlDecode,
            DataValue::Text("hello%20world%21".into()),
            &Options::new(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Text("hello world!".into()));
    }
}
