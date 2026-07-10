use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, OptGet, OptionSpec, Options,
    StreamSession, Tool, ToolError,
};

pub struct Hash;

impl Tool for Hash {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "hash".into(),
            label: "Hash".into(),
            description: "Compute a cryptographic digest of the input, as lowercase hex.".into(),
            keywords: ["hash", "sha256", "sha512", "digest", "checksum"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Bytes),
            output: DataType::Text,
            streaming: true,
            options: vec![OptionSpec::enumeration(
                "algorithm",
                "Algorithm",
                "Digest algorithm.",
                &["sha256", "sha512", "sha1", "md5", "crc32"],
            )
            .default_value("sha256".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, options: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        let hasher = match options.str_opt("algorithm").unwrap_or("sha256") {
            "sha512" => Hasher::Sha512(Sha512::new()),
            "sha1" => Hasher::Sha1(Sha1::new()),
            "md5" => Hasher::Md5(Md5::new()),
            "crc32" => Hasher::Crc32(crc32fast::Hasher::new()),
            _ => Hasher::Sha256(Sha256::new()),
        };
        Ok(Some(Box::new(HashSession { hasher })))
    }
}

enum Hasher {
    Sha256(Sha256),
    Sha512(Sha512),
    Sha1(Sha1),
    Md5(Md5),
    Crc32(crc32fast::Hasher),
}

struct HashSession {
    hasher: Hasher,
}

impl StreamSession for HashSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        match &mut self.hasher {
            Hasher::Sha256(h) => h.update(chunk),
            Hasher::Sha512(h) => h.update(chunk),
            Hasher::Sha1(h) => h.update(chunk),
            Hasher::Md5(h) => h.update(chunk),
            Hasher::Crc32(h) => h.update(chunk),
        }
        Ok(Vec::new())
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        let digest = match self.hasher {
            Hasher::Sha256(h) => hex(&h.finalize()),
            Hasher::Sha512(h) => hex(&h.finalize()),
            Hasher::Sha1(h) => hex(&h.finalize()),
            Hasher::Md5(h) => hex(&h.finalize()),
            Hasher::Crc32(h) => format!("{:08x}", h.finalize()),
        };
        Ok(digest.into_bytes())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn sha256_known_vector() {
        let out = run_single(&Hash, DataValue::Text("abc".into()), &Options::new()).unwrap();
        assert_eq!(
            out,
            DataValue::Text(
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into()
            )
        );
    }

    #[test]
    fn sha512_selected_by_option() {
        let out = run_single(
            &Hash,
            DataValue::Bytes(vec![]),
            serde_json::json!({"algorithm": "sha512"})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        let DataValue::Text(s) = out else { panic!() };
        assert_eq!(s.len(), 128);
        assert!(s.starts_with("cf83e1357eefb8bd")); // SHA-512 of empty input
    }

    #[test]
    fn legacy_algorithms() {
        let run = |algorithm: &str| {
            let out = run_single(
                &Hash,
                DataValue::Text("abc".into()),
                serde_json::json!({ "algorithm": algorithm })
                    .as_object()
                    .unwrap(),
            )
            .unwrap();
            let DataValue::Text(s) = out else { panic!() };
            s
        };
        assert_eq!(run("md5"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(run("sha1"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(run("crc32"), "352441c2");
    }

    #[test]
    fn streaming_chunks_equal_one_shot() {
        let mut session = Hash.open_stream(&Options::new()).unwrap().unwrap();
        for chunk in [b"a".as_slice(), b"b", b"c"] {
            assert!(session.update("input", 0, chunk).unwrap().is_empty());
        }
        session.end_input("input", 0).unwrap();
        let digest = session.finish().unwrap();
        assert_eq!(
            String::from_utf8(digest).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
