use crate::codec::{decode, encode};
use image::{DynamicImage, GenericImage, RgbaImage};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// The pack's first multi-input tool: two named ports with distinct roles.
pub struct Merge;

impl Tool for Merge {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "image-merge".into(),
            label: "Image Merge".into(),
            description: "Combine two images: side by side, stacked, or overlaid (second on top of first at x, y). Output uses the first image's format.".into(),
            keywords: ["image", "merge", "combine", "overlay", "stack", "side-by-side"]
                .map(String::from)
                .to_vec(),
            inputs: vec![
                InputSpec::named("first", DataType::Image)
                    .describe("The image placed first (left or top)."),
                InputSpec::named("second", DataType::Image)
                    .describe("The image placed second (right or bottom)."),
            ],
            output: DataType::Image,
            streaming: false,
            options: vec![
                OptionSpec::enumeration(
                    "mode",
                    "Mode",
                    "horizontal: first|second side by side; vertical: first above second; overlay: second drawn onto first.",
                    &["horizontal", "vertical", "overlay"],
                )
                .default_value("horizontal".into()),
                OptionSpec::integer("x", "X", "Overlay position from the left (overlay mode).", Some(0), None)
                    .default_value(0.into()),
                OptionSpec::integer("y", "Y", "Overlay position from the top (overlay mode).", Some(0), None)
                    .default_value(0.into()),
            ],
        }
    }

    fn run(&self, mut inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let (first, format) = decode(inputs.take("first"))?;
        let (second, _) = decode(inputs.take("second"))?;

        let merged = match options.str_opt("mode").unwrap_or("horizontal") {
            "vertical" => {
                let w = first.width().max(second.width());
                let h = first.height() + second.height();
                let mut canvas = RgbaImage::new(w, h);
                canvas
                    .copy_from(&first.to_rgba8(), 0, 0)
                    .map_err(|e| ToolError::new(e.to_string()))?;
                canvas
                    .copy_from(&second.to_rgba8(), 0, first.height())
                    .map_err(|e| ToolError::new(e.to_string()))?;
                DynamicImage::ImageRgba8(canvas)
            }
            "overlay" => {
                let x = options.u32_opt("x").unwrap_or(0);
                let y = options.u32_opt("y").unwrap_or(0);
                if x >= first.width() || y >= first.height() {
                    return Err(ToolError::new(format!(
                        "overlay position {x},{y} is outside the first image ({}x{})",
                        first.width(),
                        first.height()
                    )));
                }
                let mut canvas = first.to_rgba8();
                image::imageops::overlay(&mut canvas, &second.to_rgba8(), x as i64, y as i64);
                DynamicImage::ImageRgba8(canvas)
            }
            _ => {
                let w = first.width() + second.width();
                let h = first.height().max(second.height());
                let mut canvas = RgbaImage::new(w, h);
                canvas
                    .copy_from(&first.to_rgba8(), 0, 0)
                    .map_err(|e| ToolError::new(e.to_string()))?;
                canvas
                    .copy_from(&second.to_rgba8(), first.width(), 0)
                    .map_err(|e| ToolError::new(e.to_string()))?;
                DynamicImage::ImageRgba8(canvas)
            }
        };
        encode(&merged, format, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::fixtures;
    use serde_json::json;
    use toolkit_core::run_tool;

    fn merge_inputs(a: DataValue, b: DataValue) -> Inputs {
        Inputs::from([
            ("first".to_string(), vec![a]),
            ("second".to_string(), vec![b]),
        ])
    }

    #[test]
    fn horizontal_widths_add_heights_max() {
        let out = run_tool(
            &Merge,
            merge_inputs(fixtures::png(10, 20), fixtures::png(5, 30)),
            &Options::new(),
        )
        .unwrap();
        assert_eq!(fixtures::dimensions(&out), (15, 30));
    }

    #[test]
    fn vertical_heights_add_widths_max() {
        let out = run_tool(
            &Merge,
            merge_inputs(fixtures::png(10, 20), fixtures::png(5, 30)),
            json!({"mode": "vertical"}).as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(fixtures::dimensions(&out), (10, 50));
    }

    #[test]
    fn overlay_keeps_first_dimensions() {
        let out = run_tool(
            &Merge,
            merge_inputs(fixtures::png(20, 20), fixtures::png(4, 4)),
            json!({"mode": "overlay", "x": 8, "y": 8})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(fixtures::dimensions(&out), (20, 20));
    }

    #[test]
    fn overlay_out_of_bounds_errors() {
        assert!(run_tool(
            &Merge,
            merge_inputs(fixtures::png(10, 10), fixtures::png(2, 2)),
            json!({"mode": "overlay", "x": 10, "y": 0})
                .as_object()
                .unwrap(),
        )
        .is_err());
    }

    #[test]
    fn missing_port_is_a_clear_error() {
        let err = run_tool(
            &Merge,
            Inputs::from([("first".to_string(), vec![fixtures::png(4, 4)])]),
            &Options::new(),
        )
        .unwrap_err();
        assert!(err.message.contains("second"), "{err}");
    }
}
