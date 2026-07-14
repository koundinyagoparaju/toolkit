use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Decode a UUID's anatomy: version, variant, and — for the time-based
/// versions — the embedded timestamp as unix milliseconds (feed it to
/// timestamp-convert for a date).
pub struct UuidInspect;

impl Tool for UuidInspect {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "uuid-inspect".into(),
            label: "UUID Inspect".into(),
            description: "Decode a UUID: version, variant, and the embedded timestamp of v1/v6 \
                          (gregorian) and v7 (unix) as unix milliseconds."
                .into(),
            keywords: [
                "uuid",
                "guid",
                "inspect",
                "version",
                "variant",
                "decode",
                "timestamp",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "01912d68-783e-7a03-8467-5661c1f0c9f1"),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let trimmed = text.trim();
        let without_urn = trimmed
            .get(..9)
            .filter(|p| p.eq_ignore_ascii_case("urn:uuid:"))
            .map_or(trimmed, |_| &trimmed[9..]);
        let cleaned: String = without_urn
            .chars()
            .filter(|c| *c != '-' && *c != '{' && *c != '}')
            .collect();
        if cleaned.len() != 32 || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ToolError::new(format!(
                "\"{}\" is not a UUID (32 hex digits expected)",
                text.trim()
            )));
        }
        let mut bytes = [0u8; 16];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16).expect("validated hex");
        }

        let canonical = format!(
            "{}-{}-{}-{}-{}",
            &cleaned[..8].to_lowercase(),
            &cleaned[8..12].to_lowercase(),
            &cleaned[12..16].to_lowercase(),
            &cleaned[16..20].to_lowercase(),
            &cleaned[20..].to_lowercase()
        );
        let version = bytes[6] >> 4;
        let variant = match bytes[8] >> 5 {
            0b000..=0b011 => "ncs (reserved)",
            0b100 | 0b101 => "rfc 9562",
            0b110 => "microsoft (reserved)",
            _ => "future (reserved)",
        };
        let nil = bytes.iter().all(|&b| b == 0);
        let max = bytes.iter().all(|&b| b == 0xff);

        let mut out = serde_json::json!({
            "uuid": canonical,
            "version": version,
            "variant": variant,
        });
        if nil {
            out["special"] = serde_json::json!("nil");
        } else if max {
            out["special"] = serde_json::json!("max");
        }

        match version {
            1 | 6 => {
                // 60-bit count of 100ns intervals since 1582-10-15.
                let ticks = if version == 1 {
                    (u64::from(bytes[6] & 0x0f) << 56)
                        | (u64::from(bytes[7]) << 48)
                        | (u64::from(bytes[4]) << 40)
                        | (u64::from(bytes[5]) << 32)
                        | (u64::from(bytes[0]) << 24)
                        | (u64::from(bytes[1]) << 16)
                        | (u64::from(bytes[2]) << 8)
                        | u64::from(bytes[3])
                } else {
                    // v6 stores the same clock high-to-low.
                    (u64::from(bytes[0]) << 52)
                        | (u64::from(bytes[1]) << 44)
                        | (u64::from(bytes[2]) << 36)
                        | (u64::from(bytes[3]) << 28)
                        | (u64::from(bytes[4]) << 20)
                        | (u64::from(bytes[5]) << 12)
                        | (u64::from(bytes[6] & 0x0f) << 8)
                        | u64::from(bytes[7])
                };
                // Gregorian epoch offset: 122192928000000000 ticks.
                let unix_ms = (ticks as i128 - 122_192_928_000_000_000) / 10_000;
                out["timestamp_unix_ms"] = serde_json::json!(unix_ms as i64);
            }
            7 => {
                let ms = (u64::from(bytes[0]) << 40)
                    | (u64::from(bytes[1]) << 32)
                    | (u64::from(bytes[2]) << 24)
                    | (u64::from(bytes[3]) << 16)
                    | (u64::from(bytes[4]) << 8)
                    | u64::from(bytes[5]);
                out["timestamp_unix_ms"] = serde_json::json!(ms);
            }
            _ => {}
        }
        Ok(DataValue::Json(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn inspect(input: &str) -> Result<serde_json::Value, ToolError> {
        run_single(&UuidInspect, DataValue::Text(input.into()), &Options::new()).map(|v| {
            let DataValue::Json(v) = v else {
                unreachable!()
            };
            v
        })
    }

    #[test]
    fn versions_variants_and_forms() {
        let v = inspect("01912d68-783e-7a03-8467-5661c1f0c9f1").unwrap();
        assert_eq!(v["version"], 7);
        assert_eq!(v["variant"], "rfc 9562");
        // First 48 bits: 0x01912d68783e = 1723043706942 ms (2024-08-07).
        assert_eq!(v["timestamp_unix_ms"], 1723043706942u64);

        // Braces, urn prefix, and uppercase all normalize.
        let v = inspect("URN:UUID:6BA7B810-9DAD-11D1-80B4-00C04FD430C8").unwrap();
        assert_eq!(v["version"], 1);
        // RFC 4122's own DNS namespace UUID: 1998-02.
        assert_eq!(v["timestamp_unix_ms"], 886630433151i64);

        assert_eq!(
            inspect("{00000000-0000-0000-0000-000000000000}").unwrap()["special"],
            "nil"
        );
    }

    #[test]
    fn junk_errors() {
        assert!(inspect("not-a-uuid").is_err());
        assert!(inspect("6ba7b8109dad11d180b400c04fd430c").is_err()); // 31 digits
    }
}
