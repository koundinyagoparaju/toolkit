use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, OptGet, OptionSpec, Options,
    StreamSession, Tool, ToolError,
};

pub struct HexEncode;

impl Tool for HexEncode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "hex-encode".into(),
            label: "Hex Encode".into(),
            description: "Encode bytes as a hexadecimal string.".into(),
            keywords: ["hex", "hexadecimal", "encode", "dump"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Bytes),
            output: DataType::Text,
            streaming: true,
            options: vec![
                OptionSpec::bool("uppercase", "Uppercase", "Emit A-F instead of a-f.")
                    .default_value(false.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, options: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        Ok(Some(Box::new(EncodeSession {
            digits: if options.bool_opt("uppercase").unwrap_or(false) {
                b"0123456789ABCDEF"
            } else {
                b"0123456789abcdef"
            },
        })))
    }
}

struct EncodeSession {
    digits: &'static [u8; 16],
}

impl StreamSession for EncodeSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        let mut out = Vec::with_capacity(chunk.len() * 2);
        for b in chunk {
            out.push(self.digits[(b >> 4) as usize]);
            out.push(self.digits[(b & 0x0f) as usize]);
        }
        Ok(out)
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }
}

pub struct HexDecode;

impl Tool for HexDecode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "hex-decode".into(),
            label: "Hex Decode".into(),
            description: "Decode a hexadecimal string to bytes. Whitespace and an optional 0x prefix are ignored.".into(),
            keywords: ["hex", "hexadecimal", "decode"].map(String::from).to_vec(),
            inputs: InputSpec::sole(DataType::Text),
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
        Ok(Some(Box::new(DecodeSession {
            state: DecodeState::Start,
        })))
    }
}

/// Streaming hex decode. `Saw0` holds a leading '0' until we know whether
/// it starts a "0x" prefix (dropped) or is a plain digit.
enum DecodeState {
    Start,
    Saw0,
    Body { pending: Option<u8> },
}

struct DecodeSession {
    state: DecodeState,
}

impl DecodeSession {
    fn digit(c: u8) -> Result<u8, ToolError> {
        (c as char)
            .to_digit(16)
            .map(|d| d as u8)
            .ok_or_else(|| ToolError::new(format!("invalid hex digit '{}'", c as char)))
    }

    fn body(&mut self, c: u8, out: &mut Vec<u8>) -> Result<(), ToolError> {
        let d = Self::digit(c)?;
        let pending = match &mut self.state {
            DecodeState::Body { pending } => pending,
            _ => unreachable!(),
        };
        match pending.take() {
            Some(high) => out.push((high << 4) | d),
            None => *pending = Some(d),
        }
        Ok(())
    }
}

impl StreamSession for DecodeSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        let mut out = Vec::with_capacity(chunk.len() / 2);
        for &c in chunk {
            if c.is_ascii_whitespace() {
                continue;
            }
            match self.state {
                DecodeState::Start => {
                    if c == b'0' {
                        self.state = DecodeState::Saw0;
                    } else {
                        self.state = DecodeState::Body { pending: None };
                        self.body(c, &mut out)?;
                    }
                }
                DecodeState::Saw0 => {
                    self.state = DecodeState::Body { pending: None };
                    if c != b'x' {
                        self.body(b'0', &mut out)?;
                        self.body(c, &mut out)?;
                    }
                }
                DecodeState::Body { .. } => self.body(c, &mut out)?,
            }
        }
        Ok(out)
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        match self.state {
            DecodeState::Start | DecodeState::Body { pending: None } => Ok(Vec::new()),
            DecodeState::Saw0 | DecodeState::Body { pending: Some(_) } => {
                Err(ToolError::new("hex input has an odd number of digits"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn round_trip() {
        let data = DataValue::Bytes(vec![0x00, 0x7f, 0xff]);
        let encoded = run_single(&HexEncode, data.clone(), &Options::new()).unwrap();
        assert_eq!(encoded, DataValue::Text("007fff".into()));
        assert_eq!(
            run_single(&HexDecode, encoded, &Options::new()).unwrap(),
            data
        );
    }

    #[test]
    fn decode_tolerates_prefix_whitespace_and_case() {
        let out = run_single(
            &HexDecode,
            DataValue::Text("0xDE AD\nbe ef".into()),
            &Options::new(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Bytes(vec![0xde, 0xad, 0xbe, 0xef]));
    }

    #[test]
    fn odd_length_and_bad_digits_error() {
        assert!(run_single(&HexDecode, DataValue::Text("abc".into()), &Options::new()).is_err());
        assert!(run_single(&HexDecode, DataValue::Text("zz".into()), &Options::new()).is_err());
    }

    #[test]
    fn leading_zero_without_x_is_data_not_prefix() {
        let out = run_single(&HexDecode, DataValue::Text("00ff".into()), &Options::new()).unwrap();
        assert_eq!(out, DataValue::Bytes(vec![0x00, 0xff]));
    }

    #[test]
    fn streaming_splits_prefix_and_pairs_across_chunks() {
        let mut dec = HexDecode.open_stream(&Options::new()).unwrap().unwrap();
        let mut out = Vec::new();
        for chunk in ["0", "xd", "e a", "d"] {
            out.extend(dec.update("input", 0, chunk.as_bytes()).unwrap());
        }
        dec.end_input("input", 0).unwrap();
        out.extend(dec.finish().unwrap());
        assert_eq!(out, vec![0xde, 0xad]);
    }
}
