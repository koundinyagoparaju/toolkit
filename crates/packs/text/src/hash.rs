use sha2::{Digest, Sha256, Sha512};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct Hash;

impl Tool for Hash {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "hash".into(),
            label: "Hash".into(),
            description: "Compute a cryptographic digest of the input, as lowercase hex.".into(),
            keywords: ["hash", "sha256", "sha512", "digest", "checksum"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Bytes),
            output: DataType::Text,
            options: vec![OptionSpec::enumeration(
                "algorithm",
                "Algorithm",
                "Digest algorithm.",
                &["sha256", "sha512"],
            )
            .default_value("sha256".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(bytes) = inputs.sole() else {
            unreachable!()
        };
        let digest = match options.str_opt("algorithm").unwrap_or("sha256") {
            "sha512" => hex(&Sha512::digest(&bytes)),
            _ => hex(&Sha256::digest(&bytes)),
        };
        Ok(DataValue::Text(digest))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn sha256_known_vector() {
        let out = run_single(&Hash, DataValue::Text("abc".into()), &Options::new()).unwrap();
        assert_eq!(
            out,
            DataValue::Text(
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into()
            )
        );
    }

    #[test]
    fn sha512_selected_by_option() {
        let out = run_single(
            &Hash,
            DataValue::Bytes(vec![]),
            serde_json::json!({"algorithm": "sha512"})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        let DataValue::Text(s) = out else { panic!() };
        assert_eq!(s.len(), 128);
        assert!(s.starts_with("cf83e1357eefb8bd")); // SHA-512 of empty input
    }
}
