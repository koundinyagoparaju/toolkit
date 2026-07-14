use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// TOTP (RFC 6238) codes from a base32 secret and an explicit timestamp
/// — the clock arrives as an option (like cron-explain), so the tool
/// stays pure and the 2FA secret never leaves the device. Verified
/// against the RFC's own test vectors.
pub struct Totp;

impl Tool for Totp {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "totp".into(),
            label: "TOTP Code".into(),
            description: "Compute a TOTP code (RFC 6238) from a base32 secret at an explicit \
                          unix timestamp — debug 2FA enrollment without the secret touching a \
                          website. sha1 (the standard), sha256, and sha512."
                .into(),
            keywords: [
                "totp",
                "otp",
                "2fa",
                "authenticator",
                "code",
                "rfc6238",
                "hotp",
                "base32",
            ]
            .map(String::from)
            .to_vec(),
            inputs: vec![InputSpec::named(InputSpec::SOLE_NAME, DataType::Text)
                .describe("The base32 secret (as in an otpauth:// URL), padding optional.")
                .example("JBSWY3DPEHPK3PXP")],
            output: DataType::Json,
            streaming: false,
            options: vec![
                OptionSpec::string(
                    "at",
                    "At",
                    "Unix timestamp (seconds) the code is computed for.",
                )
                .default_value("1700000000".into()),
                OptionSpec::integer("digits", "Digits", "", Some(6), Some(10))
                    .default_value(6.into()),
                OptionSpec::integer("period", "Period (s)", "", Some(1), Some(300))
                    .default_value(30.into()),
                OptionSpec::enumeration(
                    "algorithm",
                    "Algorithm",
                    "",
                    &["sha1", "sha256", "sha512"],
                )
                .default_value("sha1".into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(secret) = inputs.sole() else {
            unreachable!()
        };
        let cleaned: String = secret
            .trim()
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '=')
            .map(|c| c.to_ascii_uppercase())
            .collect();
        let key = data_encoding::BASE32_NOPAD
            .decode(cleaned.as_bytes())
            .map_err(|_| ToolError::new("the secret is not valid base32"))?;

        let at: i64 = options
            .str_opt("at")
            .unwrap_or("1700000000")
            .trim()
            .parse()
            .map_err(|_| ToolError::new("\"at\" must be a unix timestamp in seconds"))?;
        if at < 0 {
            return Err(ToolError::new("\"at\" must not be negative"));
        }
        let digits = options.u32_opt("digits").unwrap_or(6);
        let period = options.u32_opt("period").unwrap_or(30) as i64;
        let counter = (at / period) as u64;

        macro_rules! hotp {
            ($hash:ty) => {{
                let mut mac = <Hmac<$hash> as KeyInit>::new_from_slice(&key)
                    .expect("hmac takes any key size");
                mac.update(&counter.to_be_bytes());
                mac.finalize().into_bytes().to_vec()
            }};
        }
        let mac: Vec<u8> = match options.str_opt("algorithm").unwrap_or("sha1") {
            "sha256" => hotp!(sha2::Sha256),
            "sha512" => hotp!(sha2::Sha512),
            _ => hotp!(sha1::Sha1),
        };
        let code = truncate(&mac, digits);
        Ok(DataValue::Json(serde_json::json!({
            "code": format!("{code:0width$}", width = digits as usize),
            "at": at,
            "period": period,
            "expires_in": period - (at % period),
        })))
    }
}

/// RFC 4226 dynamic truncation.
fn truncate(mac: &[u8], digits: u32) -> u64 {
    let offset = (mac[mac.len() - 1] & 0x0f) as usize;
    let bin = (u64::from(mac[offset] & 0x7f) << 24)
        | (u64::from(mac[offset + 1]) << 16)
        | (u64::from(mac[offset + 2]) << 8)
        | u64::from(mac[offset + 3]);
    bin % 10u64.pow(digits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn code(secret: &str, sets: &[(&str, serde_json::Value)]) -> String {
        let mut opts = Options::new();
        for (k, v) in sets {
            opts.insert((*k).into(), v.clone());
        }
        let DataValue::Json(v) = run_single(&Totp, DataValue::Text(secret.into()), &opts).unwrap()
        else {
            unreachable!()
        };
        v["code"].as_str().unwrap().to_string()
    }

    /// RFC 6238 Appendix B vectors: seed "12345678901234567890" (base32
    /// GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ), 8 digits, SHA-1.
    #[test]
    fn rfc6238_sha1_vectors() {
        const SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        for (at, expected) in [
            (59, "94287082"),
            (1111111109, "07081804"),
            (1111111111, "14050471"),
            (1234567890, "89005924"),
            (2000000000, "69279037"),
        ] {
            assert_eq!(
                code(
                    SECRET,
                    &[("at", at.to_string().into()), ("digits", 8.into())]
                ),
                expected,
                "at {at}"
            );
        }
    }

    /// RFC 6238 SHA-256 vector (seed repeated to 32 bytes).
    #[test]
    fn rfc6238_sha256_vector() {
        const SECRET32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA";
        assert_eq!(
            code(
                SECRET32,
                &[
                    ("at", "59".into()),
                    ("digits", 8.into()),
                    ("algorithm", "sha256".into()),
                ],
            ),
            "46119246"
        );
    }

    #[test]
    fn padding_whitespace_and_errors() {
        // Same secret with padding and spaces.
        assert_eq!(
            code("jbsw y3dp ehpk 3pxp====", &[("at", "1700000000".into())]),
            code("JBSWY3DPEHPK3PXP", &[("at", "1700000000".into())]),
        );
        let mut opts = Options::new();
        assert!(run_single(&Totp, DataValue::Text("not base32!!".into()), &opts).is_err());
        opts.insert("at".into(), "not-a-time".into());
        assert!(run_single(&Totp, DataValue::Text("JBSWY3DP".into()), &opts).is_err());
    }
}
