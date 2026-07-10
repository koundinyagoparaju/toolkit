use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct FileType;

/// (magic bytes, offset, description, extension, mime)
const SIGNATURES: &[(&[u8], usize, &str, &str, &str)] = &[
    (
        &[0x89, b'P', b'N', b'G'],
        0,
        "PNG image",
        "png",
        "image/png",
    ),
    (&[0xff, 0xd8, 0xff], 0, "JPEG image", "jpg", "image/jpeg"),
    (b"GIF8", 0, "GIF image", "gif", "image/gif"),
    (b"BM", 0, "BMP image", "bmp", "image/bmp"),
    (b"WEBP", 8, "WebP image", "webp", "image/webp"),
    (b"%PDF", 0, "PDF document", "pdf", "application/pdf"),
    (
        &[0x50, 0x4b, 0x03, 0x04],
        0,
        "ZIP archive (also docx/xlsx/jar/apk)",
        "zip",
        "application/zip",
    ),
    (
        &[0x1f, 0x8b],
        0,
        "gzip compressed data",
        "gz",
        "application/gzip",
    ),
    (
        b"7z\xbc\xaf\x27\x1c",
        0,
        "7-Zip archive",
        "7z",
        "application/x-7z-compressed",
    ),
    (b"Rar!", 0, "RAR archive", "rar", "application/vnd.rar"),
    (b"ustar", 257, "tar archive", "tar", "application/x-tar"),
    (b"ID3", 0, "MP3 audio (ID3)", "mp3", "audio/mpeg"),
    (b"OggS", 0, "Ogg container", "ogg", "audio/ogg"),
    (b"fLaC", 0, "FLAC audio", "flac", "audio/flac"),
    (b"ftyp", 4, "MP4 container", "mp4", "video/mp4"),
    (
        b"RIFF",
        0,
        "RIFF container (wav/avi/webp)",
        "riff",
        "application/octet-stream",
    ),
    (
        &[0x00, 0x61, 0x73, 0x6d],
        0,
        "WebAssembly module",
        "wasm",
        "application/wasm",
    ),
    (
        &[0x7f, b'E', b'L', b'F'],
        0,
        "ELF executable",
        "elf",
        "application/x-executable",
    ),
    (
        b"MZ",
        0,
        "Windows executable",
        "exe",
        "application/vnd.microsoft.portable-executable",
    ),
    (
        b"SQLite format 3",
        0,
        "SQLite database",
        "sqlite",
        "application/vnd.sqlite3",
    ),
    (b"\x00\x01\x00\x00", 0, "TrueType font", "ttf", "font/ttf"),
    (b"wOFF", 0, "WOFF font", "woff", "font/woff"),
    (b"wOF2", 0, "WOFF2 font", "woff2", "font/woff2"),
];

impl Tool for FileType {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "file-type".into(),
            label: "File Type".into(),
            description: "Identify a file's type from its magic bytes.".into(),
            keywords: ["file", "type", "magic", "mime", "identify", "detect"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Bytes),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(bytes) = inputs.sole() else {
            unreachable!()
        };
        for (magic, offset, description, extension, mime) in SIGNATURES {
            if bytes.len() >= offset + magic.len()
                && &bytes[*offset..offset + magic.len()] == *magic
            {
                return Ok(DataValue::Json(serde_json::json!({
                    "description": description,
                    "extension": extension,
                    "mime": mime,
                })));
            }
        }
        let looks_text = !bytes.is_empty()
            && std::str::from_utf8(&bytes).is_ok()
            && !bytes.iter().take(4096).any(|b| *b < 9);
        Ok(DataValue::Json(serde_json::json!({
            "description": if bytes.is_empty() {
                "empty"
            } else if looks_text {
                "plain text (UTF-8)"
            } else {
                "unknown binary data"
            },
            "extension": if looks_text { "txt" } else { "bin" },
            "mime": if looks_text { "text/plain" } else { "application/octet-stream" },
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn identify(bytes: Vec<u8>) -> serde_json::Value {
        let out = run_single(&FileType, DataValue::Bytes(bytes), &Options::new()).unwrap();
        let DataValue::Json(v) = out else { panic!() };
        v
    }

    #[test]
    fn detects_common_signatures() {
        assert_eq!(
            identify(vec![0x89, b'P', b'N', b'G', 13, 10])["extension"],
            "png"
        );
        assert_eq!(identify(b"%PDF-1.7".to_vec())["extension"], "pdf");
        assert_eq!(identify(vec![0x1f, 0x8b, 8])["extension"], "gz");
        assert_eq!(
            identify(vec![0x00, 0x61, 0x73, 0x6d, 1])["extension"],
            "wasm"
        );
    }

    #[test]
    fn falls_back_to_text_or_binary() {
        assert_eq!(identify(b"hello world".to_vec())["extension"], "txt");
        assert_eq!(identify(vec![0x01, 0x02, 0xff])["extension"], "bin");
    }
}
