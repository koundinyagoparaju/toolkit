use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct CsvToJson;

impl Tool for CsvToJson {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "csv-to-json".into(),
            label: "CSV → JSON".into(),
            description: "Parse CSV into a JSON array (objects keyed by the header row, or arrays without one).".into(),
            keywords: ["csv", "json", "convert", "table", "spreadsheet"].map(String::from).to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "name,age\nAda,36\nAlan,41\n"),
            output: DataType::Json,
            streaming: false,
            options: vec![
                OptionSpec::bool("headers", "First row is a header", "")
                    .default_value(true.into()),
                OptionSpec::string("delimiter", "Delimiter", "A single character, e.g. ; or \\t.")
                    .default_value(",".into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let delimiter = parse_delimiter(options.str_opt("delimiter").unwrap_or(","))?;
        let headers = options.bool_opt("headers").unwrap_or(true);
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .has_headers(headers)
            .flexible(true)
            .from_reader(text.as_bytes());

        let mut rows = Vec::new();
        if headers {
            let header: Vec<String> = reader
                .headers()
                .map_err(|e| ToolError::new(format!("invalid CSV: {e}")))?
                .iter()
                .map(String::from)
                .collect();
            for record in reader.records() {
                let record = record.map_err(|e| ToolError::new(format!("invalid CSV: {e}")))?;
                let mut obj = serde_json::Map::new();
                for (i, field) in record.iter().enumerate() {
                    let key = header
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("column_{i}"));
                    obj.insert(key, serde_json::Value::String(field.to_string()));
                }
                rows.push(serde_json::Value::Object(obj));
            }
        } else {
            for record in reader.records() {
                let record = record.map_err(|e| ToolError::new(format!("invalid CSV: {e}")))?;
                rows.push(serde_json::Value::Array(
                    record
                        .iter()
                        .map(|f| serde_json::Value::String(f.to_string()))
                        .collect(),
                ));
            }
        }
        Ok(DataValue::Json(serde_json::Value::Array(rows)))
    }
}

pub struct JsonToCsv;

impl Tool for JsonToCsv {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "json-to-csv".into(),
            label: "JSON → CSV".into(),
            description: "Convert a JSON array of objects to CSV (columns in first-seen order)."
                .into(),
            keywords: ["json", "csv", "convert", "table", "export"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Json,
                r#"[{"name":"Ada","age":36},{"name":"Alan","age":41}]"#,
            ),
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::string("delimiter", "Delimiter", "").default_value(",".into())
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Json(value) = inputs.sole() else {
            unreachable!()
        };
        let delimiter = parse_delimiter(options.str_opt("delimiter").unwrap_or(","))?;
        let serde_json::Value::Array(rows) = value else {
            return Err(ToolError::new("input must be a JSON array of objects"));
        };
        // Columns in first-seen order across all rows.
        let mut columns: Vec<String> = Vec::new();
        for row in &rows {
            let serde_json::Value::Object(obj) = row else {
                return Err(ToolError::new("every array element must be an object"));
            };
            for key in obj.keys() {
                if !columns.iter().any(|c| c == key) {
                    columns.push(key.clone());
                }
            }
        }
        let mut writer = csv::WriterBuilder::new()
            .delimiter(delimiter)
            .from_writer(Vec::new());
        writer
            .write_record(&columns)
            .map_err(|e| ToolError::new(e.to_string()))?;
        for row in &rows {
            let obj = row.as_object().expect("checked above");
            let record: Vec<String> = columns
                .iter()
                .map(|c| match obj.get(c) {
                    None | Some(serde_json::Value::Null) => String::new(),
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(other) => other.to_string(),
                })
                .collect();
            writer
                .write_record(&record)
                .map_err(|e| ToolError::new(e.to_string()))?;
        }
        let bytes = writer
            .into_inner()
            .map_err(|e| ToolError::new(e.to_string()))?;
        Ok(DataValue::Text(
            String::from_utf8(bytes).expect("csv writer emits UTF-8"),
        ))
    }
}

fn parse_delimiter(s: &str) -> Result<u8, ToolError> {
    let unescaped = if s == "\\t" { "\t" } else { s };
    let bytes = unescaped.as_bytes();
    if bytes.len() != 1 {
        return Err(ToolError::new("delimiter must be a single character"));
    }
    Ok(bytes[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn csv_json_round_trip() {
        let csv_in = DataValue::Text("name,age\nada,36\ngrace,45".into());
        let json = run_single(&CsvToJson, csv_in, &Options::new()).unwrap();
        assert_eq!(
            json,
            DataValue::Json(serde_json::json!([
                {"name": "ada", "age": "36"},
                {"name": "grace", "age": "45"}
            ]))
        );
        let back = run_single(&JsonToCsv, json, &Options::new()).unwrap();
        let DataValue::Text(t) = back else { panic!() };
        assert!(t.starts_with("name,age\n"));
        assert!(t.contains("ada,36"));
    }

    #[test]
    fn union_of_columns() {
        let json = DataValue::Json(serde_json::json!([{"a": 1}, {"b": "x"}]));
        let out = run_single(&JsonToCsv, json, &Options::new()).unwrap();
        let DataValue::Text(t) = out else { panic!() };
        assert!(t.starts_with("a,b\n"));
        assert!(t.contains("1,\n") && t.contains(",x"));
    }
}
