use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Parse a human duration ("1h 30m", "90m", "1:30:00", bare seconds)
/// into exact numbers plus canonical human and ISO 8601 forms — the
/// "how many seconds is that?" arithmetic nobody should do by eye.
pub struct DurationConvert;

impl Tool for DurationConvert {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "duration-convert".into(),
            label: "Duration Convert".into(),
            description: "Parse a duration — unit form (`1d 2h 30m`, `90m`, `1.5h`), clock form \
                          (`1:30:00`), or bare seconds — into total seconds/milliseconds, a \
                          canonical human form, and ISO 8601."
                .into(),
            keywords: [
                "duration", "time", "seconds", "minutes", "hours", "parse", "convert", "human",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "1h 30m"),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let ms = parse_duration_ms(text.trim())?;
        Ok(DataValue::Json(serde_json::json!({
            "milliseconds": ms,
            "seconds": ms as f64 / 1000.0,
            "human": human(ms),
            "iso8601": iso8601(ms),
        })))
    }
}

const UNITS: &[(&str, u64)] = &[
    ("ms", 1),
    ("s", 1000),
    ("m", 60_000),
    ("h", 3_600_000),
    ("d", 86_400_000),
    ("w", 604_800_000),
];

fn parse_duration_ms(text: &str) -> Result<u64, ToolError> {
    let err = || {
        ToolError::new(format!(
            "\"{text}\" is not a duration (try \"1h 30m\", \"90m\", \"1:30:00\", or seconds)"
        ))
    };
    if text.is_empty() {
        return Err(err());
    }
    // Clock form: [hh:]mm:ss
    if text.contains(':') {
        let parts: Vec<&str> = text.split(':').collect();
        if !(2..=3).contains(&parts.len()) {
            return Err(err());
        }
        let mut total = 0u64;
        for part in &parts {
            let v: u64 = part.parse().map_err(|_| err())?;
            total = total
                .checked_mul(60)
                .and_then(|t| t.checked_add(v))
                .ok_or_else(err)?;
        }
        return total.checked_mul(1000).ok_or_else(err);
    }
    // Bare number = seconds.
    if let Ok(secs) = text.parse::<f64>() {
        if !secs.is_finite() || !(0.0..=9.0e15).contains(&secs) {
            return Err(err());
        }
        return Ok((secs * 1000.0).round() as u64);
    }
    // Unit form: sequence of <number><unit>, whitespace optional.
    let mut total = 0f64;
    let mut rest = text;
    let mut any = false;
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let num_len = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .ok_or_else(err)?;
        if num_len == 0 {
            return Err(err());
        }
        let value: f64 = rest[..num_len].parse().map_err(|_| err())?;
        rest = &rest[num_len..];
        let unit_len = rest
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        let unit = &rest[..unit_len];
        rest = &rest[unit_len..];
        let (_, factor) = UNITS.iter().find(|(u, _)| *u == unit).ok_or_else(|| {
            ToolError::new(format!("unknown unit \"{unit}\" (ms, s, m, h, d, w)"))
        })?;
        total += value * *factor as f64;
        any = true;
    }
    if !any || !total.is_finite() || !(0.0..=9.0e15).contains(&total) {
        return Err(err());
    }
    Ok(total.round() as u64)
}

fn human(ms: u64) -> String {
    if ms == 0 {
        return "0s".into();
    }
    let mut out = Vec::new();
    let mut rest = ms;
    for (unit, factor) in [
        ("d", 86_400_000),
        ("h", 3_600_000),
        ("m", 60_000),
        ("s", 1000),
    ] {
        let n = rest / factor;
        if n > 0 {
            out.push(format!("{n}{unit}"));
            rest %= factor;
        }
    }
    if rest > 0 {
        out.push(format!("{rest}ms"));
    }
    out.join(" ")
}

fn iso8601(ms: u64) -> String {
    if ms == 0 {
        return "PT0S".into();
    }
    let days = ms / 86_400_000;
    let h = ms % 86_400_000 / 3_600_000;
    let m = ms % 3_600_000 / 60_000;
    let ms_rest = ms % 60_000;
    let mut out = String::from("P");
    if days > 0 {
        out.push_str(&format!("{days}D"));
    }
    if h > 0 || m > 0 || ms_rest > 0 {
        out.push('T');
        if h > 0 {
            out.push_str(&format!("{h}H"));
        }
        if m > 0 {
            out.push_str(&format!("{m}M"));
        }
        if ms_rest > 0 {
            let secs = ms_rest as f64 / 1000.0;
            if secs.fract() == 0.0 {
                out.push_str(&format!("{}S", secs as u64));
            } else {
                out.push_str(&format!("{secs}S"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn parse(input: &str) -> Result<serde_json::Value, ToolError> {
        run_single(
            &DurationConvert,
            DataValue::Text(input.into()),
            &Options::new(),
        )
        .map(|v| {
            let DataValue::Json(v) = v else {
                unreachable!()
            };
            v
        })
    }

    #[test]
    fn all_three_forms() {
        let v = parse("1h 30m").unwrap();
        assert_eq!(v["seconds"], 5400.0);
        assert_eq!(v["human"], "1h 30m");
        assert_eq!(v["iso8601"], "PT1H30M");

        assert_eq!(parse("90m").unwrap()["seconds"], 5400.0);
        assert_eq!(parse("1:30:00").unwrap()["seconds"], 5400.0);
        assert_eq!(parse("5400").unwrap()["human"], "1h 30m");
        assert_eq!(parse("1.5h").unwrap()["seconds"], 5400.0);
        assert_eq!(parse("2d").unwrap()["iso8601"], "P2D");
        assert_eq!(parse("250ms").unwrap()["milliseconds"], 250);
        assert_eq!(parse("02:15").unwrap()["seconds"], 135.0);
    }

    #[test]
    fn junk_errors() {
        for bad in ["", "abc", "1x", "1:2:3:4", "-5", "h1"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
