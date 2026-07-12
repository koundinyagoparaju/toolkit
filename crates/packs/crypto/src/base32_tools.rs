use data_encoding::BASE32;
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct Base32Encode;

impl Tool for Base32Encode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "base32-encode".into(),
            label: "Base32 Encode".into(),
            description: "Encode data as Base32 text (RFC 4648, as used by TOTP secrets).".into(),
            keywords: ["base32", "encode", "totp", "rfc4648"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(DataType::Bytes, "hello world"),
            output: DataType::Text,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(bytes) = inputs.sole() else {
            unreachable!()
        };
        Ok(DataValue::Text(BASE32.encode(&bytes)))
    }
}

pub struct Base32Decode;

impl Tool for Base32Decode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "base32-decode".into(),
            label: "Base32 Decode".into(),
            description: "Decode Base32 text to bytes. Case and whitespace are forgiven; padding is optional.".into(),
            keywords: ["base32", "decode", "totp"].map(String::from).to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "NBSWY3DP"),
            output: DataType::Bytes,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let compact: String = text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '=')
            .map(|c| c.to_ascii_uppercase())
            .collect();
        data_encoding::BASE32_NOPAD
            .decode(compact.as_bytes())
            .map(DataValue::Bytes)
            .map_err(|_| ToolError::new("input is not valid base32"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn round_trip_and_lenient_decode() {
        let data = DataValue::Bytes(b"hello".to_vec());
        let enc = run_single(&Base32Encode, data.clone(), &Options::new()).unwrap();
        assert_eq!(enc, DataValue::Text("NBSWY3DP".into()));
        for variant in ["NBSWY3DP", "nbswy3dp", "NBSW Y3DP", "NBSWY3DP======"] {
            let out = run_single(
                &Base32Decode,
                DataValue::Text(variant.into()),
                &Options::new(),
            )
            .unwrap();
            assert_eq!(out, data, "{variant}");
        }
        assert!(run_single(
            &Base32Decode,
            DataValue::Text("18!!".into()),
            &Options::new()
        )
        .is_err());
    }
}
