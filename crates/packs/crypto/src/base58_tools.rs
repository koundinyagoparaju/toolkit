use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct Base58Encode;

impl Tool for Base58Encode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "base58-encode".into(),
            label: "Base58 Encode".into(),
            description: "Encode data as Base58 text (Bitcoin alphabet).".into(),
            keywords: ["base58", "encode", "bitcoin", "wallet"]
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
        Ok(DataValue::Text(bs58::encode(bytes).into_string()))
    }
}

pub struct Base58Decode;

impl Tool for Base58Decode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "base58-decode".into(),
            label: "Base58 Decode".into(),
            description: "Decode Base58 text (Bitcoin alphabet) to bytes.".into(),
            keywords: ["base58", "decode", "bitcoin", "wallet"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "Cn8eVZg"),
            output: DataType::Bytes,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        bs58::decode(text.trim())
            .into_vec()
            .map(DataValue::Bytes)
            .map_err(|e| ToolError::new(format!("invalid base58: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn round_trip() {
        let data = DataValue::Bytes(b"hello".to_vec());
        let enc = run_single(&Base58Encode, data.clone(), &Options::new()).unwrap();
        assert_eq!(enc, DataValue::Text("Cn8eVZg".into()));
        let dec = run_single(&Base58Decode, enc, &Options::new()).unwrap();
        assert_eq!(dec, data);
        assert!(run_single(
            &Base58Decode,
            DataValue::Text("0OIl".into()),
            &Options::new()
        )
        .is_err());
    }
}
