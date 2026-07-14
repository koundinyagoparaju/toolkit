use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// Per-column summary of a CSV — row count, inferred types, numeric
/// ranges, empties, samples — so the shape of a big export fits in a
/// screenful (or a context window) instead of the whole file.
pub struct CsvStats;

const SAMPLE_CAP: usize = 3;

impl Tool for CsvStats {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "csv-stats".into(),
            label: "CSV Stats".into(),
            description: "Summarize a CSV per column: inferred type, numeric min/max, empty and \
                          distinct counts, sample values — the shape of a big export without \
                          reading it."
                .into(),
            keywords: [
                "csv", "stats", "summary", "columns", "types", "explore", "profile", "data",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Text,
                "name,age,city\nAda,36,London\nAlan,41,\nGrace,85,Arlington\n",
            ),
            output: DataType::Json,
            streaming: false,
            options: vec![
                OptionSpec::bool("headers", "First row is a header", "").default_value(true.into()),
                OptionSpec::string(
                    "delimiter",
                    "Delimiter",
                    "A single character, e.g. ; or \\t.",
                )
                .default_value(",".into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let headers = options.bool_opt("headers").unwrap_or(true);
        let delimiter = options.str_opt("delimiter").unwrap_or(",");
        let [delimiter] = delimiter.as_bytes() else {
            return Err(ToolError::new("delimiter must be a single character"));
        };
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(*delimiter)
            .has_headers(headers)
            .flexible(true)
            .from_reader(text.as_bytes());

        let names: Vec<String> = if headers {
            reader
                .headers()
                .map_err(|e| ToolError::new(format!("bad CSV: {e}")))?
                .iter()
                .map(String::from)
                .collect()
        } else {
            Vec::new()
        };

        let mut columns: Vec<Column> = names.into_iter().map(Column::new).collect();
        let mut rows = 0u64;
        for record in reader.records() {
            let record = record.map_err(|e| ToolError::new(format!("bad CSV: {e}")))?;
            rows += 1;
            for (i, field) in record.iter().enumerate() {
                if columns.len() <= i {
                    columns.push(Column::new(format!("column_{}", i + 1)));
                }
                columns[i].feed(field);
            }
        }
        Ok(DataValue::Json(serde_json::json!({
            "rows": rows,
            "columns": columns.iter().map(Column::report).collect::<Vec<_>>(),
        })))
    }
}

struct Column {
    name: String,
    non_empty: u64,
    empty: u64,
    numeric: u64,
    boolish: u64,
    min: Option<f64>,
    max: Option<f64>,
    distinct: std::collections::HashSet<String>,
    distinct_capped: bool,
    samples: Vec<String>,
}

impl Column {
    fn new(name: String) -> Self {
        Column {
            name,
            non_empty: 0,
            empty: 0,
            numeric: 0,
            boolish: 0,
            min: None,
            max: None,
            distinct: std::collections::HashSet::new(),
            distinct_capped: false,
            samples: Vec::new(),
        }
    }

    fn feed(&mut self, field: &str) {
        let field = field.trim();
        if field.is_empty() {
            self.empty += 1;
            return;
        }
        self.non_empty += 1;
        if let Ok(v) = field.parse::<f64>() {
            if v.is_finite() {
                self.numeric += 1;
                self.min = Some(self.min.map_or(v, |m| m.min(v)));
                self.max = Some(self.max.map_or(v, |m| m.max(v)));
            }
        }
        if matches!(
            field.to_ascii_lowercase().as_str(),
            "true" | "false" | "yes" | "no"
        ) {
            self.boolish += 1;
        }
        // Distinct counting caps at 1000 values so a high-cardinality
        // column can't balloon memory; past the cap we report ">1000".
        if !self.distinct_capped && !self.distinct.contains(field) {
            if self.distinct.len() >= 1000 {
                self.distinct_capped = true;
            } else {
                self.distinct.insert(field.to_string());
            }
        }
        if self.samples.len() < SAMPLE_CAP && !self.samples.iter().any(|s| s == field) {
            let sample: String = field.chars().take(40).collect();
            self.samples.push(sample);
        }
    }

    fn report(&self) -> serde_json::Value {
        let kind = if self.non_empty == 0 {
            "empty"
        } else if self.numeric == self.non_empty {
            "number"
        } else if self.boolish == self.non_empty {
            "boolean"
        } else {
            "text"
        };
        let mut out = serde_json::json!({
            "name": self.name,
            "type": kind,
            "non_empty": self.non_empty,
            "empty": self.empty,
            "distinct": if self.distinct_capped {
                serde_json::json!(">1000")
            } else {
                serde_json::json!(self.distinct.len())
            },
            "samples": self.samples,
        });
        if kind == "number" {
            out["min"] = serde_json::json!(self.min);
            out["max"] = serde_json::json!(self.max);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn stats(input: &str) -> serde_json::Value {
        let DataValue::Json(v) =
            run_single(&CsvStats, DataValue::Text(input.into()), &Options::new()).unwrap()
        else {
            unreachable!()
        };
        v
    }

    #[test]
    fn types_ranges_and_empties() {
        let v = stats("name,age,city\nAda,36,London\nAlan,41,\nGrace,85,Arlington\n");
        assert_eq!(v["rows"], 3);
        let cols = v["columns"].as_array().unwrap();
        assert_eq!(cols[0]["type"], "text");
        assert_eq!(cols[1]["type"], "number");
        assert_eq!(cols[1]["min"], 36.0);
        assert_eq!(cols[1]["max"], 85.0);
        assert_eq!(cols[2]["empty"], 1);
        assert_eq!(cols[0]["distinct"], 3);
    }

    #[test]
    fn headerless_and_boolean_columns() {
        let mut opts = Options::new();
        opts.insert("headers".into(), false.into());
        let DataValue::Json(v) = run_single(
            &CsvStats,
            DataValue::Text("true,1\nfalse,2\n".into()),
            &opts,
        )
        .unwrap() else {
            unreachable!()
        };
        let cols = v["columns"].as_array().unwrap();
        assert_eq!(cols[0]["name"], "column_1");
        assert_eq!(cols[0]["type"], "boolean");
        assert_eq!(cols[1]["type"], "number");
    }

    #[test]
    fn ragged_rows_extend_columns() {
        let v = stats("a,b\n1,2,3\n");
        assert_eq!(v["columns"].as_array().unwrap().len(), 3);
        assert_eq!(v["columns"][2]["name"], "column_3");
    }
}
