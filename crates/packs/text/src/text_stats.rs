use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, Options, StreamSession, Tool,
    ToolError,
};

/// Count lines, words, characters, and bytes — `wc` plus line-ending
/// detection, as JSON so it drops into a chain as an inspection step.
/// Streams: counts accumulate chunk by chunk, so it sizes a gigabyte
/// without buffering it.
pub struct TextStats;

impl Tool for TextStats {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "text-stats".into(),
            label: "Text Stats".into(),
            description: "Count lines, words, characters, and bytes, and detect the line ending (lf/crlf/mixed). Emits JSON.".into(),
            keywords: ["stats", "count", "wc", "lines", "words", "characters", "bytes"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Json,
            streaming: true,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, _: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        Ok(Some(Box::new(StatsSession::default())))
    }
}

#[derive(Default)]
struct StatsSession {
    bytes: u64,
    chars: u64,
    newlines: u64,
    words: u64,
    in_word: bool,
    prev_cr: bool, // last byte seen was '\r'
    last_byte: Option<u8>,
    has_crlf: bool,
    has_bare_lf: bool,
}

impl StreamSession for StatsSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        for &b in chunk {
            self.bytes += 1;
            // A char is any byte that isn't a UTF-8 continuation byte
            // (input on a text port is valid UTF-8).
            if b & 0xC0 != 0x80 {
                self.chars += 1;
            }
            if b == b'\n' {
                self.newlines += 1;
                if self.prev_cr {
                    self.has_crlf = true;
                } else {
                    self.has_bare_lf = true;
                }
            }
            let is_ws = b.is_ascii_whitespace();
            if is_ws {
                self.in_word = false;
            } else if !self.in_word {
                self.words += 1;
                self.in_word = true;
            }
            self.prev_cr = b == b'\r';
            self.last_byte = Some(b);
        }
        Ok(Vec::new())
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        // Lines of content: newline-terminated lines, plus a final line
        // if the text is non-empty and doesn't end in a newline.
        let lines = self.newlines + u64::from(self.bytes > 0 && self.last_byte != Some(b'\n'));
        let line_ending = match (self.has_crlf, self.has_bare_lf) {
            (true, true) => "mixed",
            (true, false) => "crlf",
            (false, true) => "lf",
            (false, false) => "none",
        };
        let value = serde_json::json!({
            "lines": lines,
            "words": self.words,
            "chars": self.chars,
            "bytes": self.bytes,
            "line_ending": line_ending,
        });
        Ok(serde_json::to_vec(&value).expect("stats JSON serializes"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn stats(input: &str) -> serde_json::Value {
        let DataValue::Json(v) =
            run_single(&TextStats, DataValue::Text(input.into()), &Options::new()).unwrap()
        else {
            unreachable!()
        };
        v
    }

    #[test]
    fn counts_and_line_endings() {
        let v = stats("hello world\nsecond line\n");
        assert_eq!(v["lines"], 2);
        assert_eq!(v["words"], 4);
        assert_eq!(v["bytes"], 24);
        assert_eq!(v["line_ending"], "lf");

        // No trailing newline still counts the last line.
        assert_eq!(stats("a\nb")["lines"], 2);
        assert_eq!(stats("")["lines"], 0);
        assert_eq!(stats("one\r\ntwo\r\n")["line_ending"], "crlf");
        assert_eq!(stats("a\r\nb\nc")["line_ending"], "mixed");
    }

    #[test]
    fn counts_multibyte_chars() {
        let v = stats("héllo"); // 5 chars, 6 bytes (é is 2 bytes)
        assert_eq!(v["chars"], 5);
        assert_eq!(v["bytes"], 6);
    }

    #[test]
    fn chunk_boundaries_do_not_change_counts() {
        // Drive the session with a word split across a chunk boundary.
        let mut session = StatsSession::default();
        session.update("input", 0, b"hel").unwrap();
        session.update("input", 0, b"lo wor").unwrap();
        session.update("input", 0, b"ld").unwrap();
        let out = Box::new(session).finish().unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["words"], 2);
        assert_eq!(v["chars"], 11);
    }
}
