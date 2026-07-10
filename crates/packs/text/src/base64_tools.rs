use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct Base64Encode;

impl Tool for Base64Encode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "base64-encode".into(),
            label: "Base64 Encode".into(),
            description: "Encode data as Base64 text.".into(),
            keywords: ["base64", "encode", "btoa"].map(String::from).to_vec(),
            inputs: InputSpec::sole(DataType::Bytes),
            output: DataType::Text,
            options: vec![OptionSpec::bool(
                "url_safe",
                "URL-safe alphabet",
                "Use the URL-safe alphabet (- and _ instead of + and /), without padding.",
            )
            .default_value(false.into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(bytes) = inputs.sole() else {
            unreachable!()
        };
        let encoded = if options.bool_opt("url_safe").unwrap_or(false) {
            URL_SAFE_NO_PAD.encode(&bytes)
        } else {
            STANDARD.encode(&bytes)
        };
        Ok(DataValue::Text(encoded))
    }
}

pub struct Base64Decode;

impl Tool for Base64Decode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "base64-decode".into(),
            label: "Base64 Decode".into(),
            description: "Decode Base64 text to its original bytes. Accepts standard and URL-safe alphabets, with or without padding; whitespace is ignored.".into(),
            keywords: ["base64", "decode", "atob"].map(String::from).to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Bytes,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        // Be liberal in what we accept: try each common variant.
        for engine in [&STANDARD, &STANDARD_NO_PAD, &URL_SAFE, &URL_SAFE_NO_PAD] {
            if let Ok(bytes) = engine.decode(&compact) {
                return Ok(DataValue::Bytes(bytes));
            }
        }
        Err(ToolError::new("input is not valid base64"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn encode_decode_round_trip() {
        let data = DataValue::Bytes(vec![0, 1, 254, 255]);
        let encoded = run_single(&Base64Encode, data.clone(), &Options::new()).unwrap();
        let decoded = run_single(&Base64Decode, encoded, &Options::new()).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_accepts_all_variants_and_whitespace() {
        for text in ["aGVsbG8=", "aGVsbG8", "aGVs\nbG8=", "  aGVsbG8=  "] {
            let out =
                run_single(&Base64Decode, DataValue::Text(text.into()), &Options::new()).unwrap();
            assert_eq!(out, DataValue::Bytes(b"hello".to_vec()), "input: {text:?}");
        }
    }

    #[test]
    fn url_safe_option() {
        let out = run_single(
            &Base64Encode,
            DataValue::Bytes(vec![251, 255]),
            &serde_json::json!({"url_safe": true})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Text("-_8".into()));
    }

    #[test]
    fn invalid_input_errors() {
        assert!(run_single(
            &Base64Decode,
            DataValue::Text("!!!".into()),
            &Options::new()
        )
        .is_err());
    }
}
