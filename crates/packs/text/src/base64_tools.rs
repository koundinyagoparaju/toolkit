use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, OptGet, OptionSpec, Options,
    StreamSession, Tool, ToolError,
};

pub struct Base64Encode;

impl Tool for Base64Encode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "base64-encode".into(),
            label: "Base64 Encode".into(),
            description: "Encode data as Base64 text.".into(),
            keywords: ["base64", "encode", "btoa"].map(String::from).to_vec(),
            inputs: InputSpec::sole_example(DataType::Bytes, "hello world"),
            output: DataType::Text,
            streaming: true,
            options: vec![OptionSpec::bool(
                "url_safe",
                "URL-safe alphabet",
                "Use the URL-safe alphabet (- and _ instead of + and /), without padding.",
            )
            .default_value(false.into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, options: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        Ok(Some(Box::new(EncodeSession {
            url_safe: options.bool_opt("url_safe").unwrap_or(false),
            carry: Vec::new(),
        })))
    }
}

/// Encodes in 3-byte groups, carrying at most 2 bytes between chunks.
struct EncodeSession {
    url_safe: bool,
    carry: Vec<u8>,
}

impl EncodeSession {
    fn encode(&self, bytes: &[u8]) -> String {
        if self.url_safe {
            URL_SAFE_NO_PAD.encode(bytes)
        } else {
            STANDARD.encode(bytes)
        }
    }
}

impl StreamSession for EncodeSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        self.carry.extend_from_slice(chunk);
        let whole = self.carry.len() - self.carry.len() % 3;
        let out = self.encode(&self.carry[..whole]).into_bytes();
        self.carry.drain(..whole);
        Ok(out)
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        Ok(self.encode(&self.carry).into_bytes())
    }
}

pub struct Base64Decode;

impl Tool for Base64Decode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "base64-decode".into(),
            label: "Base64 Decode".into(),
            description: "Decode Base64 text to its original bytes. Accepts standard and URL-safe alphabets, with or without padding; whitespace is ignored.".into(),
            keywords: ["base64", "decode", "atob"].map(String::from).to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "eyJoZWxsbyI6IndvcmxkIn0="),
            output: DataType::Bytes,
            streaming: true,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, _: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        Ok(Some(Box::new(DecodeSession { quad: Vec::new() })))
    }
}

/// Sextet values for both alphabets at once ('+'/'-' = 62, '/'/'_' = 63);
/// the union is unambiguous, which is what makes lenient streaming decode
/// possible without buffering the whole input to guess the variant.
fn sextet(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

/// Decodes in 4-character groups, carrying at most 3 characters. Whitespace
/// and '=' padding are skipped wherever they appear.
struct DecodeSession {
    quad: Vec<u8>,
}

impl StreamSession for DecodeSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        let mut out = Vec::with_capacity(chunk.len() * 3 / 4);
        for &c in chunk {
            if c.is_ascii_whitespace() || c == b'=' {
                continue;
            }
            let Some(v) = sextet(c) else {
                return Err(ToolError::new("input is not valid base64"));
            };
            self.quad.push(v);
            if self.quad.len() == 4 {
                out.push((self.quad[0] << 2) | (self.quad[1] >> 4));
                out.push((self.quad[1] << 4) | (self.quad[2] >> 2));
                out.push((self.quad[2] << 6) | self.quad[3]);
                self.quad.clear();
            }
        }
        Ok(out)
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        match self.quad.as_slice() {
            [] => Ok(Vec::new()),
            [a, b] => Ok(vec![(a << 2) | (b >> 4)]),
            [a, b, c] => Ok(vec![(a << 2) | (b >> 4), (b << 4) | (c >> 2)]),
            _ => Err(ToolError::new("input is not valid base64")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn encode_decode_round_trip() {
        let data = DataValue::Bytes(vec![0, 1, 254, 255]);
        let encoded = run_single(&Base64Encode, data.clone(), &Options::new()).unwrap();
        let decoded = run_single(&Base64Decode, encoded, &Options::new()).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_accepts_all_variants_and_whitespace() {
        for text in ["aGVsbG8=", "aGVsbG8", "aGVs\nbG8=", "  aGVsbG8=  "] {
            let out =
                run_single(&Base64Decode, DataValue::Text(text.into()), &Options::new()).unwrap();
            assert_eq!(out, DataValue::Bytes(b"hello".to_vec()), "input: {text:?}");
        }
    }

    #[test]
    fn url_safe_option() {
        let out = run_single(
            &Base64Encode,
            DataValue::Bytes(vec![251, 255]),
            serde_json::json!({"url_safe": true}).as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Text("-_8".into()));
    }

    #[test]
    fn invalid_input_errors() {
        assert!(run_single(
            &Base64Decode,
            DataValue::Text("!!!".into()),
            &Options::new()
        )
        .is_err());
    }

    #[test]
    fn streaming_survives_awkward_chunk_boundaries() {
        // Encode 5 bytes 1 byte at a time.
        let mut enc = Base64Encode.open_stream(&Options::new()).unwrap().unwrap();
        let mut encoded = Vec::new();
        for b in b"hello" {
            encoded.extend(enc.update("input", 0, &[*b]).unwrap());
        }
        encoded.extend(enc.end_input("input", 0).unwrap());
        encoded.extend(enc.finish().unwrap());
        assert_eq!(String::from_utf8(encoded.clone()).unwrap(), "aGVsbG8=");

        // Decode it back 1 char at a time, with whitespace injected.
        let mut dec = Base64Decode.open_stream(&Options::new()).unwrap().unwrap();
        let mut decoded = Vec::new();
        for c in encoded {
            decoded.extend(dec.update("input", 0, &[c]).unwrap());
            decoded.extend(dec.update("input", 0, b"\n").unwrap());
        }
        dec.end_input("input", 0).unwrap();
        decoded.extend(dec.finish().unwrap());
        assert_eq!(decoded, b"hello");
    }
}
