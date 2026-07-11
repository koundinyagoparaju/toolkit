use data_encoding::BASE64URL_NOPAD;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha384, Sha512};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// Verify a JWT's HMAC signature with the secret as a separate port — so
/// the signing key never leaves your device. Complements jwt-decode,
/// which only inspects. HMAC algorithms only (HS256/384/512); RS/ES need
/// a public key and are out of scope.
pub struct JwtVerify;

impl Tool for JwtVerify {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "jwt-verify".into(),
            label: "JWT Verify".into(),
            description: "Verify a JWT's HMAC signature with a secret key. Returns the header and claims when valid, an error when not. Does not check expiry (no clock in a pure tool).".into(),
            keywords: ["jwt", "verify", "signature", "hmac", "token", "auth", "validate"]
                .map(String::from)
                .to_vec(),
            inputs: vec![
                InputSpec::named("token", DataType::Text),
                InputSpec::named("key", DataType::Bytes),
            ],
            output: DataType::Json,
            streaming: false,
            options: vec![OptionSpec::enumeration(
                "algorithm",
                "Algorithm",
                "Required algorithm. \"auto\" trusts the token header; pinning one prevents algorithm-confusion attacks.",
                &["auto", "HS256", "HS384", "HS512"],
            )
            .default_value("auto".into())],
        }
    }

    fn run(&self, mut inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(token) = inputs.take("token") else {
            unreachable!()
        };
        let DataValue::Bytes(key) = inputs.take("key") else {
            unreachable!()
        };
        let token = token.trim();
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(ToolError::new(format!(
                "a JWT has 3 dot-separated parts, found {}",
                parts.len()
            )));
        }
        let header: serde_json::Value = decode_json(parts[0], "header")?;
        let alg = header
            .get("alg")
            .and_then(|a| a.as_str())
            .ok_or_else(|| ToolError::new("token header has no \"alg\""))?;

        let required = options.str_opt("algorithm").unwrap_or("auto");
        if required != "auto" && required != alg {
            return Err(ToolError::new(format!(
                "token uses {alg} but {required} was required"
            )));
        }

        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let signature = BASE64URL_NOPAD
            .decode(parts[2].as_bytes())
            .map_err(|_| ToolError::new("signature is not valid base64url"))?;

        let msg = signing_input.as_bytes();
        // Constant-time verification via each mac's own verify_slice.
        let bad_key = || ToolError::new("key is not valid for HMAC");
        let ok = match alg {
            "HS256" => {
                let mut m = <Hmac<Sha256> as Mac>::new_from_slice(&key).map_err(|_| bad_key())?;
                m.update(msg);
                m.verify_slice(&signature).is_ok()
            }
            "HS384" => {
                let mut m = <Hmac<Sha384> as Mac>::new_from_slice(&key).map_err(|_| bad_key())?;
                m.update(msg);
                m.verify_slice(&signature).is_ok()
            }
            "HS512" => {
                let mut m = <Hmac<Sha512> as Mac>::new_from_slice(&key).map_err(|_| bad_key())?;
                m.update(msg);
                m.verify_slice(&signature).is_ok()
            }
            other => {
                return Err(ToolError::new(format!(
                "algorithm {other} is not an HMAC algorithm; only HS256/HS384/HS512 are supported"
            )))
            }
        };
        if !ok {
            return Err(ToolError::new("signature verification failed"));
        }

        Ok(DataValue::Json(serde_json::json!({
            "valid": true,
            "algorithm": alg,
            "header": header,
            "payload": decode_json(parts[1], "payload")?,
        })))
    }
}

fn decode_json(part: &str, what: &str) -> Result<serde_json::Value, ToolError> {
    let bytes = BASE64URL_NOPAD
        .decode(part.as_bytes())
        .map_err(|_| ToolError::new(format!("{what} is not valid base64url")))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| ToolError::new(format!("{what} is not valid JSON: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_tool;

    // The jwt.io example: HS256, secret "your-256-bit-secret".
    const TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    const SECRET: &[u8] = b"your-256-bit-secret";

    fn run(token: &str, key: &[u8], alg: &str) -> Result<serde_json::Value, ToolError> {
        let mut inputs = Inputs::new();
        inputs.insert("token".into(), vec![DataValue::Text(token.into())]);
        inputs.insert("key".into(), vec![DataValue::Bytes(key.to_vec())]);
        let mut opts = Options::new();
        opts.insert("algorithm".into(), alg.into());
        run_tool(&JwtVerify, inputs, &opts).map(|v| match v {
            DataValue::Json(j) => j,
            _ => unreachable!(),
        })
    }

    #[test]
    fn valid_signature_returns_claims() {
        let v = run(TOKEN, SECRET, "auto").unwrap();
        assert_eq!(v["valid"], true);
        assert_eq!(v["payload"]["name"], "John Doe");
    }

    #[test]
    fn wrong_key_fails() {
        assert!(run(TOKEN, b"wrong", "auto").is_err());
    }

    #[test]
    fn pinned_algorithm_mismatch_rejected() {
        assert!(run(TOKEN, SECRET, "HS512").is_err());
        assert!(run(TOKEN, SECRET, "HS256").is_ok());
    }
}
