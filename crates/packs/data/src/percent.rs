use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// The everyday percentage forms people (and models) fumble: X% of Y,
/// what percent X is of Y, and percent change from X to Y.
pub struct PercentCalc;

impl Tool for PercentCalc {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "percent-calc".into(),
            label: "Percent Calculator".into(),
            description: "Percentages from two numbers \"a b\": a% of b (percent-of), a as a \
                          percent of b (what-percent), or the percent change from a to b \
                          (change)."
                .into(),
            keywords: [
                "percent",
                "percentage",
                "of",
                "change",
                "increase",
                "decrease",
                "ratio",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "15 80"),
            output: DataType::Json,
            streaming: false,
            options: vec![OptionSpec::enumeration(
                "operation",
                "Operation",
                "percent-of: a% of b. what-percent: a as a percent of b. change: percent \
                 change from a to b.",
                &["percent-of", "what-percent", "change"],
            )
            .default_value("percent-of".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let parts: Vec<&str> = text
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .collect();
        let [a, b] = parts.as_slice() else {
            return Err(ToolError::new("input must be two numbers, e.g. \"15 80\""));
        };
        let parse = |s: &str| -> Result<f64, ToolError> {
            s.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| ToolError::new(format!("\"{s}\" is not a number")))
        };
        let (a, b) = (parse(a)?, parse(b)?);

        let (result, explanation) = match options.str_opt("operation").unwrap_or("percent-of") {
            "what-percent" => {
                if b == 0.0 {
                    return Err(ToolError::new("cannot take a percent of zero"));
                }
                let r = a / b * 100.0;
                (r, format!("{a} is {r}% of {b}"))
            }
            "change" => {
                if a == 0.0 {
                    return Err(ToolError::new("percent change from zero is undefined"));
                }
                let r = (b - a) / a * 100.0;
                (r, format!("{a} -> {b} is a {r}% change"))
            }
            _ => {
                let r = a / 100.0 * b;
                (r, format!("{a}% of {b} is {r}"))
            }
        };
        Ok(DataValue::Json(serde_json::json!({
            "result": result,
            "explanation": explanation,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn pct(input: &str, op: &str) -> Result<serde_json::Value, ToolError> {
        let mut opts = Options::new();
        opts.insert("operation".into(), op.into());
        run_single(&PercentCalc, DataValue::Text(input.into()), &opts).map(|v| {
            let DataValue::Json(v) = v else {
                unreachable!()
            };
            v
        })
    }

    #[test]
    fn three_operations() {
        assert_eq!(pct("15 80", "percent-of").unwrap()["result"], 12.0);
        assert_eq!(pct("12 80", "what-percent").unwrap()["result"], 15.0);
        assert_eq!(pct("80 100", "change").unwrap()["result"], 25.0);
        assert_eq!(pct("100 80", "change").unwrap()["result"], -20.0);
        assert_eq!(
            pct("15 80", "percent-of").unwrap()["explanation"],
            "15% of 80 is 12"
        );
    }

    #[test]
    fn junk_errors() {
        assert!(pct("15", "percent-of").is_err());
        assert!(pct("a b", "percent-of").is_err());
        assert!(pct("5 0", "what-percent").is_err());
        assert!(pct("0 5", "change").is_err());
    }
}
