use percent_encoding::{percent_encode, AsciiSet, CONTROLS};
use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, Options, StreamSession, Tool,
    ToolError,
};

/// Everything except RFC 3986 unreserved characters (A-Z a-z 0-9 - _ . ~),
/// matching JavaScript's encodeURIComponent behavior for the common cases.
const COMPONENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}')
    .add(b'!');

pub struct UrlEncode;

impl Tool for UrlEncode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "url-encode".into(),
            label: "URL Encode".into(),
            description: "Percent-encode text for safe use in a URL component.".into(),
            keywords: ["url", "encode", "percent", "uri", "escape"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: true,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, _: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        Ok(Some(Box::new(EncodeSession)))
    }
}

/// Percent-encoding is per-byte, so chunk boundaries need no carry — a
/// multibyte UTF-8 character split across chunks encodes byte-by-byte to
/// the same %XX sequence either way.
struct EncodeSession;

impl StreamSession for EncodeSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        Ok(percent_encode(chunk, COMPONENT).to_string().into_bytes())
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }
}

pub struct UrlDecode;

impl Tool for UrlDecode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "url-decode".into(),
            label: "URL Decode".into(),
            description: "Decode percent-encoded text (e.g. %20 becomes a space).".into(),
            keywords: ["url", "decode", "percent", "uri", "unescape"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: true,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, _: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        Ok(Some(Box::new(DecodeSession { escape: Vec::new() })))
    }
}

/// Carries a partial %XX escape (1-2 bytes) across chunk boundaries.
/// Malformed escapes pass through literally, matching percent_decode.
struct DecodeSession {
    escape: Vec<u8>,
}

impl DecodeSession {
    fn resolve(&mut self, out: &mut Vec<u8>) {
        // self.escape is [b'%', hi, lo]; emit the byte or the literal text.
        let hi = (self.escape[1] as char).to_digit(16);
        let lo = (self.escape[2] as char).to_digit(16);
        match (hi, lo) {
            (Some(h), Some(l)) => out.push(((h << 4) | l) as u8),
            _ => out.extend_from_slice(&self.escape),
        }
        self.escape.clear();
    }
}

impl StreamSession for DecodeSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        let mut out = Vec::with_capacity(chunk.len());
        for &c in chunk {
            if self.escape.is_empty() {
                if c == b'%' {
                    self.escape.push(c);
                } else {
                    out.push(c);
                }
            } else {
                self.escape.push(c);
                if self.escape.len() == 3 {
                    self.resolve(&mut out);
                }
            }
        }
        Ok(out)
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(mut self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        // A trailing partial escape passes through literally.
        Ok(std::mem::take(&mut self.escape))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn round_trip() {
        let original = DataValue::Text("a b&c=d?e/f#g%h+ü".into());
        let encoded = run_single(&UrlEncode, original.clone(), &Options::new()).unwrap();
        let DataValue::Text(ref e) = encoded else {
            panic!()
        };
        assert!(!e.contains(' ') && !e.contains('&') && !e.contains('#'));
        assert_eq!(
            run_single(&UrlDecode, encoded, &Options::new()).unwrap(),
            original
        );
    }

    #[test]
    fn decode_plain_percent_sequences() {
        let out = run_single(
            &UrlDecode,
            DataValue::Text("hello%20world%21".into()),
            &Options::new(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Text("hello world!".into()));
    }

    #[test]
    fn malformed_escapes_pass_through() {
        let out = run_single(
            &UrlDecode,
            DataValue::Text("100%zz and 50%".into()),
            &Options::new(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Text("100%zz and 50%".into()));
    }

    #[test]
    fn streaming_splits_escape_across_chunks() {
        let mut dec = UrlDecode.open_stream(&Options::new()).unwrap().unwrap();
        let mut out = Vec::new();
        for chunk in ["hello%", "2", "0world"] {
            out.extend(dec.update("input", 0, chunk.as_bytes()).unwrap());
        }
        dec.end_input("input", 0).unwrap();
        out.extend(dec.finish().unwrap());
        assert_eq!(out, b"hello world");
    }
}
