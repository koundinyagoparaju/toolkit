use crate::color::parse_color;
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// WCAG contrast ratio between two colors, with the AA/AAA verdicts —
/// the accessibility check designers do in a web form, computed locally
/// from the spec's relative-luminance formula.
pub struct ContrastRatio;

impl Tool for ContrastRatio {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "contrast-ratio".into(),
            label: "Contrast Ratio".into(),
            description: "WCAG 2 contrast ratio between a foreground color and a background \
                          (hex/rgb()/hsl()), with pass/fail for AA and AAA at normal and large \
                          text sizes."
                .into(),
            keywords: [
                "contrast",
                "wcag",
                "accessibility",
                "a11y",
                "color",
                "ratio",
                "aa",
                "aaa",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "#1e90ff"),
            output: DataType::Json,
            streaming: false,
            options: vec![OptionSpec::string(
                "against",
                "Against",
                "The background color to compare with.",
            )
            .default_value("#ffffff".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let fg = parse_color(text.trim())?;
        let bg = parse_color(options.str_opt("against").unwrap_or("#ffffff").trim())?;
        let (l1, l2) = (luminance(fg), luminance(bg));
        let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
        let ratio = (hi + 0.05) / (lo + 0.05);
        let rounded = (ratio * 100.0).round() / 100.0;
        Ok(DataValue::Json(serde_json::json!({
            "ratio": rounded,
            "aa_normal": ratio >= 4.5,
            "aa_large": ratio >= 3.0,
            "aaa_normal": ratio >= 7.0,
            "aaa_large": ratio >= 4.5,
        })))
    }
}

/// WCAG 2 relative luminance of an sRGB color.
fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let linear = |c: u8| {
        let c = c as f64 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn ratio(fg: &str, bg: &str) -> serde_json::Value {
        let mut opts = Options::new();
        opts.insert("against".into(), bg.into());
        let DataValue::Json(v) =
            run_single(&ContrastRatio, DataValue::Text(fg.into()), &opts).unwrap()
        else {
            unreachable!()
        };
        v
    }

    #[test]
    fn spec_anchors() {
        // Black on white is the WCAG maximum, 21:1.
        assert_eq!(ratio("#000000", "#ffffff")["ratio"], 21.0);
        assert_eq!(ratio("#ffffff", "#000000")["ratio"], 21.0); // symmetric
        assert_eq!(ratio("#777777", "#777777")["ratio"], 1.0);

        // dodgerblue on white: ~3.15 — passes AA large only.
        let v = ratio("#1e90ff", "#ffffff");
        assert_eq!(v["aa_large"], true);
        assert_eq!(v["aa_normal"], false);
        let r = v["ratio"].as_f64().unwrap();
        assert!((3.0..3.3).contains(&r), "{r}");
    }

    #[test]
    fn junk_errors() {
        assert!(run_single(
            &ContrastRatio,
            DataValue::Text("nope".into()),
            &Options::new()
        )
        .is_err());
    }
}
