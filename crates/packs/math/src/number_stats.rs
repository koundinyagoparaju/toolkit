use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Descriptive statistics over a plain list of numbers — csv-stats'
/// little sibling for when the data isn't a table.
pub struct NumberStats;

impl Tool for NumberStats {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "number-stats".into(),
            label: "Number Stats".into(),
            description: "Descriptive statistics for a list of numbers (whitespace-, comma-, or \
                          newline-separated): count, sum, min/max, mean, median, standard \
                          deviation, and percentiles."
                .into(),
            keywords: [
                "stats",
                "statistics",
                "mean",
                "median",
                "stddev",
                "percentile",
                "average",
                "numbers",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "3 1 4 1 5 9 2 6"),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let mut values = Vec::new();
        for raw in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
            if raw.is_empty() {
                continue;
            }
            let v: f64 = raw
                .parse()
                .map_err(|_| ToolError::new(format!("\"{raw}\" is not a number")))?;
            if !v.is_finite() {
                return Err(ToolError::new(format!("\"{raw}\" is not a finite number")));
            }
            values.push(v);
        }
        if values.is_empty() {
            return Err(ToolError::new("no numbers in the input"));
        }

        let n = values.len();
        let sum: f64 = values.iter().sum();
        let mean = sum / n as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;

        let mut sorted = values;
        sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite"));

        let mut out = serde_json::json!({
            "count": n,
            "sum": sum,
            "min": sorted[0],
            "max": sorted[n - 1],
            "mean": mean,
            "median": percentile(&sorted, 50.0),
            "stddev": variance.sqrt(),
        });
        // Sample stddev (n-1) needs at least two values.
        if n > 1 {
            out["sample_stddev"] =
                serde_json::json!((variance * n as f64 / (n as f64 - 1.0)).sqrt());
        }
        out["percentiles"] = serde_json::json!({
            "p25": percentile(&sorted, 25.0),
            "p75": percentile(&sorted, 75.0),
            "p90": percentile(&sorted, 90.0),
            "p99": percentile(&sorted, 99.0),
        });
        Ok(DataValue::Json(out))
    }
}

/// Linear interpolation between closest ranks (the common "type 7").
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = p / 100.0 * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (rank - lo as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn stats(input: &str) -> serde_json::Value {
        let DataValue::Json(v) =
            run_single(&NumberStats, DataValue::Text(input.into()), &Options::new()).unwrap()
        else {
            unreachable!()
        };
        v
    }

    #[test]
    fn known_vectors() {
        // Classic population-stddev example: mean 5, stddev 2.
        let v = stats("2, 4, 4, 4, 5, 5, 7, 9");
        assert_eq!(v["count"], 8);
        assert_eq!(v["mean"], 5.0);
        assert_eq!(v["stddev"], 2.0);
        assert_eq!(v["median"], 4.5);
        assert_eq!(v["min"], 2.0);
        assert_eq!(v["max"], 9.0);

        // Odd count: exact middle; p25 interpolates.
        let v = stats("1\n2\n3\n4\n5");
        assert_eq!(v["median"], 3.0);
        assert_eq!(v["percentiles"]["p25"], 2.0);

        // A single value has no sample stddev.
        let v = stats("42");
        assert_eq!(v["stddev"], 0.0);
        assert!(v.get("sample_stddev").is_none());
    }

    #[test]
    fn junk_errors() {
        assert!(run_single(
            &NumberStats,
            DataValue::Text("1 two 3".into()),
            &Options::new()
        )
        .is_err());
        assert!(run_single(&NumberStats, DataValue::Text("  ".into()), &Options::new()).is_err());
    }
}
