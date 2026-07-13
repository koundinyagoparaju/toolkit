use std::collections::HashMap;
use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, OptGet, OptionSpec, Options,
    StreamSession, Tool, ToolError,
};

/// `sort | uniq -c` as one step: count duplicate lines (most frequent
/// first) or deduplicate keeping first-seen order. Streams, so a huge
/// log can flow through without being buffered.
pub struct TextUniq;

impl Tool for TextUniq {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "text-uniq".into(),
            label: "Unique Lines".into(),
            description: "Count duplicate lines (`count<TAB>line`, most frequent first) or \
                          deduplicate keeping first-seen order — `sort | uniq -c | sort -rn` \
                          as one streaming step."
                .into(),
            keywords: [
                "uniq",
                "unique",
                "duplicate",
                "count",
                "lines",
                "dedupe",
                "frequency",
                "log",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Text,
                "apple\nbanana\napple\ncherry\nbanana\napple\n",
            ),
            output: DataType::Text,
            streaming: true,
            options: vec![OptionSpec::enumeration(
                "mode",
                "Mode",
                "count: `count<TAB>line`, most frequent first (ties keep first-seen order). \
                 dedupe: each distinct line once, first-seen order.",
                &["count", "dedupe"],
            )
            .default_value("count".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, options: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        Ok(Some(Box::new(UniqSession {
            count_mode: options.str_opt("mode").unwrap_or("count") != "dedupe",
            pending: Vec::new(),
            counts: HashMap::new(),
            order: Vec::new(),
        })))
    }
}

struct UniqSession {
    count_mode: bool,
    /// Bytes after the last newline of the previous chunk.
    pending: Vec<u8>,
    /// line -> (first-seen rank, count)
    counts: HashMap<String, (usize, u64)>,
    order: Vec<String>,
}

impl UniqSession {
    fn take_line(&mut self, line: &[u8]) {
        // Text-port input is valid UTF-8, and chunks split only at the
        // newlines we cut on, so each assembled line is valid UTF-8 too.
        let line = String::from_utf8_lossy(line).into_owned();
        let rank = self.order.len();
        let entry = self.counts.entry(line.clone()).or_insert((rank, 0));
        if entry.1 == 0 {
            self.order.push(line);
        }
        entry.1 += 1;
    }
}

impl StreamSession for UniqSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        let mut rest = chunk;
        while let Some(nl) = rest.iter().position(|&b| b == b'\n') {
            let mut line = std::mem::take(&mut self.pending);
            line.extend_from_slice(&rest[..nl]);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.take_line(&line);
            rest = &rest[nl + 1..];
        }
        self.pending.extend_from_slice(rest);
        Ok(Vec::new())
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(mut self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.take_line(&line);
        }
        let mut out = String::new();
        if self.count_mode {
            let mut rows: Vec<(&String, &(usize, u64))> = self.counts.iter().collect();
            rows.sort_by_key(|(_, (rank, count))| (std::cmp::Reverse(*count), *rank));
            for (line, (_, count)) in rows {
                out.push_str(&format!("{count}\t{line}\n"));
            }
        } else {
            for line in &self.order {
                out.push_str(line);
                out.push('\n');
            }
        }
        Ok(out.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn uniq(input: &str, mode: &str) -> String {
        let mut opts = Options::new();
        opts.insert("mode".into(), mode.into());
        let DataValue::Text(out) =
            run_single(&TextUniq, DataValue::Text(input.into()), &opts).unwrap()
        else {
            unreachable!()
        };
        out
    }

    #[test]
    fn counts_most_frequent_first() {
        assert_eq!(
            uniq("apple\nbanana\napple\ncherry\nbanana\napple\n", "count"),
            "3\tapple\n2\tbanana\n1\tcherry\n"
        );
    }

    #[test]
    fn ties_keep_first_seen_order_and_dedupe_preserves_order() {
        assert_eq!(uniq("b\na\nb\na\n", "count"), "2\tb\n2\ta\n");
        assert_eq!(uniq("b\na\nb\na\nc", "dedupe"), "b\na\nc\n");
    }

    #[test]
    fn crlf_and_missing_trailing_newline() {
        assert_eq!(uniq("x\r\nx\ny", "count"), "2\tx\n1\ty\n");
        assert_eq!(uniq("", "count"), "");
    }

    #[test]
    fn chunk_boundaries_do_not_change_counts() {
        let mut s = UniqSession {
            count_mode: true,
            pending: Vec::new(),
            counts: HashMap::new(),
            order: Vec::new(),
        };
        s.update("input", 0, b"app").unwrap();
        s.update("input", 0, b"le\napple\nban").unwrap();
        s.update("input", 0, b"ana").unwrap();
        let out = Box::new(s).finish().unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "2\tapple\n1\tbanana\n");
    }
}
