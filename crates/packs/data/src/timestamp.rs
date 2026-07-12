use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct TimestampConvert;

impl Tool for TimestampConvert {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "timestamp-convert".into(),
            label: "Timestamp Convert".into(),
            description: "Convert between unix timestamps (seconds/millis) and ISO 8601 dates — paste either form.".into(),
            keywords: ["timestamp", "unix", "epoch", "date", "time", "iso8601", "convert"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "1700000000"),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let input = text.trim();

        let datetime = if let Ok(n) = input.parse::<i64>() {
            // Magnitude heuristic: seconds < 1e11 < millis < 1e14 < micros.
            let (secs, nanos) = if n.abs() < 100_000_000_000 {
                (n, 0)
            } else if n.abs() < 100_000_000_000_000 {
                (n.div_euclid(1000), (n.rem_euclid(1000) * 1_000_000) as u32)
            } else {
                (
                    n.div_euclid(1_000_000),
                    (n.rem_euclid(1_000_000) * 1000) as u32,
                )
            };
            OffsetDateTime::from_unix_timestamp(secs)
                .map_err(|_| ToolError::new("timestamp out of range"))?
                + time::Duration::nanoseconds(nanos as i64)
        } else {
            OffsetDateTime::parse(input, &Rfc3339).map_err(|_| {
                ToolError::new(
                    "expected a unix timestamp or an ISO 8601 / RFC 3339 date \
                     like 2026-07-10T12:00:00Z",
                )
            })?
        };

        let utc = datetime.to_offset(time::UtcOffset::UTC);
        Ok(DataValue::Json(serde_json::json!({
            "unix": utc.unix_timestamp(),
            "unix_ms": (utc.unix_timestamp_nanos() / 1_000_000) as i64,
            "iso8601": utc.format(&Rfc3339).map_err(|e| ToolError::new(e.to_string()))?,
            "weekday": utc.weekday().to_string(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn convert(s: &str) -> serde_json::Value {
        let out = run_single(
            &TimestampConvert,
            DataValue::Text(s.into()),
            &Options::new(),
        )
        .unwrap();
        let DataValue::Json(v) = out else { panic!() };
        v
    }

    #[test]
    fn unix_seconds_millis_and_iso_agree() {
        let v = convert("86400");
        assert_eq!(v["iso8601"], "1970-01-02T00:00:00Z");
        assert_eq!(v["weekday"], "Friday");

        // Modern date: seconds and millis forms round-trip to the same ISO.
        let iso = convert("2026-07-10T12:34:56Z");
        for numeric in [iso["unix"].to_string(), iso["unix_ms"].to_string()] {
            assert_eq!(convert(&numeric)["iso8601"], "2026-07-10T12:34:56Z");
        }
    }

    #[test]
    fn offset_normalized_to_utc() {
        let v = convert("2026-07-10T05:30:00+05:30");
        assert_eq!(v["iso8601"], "2026-07-10T00:00:00Z");
    }

    #[test]
    fn garbage_errors() {
        assert!(run_single(
            &TimestampConvert,
            DataValue::Text("yesterday".into()),
            &Options::new()
        )
        .is_err());
    }
}
