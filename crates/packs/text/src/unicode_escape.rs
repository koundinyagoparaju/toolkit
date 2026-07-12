use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct UnicodeEscape;

impl Tool for UnicodeEscape {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "unicode-escape".into(),
            label: "Unicode Escape".into(),
            description: "Escape non-ASCII characters as \\uXXXX (JS/JSON, with surrogate pairs) or \\u{…} (Rust).".into(),
            keywords: ["unicode", "escape", "codepoint", "js", "json"].map(String::from).to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "héllo wörld ✓"),
            output: DataType::Text,
            streaming: false,
            options: vec![OptionSpec::enumeration("format", "Format", "", &["js", "rust"])
                .default_value("js".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let rust = options.str_opt("format").unwrap_or("js") == "rust";
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if c.is_ascii() && !c.is_ascii_control() {
                out.push(c);
            } else if rust {
                out.push_str(&format!("\\u{{{:x}}}", c as u32));
            } else {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
        Ok(DataValue::Text(out))
    }
}

pub struct UnicodeUnescape;

impl Tool for UnicodeUnescape {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "unicode-unescape".into(),
            label: "Unicode Unescape".into(),
            description: "Decode \\uXXXX (including surrogate pairs), \\u{…}, \\xNN and common backslash escapes.".into(),
            keywords: ["unicode", "unescape", "decode", "codepoint"].map(String::from).to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, r"h\u00e9llo w\u00f6rld \u2713"),
            output: DataType::Text,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        let mut pending_high: Option<u16> = None;

        while let Some(c) = chars.next() {
            if c != '\\' {
                flush_surrogate(&mut pending_high, &mut out);
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('u') => {
                    if chars.peek() == Some(&'{') {
                        chars.next();
                        let hex: String = chars.by_ref().take_while(|&c| c != '}').collect();
                        flush_surrogate(&mut pending_high, &mut out);
                        let code = u32::from_str_radix(&hex, 16)
                            .map_err(|_| ToolError::new(format!("bad escape \\u{{{hex}}}")))?;
                        out.push(
                            char::from_u32(code)
                                .ok_or_else(|| ToolError::new("invalid code point"))?,
                        );
                    } else {
                        let hex: String = chars.by_ref().take(4).collect();
                        let unit = u16::from_str_radix(&hex, 16)
                            .map_err(|_| ToolError::new(format!("bad escape \\u{hex}")))?;
                        match (pending_high.take(), unit) {
                            (Some(high), 0xDC00..=0xDFFF) => {
                                let code = 0x10000
                                    + ((high as u32 - 0xD800) << 10)
                                    + (unit as u32 - 0xDC00);
                                out.push(char::from_u32(code).expect("valid surrogate pair"));
                            }
                            (high, 0xD800..=0xDBFF) => {
                                if let Some(h) = high {
                                    push_lone(h, &mut out);
                                }
                                pending_high = Some(unit);
                            }
                            (high, _) => {
                                if let Some(h) = high {
                                    push_lone(h, &mut out);
                                }
                                out.push(
                                    char::from_u32(unit as u32)
                                        .ok_or_else(|| ToolError::new("invalid code point"))?,
                                );
                            }
                        }
                    }
                }
                Some('x') => {
                    flush_surrogate(&mut pending_high, &mut out);
                    let hex: String = chars.by_ref().take(2).collect();
                    let code = u8::from_str_radix(&hex, 16)
                        .map_err(|_| ToolError::new(format!("bad escape \\x{hex}")))?;
                    out.push(code as char);
                }
                Some(simple) => {
                    flush_surrogate(&mut pending_high, &mut out);
                    out.push(match simple {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '0' => '\0',
                        other => other, // includes \\ and \" etc.
                    });
                }
                None => {
                    flush_surrogate(&mut pending_high, &mut out);
                    out.push('\\');
                }
            }
        }
        flush_surrogate(&mut pending_high, &mut out);
        Ok(DataValue::Text(out))
    }
}

fn flush_surrogate(pending: &mut Option<u16>, out: &mut String) {
    if let Some(h) = pending.take() {
        push_lone(h, out);
    }
}

fn push_lone(unit: u16, out: &mut String) {
    // A lone surrogate cannot be a char; keep it visible as its escape.
    out.push_str(&format!("\\u{unit:04x}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::run_single;

    #[test]
    fn escape_round_trips_including_surrogate_pairs() {
        let original = DataValue::Text("héllo 😀 → end".into());
        let js = run_single(&UnicodeEscape, original.clone(), &Options::new()).unwrap();
        let DataValue::Text(ref e) = js else { panic!() };
        assert!(e.contains("\\ud83d\\ude00"), "{e}"); // 😀 as a pair
        let back = run_single(&UnicodeUnescape, js, &Options::new()).unwrap();
        assert_eq!(back, original);

        let rust = run_single(
            &UnicodeEscape,
            original.clone(),
            json!({"format": "rust"}).as_object().unwrap(),
        )
        .unwrap();
        let DataValue::Text(ref r) = rust else {
            panic!()
        };
        assert!(r.contains("\\u{1f600}"), "{r}");
        let back = run_single(&UnicodeUnescape, rust, &Options::new()).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn simple_escapes_and_bad_input() {
        let out = run_single(
            &UnicodeUnescape,
            DataValue::Text("a\\tb\\nc".into()),
            &Options::new(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Text("a\tb\nc".into()));
        assert!(run_single(
            &UnicodeUnescape,
            DataValue::Text("\\uZZZZ".into()),
            &Options::new()
        )
        .is_err());
    }
}
