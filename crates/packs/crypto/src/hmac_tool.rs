use hmac::{Hmac as HmacImpl, Mac};
use sha2::{Sha256, Sha512};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// HMAC signing with the key and message as separate named ports — the
/// whole point of running this client-side is never pasting signing keys
/// into a website.
pub struct Hmac;

impl Tool for Hmac {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "hmac".into(),
            label: "HMAC".into(),
            description: "Compute an HMAC signature of a message with a secret key — e.g. to debug webhook signatures without the key ever leaving your device.".into(),
            keywords: ["hmac", "signature", "webhook", "sign", "verify", "sha256"]
                .map(String::from)
                .to_vec(),
            inputs: vec![
                InputSpec::named("key", DataType::Bytes).example("secret-key"),
                InputSpec::named("message", DataType::Bytes).example("message to authenticate"),
            ],
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::enumeration("algorithm", "Algorithm", "", &["sha256", "sha512"])
                    .default_value("sha256".into()),
                OptionSpec::enumeration(
                    "encoding",
                    "Output encoding",
                    "",
                    &["hex", "base64"],
                )
                .default_value("hex".into()),
            ],
        }
    }

    fn run(&self, mut inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(key) = inputs.take("key") else {
            unreachable!()
        };
        let DataValue::Bytes(message) = inputs.take("message") else {
            unreachable!()
        };
        let mac = match options.str_opt("algorithm").unwrap_or("sha256") {
            "sha512" => {
                let mut m = <HmacImpl<Sha512> as Mac>::new_from_slice(&key)
                    .map_err(|e| ToolError::new(e.to_string()))?;
                m.update(&message);
                m.finalize().into_bytes().to_vec()
            }
            _ => {
                let mut m = <HmacImpl<Sha256> as Mac>::new_from_slice(&key)
                    .map_err(|e| ToolError::new(e.to_string()))?;
                m.update(&message);
                m.finalize().into_bytes().to_vec()
            }
        };
        let out = match options.str_opt("encoding").unwrap_or("hex") {
            "base64" => data_encoding::BASE64.encode(&mac),
            _ => data_encoding::HEXLOWER.encode(&mac),
        };
        Ok(DataValue::Text(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::run_tool;

    fn km(key: &str, msg: &str) -> Inputs {
        Inputs::from([
            ("key".to_string(), vec![DataValue::Text(key.into())]),
            ("message".to_string(), vec![DataValue::Text(msg.into())]),
        ])
    }

    #[test]
    fn rfc4231_style_vector() {
        // HMAC-SHA256("key", "The quick brown fox jumps over the lazy dog")
        let out = run_tool(
            &Hmac,
            km("key", "The quick brown fox jumps over the lazy dog"),
            &Options::new(),
        )
        .unwrap();
        assert_eq!(
            out,
            DataValue::Text(
                "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8".into()
            )
        );
    }

    #[test]
    fn base64_encoding_option() {
        let out = run_tool(
            &Hmac,
            km("k", "m"),
            json!({"encoding": "base64"}).as_object().unwrap(),
        )
        .unwrap();
        let DataValue::Text(s) = out else { panic!() };
        assert!(s.ends_with('=') || s.len() == 44);
    }
}
