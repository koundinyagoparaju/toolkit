use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct ColorConvert;

impl Tool for ColorConvert {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "color-convert".into(),
            label: "Color Convert".into(),
            description: "Convert a color between hex, rgb() and hsl() notations.".into(),
            keywords: ["color", "hex", "rgb", "hsl", "convert", "css"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let (r, g, b) = parse_color(text.trim())?;
        let (h, s, l) = rgb_to_hsl(r, g, b);
        Ok(DataValue::Json(serde_json::json!({
            "hex": format!("#{r:02x}{g:02x}{b:02x}"),
            "rgb": {"r": r, "g": g, "b": b},
            "css_rgb": format!("rgb({r}, {g}, {b})"),
            "hsl": {"h": h, "s": s, "l": l},
            "css_hsl": format!("hsl({h}, {s}%, {l}%)"),
        })))
    }
}

fn parse_color(s: &str) -> Result<(u8, u8, u8), ToolError> {
    let invalid = || ToolError::new("expected #rgb, #rrggbb, rgb(r, g, b) or hsl(h, s%, l%)");
    if let Some(hex) = s.strip_prefix('#') {
        let hex: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
        // Byte-sliced below — a multibyte char would make those slices
        // panic mid-character (found by fuzzing), and can't be hex anyway.
        if !hex.is_ascii() {
            return Err(invalid());
        }
        let expand = |c: char| u8::from_str_radix(&format!("{c}{c}"), 16);
        return match hex.len() {
            3 => {
                let mut it = hex.chars();
                Ok((
                    expand(it.next().unwrap()).map_err(|_| invalid())?,
                    expand(it.next().unwrap()).map_err(|_| invalid())?,
                    expand(it.next().unwrap()).map_err(|_| invalid())?,
                ))
            }
            6 | 8 => Ok((
                u8::from_str_radix(&hex[0..2], 16).map_err(|_| invalid())?,
                u8::from_str_radix(&hex[2..4], 16).map_err(|_| invalid())?,
                u8::from_str_radix(&hex[4..6], 16).map_err(|_| invalid())?,
            )),
            _ => Err(invalid()),
        };
    }
    let lower = s.to_ascii_lowercase();
    if let Some(args) = lower.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = args.split(',').map(str::trim).collect();
        if parts.len() != 3 {
            return Err(invalid());
        }
        let channel = |p: &str| p.parse::<u8>().map_err(|_| invalid());
        return Ok((channel(parts[0])?, channel(parts[1])?, channel(parts[2])?));
    }
    if let Some(args) = lower.strip_prefix("hsl(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = args.split(',').map(str::trim).collect();
        if parts.len() != 3 {
            return Err(invalid());
        }
        let h: f64 = parts[0].parse().map_err(|_| invalid())?;
        let s: f64 = parts[1]
            .trim_end_matches('%')
            .parse()
            .map_err(|_| invalid())?;
        let l: f64 = parts[2]
            .trim_end_matches('%')
            .parse()
            .map_err(|_| invalid())?;
        return Ok(hsl_to_rgb(h, s / 100.0, l / 100.0));
    }
    Err(invalid())
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (u32, u32, u32) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f64::EPSILON {
        return (0, 0, (l * 100.0).round() as u32);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < f64::EPSILON {
        ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < f64::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (
        h.round() as u32 % 360,
        (s * 100.0).round() as u32,
        (l * 100.0).round() as u32,
    )
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn convert(s: &str) -> serde_json::Value {
        let out = run_single(&ColorConvert, DataValue::Text(s.into()), &Options::new()).unwrap();
        let DataValue::Json(v) = out else { panic!() };
        v
    }

    #[test]
    fn all_notations_agree() {
        for input in ["#1e90ff", "rgb(30, 144, 255)"] {
            let v = convert(input);
            assert_eq!(v["hex"], "#1e90ff", "input {input}");
            assert_eq!(v["hsl"]["h"], 210);
        }
        // HSL primaries convert exactly.
        assert_eq!(convert("hsl(120, 100%, 50%)")["hex"], "#00ff00");
        assert_eq!(convert("#fff")["rgb"]["r"], 255);
    }

    #[test]
    fn invalid_color_errors() {
        assert!(run_single(
            &ColorConvert,
            DataValue::Text("blueish".into()),
            &Options::new()
        )
        .is_err());
    }

    #[test]
    fn multibyte_hex_errors_instead_of_panicking() {
        // Fuzzer-found: "<\u{feff}2_" is 6 bytes after '#', and the old
        // byte slicing cut the BOM mid-character.
        for input in ["#<\u{feff}2_", "#\u{e9}\u{e9}\u{e9}", "#ééé"] {
            assert!(run_single(
                &ColorConvert,
                DataValue::Text(input.into()),
                &Options::new()
            )
            .is_err());
        }
    }
}
