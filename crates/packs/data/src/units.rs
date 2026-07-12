use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// Parse the sole text input as a finite number.
fn parse_value(inputs: Inputs) -> Result<f64, ToolError> {
    let DataValue::Text(text) = inputs.sole() else {
        unreachable!()
    };
    let trimmed = text.trim();
    let value: f64 = trimmed
        .parse()
        .map_err(|_| ToolError::new(format!("\"{trimmed}\" is not a number")))?;
    if !value.is_finite() {
        return Err(ToolError::new("value must be finite"));
    }
    Ok(value)
}

fn format_value(value: f64) -> Result<DataValue, ToolError> {
    if !value.is_finite() {
        return Err(ToolError::new("result is too large to represent"));
    }
    Ok(DataValue::Text(value.to_string()))
}

/// A unit-conversion tool for units that are pure scale factors of a
/// common base unit (temperature is affine, pixels are context-dependent
/// — those are separate tools below).
pub struct LinearUnits {
    name: &'static str,
    label: &'static str,
    description: &'static str,
    keywords: &'static [&'static str],
    /// (unit, how many base units one of it is)
    units: &'static [(&'static str, f64)],
}

impl LinearUnits {
    fn factor(&self, options: &Options, key: &str) -> Result<f64, ToolError> {
        let unit = options.str_opt(key).expect("required");
        self.units
            .iter()
            .find(|(name, _)| *name == unit)
            .map(|(_, factor)| *factor)
            .ok_or_else(|| ToolError::new(format!("unknown unit \"{unit}\"")))
    }
}

impl Tool for LinearUnits {
    fn manifest(&self) -> Manifest {
        let names: Vec<&str> = self.units.iter().map(|(name, _)| *name).collect();
        Manifest {
            name: self.name.into(),
            label: self.label.into(),
            description: self.description.into(),
            keywords: self.keywords.iter().map(|s| s.to_string()).collect(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::enumeration("from", "From unit", "Unit of the input.", &names)
                    .required(),
                OptionSpec::enumeration("to", "To unit", "Unit of the output.", &names).required(),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let value = parse_value(inputs)?;
        let result = value * self.factor(options, "from")? / self.factor(options, "to")?;
        format_value(result)
    }
}

pub const DATA_SIZE: LinearUnits = LinearUnits {
    name: "data-size-convert",
    label: "Data Size Convert",
    description: "Convert between data sizes: the decimal family (kb = 1000 b) \
                  and the binary family (kib = 1024 b).",
    keywords: &[
        "unit", "convert", "bytes", "kb", "kib", "mb", "mib", "gb", "gib", "tb", "tib", "storage",
        "size",
    ],
    units: &[
        ("b", 1.0),
        ("kb", 1e3),
        ("mb", 1e6),
        ("gb", 1e9),
        ("tb", 1e12),
        ("kib", 1024.0),
        ("mib", 1_048_576.0),
        ("gib", 1_073_741_824.0),
        ("tib", 1_099_511_627_776.0),
    ],
};

pub const LENGTH: LinearUnits = LinearUnits {
    name: "length-convert",
    label: "Length Convert",
    description: "Convert lengths between metric and imperial units.",
    keywords: &[
        "unit", "convert", "length", "distance", "metric", "imperial", "mm", "cm", "m", "km",
        "inch", "foot", "yard", "mile",
    ],
    units: &[
        ("mm", 0.001),
        ("cm", 0.01),
        ("m", 1.0),
        ("km", 1000.0),
        ("in", 0.0254),
        ("ft", 0.3048),
        ("yd", 0.9144),
        ("mi", 1609.344),
    ],
};

pub const MASS: LinearUnits = LinearUnits {
    name: "mass-convert",
    label: "Mass Convert",
    description: "Convert masses between metric and imperial units.",
    keywords: &[
        "unit", "convert", "mass", "weight", "metric", "imperial", "mg", "g", "kg", "tonne",
        "ounce", "pound", "stone",
    ],
    units: &[
        ("mg", 0.001),
        ("g", 1.0),
        ("kg", 1000.0),
        ("tonne", 1e6),
        ("oz", 28.349_523_125),
        ("lb", 453.592_37),
        ("st", 6_350.293_18),
    ],
};

pub const VOLUME: LinearUnits = LinearUnits {
    name: "volume-convert",
    label: "Volume Convert",
    description: "Convert volumes between metric and US customary units \
                  (cup, pint, quart, gal are US measures).",
    keywords: &[
        "unit", "convert", "volume", "metric", "ml", "l", "liter", "litre", "teaspoon",
        "tablespoon", "cup", "pint", "quart", "gallon", "cooking",
    ],
    units: &[
        ("ml", 0.001),
        ("l", 1.0),
        ("m3", 1000.0),
        ("tsp", 0.004_928_921_593_75),
        ("tbsp", 0.014_786_764_781_25),
        ("floz", 0.029_573_529_562_5),
        ("cup", 0.236_588_236_5),
        ("pint", 0.473_176_473),
        ("quart", 0.946_352_946),
        ("gal", 3.785_411_784),
    ],
};

/// Temperature scales differ by offset as well as scale, so this can't be
/// a `LinearUnits` table — everything routes through kelvin.
pub struct TemperatureConvert;

const TEMPERATURE_UNITS: &[&str] = &["celsius", "fahrenheit", "kelvin"];

impl Tool for TemperatureConvert {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "temperature-convert".into(),
            label: "Temperature Convert".into(),
            description: "Convert a temperature between celsius, fahrenheit, and kelvin.".into(),
            keywords: [
                "unit",
                "convert",
                "temperature",
                "celsius",
                "fahrenheit",
                "kelvin",
                "degrees",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::enumeration("from", "From scale", "Scale of the input.", TEMPERATURE_UNITS)
                    .required(),
                OptionSpec::enumeration("to", "To scale", "Scale of the output.", TEMPERATURE_UNITS)
                    .required(),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let value = parse_value(inputs)?;
        let kelvin = match options.str_opt("from").expect("required") {
            "celsius" => value + 273.15,
            "fahrenheit" => (value - 32.0) * 5.0 / 9.0 + 273.15,
            _ => value,
        };
        let result = match options.str_opt("to").expect("required") {
            "celsius" => kelvin - 273.15,
            "fahrenheit" => (kelvin - 273.15) * 9.0 / 5.0 + 32.0,
            _ => kelvin,
        };
        format_value(result)
    }
}

/// Pixel units are context-dependent: physical units need a DPI, em/rem
/// need a font size. Defaults match the CSS reference pixel (96 dpi,
/// 16 px font), under which 1 pt = 4/3 px.
pub struct PxConvert;

const PX_UNITS: &[&str] = &["px", "pt", "em", "rem", "in", "mm", "cm"];

impl PxConvert {
    fn px_per_unit(unit: &str, dpi: f64, font_size: f64) -> f64 {
        match unit {
            "pt" => dpi / 72.0,
            "em" | "rem" => font_size,
            "in" => dpi,
            "mm" => dpi / 25.4,
            "cm" => dpi / 2.54,
            _ => 1.0, // px
        }
    }
}

impl Tool for PxConvert {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "px-convert".into(),
            label: "Pixel Convert".into(),
            description: "Convert CSS/print lengths between px, pt, em/rem (at a font size), \
                          and physical units (at a DPI). Defaults are the CSS reference pixel: \
                          96 dpi, 16 px font."
                .into(),
            keywords: [
                "unit", "convert", "pixel", "px", "pt", "em", "rem", "dpi", "css", "font",
                "typography",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::enumeration("from", "From unit", "Unit of the input.", PX_UNITS)
                    .required(),
                OptionSpec::enumeration("to", "To unit", "Unit of the output.", PX_UNITS)
                    .required(),
                OptionSpec::float(
                    "dpi",
                    "DPI",
                    "Pixels per inch, for pt/in/mm/cm.",
                    Some(1.0),
                    None,
                )
                .default_value(96.0.into()),
                OptionSpec::float(
                    "font-size",
                    "Font size (px)",
                    "Pixels per em/rem.",
                    Some(0.01),
                    None,
                )
                .default_value(16.0.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let value = parse_value(inputs)?;
        let dpi = options.f64_opt("dpi").unwrap_or(96.0);
        let font_size = options.f64_opt("font-size").unwrap_or(16.0);
        let from = Self::px_per_unit(options.str_opt("from").expect("required"), dpi, font_size);
        let to = Self::px_per_unit(options.str_opt("to").expect("required"), dpi, font_size);
        format_value(value * from / to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn convert(tool: &dyn Tool, input: &str, from: &str, to: &str) -> Result<f64, ToolError> {
        let mut opts = Options::new();
        opts.insert("from".into(), from.into());
        opts.insert("to".into(), to.into());
        let DataValue::Text(out) = run_single(tool, DataValue::Text(input.into()), &opts)? else {
            unreachable!()
        };
        Ok(out.parse().unwrap())
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9 * expected.abs().max(1.0),
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn data_size() {
        assert_close(convert(&DATA_SIZE, "1.5", "gib", "mb").unwrap(), 1610.612736);
        assert_close(convert(&DATA_SIZE, "1", "tb", "gb").unwrap(), 1000.0);
        assert_close(convert(&DATA_SIZE, "1048576", "b", "mib").unwrap(), 1.0);
    }

    #[test]
    fn length_mass_volume() {
        assert_close(convert(&LENGTH, "5", "mi", "km").unwrap(), 8.04672);
        assert_close(convert(&LENGTH, "1", "m", "ft").unwrap(), 3.280839895013123);
        assert_close(convert(&MASS, "1", "lb", "g").unwrap(), 453.59237);
        assert_close(convert(&MASS, "75", "kg", "lb").unwrap(), 165.34669663866312);
        assert_close(convert(&VOLUME, "1", "gal", "l").unwrap(), 3.785411784);
        assert_close(convert(&VOLUME, "3", "tsp", "tbsp").unwrap(), 1.0);
    }

    #[test]
    fn temperature() {
        assert_close(convert(&TemperatureConvert, "100", "celsius", "fahrenheit").unwrap(), 212.0);
        assert_close(convert(&TemperatureConvert, "32", "fahrenheit", "celsius").unwrap(), 0.0);
        assert_close(convert(&TemperatureConvert, "0", "celsius", "kelvin").unwrap(), 273.15);
        assert_close(convert(&TemperatureConvert, "0", "kelvin", "kelvin").unwrap(), 0.0);
    }

    #[test]
    fn px_defaults_match_css_reference() {
        assert_close(convert(&PxConvert, "16", "px", "pt").unwrap(), 12.0);
        assert_close(convert(&PxConvert, "1", "in", "px").unwrap(), 96.0);
        assert_close(convert(&PxConvert, "2", "rem", "px").unwrap(), 32.0);
    }

    #[test]
    fn px_honors_context_options() {
        let mut opts = Options::new();
        opts.insert("from".into(), "in".into());
        opts.insert("to".into(), "px".into());
        opts.insert("dpi".into(), 300.0.into());
        let DataValue::Text(out) =
            run_single(&PxConvert, DataValue::Text("1".into()), &opts).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(out, "300");

        let mut opts = Options::new();
        opts.insert("from".into(), "em".into());
        opts.insert("to".into(), "px".into());
        opts.insert("font-size".into(), 20.0.into());
        let DataValue::Text(out) =
            run_single(&PxConvert, DataValue::Text("2".into()), &opts).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(out, "40");
    }

    #[test]
    fn rejects_non_numbers_and_overflow() {
        assert!(convert(&LENGTH, "abc", "m", "km").is_err());
        assert!(convert(&LENGTH, "", "m", "km").is_err());
        assert!(convert(&LENGTH, "inf", "m", "km").is_err());
        // 1e308 km in mm overflows f64.
        assert!(convert(&LENGTH, "1e308", "km", "mm").is_err());
    }
}
