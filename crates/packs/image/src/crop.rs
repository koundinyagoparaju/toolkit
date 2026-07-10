use crate::codec::{decode, encode};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct Crop;

impl Tool for Crop {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "image-crop".into(),
            label: "Image Crop".into(),
            description: "Crop a rectangular region out of an image.".into(),
            keywords: ["image", "crop", "cut", "trim", "region"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Image),
            output: DataType::Image,
            options: vec![
                OptionSpec::integer(
                    "x",
                    "X",
                    "Left edge of the region, in pixels from the left.",
                    Some(0),
                    None,
                )
                .default_value(0.into()),
                OptionSpec::integer(
                    "y",
                    "Y",
                    "Top edge of the region, in pixels from the top.",
                    Some(0),
                    None,
                )
                .default_value(0.into()),
                OptionSpec::integer("width", "Width (px)", "Region width.", Some(1), None)
                    .required(),
                OptionSpec::integer("height", "Height (px)", "Region height.", Some(1), None)
                    .required(),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let (img, format) = decode(inputs.sole())?;
        let x = options.u32_opt("x").unwrap_or(0);
        let y = options.u32_opt("y").unwrap_or(0);
        let w = options.u32_opt("width").expect("required");
        let h = options.u32_opt("height").expect("required");
        if x.saturating_add(w) > img.width() || y.saturating_add(h) > img.height() {
            return Err(ToolError::new(format!(
                "crop region {w}x{h}+{x}+{y} exceeds image bounds {}x{}",
                img.width(),
                img.height()
            )));
        }
        encode(&img.crop_imm(x, y, w, h), format, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::fixtures;
    use serde_json::json;
    use toolkit_core::run_single;

    #[test]
    fn crops_to_region() {
        let input = fixtures::png(100, 50);
        let opts = json!({"x": 10, "y": 5, "width": 30, "height": 20});
        let out = run_single(&Crop, input, opts.as_object().unwrap()).unwrap();
        assert_eq!(fixtures::dimensions(&out), (30, 20));
    }

    #[test]
    fn out_of_bounds_region_errors() {
        let input = fixtures::png(20, 20);
        let opts = json!({"x": 10, "y": 0, "width": 11, "height": 5});
        assert!(run_single(&Crop, input, opts.as_object().unwrap()).is_err());
    }
}
