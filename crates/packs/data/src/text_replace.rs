use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, OptGet, OptionSpec, Options,
    StreamSession, Tool, ToolError,
};

/// Regex find/replace with capture groups — sed's everyday job, applied
/// per line so it streams. `$1`/`${name}` in the replacement refer to
/// capture groups; patterns never span lines.
pub struct TextReplace;

impl Tool for TextReplace {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "text-replace".into(),
            label: "Find & Replace".into(),
            description: "Regex find/replace per line, with `$1`/`${name}` capture groups in \
                          the replacement. The default strips trailing whitespace. Streams."
                .into(),
            keywords: [
                "replace",
                "regex",
                "substitute",
                "sed",
                "rewrite",
                "find",
                "capture",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Text,
                "release v1.2 is out\nrelease v3.4 soon\n",
            ),
            output: DataType::Text,
            streaming: true,
            options: vec![
                OptionSpec::string("pattern", "Pattern", "Rust regex syntax; matched per line.")
                    .default_value(r"v(\d+)\.(\d+)".into()),
                OptionSpec::string(
                    "replacement",
                    "Replacement",
                    "May use $1, $2, ${name}; `$$` for a literal dollar.",
                )
                .default_value("v$1.$2.0".into()),
                OptionSpec::bool(
                    "first_only",
                    "First match only",
                    "Replace only the first match on each line.",
                )
                .default_value(false.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, options: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        let pattern = options.str_opt("pattern").unwrap_or(r"v(\d+)\.(\d+)");
        let regex =
            regex::Regex::new(pattern).map_err(|e| ToolError::new(format!("bad pattern: {e}")))?;
        Ok(Some(Box::new(ReplaceSession {
            regex,
            replacement: options
                .str_opt("replacement")
                .unwrap_or("v$1.$2.0")
                .to_string(),
            first_only: options.bool_opt("first_only").unwrap_or(false),
            pending: Vec::new(),
        })))
    }
}

struct ReplaceSession {
    regex: regex::Regex,
    replacement: String,
    first_only: bool,
    pending: Vec<u8>,
}

impl ReplaceSession {
    fn take_line(&self, raw: &[u8], out: &mut Vec<u8>) {
        let line = String::from_utf8_lossy(raw);
        let replaced = if self.first_only {
            self.regex.replace(&line, self.replacement.as_str())
        } else {
            self.regex.replace_all(&line, self.replacement.as_str())
        };
        out.extend_from_slice(replaced.as_bytes());
        out.push(b'\n');
    }
}

impl StreamSession for ReplaceSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        let mut out = Vec::new();
        let mut rest = chunk;
        while let Some(nl) = rest.iter().position(|&b| b == b'\n') {
            let mut line = std::mem::take(&mut self.pending);
            line.extend_from_slice(&rest[..nl]);
            self.take_line(&line, &mut out);
            rest = &rest[nl + 1..];
        }
        self.pending.extend_from_slice(rest);
        Ok(out)
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(mut self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        let mut out = Vec::new();
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.take_line(&line, &mut out);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn replace(input: &str, sets: &[(&str, serde_json::Value)]) -> String {
        let mut opts = Options::new();
        for (k, v) in sets {
            opts.insert((*k).into(), v.clone());
        }
        let DataValue::Text(out) =
            run_single(&TextReplace, DataValue::Text(input.into()), &opts).unwrap()
        else {
            unreachable!()
        };
        out
    }

    #[test]
    fn capture_groups_and_scope() {
        assert_eq!(
            replace("release v1.2 is out\n", &[]),
            "release v1.2.0 is out\n"
        );
        assert_eq!(
            replace(
                "a a a",
                &[("pattern", "a".into()), ("replacement", "b".into())],
            ),
            "b b b\n"
        );
        assert_eq!(
            replace(
                "a a a",
                &[
                    ("pattern", "a".into()),
                    ("replacement", "b".into()),
                    ("first_only", true.into()),
                ],
            ),
            "b a a\n"
        );
    }

    #[test]
    fn named_groups_and_literal_dollar() {
        assert_eq!(
            replace(
                "id=42",
                &[
                    ("pattern", r"id=(?P<n>\d+)".into()),
                    ("replacement", "$$${n}".into()),
                ],
            ),
            "$42\n"
        );
    }

    #[test]
    fn patterns_do_not_span_lines_and_bad_patterns_error() {
        assert_eq!(
            replace(
                "a\nb",
                &[("pattern", "a.b".into()), ("replacement", "X".into())],
            ),
            "a\nb\n"
        );
        let mut opts = Options::new();
        opts.insert("pattern".into(), "(".into());
        assert!(run_single(&TextReplace, DataValue::Text("x".into()), &opts).is_err());
    }
}
