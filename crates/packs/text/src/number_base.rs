use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// Convert an integer between numeral bases 2–36 — the programmer's
/// calculator move (hex↔dec↔bin↔octal for colors, masks, permissions).
pub struct NumberBase;

impl Tool for NumberBase {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "number-base".into(),
            label: "Number Base".into(),
            description: "Convert an integer between bases 2–36 (e.g. hex to decimal). Common prefixes (0x, 0b, 0o) are recognized; digits A–Z are case-insensitive.".into(),
            keywords: ["base", "radix", "hex", "hexadecimal", "binary", "octal", "decimal", "convert"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::integer("from", "From base", "Base of the input (2–36).", Some(2), Some(36))
                    .default_value(10.into()),
                OptionSpec::integer("to", "To base", "Base of the output (2–36).", Some(2), Some(36))
                    .default_value(16.into()),
                OptionSpec::bool("uppercase", "Uppercase", "Emit A–F… instead of a–f… for bases > 10.")
                    .default_value(false.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let from = options.u32_opt("from").unwrap_or(10);
        let to = options.u32_opt("to").unwrap_or(16);
        let uppercase = options.bool_opt("uppercase").unwrap_or(false);

        let trimmed = text.trim();
        let (sign, digits) = match trimmed.strip_prefix('-') {
            Some(rest) => ("-", rest),
            None => ("", trimmed.strip_prefix('+').unwrap_or(trimmed)),
        };
        // Recognize a base prefix only when it matches the declared `from`.
        let digits = strip_base_prefix(digits, from);
        if digits.is_empty() {
            return Err(ToolError::new("no digits to convert"));
        }

        let value = i128::from_str_radix(digits, from).map_err(|_| {
            ToolError::new(format!(
                "\"{digits}\" is not a valid base-{from} integer (or it overflows 128 bits)"
            ))
        })?;
        let value = if sign == "-" { -value } else { value };

        Ok(DataValue::Text(format_radix(value, to, uppercase)))
    }
}

fn strip_base_prefix(digits: &str, from: u32) -> &str {
    let prefix = match from {
        16 => "0x",
        2 => "0b",
        8 => "0o",
        _ => return digits,
    };
    digits
        .strip_prefix(prefix)
        .or_else(|| digits.strip_prefix(&prefix.to_uppercase()))
        .unwrap_or(digits)
}

fn format_radix(mut value: i128, radix: u32, uppercase: bool) -> String {
    if value == 0 {
        return "0".into();
    }
    let negative = value < 0;
    let alphabet: &[u8] = if uppercase {
        b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    } else {
        b"0123456789abcdefghijklmnopqrstuvwxyz"
    };
    let mut out = Vec::new();
    // Work on the magnitude; i128::MIN's magnitude fits in u128.
    let mut mag = value.unsigned_abs();
    let radix = radix as u128;
    while mag > 0 {
        out.push(alphabet[(mag % radix) as usize]);
        mag /= radix;
    }
    if negative {
        out.push(b'-');
    }
    out.reverse();
    value = 0;
    let _ = value;
    String::from_utf8(out).expect("alphabet is ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn convert(input: &str, from: i64, to: i64) -> String {
        let mut opts = Options::new();
        opts.insert("from".into(), from.into());
        opts.insert("to".into(), to.into());
        let DataValue::Text(out) =
            run_single(&NumberBase, DataValue::Text(input.into()), &opts).unwrap()
        else {
            unreachable!()
        };
        out
    }

    #[test]
    fn common_conversions() {
        assert_eq!(convert("255", 10, 16), "ff");
        assert_eq!(convert("0xff", 16, 10), "255");
        assert_eq!(convert("1010", 2, 10), "10");
        assert_eq!(convert("755", 8, 2), "111101101");
        assert_eq!(convert("-42", 10, 16), "-2a");
        assert_eq!(convert("0", 10, 2), "0");
    }

    #[test]
    fn uppercase_and_prefixes() {
        let mut opts = Options::new();
        opts.insert("from".into(), 10i64.into());
        opts.insert("to".into(), 16i64.into());
        opts.insert("uppercase".into(), true.into());
        let DataValue::Text(out) =
            run_single(&NumberBase, DataValue::Text("3735928559".into()), &opts).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(out, "DEADBEEF");
    }

    #[test]
    fn invalid_digit_errors() {
        let mut opts = Options::new();
        opts.insert("from".into(), 10i64.into());
        assert!(run_single(&NumberBase, DataValue::Text("12x".into()), &opts).is_err());
    }
}
