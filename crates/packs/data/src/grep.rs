use std::collections::VecDeque;
use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, OptGet, OptionSpec, Options,
    StreamSession, Tool, ToolError,
};

/// grep as a tool: regex-match lines with optional context, line numbers,
/// and inversion. Streams, so a huge log never has to fit anywhere — and
/// composed in a chain (`text-grep | text-uniq`) it summarizes without
/// the full file passing through whatever asked for it.
pub struct TextGrep;

impl Tool for TextGrep {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "text-grep".into(),
            label: "Grep".into(),
            description: "Filter lines by a regular expression, with line numbers, context \
                          lines, and inverted matching — grep for chains and agents. Streams: \
                          gigabyte logs flow through in constant memory."
                .into(),
            keywords: [
                "grep", "search", "filter", "regex", "match", "lines", "log", "find",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Text,
                "boot ok\nERROR disk full\nretrying\nerror: timeout\nok\n",
            ),
            output: DataType::Text,
            streaming: true,
            options: vec![
                OptionSpec::string("pattern", "Pattern", "Rust regex syntax.")
                    .default_value("(?i)error".into()),
                OptionSpec::integer(
                    "context",
                    "Context lines",
                    "Lines shown before and after each match (like grep -C).",
                    Some(0),
                    Some(10),
                )
                .default_value(0.into()),
                OptionSpec::bool(
                    "invert",
                    "Invert",
                    "Keep lines that do NOT match (grep -v).",
                )
                .default_value(false.into()),
                OptionSpec::bool(
                    "line_numbers",
                    "Line numbers",
                    "Prefix lines with their number.",
                )
                .default_value(true.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, options: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        let pattern = options.str_opt("pattern").unwrap_or("(?i)error");
        let regex =
            regex::Regex::new(pattern).map_err(|e| ToolError::new(format!("bad pattern: {e}")))?;
        Ok(Some(Box::new(GrepSession {
            regex,
            invert: options.bool_opt("invert").unwrap_or(false),
            line_numbers: options.bool_opt("line_numbers").unwrap_or(true),
            context: options.u32_opt("context").unwrap_or(0) as usize,
            pending: Vec::new(),
            line_no: 0,
            before: VecDeque::new(),
            after_left: 0,
            last_printed: 0,
        })))
    }
}

struct GrepSession {
    regex: regex::Regex,
    invert: bool,
    line_numbers: bool,
    context: usize,
    /// Bytes after the last newline of the previous chunk.
    pending: Vec<u8>,
    line_no: u64,
    /// Up to `context` lines preceding the current one: (number, text).
    before: VecDeque<(u64, String)>,
    /// Context lines still owed after the last match.
    after_left: usize,
    /// Highest line number emitted, to avoid duplicates and place "--".
    last_printed: u64,
}

impl GrepSession {
    fn emit(&mut self, out: &mut Vec<u8>, no: u64, line: &str, is_match: bool) {
        if no <= self.last_printed {
            return;
        }
        // A gap between printed groups gets grep's "--" separator.
        if self.context > 0 && self.last_printed > 0 && no > self.last_printed + 1 {
            out.extend_from_slice(b"--\n");
        }
        if self.line_numbers {
            let sep = if is_match { ':' } else { '-' };
            out.extend_from_slice(format!("{no}{sep}{line}\n").as_bytes());
        } else {
            out.extend_from_slice(format!("{line}\n").as_bytes());
        }
        self.last_printed = no;
    }

    fn take_line(&mut self, raw: &[u8], out: &mut Vec<u8>) {
        self.line_no += 1;
        let no = self.line_no;
        let mut line = String::from_utf8_lossy(raw).into_owned();
        if line.ends_with('\r') {
            line.pop();
        }
        let is_match = self.regex.is_match(&line) != self.invert;
        if is_match {
            let before: Vec<(u64, String)> = self.before.drain(..).collect();
            for (n, l) in before {
                self.emit(out, n, &l, false);
            }
            self.emit(out, no, &line, true);
            self.after_left = self.context;
        } else {
            if self.after_left > 0 {
                self.after_left -= 1;
                self.emit(out, no, &line, false);
            }
            if self.context > 0 {
                if self.before.len() == self.context {
                    self.before.pop_front();
                }
                self.before.push_back((no, line));
            }
        }
    }
}

impl StreamSession for GrepSession {
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

    fn grep(input: &str, sets: &[(&str, serde_json::Value)]) -> String {
        let mut opts = Options::new();
        for (k, v) in sets {
            opts.insert((*k).into(), v.clone());
        }
        let DataValue::Text(out) =
            run_single(&TextGrep, DataValue::Text(input.into()), &opts).unwrap()
        else {
            unreachable!()
        };
        out
    }

    const LOG: &str = "boot ok\nERROR disk full\nretrying\nerror: timeout\nok\n";

    #[test]
    fn default_finds_errors_case_insensitively() {
        assert_eq!(grep(LOG, &[]), "2:ERROR disk full\n4:error: timeout\n");
    }

    #[test]
    fn context_with_separators_and_no_duplicates() {
        let out = grep(
            "a\nx1\nMATCH\ny1\nb\nc\nd\nx2\nMATCH\n",
            &[("pattern", "MATCH".into()), ("context", 1.into())],
        );
        assert_eq!(out, "2-x1\n3:MATCH\n4-y1\n--\n8-x2\n9:MATCH\n");

        // Overlapping context regions never repeat lines.
        let out = grep(
            "M\nmid\nM\n",
            &[("pattern", "M".into()), ("context", 2.into())],
        );
        assert_eq!(out, "1:M\n2-mid\n3:M\n");
    }

    #[test]
    fn invert_and_plain_output() {
        let out = grep(
            LOG,
            &[
                ("pattern", "(?i)error".into()),
                ("invert", true.into()),
                ("line_numbers", false.into()),
            ],
        );
        assert_eq!(out, "boot ok\nretrying\nok\n");
    }

    #[test]
    fn bad_pattern_errors_and_final_line_counts() {
        let mut opts = Options::new();
        opts.insert("pattern".into(), "(".into());
        assert!(run_single(&TextGrep, DataValue::Text("x".into()), &opts).is_err());
        assert_eq!(grep("no newline ERROR", &[]), "1:no newline ERROR\n");
    }
}
