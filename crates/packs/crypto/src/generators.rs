use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// UUID v4 from 16 driver-supplied entropy bytes. Deterministic given the
/// same entropy — wire fixed bytes into the entropy port for reproducible
/// output, or let the driver supply OS/browser randomness (the default).
pub struct Uuid;

impl Tool for Uuid {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "uuid".into(),
            label: "UUID".into(),
            description: "Generate a random (version 4) UUID.".into(),
            keywords: ["uuid", "guid", "identifier", "random", "generate"]
                .map(String::from)
                .to_vec(),
            inputs: vec![InputSpec::entropy()],
            output: DataType::Text,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, mut inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(entropy) = inputs.take("entropy") else {
            unreachable!()
        };
        let mut b: [u8; 16] = entropy
            .get(..16)
            .ok_or_else(|| ToolError::new("need at least 16 bytes of entropy"))?
            .try_into()
            .expect("sliced to 16");
        b[6] = (b[6] & 0x0f) | 0x40; // version 4
        b[8] = (b[8] & 0x3f) | 0x80; // RFC 4122 variant
        let h = data_encoding::HEXLOWER.encode(&b);
        Ok(DataValue::Text(format!(
            "{}-{}-{}-{}-{}",
            &h[0..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..32]
        )))
    }
}

/// Password generator: unbiased characters via rejection sampling over the
/// driver-supplied entropy.
pub struct PasswordGen;

impl Tool for PasswordGen {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "password-gen".into(),
            label: "Password Generator".into(),
            description: "Generate a random password. The randomness comes from your own device (OS or browser CSPRNG) and never leaves it.".into(),
            keywords: ["password", "random", "generate", "secret", "passphrase"]
                .map(String::from)
                .to_vec(),
            inputs: vec![InputSpec::entropy()],
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::integer("length", "Length", "", Some(4), Some(128))
                    .default_value(20.into()),
                OptionSpec::bool("lowercase", "a-z", "").default_value(true.into()),
                OptionSpec::bool("uppercase", "A-Z", "").default_value(true.into()),
                OptionSpec::bool("digits", "0-9", "").default_value(true.into()),
                OptionSpec::bool("symbols", "!@#$…", "").default_value(true.into()),
            ],
        }
    }

    fn run(&self, mut inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(entropy) = inputs.take("entropy") else {
            unreachable!()
        };
        let mut alphabet = String::new();
        if options.bool_opt("lowercase").unwrap_or(true) {
            alphabet.push_str("abcdefghijklmnopqrstuvwxyz");
        }
        if options.bool_opt("uppercase").unwrap_or(true) {
            alphabet.push_str("ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        }
        if options.bool_opt("digits").unwrap_or(true) {
            alphabet.push_str("0123456789");
        }
        if options.bool_opt("symbols").unwrap_or(true) {
            alphabet.push_str("!@#$%^&*()-_=+[]{};:,.<>?/~");
        }
        if alphabet.is_empty() {
            return Err(ToolError::new("enable at least one character set"));
        }
        let chars: Vec<char> = alphabet.chars().collect();
        let length = options.u32_opt("length").unwrap_or(20) as usize;

        // Rejection sampling: discard bytes >= the largest multiple of
        // len(chars), so every character is equally likely.
        let limit = 256 - (256 % chars.len());
        let mut password = String::with_capacity(length);
        let mut source = entropy.iter();
        while password.len() < length {
            let b = *source
                .next()
                .ok_or_else(|| ToolError::new("entropy exhausted (this should not happen)"))?
                as usize;
            if b < limit {
                password.push(chars[b % chars.len()]);
            }
        }
        Ok(DataValue::Text(password))
    }
}

/// Raw random bytes, for chains like `random-bytes | hex-encode`.
pub struct RandomBytes;

impl Tool for RandomBytes {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "random-bytes".into(),
            label: "Random Bytes".into(),
            description: "Generate cryptographically random bytes (chain into hex-encode or base64-encode for keys, tokens, and salts).".into(),
            keywords: ["random", "bytes", "key", "token", "salt", "nonce", "generate"]
                .map(String::from)
                .to_vec(),
            inputs: vec![InputSpec::entropy()],
            output: DataType::Bytes,
            streaming: false,
            options: vec![OptionSpec::integer("length", "Length (bytes)", "", Some(1), Some(1024))
                .default_value(32.into())],
        }
    }

    fn run(&self, mut inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(entropy) = inputs.take("entropy") else {
            unreachable!()
        };
        let length = options.u32_opt("length").unwrap_or(32) as usize;
        entropy
            .get(..length)
            .map(|b| DataValue::Bytes(b.to_vec()))
            .ok_or_else(|| ToolError::new("not enough entropy provided"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::{run_tool, ENTROPY_LEN};

    fn entropy(fill: u8) -> Inputs {
        Inputs::from([(
            "entropy".to_string(),
            vec![DataValue::Bytes(vec![fill; ENTROPY_LEN])],
        )])
    }

    #[test]
    fn uuid_is_deterministic_given_entropy_and_well_formed() {
        let a = run_tool(&Uuid, entropy(0xab), &Options::new()).unwrap();
        let b = run_tool(&Uuid, entropy(0xab), &Options::new()).unwrap();
        assert_eq!(a, b);
        let DataValue::Text(s) = a else { panic!() };
        assert_eq!(s.len(), 36);
        assert_eq!(s.as_bytes()[14], b'4'); // version nibble
        assert!(matches!(s.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    }

    #[test]
    fn password_respects_length_and_charset() {
        let opts = json!({"length": 32, "symbols": false, "uppercase": false});
        let out = run_tool(&PasswordGen, entropy(0x37), opts.as_object().unwrap()).unwrap();
        let DataValue::Text(s) = out else { panic!() };
        assert_eq!(s.chars().count(), 32);
        assert!(s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn empty_charset_is_an_error() {
        let opts =
            json!({"lowercase": false, "uppercase": false, "digits": false, "symbols": false});
        assert!(run_tool(&PasswordGen, entropy(1), opts.as_object().unwrap()).is_err());
    }

    #[test]
    fn random_bytes_slices_entropy() {
        let out = run_tool(
            &RandomBytes,
            entropy(0x11),
            json!({"length": 8}).as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Bytes(vec![0x11; 8]));
    }
}
