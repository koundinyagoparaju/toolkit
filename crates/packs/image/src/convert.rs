use crate::codec::{decode, encode, format_from_name};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct Convert;

impl Tool for Convert {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "image-convert".into(),
            label: "Image Convert".into(),
            description: "Convert an image to another format (input may additionally be webp)."
                .into(),
            keywords: [
                "image", "convert", "format", "png", "jpeg", "gif", "bmp", "webp",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole(DataType::Image),
            output: DataType::Image,
            streaming: false,
            options: vec![
                OptionSpec::enumeration(
                    "format",
                    "Output format",
                    "Format to encode to.",
                    &["png", "jpeg", "gif", "bmp"],
                )
                .required(),
                OptionSpec::integer(
                    "quality",
                    "Quality",
                    "JPEG quality (1-100). Ignored for other formats.",
                    Some(1),
                    Some(100),
                )
                .default_value(85.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let (img, _) = decode(inputs.sole())?;
        let format = format_from_name(options.str_opt("format").expect("required"))?;
        let quality = options.u32_opt("quality").map(|q| q as u8);
        encode(&img, format, quality)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::fixtures;
    use serde_json::json;
    use toolkit_core::run_single;

    #[test]
    fn png_to_jpeg() {
        let input = fixtures::png(16, 16);
        let opts = json!({"format": "jpeg"});
        let out = run_single(&Convert, input, opts.as_object().unwrap()).unwrap();
        let DataValue::Image {
            ref bytes,
            ref format,
        } = out
        else {
            panic!()
        };
        assert_eq!(format, "jpeg");
        assert_eq!(&bytes[..3], &[0xff, 0xd8, 0xff], "JPEG magic bytes");
        assert_eq!(fixtures::dimensions(&out), (16, 16));
    }

    #[test]
    fn jpeg_back_to_png() {
        let png = fixtures::png(8, 8);
        let jpeg = run_single(
            &Convert,
            png,
            json!({"format": "jpeg"}).as_object().unwrap(),
        )
        .unwrap();
        let back = run_single(
            &Convert,
            jpeg,
            json!({"format": "png"}).as_object().unwrap(),
        )
        .unwrap();
        let DataValue::Image { ref bytes, .. } = back else {
            panic!()
        };
        assert_eq!(&bytes[1..4], b"PNG");
    }
}
