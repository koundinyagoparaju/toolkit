use crate::codec::{decode, encode};
use image::imageops::FilterType;
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct Resize;

impl Tool for Resize {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "image-resize".into(),
            label: "Image Resize".into(),
            description: "Resize an image. Give width, height, or both; \"fit\" preserves aspect ratio within the given bounds, \"exact\" stretches to them.".into(),
            keywords: ["image", "resize", "scale", "thumbnail", "shrink"].map(String::from).to_vec(),
            inputs: InputSpec::sole(DataType::Image),
            output: DataType::Image,
            options: vec![
                OptionSpec::integer("width", "Width (px)", "Target width in pixels.", Some(1), Some(20_000)),
                OptionSpec::integer("height", "Height (px)", "Target height in pixels.", Some(1), Some(20_000)),
                OptionSpec::enumeration(
                    "mode",
                    "Mode",
                    "fit: largest size preserving aspect ratio within width x height; exact: stretch to width x height.",
                    &["fit", "exact"],
                )
                .default_value("fit".into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let (img, format) = decode(inputs.sole())?;
        let width = options.u32_opt("width");
        let height = options.u32_opt("height");
        let (w, h) = match (width, height) {
            (None, None) => {
                return Err(ToolError::new("give at least one of width or height"));
            }
            // With one dimension given, the other keeps the aspect ratio.
            (Some(w), None) => {
                let h = (w as f64 * img.height() as f64 / img.width() as f64)
                    .round()
                    .max(1.0);
                (w, h as u32)
            }
            (None, Some(h)) => {
                let w = (h as f64 * img.width() as f64 / img.height() as f64)
                    .round()
                    .max(1.0);
                (w as u32, h)
            }
            (Some(w), Some(h)) => (w, h),
        };
        let resized = match options.str_opt("mode").unwrap_or("fit") {
            "exact" => img.resize_exact(w, h, FilterType::Lanczos3),
            _ => img.resize(w, h, FilterType::Lanczos3),
        };
        encode(&resized, format, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::fixtures;
    use serde_json::json;
    use toolkit_core::run_single;

    #[test]
    fn fit_preserves_aspect_ratio() {
        let input = fixtures::png(100, 50);
        let opts = json!({"width": 40, "height": 40});
        let out = run_single(&Resize, input, opts.as_object().unwrap()).unwrap();
        assert_eq!(fixtures::dimensions(&out), (40, 20));
    }

    #[test]
    fn exact_stretches() {
        let input = fixtures::png(100, 50);
        let opts = json!({"width": 40, "height": 40, "mode": "exact"});
        let out = run_single(&Resize, input, opts.as_object().unwrap()).unwrap();
        assert_eq!(fixtures::dimensions(&out), (40, 40));
    }

    #[test]
    fn single_dimension_keeps_aspect() {
        let input = fixtures::png(100, 50);
        let out = run_single(&Resize, input, json!({"width": 50}).as_object().unwrap()).unwrap();
        assert_eq!(fixtures::dimensions(&out), (50, 25));
    }

    #[test]
    fn no_dimensions_is_an_error() {
        let input = fixtures::png(10, 10);
        assert!(run_single(&Resize, input, &Options::new()).is_err());
    }
}
