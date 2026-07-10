use crate::codec::{decode, encode};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct ImageRotate;

impl Tool for ImageRotate {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "image-rotate".into(),
            label: "Image Rotate / Flip".into(),
            description: "Rotate an image in 90° steps or mirror it.".into(),
            keywords: ["image", "rotate", "flip", "mirror", "orientation"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Image),
            output: DataType::Image,
            streaming: false,
            options: vec![OptionSpec::enumeration(
                "mode",
                "Mode",
                "",
                &[
                    "rotate90",
                    "rotate180",
                    "rotate270",
                    "flip-horizontal",
                    "flip-vertical",
                ],
            )
            .required()],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let (img, format) = decode(inputs.sole())?;
        let out = match options.str_opt("mode").expect("required") {
            "rotate90" => img.rotate90(),
            "rotate180" => img.rotate180(),
            "rotate270" => img.rotate270(),
            "flip-horizontal" => img.fliph(),
            _ => img.flipv(),
        };
        encode(&out, format, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::fixtures;
    use serde_json::json;
    use toolkit_core::run_single;

    #[test]
    fn rotate90_swaps_dimensions() {
        let out = run_single(
            &ImageRotate,
            fixtures::png(30, 10),
            json!({"mode": "rotate90"}).as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(fixtures::dimensions(&out), (10, 30));
    }

    #[test]
    fn flip_keeps_dimensions() {
        let out = run_single(
            &ImageRotate,
            fixtures::png(30, 10),
            json!({"mode": "flip-horizontal"}).as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(fixtures::dimensions(&out), (30, 10));
    }
}
