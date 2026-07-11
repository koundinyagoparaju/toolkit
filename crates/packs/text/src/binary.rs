use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// Render each byte as an 8-bit binary string — the "show me the bits"
/// view of text or data.
pub struct TextToBinary;

impl Tool for TextToBinary {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "text-to-binary".into(),
            label: "Text to Binary".into(),
            description: "Render each byte as 8 bits (e.g. \"Hi\" -> 01001000 01101001).".into(),
            keywords: ["binary", "bits", "text", "ascii", "encode"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Bytes),
            output: DataType::Text,
            streaming: false,
            options: vec![OptionSpec::string(
                "separator",
                "Separator",
                "Placed between bytes (default a single space; use \"\" for none).",
            )
            .default_value(" ".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(bytes) = inputs.sole() else {
            unreachable!()
        };
        let sep = options.str_opt("separator").unwrap_or(" ");
        let out = bytes
            .iter()
            .map(|b| format!("{b:08b}"))
            .collect::<Vec<_>>()
            .join(sep);
        Ok(DataValue::Text(out))
    }
}

/// Parse a binary string back to bytes. Whitespace and common separators
/// between bytes are ignored; groups must be 8 bits.
pub struct BinaryToText;

impl Tool for BinaryToText {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "binary-to-text".into(),
            label: "Binary to Text".into(),
            description: "Parse a binary string (8 bits per byte) back to bytes. Separators between bytes are ignored.".into(),
            keywords: ["binary", "bits", "decode", "text", "ascii"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Bytes,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        // Keep only 0/1; every 8 make a byte.
        let bits: Vec<u8> = text.bytes().filter(|b| *b == b'0' || *b == b'1').collect();
        if !bits.len().is_multiple_of(8) {
            return Err(ToolError::new(format!(
                "expected a multiple of 8 bits, got {}",
                bits.len()
            )));
        }
        let out = bits
            .chunks(8)
            .map(|chunk| chunk.iter().fold(0u8, |acc, b| (acc << 1) | (b - b'0')))
            .collect();
        Ok(DataValue::Bytes(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn round_trip() {
        let DataValue::Text(bin) = run_single(
            &TextToBinary,
            DataValue::Bytes(b"Hi".to_vec()),
            &Options::new(),
        )
        .unwrap() else {
            unreachable!()
        };
        assert_eq!(bin, "01001000 01101001");

        let DataValue::Bytes(back) =
            run_single(&BinaryToText, DataValue::Text(bin), &Options::new()).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(back, b"Hi");
    }

    #[test]
    fn separators_ignored_and_bad_length_errors() {
        let DataValue::Bytes(back) = run_single(
            &BinaryToText,
            DataValue::Text("01001000\n0110-1001".into()),
            &Options::new(),
        )
        .unwrap() else {
            unreachable!()
        };
        assert_eq!(back, b"Hi");
        assert!(run_single(
            &BinaryToText,
            DataValue::Text("0100100".into()),
            &Options::new()
        )
        .is_err());
    }
}
