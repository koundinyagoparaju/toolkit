use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct HexEncode;

impl Tool for HexEncode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "hex-encode".into(),
            label: "Hex Encode".into(),
            description: "Encode bytes as a hexadecimal string.".into(),
            keywords: ["hex", "hexadecimal", "encode", "dump"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Bytes),
            output: DataType::Text,
            options: vec![
                OptionSpec::bool("uppercase", "Uppercase", "Emit A-F instead of a-f.")
                    .default_value(false.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(bytes) = inputs.sole() else {
            unreachable!()
        };
        let digits: &[u8; 16] = if options.bool_opt("uppercase").unwrap_or(false) {
            b"0123456789ABCDEF"
        } else {
            b"0123456789abcdef"
        };
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in &bytes {
            out.push(digits[(b >> 4) as usize] as char);
            out.push(digits[(b & 0x0f) as usize] as char);
        }
        Ok(DataValue::Text(out))
    }
}

pub struct HexDecode;

impl Tool for HexDecode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "hex-decode".into(),
            label: "Hex Decode".into(),
            description: "Decode a hexadecimal string to bytes. Whitespace and an optional 0x prefix are ignored.".into(),
            keywords: ["hex", "hexadecimal", "decode"].map(String::from).to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Bytes,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let compact: String = text
            .strip_prefix("0x")
            .unwrap_or(&text)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if !compact.len().is_multiple_of(2) {
            return Err(ToolError::new("hex input has an odd number of digits"));
        }
        let nibble = |c: char| -> Result<u8, ToolError> {
            c.to_digit(16)
                .map(|d| d as u8)
                .ok_or_else(|| ToolError::new(format!("invalid hex digit '{c}'")))
        };
        let chars: Vec<char> = compact.chars().collect();
        let mut bytes = Vec::with_capacity(chars.len() / 2);
        for pair in chars.chunks(2) {
            bytes.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
        }
        Ok(DataValue::Bytes(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn round_trip() {
        let data = DataValue::Bytes(vec![0x00, 0x7f, 0xff]);
        let encoded = run_single(&HexEncode, data.clone(), &Options::new()).unwrap();
        assert_eq!(encoded, DataValue::Text("007fff".into()));
        assert_eq!(
            run_single(&HexDecode, encoded, &Options::new()).unwrap(),
            data
        );
    }

    #[test]
    fn decode_tolerates_prefix_whitespace_and_case() {
        let out = run_single(
            &HexDecode,
            DataValue::Text("0xDE AD\nbe ef".into()),
            &Options::new(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef]));
    }

    #[test]
    fn odd_length_and_bad_digits_error() {
        assert!(run_single(&HexDecode, DataValue::Text("abc".into()), &Options::new()).is_err());
        assert!(run_single(&HexDecode, DataValue::Text("zz".into()), &Options::new()).is_err());
    }
}
