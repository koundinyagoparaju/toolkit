use flate2::{write, Compression};
use std::io::Write;
use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, OptGet, OptionSpec, Options,
    StreamSession, Tool, ToolError,
};

fn format_option() -> OptionSpec {
    OptionSpec::enumeration(
        "format",
        "Format",
        "gzip: the .gz file format; zlib: RFC 1950; raw: bare deflate (as used by SAML redirects).",
        &["gzip", "zlib", "raw"],
    )
    .default_value("gzip".into())
}

pub struct Gzip;

impl Tool for Gzip {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "gzip".into(),
            label: "Compress (deflate)".into(),
            description:
                "Compress data as gzip, zlib, or raw deflate — streaming, so any size works.".into(),
            keywords: ["gzip", "compress", "deflate", "zlib", "zip"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Bytes,
                "The quick brown fox jumps over the lazy dog",
            ),
            output: DataType::Bytes,
            streaming: true,
            options: vec![
                format_option(),
                OptionSpec::integer("level", "Compression level", "", Some(0), Some(9))
                    .default_value(6.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, options: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        let level = Compression::new(options.u32_opt("level").unwrap_or(6));
        let encoder = match options.str_opt("format").unwrap_or("gzip") {
            "zlib" => Encoder::Zlib(write::ZlibEncoder::new(Vec::new(), level)),
            "raw" => Encoder::Raw(write::DeflateEncoder::new(Vec::new(), level)),
            _ => Encoder::Gz(write::GzEncoder::new(Vec::new(), level)),
        };
        Ok(Some(Box::new(CompressSession { encoder })))
    }
}

enum Encoder {
    Gz(write::GzEncoder<Vec<u8>>),
    Zlib(write::ZlibEncoder<Vec<u8>>),
    Raw(write::DeflateEncoder<Vec<u8>>),
}

struct CompressSession {
    encoder: Encoder,
}

impl StreamSession for CompressSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        let fail = |e: std::io::Error| ToolError::new(format!("compression failed: {e}"));
        match &mut self.encoder {
            Encoder::Gz(e) => {
                e.write_all(chunk).map_err(fail)?;
                Ok(std::mem::take(e.get_mut()))
            }
            Encoder::Zlib(e) => {
                e.write_all(chunk).map_err(fail)?;
                Ok(std::mem::take(e.get_mut()))
            }
            Encoder::Raw(e) => {
                e.write_all(chunk).map_err(fail)?;
                Ok(std::mem::take(e.get_mut()))
            }
        }
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        let fail = |e: std::io::Error| ToolError::new(format!("compression failed: {e}"));
        match self.encoder {
            Encoder::Gz(e) => e.finish().map_err(fail),
            Encoder::Zlib(e) => e.finish().map_err(fail),
            Encoder::Raw(e) => e.finish().map_err(fail),
        }
    }
}

pub struct Gunzip;

impl Tool for Gunzip {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "gunzip".into(),
            label: "Decompress (inflate)".into(),
            description: "Decompress gzip, zlib, or raw deflate data — e.g. base64-decode | gunzip format=raw for SAML redirect payloads.".into(),
            keywords: ["gunzip", "decompress", "inflate", "gzip", "zlib", "saml"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Bytes),
            output: DataType::Bytes,
            streaming: true,
            options: vec![format_option()],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, options: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        let decoder = match options.str_opt("format").unwrap_or("gzip") {
            "zlib" => Decoder::Zlib(write::ZlibDecoder::new(Vec::new())),
            "raw" => Decoder::Raw(write::DeflateDecoder::new(Vec::new())),
            _ => Decoder::Gz(write::GzDecoder::new(Vec::new())),
        };
        Ok(Some(Box::new(DecompressSession { decoder })))
    }
}

enum Decoder {
    Gz(write::GzDecoder<Vec<u8>>),
    Zlib(write::ZlibDecoder<Vec<u8>>),
    Raw(write::DeflateDecoder<Vec<u8>>),
}

struct DecompressSession {
    decoder: Decoder,
}

impl StreamSession for DecompressSession {
    fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        let fail = |e: std::io::Error| ToolError::new(format!("invalid compressed data: {e}"));
        match &mut self.decoder {
            Decoder::Gz(d) => {
                d.write_all(chunk).map_err(fail)?;
                Ok(std::mem::take(d.get_mut()))
            }
            Decoder::Zlib(d) => {
                d.write_all(chunk).map_err(fail)?;
                Ok(std::mem::take(d.get_mut()))
            }
            Decoder::Raw(d) => {
                d.write_all(chunk).map_err(fail)?;
                Ok(std::mem::take(d.get_mut()))
            }
        }
    }

    fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
        Ok(Vec::new())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        let fail = |e: std::io::Error| {
            ToolError::new(format!("compressed stream is invalid or truncated: {e}"))
        };
        match self.decoder {
            Decoder::Gz(d) => d.finish().map_err(fail),
            Decoder::Zlib(d) => d.finish().map_err(fail),
            Decoder::Raw(d) => d.finish().map_err(fail),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::run_single;

    fn round_trip(format: &str) {
        let opts = json!({ "format": format });
        let data = DataValue::Bytes(b"hello hello hello hello hello".repeat(50));
        let packed = run_single(&Gzip, data.clone(), opts.as_object().unwrap()).unwrap();
        let DataValue::Bytes(ref p) = packed else {
            panic!()
        };
        assert!(p.len() < 200, "{format}: compressed to {}", p.len());
        let back = run_single(&Gunzip, packed, opts.as_object().unwrap()).unwrap();
        assert_eq!(back, data, "{format}");
    }

    #[test]
    fn all_formats_round_trip() {
        for format in ["gzip", "zlib", "raw"] {
            round_trip(format);
        }
    }

    #[test]
    fn gzip_magic_bytes() {
        let out = run_single(&Gzip, DataValue::Text("x".into()), &Options::new()).unwrap();
        let DataValue::Bytes(b) = out else { panic!() };
        assert_eq!(&b[..2], &[0x1f, 0x8b]);
    }

    #[test]
    fn truncated_and_garbage_error() {
        assert!(run_single(
            &Gunzip,
            DataValue::Bytes(vec![0x1f, 0x8b, 8, 0]),
            &Options::new()
        )
        .is_err());
        assert!(run_single(
            &Gunzip,
            DataValue::Bytes(b"definitely not compressed data".to_vec()),
            &Options::new()
        )
        .is_err());
    }

    #[test]
    fn streaming_chunk_boundaries() {
        let data: Vec<u8> = (0..10_000u32).flat_map(|i| i.to_le_bytes()).collect();
        let packed = run_single(&Gzip, DataValue::Bytes(data.clone()), &Options::new()).unwrap();
        let DataValue::Bytes(packed) = packed else {
            panic!()
        };

        let mut session = Gunzip.open_stream(&Options::new()).unwrap().unwrap();
        let mut out = Vec::new();
        for chunk in packed.chunks(7) {
            out.extend(session.update("input", 0, chunk).unwrap());
        }
        session.end_input("input", 0).unwrap();
        out.extend(session.finish().unwrap());
        assert_eq!(out, data);
    }
}
