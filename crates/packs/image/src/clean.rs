use crate::codec::{decode, encode};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// Metadata stripping via decode + re-encode: only pixels survive, so
/// EXIF, GPS coordinates, camera serials, thumbnails and color-profile
/// blobs are all removed.
pub struct ImageClean;

impl Tool for ImageClean {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "image-clean".into(),
            label: "Image Clean (strip metadata)".into(),
            description: "Remove ALL metadata from a photo — EXIF, GPS location, camera info, embedded thumbnails — by re-encoding only the pixels. Check what a photo leaks first with exif-view.".into(),
            keywords: ["image", "exif", "metadata", "strip", "gps", "privacy", "clean"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Image),
            output: DataType::Image,
            streaming: false,
            options: vec![OptionSpec::integer(
                "quality",
                "JPEG quality",
                "Re-encode quality for JPEGs (other formats are lossless).",
                Some(1),
                Some(100),
            )
            .default_value(92.into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let (img, format) = decode(inputs.sole())?;
        let quality = options.u32_opt("quality").map(|q| q as u8);
        encode(&img, format, quality)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::fixtures;
    use toolkit_core::run_single;

    #[test]
    fn keeps_pixels_and_format() {
        let out = run_single(&ImageClean, fixtures::png(12, 8), &Options::new()).unwrap();
        let DataValue::Image { ref format, .. } = out else {
            panic!()
        };
        assert_eq!(format, "png");
        assert_eq!(fixtures::dimensions(&out), (12, 8));
    }
}
