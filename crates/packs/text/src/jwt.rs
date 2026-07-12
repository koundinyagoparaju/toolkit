use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Decodes (does not verify!) a JSON Web Token into its parts. Verification
/// is deliberately out of scope: it would need the key, and this is an
/// inspection tool.
pub struct JwtDecode;

impl Tool for JwtDecode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "jwt-decode".into(),
            label: "JWT Decode".into(),
            description: "Decode a JSON Web Token into header, payload, and signature. Does not verify the signature.".into(),
            keywords: ["jwt", "token", "decode", "json web token", "auth"].map(String::from).to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let token = text.trim();
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(ToolError::new(format!(
                "a JWT has 3 dot-separated parts, found {}",
                parts.len()
            )));
        }
        let decode_json = |part: &str, what: &str| -> Result<serde_json::Value, ToolError> {
            let bytes = URL_SAFE_NO_PAD
                .decode(part)
                .map_err(|_| ToolError::new(format!("{what} is not valid base64url")))?;
            serde_json::from_slice(&bytes)
                .map_err(|e| ToolError::new(format!("{what} is not valid JSON: {e}")))
        };
        Ok(DataValue::Json(serde_json::json!({
            "header": decode_json(parts[0], "header")?,
            "payload": decode_json(parts[1], "payload")?,
            "signature_base64url": parts[2],
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    /// {"alg":"HS256","typ":"JWT"} . {"sub":"1234567890","name":"John Doe","iat":1516239022}
    const TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

    #[test]
    fn decodes_the_classic_example_token() {
        let out = run_single(&JwtDecode, DataValue::Text(TOKEN.into()), &Options::new()).unwrap();
        let DataValue::Json(v) = out else { panic!() };
        assert_eq!(v["header"]["alg"], "HS256");
        assert_eq!(v["payload"]["name"], "John Doe");
        assert_eq!(v["payload"]["iat"], 1516239022);
    }

    #[test]
    fn rejects_non_jwt_input() {
        assert!(run_single(&JwtDecode, DataValue::Text("a.b".into()), &Options::new()).is_err());
        assert!(run_single(&JwtDecode, DataValue::Text("x.y.z".into()), &Options::new()).is_err());
    }
}
