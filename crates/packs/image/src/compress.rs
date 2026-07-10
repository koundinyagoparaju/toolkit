use crate::codec::{decode, encode};
use image::ImageFormat;
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct Compress;

impl Tool for Compress {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "image-compress".into(),
            label: "Image Compress".into(),
            description: "Reduce an image's file size, keeping its format. JPEG uses the quality setting (lossy); PNG is re-encoded losslessly at maximum compression.".into(),
            keywords: ["image", "compress", "optimize", "quality", "size"].map(String::from).to_vec(),
            inputs: InputSpec::sole(DataType::Image),
            output: DataType::Image,
            options: vec![OptionSpec::integer(
                "quality",
                "Quality",
                "JPEG quality (1-100); lower is smaller. Ignored for PNG (lossless).",
                Some(1),
                Some(100),
            )
            .default_value(75.into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let (img, format) = decode(inputs.sole())?;
        match format {
            ImageFormat::Jpeg | ImageFormat::Png => {
                let quality = options.u32_opt("quality").map(|q| q as u8);
                encode(&img, format, quality)
            }
            other => Err(ToolError::new(format!(
                "compression is supported for png and jpeg; got {}. Use image-convert first.",
                crate::codec::format_name(other)
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::fixtures;
    use crate::convert::Convert;
    use serde_json::json;
    use toolkit_core::run_single;

    #[test]
    fn jpeg_quality_reduces_size() {
        // A noisy-ish checker at high vs low quality.
        let png = fixtures::png(64, 64);
        let jpeg_hi = run_single(
            &Convert,
            png,
            json!({"format": "jpeg", "quality": 100})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        let DataValue::Image {
            bytes: ref hi_bytes,
            ..
        } = jpeg_hi
        else {
            panic!()
        };
        let hi_len = hi_bytes.len();

        let compressed = run_single(
            &Compress,
            jpeg_hi.clone(),
            json!({"quality": 10}).as_object().unwrap(),
        )
        .unwrap();
        let DataValue::Image {
            ref bytes,
            ref format,
        } = compressed
        else {
            panic!()
        };
        assert_eq!(format, "jpeg");
        assert!(bytes.len() < hi_len, "expected {} < {hi_len}", bytes.len());
    }

    #[test]
    fn png_stays_lossless_png() {
        let out = run_single(&Compress, fixtures::png(32, 32), &Options::new()).unwrap();
        let DataValue::Image {
            ref bytes,
            ref format,
        } = out
        else {
            panic!()
        };
        assert_eq!(format, "png");
        assert_eq!(&bytes[1..4], b"PNG");
        assert_eq!(fixtures::dimensions(&out), (32, 32));
    }
}
