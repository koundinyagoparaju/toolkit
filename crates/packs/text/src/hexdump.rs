use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, Options, StreamSession, Tool,
    ToolError,
};

/// xxd-style hex view: offset, sixteen bytes as eight hex pairs, ASCII
/// gutter. The output format matches `xxd` exactly (verified by a
/// differential test), so eyes trained on xxd read it natively. Streams.
pub struct Hexdump;

impl Tool for Hexdump {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "hexdump".into(),
            label: "Hexdump".into(),
            description: "Inspect bytes as an xxd-style dump: offset, hex pairs, ASCII gutter. \
                          Streams, so any size works."
                .into(),
            keywords: [
                "hex", "dump", "xxd", "bytes", "inspect", "binary", "offset", "view",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Bytes, "Hello, toolkit!"),
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
        Ok(Some(Box::new(DumpSession {
            offset: 0,
            pending: Vec::new(),
        })))
    }
}

struct DumpSession {
    offset: u64,
    /// Bytes short of a full 16-byte row.
    pending: Vec<u8>,
}

/// One xxd row: "00000000: 4865 6c6c 6f2c 2074 6f6f 6c6b 6974 2100  Hello, toolkit!."
fn row(offset: u64, bytes: &[u8], out: &mut String) {
    out.push_str(&format!("{offset:08x}: "));
    for i in 0..16 {
        match bytes.get(i) {
            Some(b) => out.push_str(&format!("{b:02x}")),
            None => out.push_str("  "),
        }
        if i % 2 == 1 {
            out.push(' ');
        }
    }
    out.push(' ');
    for &b in bytes {
        out.push(if (0x20..0x7f).contains(&b) {
            b as char
        } else {
            '.'
        });
    }
    out.push('\n');
}

impl StreamSession for DumpSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        let mut out = String::new();
        let mut data = std::mem::take(&mut self.pending);
        data.extend_from_slice(chunk);
        let mut rest = data.as_slice();
        while rest.len() >= 16 {
            row(self.offset, &rest[..16], &mut out);
            self.offset += 16;
            rest = &rest[16..];
        }
        self.pending = rest.to_vec();
        Ok(out.into_bytes())
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        let mut out = String::new();
        if !self.pending.is_empty() {
            row(self.offset, &self.pending, &mut out);
        }
        Ok(out.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn dump(input: &[u8]) -> String {
        let DataValue::Text(out) =
            run_single(&Hexdump, DataValue::Bytes(input.to_vec()), &Options::new()).unwrap()
        else {
            unreachable!()
        };
        out
    }

    #[test]
    fn full_and_partial_rows() {
        assert_eq!(
            dump(b"Hello, toolkit!"),
            "00000000: 4865 6c6c 6f2c 2074 6f6f 6c6b 6974 21    Hello, toolkit!\n"
        );
        assert_eq!(
            dump(&[0u8; 17]).lines().nth(1).unwrap(),
            "00000010: 00                                       ."
        );
        assert_eq!(dump(b""), "");
    }

    #[test]
    fn nonprintable_bytes_become_dots() {
        let out = dump(&[0x00, 0x1f, 0x7f, 0xff, b'A']);
        assert!(out.ends_with("....A\n"), "{out:?}");
    }
}
