use std::collections::VecDeque;
use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, OptGet, OptionSpec, Options,
    StreamSession, Tool, ToolError,
};

/// Extract a line range — head, tail, or a window — without the rest of
/// the file going anywhere. Streams, so "lines 100120 of a gigabyte
/// log" costs a few kilobytes of memory.
pub struct TextSlice;

impl Tool for TextSlice {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "text-slice".into(),
            label: "Line Slice".into(),
            description: "Extract a line range: `100:120` (1-based, inclusive), `:20` (head), \
                          `50:` (from line 50), `-20:` (tail). Streams in constant memory."
                .into(),
            keywords: [
                "lines", "slice", "head", "tail", "range", "window", "extract", "log",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "one\ntwo\nthree\nfour\nfive\n"),
            output: DataType::Text,
            streaming: true,
            options: vec![OptionSpec::string(
                "lines",
                "Lines",
                "`A:B` inclusive, `:B` head, `A:` from A, `-N:` last N, or a single line number.",
            )
            .default_value("2:4".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, options: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        let spec = parse_range(options.str_opt("lines").unwrap_or("2:4"))?;
        Ok(Some(Box::new(SliceSession {
            spec,
            pending: Vec::new(),
            line_no: 0,
            tail: VecDeque::new(),
        })))
    }
}

enum Range {
    /// 1-based inclusive window; end None = to the end.
    Window { start: u64, end: Option<u64> },
    /// The last N lines.
    Tail(usize),
}

fn parse_range(raw: &str) -> Result<Range, ToolError> {
    let raw = raw.trim();
    let err = || {
        ToolError::new(format!(
            "bad line range \"{raw}\" (try 100:120, :20, 50:, -20:)"
        ))
    };
    if let Some(rest) = raw.strip_prefix('-') {
        let n: usize = rest
            .strip_suffix(':')
            .unwrap_or(rest)
            .parse()
            .map_err(|_| err())?;
        if n == 0 {
            return Err(err());
        }
        return Ok(Range::Tail(n));
    }
    let (a, b) = match raw.split_once(':') {
        None => {
            let n: u64 = raw.parse().map_err(|_| err())?;
            (n, Some(n))
        }
        Some((a, b)) => {
            let start = if a.is_empty() {
                1
            } else {
                a.parse().map_err(|_| err())?
            };
            let end = if b.is_empty() {
                None
            } else {
                Some(b.parse().map_err(|_| err())?)
            };
            (start, end)
        }
    };
    if a == 0 || matches!(b, Some(b) if b < a) {
        return Err(err());
    }
    Ok(Range::Window { start: a, end: b })
}

struct SliceSession {
    spec: Range,
    pending: Vec<u8>,
    line_no: u64,
    tail: VecDeque<Vec<u8>>,
}

impl SliceSession {
    fn take_line(&mut self, line: Vec<u8>, out: &mut Vec<u8>) {
        self.line_no += 1;
        match &self.spec {
            Range::Window { start, end } => {
                let past = matches!(end, Some(e) if self.line_no > *e);
                if self.line_no >= *start && !past {
                    out.extend_from_slice(&line);
                    out.push(b'\n');
                }
            }
            Range::Tail(n) => {
                if self.tail.len() == *n {
                    self.tail.pop_front();
                }
                self.tail.push_back(line);
            }
        }
    }
}

impl StreamSession for SliceSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        let mut out = Vec::new();
        let mut rest = chunk;
        while let Some(nl) = rest.iter().position(|&b| b == b'\n') {
            let mut line = std::mem::take(&mut self.pending);
            line.extend_from_slice(&rest[..nl]);
            self.take_line(line, &mut out);
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
            self.take_line(line, &mut out);
        }
        for line in &self.tail {
            out.extend_from_slice(line);
            out.push(b'\n');
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn slice(input: &str, lines: &str) -> Result<String, ToolError> {
        let mut opts = Options::new();
        opts.insert("lines".into(), lines.into());
        run_single(&TextSlice, DataValue::Text(input.into()), &opts).map(|v| {
            let DataValue::Text(out) = v else {
                unreachable!()
            };
            out
        })
    }

    const FIVE: &str = "one\ntwo\nthree\nfour\nfive\n";

    #[test]
    fn windows_heads_tails_and_singles() {
        assert_eq!(slice(FIVE, "2:4").unwrap(), "two\nthree\nfour\n");
        assert_eq!(slice(FIVE, ":2").unwrap(), "one\ntwo\n");
        assert_eq!(slice(FIVE, "4:").unwrap(), "four\nfive\n");
        assert_eq!(slice(FIVE, "-2:").unwrap(), "four\nfive\n");
        assert_eq!(slice(FIVE, "3").unwrap(), "three\n");
        assert_eq!(slice("no newline", "1").unwrap(), "no newline\n");
        assert_eq!(slice(FIVE, "9:12").unwrap(), "");
    }

    #[test]
    fn bad_ranges_error() {
        for bad in ["0:2", "5:2", "x", "-0:", ""] {
            assert!(slice(FIVE, bad).is_err(), "{bad}");
        }
    }
}
