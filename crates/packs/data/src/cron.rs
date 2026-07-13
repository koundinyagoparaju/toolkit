use std::collections::BTreeSet;
use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime, Time};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// Explain a cron expression and compute its next occurrences from a
/// given reference time. The clock arrives as an explicit option (like
/// entropy arrives on a port), so the tool stays a pure function — and
/// the answer is exact instead of guessed.
pub struct CronExplain;

const MONTH_NAMES: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];
const DAY_NAMES: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

impl Tool for CronExplain {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "cron-explain".into(),
            label: "Cron Explain".into(),
            description: "Explain a cron expression (5 fields or @daily-style aliases) and list \
                          its next occurrences from a reference time (UTC). Names, ranges, \
                          steps, and the standard day-of-month/day-of-week OR rule."
                .into(),
            keywords: [
                "cron",
                "crontab",
                "schedule",
                "explain",
                "next",
                "occurrence",
                "timer",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "*/15 9-17 * * MON-FRI"),
            output: DataType::Json,
            streaming: false,
            options: vec![
                OptionSpec::string(
                    "from",
                    "From",
                    "Reference time: RFC 3339 (2026-01-01T00:00:00Z) or a unix timestamp. \
                     Occurrences are computed strictly after this instant, in UTC.",
                )
                .default_value("2026-01-01T00:00:00Z".into()),
                OptionSpec::integer(
                    "count",
                    "Count",
                    "How many occurrences to list.",
                    Some(1),
                    Some(20),
                )
                .default_value(5.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let expression = text.trim();
        let spec = CronSpec::parse(expression)?;
        let from = parse_from(options.str_opt("from").unwrap_or("2026-01-01T00:00:00Z"))?;
        let count = options.u32_opt("count").unwrap_or(5) as usize;
        let next = spec.occurrences_after(from, count)?;
        Ok(DataValue::Json(serde_json::json!({
            "expression": expression,
            "description": spec.describe(),
            "next": next
                .iter()
                .map(|t| t.format(&Rfc3339).expect("UTC datetimes format"))
                .collect::<Vec<_>>(),
        })))
    }
}

fn parse_from(from: &str) -> Result<OffsetDateTime, ToolError> {
    let from = from.trim();
    if let Ok(secs) = from.parse::<i64>() {
        return OffsetDateTime::from_unix_timestamp(secs)
            .map_err(|_| ToolError::new(format!("timestamp {secs} is out of range")));
    }
    OffsetDateTime::parse(from, &Rfc3339)
        .map(|t| t.to_offset(time::UtcOffset::UTC))
        .map_err(|_| ToolError::new(format!("\"{from}\" is not RFC 3339 or a unix timestamp")))
}

struct Field {
    values: BTreeSet<u8>,
    any: bool, // written as "*" (matters for the dom/dow OR rule)
    raw: String,
}

struct CronSpec {
    minute: Field,
    hour: Field,
    dom: Field,
    month: Field,
    dow: Field,
}

impl CronSpec {
    fn parse(expression: &str) -> Result<CronSpec, ToolError> {
        let expression = match expression {
            "@yearly" | "@annually" => "0 0 1 1 *",
            "@monthly" => "0 0 1 * *",
            "@weekly" => "0 0 * * 0",
            "@daily" | "@midnight" => "0 0 * * *",
            "@hourly" => "0 * * * *",
            other => other,
        };
        let fields: Vec<&str> = expression.split_whitespace().collect();
        let [minute, hour, dom, month, dow] = fields.as_slice() else {
            return Err(ToolError::new(format!(
                "expected 5 fields (minute hour day-of-month month day-of-week) or an alias \
                 like @daily, got {}",
                fields.len()
            )));
        };
        Ok(CronSpec {
            minute: parse_field(minute, 0, 59, None)?,
            hour: parse_field(hour, 0, 23, None)?,
            dom: parse_field(dom, 1, 31, None)?,
            month: parse_field(month, 1, 12, Some(&MONTH_NAMES))?,
            dow: parse_field(dow, 0, 7, Some(&DAY_NAMES)).map(|mut f| {
                // 7 is Sunday too.
                if f.values.remove(&7) {
                    f.values.insert(0);
                }
                f
            })?,
        })
    }

    /// The standard rule: when both day fields are restricted, a day
    /// matches if EITHER does; otherwise the restricted one decides.
    fn day_matches(&self, date: Date) -> bool {
        let dom_ok = self.dom.values.contains(&(date.day()));
        let dow_ok = self
            .dow
            .values
            .contains(&(date.weekday().number_days_from_sunday()));
        match (self.dom.any, self.dow.any) {
            (false, false) => dom_ok || dow_ok,
            (false, true) => dom_ok,
            (true, false) => dow_ok,
            (true, true) => true,
        }
    }

    fn occurrences_after(
        &self,
        from: OffsetDateTime,
        count: usize,
    ) -> Result<Vec<OffsetDateTime>, ToolError> {
        let mut out = Vec::new();
        let mut date = from.date();
        // Five years bounds pathological specs like "0 0 30 2 *".
        for _ in 0..(366 * 5) {
            if self.month.values.contains(&(date.month() as u8)) && self.day_matches(date) {
                for &h in &self.hour.values {
                    for &m in &self.minute.values {
                        let t = date
                            .with_time(Time::from_hms(h, m, 0).expect("validated ranges"))
                            .assume_utc();
                        if t > from {
                            out.push(t);
                            if out.len() == count {
                                return Ok(out);
                            }
                        }
                    }
                }
            }
            date = date
                .next_day()
                .ok_or_else(|| ToolError::new("date overflow while searching"))?;
        }
        if out.is_empty() {
            return Err(ToolError::new(
                "no occurrence within 5 years — this expression may never match \
                 (e.g. day 30 in February)",
            ));
        }
        Ok(out)
    }

    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.minute.any && self.hour.any {
            parts.push("every minute".to_string());
        } else if self.minute.any {
            parts.push(format!("every minute of hour {}", self.hour.raw));
        } else if self.hour.any {
            parts.push(format!("at minute {} of every hour", self.minute.raw));
        } else {
            parts.push(format!(
                "at minute {} past hour {}",
                self.minute.raw, self.hour.raw
            ));
        }
        match (self.dom.any, self.dow.any) {
            (false, false) => parts.push(format!(
                "on day-of-month {} or {}",
                self.dom.raw,
                name_set(&self.dow.values, &DAY_NAMES)
            )),
            (false, true) => parts.push(format!("on day-of-month {}", self.dom.raw)),
            (true, false) => parts.push(format!("on {}", name_set(&self.dow.values, &DAY_NAMES))),
            (true, true) => {}
        }
        if !self.month.any {
            parts.push(format!("in {}", name_set(&self.month.values, &MONTH_NAMES)));
        }
        parts.join(", ")
    }
}

/// Render a value set as names, e.g. {1,2,3} of DAY_NAMES -> "MON-WED".
fn name_set(values: &BTreeSet<u8>, names: &[&str]) -> String {
    let offset = if names.len() == 12 { 1 } else { 0 }; // months are 1-based
    let list: Vec<u8> = values.iter().copied().collect();
    let contiguous = list.windows(2).all(|w| w[1] == w[0] + 1);
    let name = |v: u8| names[(v - offset) as usize].to_string();
    if contiguous && list.len() > 2 {
        format!(
            "{}-{}",
            name(list[0]),
            name(*list.last().expect("non-empty"))
        )
    } else {
        list.iter().map(|&v| name(v)).collect::<Vec<_>>().join(",")
    }
}

fn parse_field(raw: &str, min: u8, max: u8, names: Option<&[&str]>) -> Result<Field, ToolError> {
    let err = |msg: String| ToolError::new(format!("field \"{raw}\": {msg}"));
    let resolve = |token: &str| -> Result<u8, ToolError> {
        if let Some(names) = names {
            if let Some(i) = names.iter().position(|n| n.eq_ignore_ascii_case(token)) {
                return Ok(i as u8 + if names.len() == 12 { 1 } else { 0 });
            }
        }
        token
            .parse::<u8>()
            .ok()
            .filter(|v| (min..=max).contains(v))
            .ok_or_else(|| err(format!("\"{token}\" is not a value in {min}-{max}")))
    };

    let mut values = BTreeSet::new();
    for part in raw.split(',') {
        let (base, step) = match part.split_once('/') {
            Some((b, s)) => (
                b,
                s.parse::<u8>()
                    .ok()
                    .filter(|&s| s > 0)
                    .ok_or_else(|| err(format!("\"/{s}\" is not a valid step")))?,
            ),
            None => (part, 1),
        };
        let (lo, hi) = if base == "*" {
            (min, max)
        } else if let Some((a, b)) = base.split_once('-') {
            (resolve(a)?, resolve(b)?)
        } else {
            let v = resolve(base)?;
            // A bare value with a step ("5/15") ranges to the max.
            if part.contains('/') {
                (v, max)
            } else {
                (v, v)
            }
        };
        if lo > hi {
            return Err(err(format!("range {lo}-{hi} is backwards")));
        }
        let mut v = lo;
        while v <= hi {
            values.insert(v);
            match v.checked_add(step) {
                Some(n) => v = n,
                None => break,
            }
        }
    }
    Ok(Field {
        values,
        any: raw == "*",
        raw: raw.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn explain(expr: &str, from: &str, count: i64) -> Result<serde_json::Value, ToolError> {
        let mut opts = Options::new();
        opts.insert("from".into(), from.into());
        opts.insert("count".into(), count.into());
        run_single(&CronExplain, DataValue::Text(expr.into()), &opts).map(|v| {
            let DataValue::Json(v) = v else {
                unreachable!()
            };
            v
        })
    }

    #[test]
    fn business_hours_quarter_past() {
        let v = explain("*/15 9-17 * * MON-FRI", "2026-01-01T00:00:00Z", 3).unwrap();
        // 2026-01-01 is a Thursday.
        assert_eq!(v["next"][0], "2026-01-01T09:00:00Z");
        assert_eq!(v["next"][1], "2026-01-01T09:15:00Z");
        assert_eq!(
            v["description"],
            "at minute */15 past hour 9-17, on MON-FRI"
        );
    }

    #[test]
    fn aliases_names_and_weekend_skip() {
        let v = explain("@daily", "2026-01-01T12:00:00Z", 1).unwrap();
        assert_eq!(v["next"][0], "2026-01-02T00:00:00Z");

        // 2026-01-02 is a Friday; next Monday is the 5th.
        let v = explain("0 0 * * MON", "2026-01-02T00:00:00Z", 1).unwrap();
        assert_eq!(v["next"][0], "2026-01-05T00:00:00Z");

        let v = explain("0 12 1 JAN *", "2026-02-01T00:00:00Z", 1).unwrap();
        assert_eq!(v["next"][0], "2027-01-01T12:00:00Z");
    }

    #[test]
    fn dom_dow_or_rule() {
        // Both restricted: the 15th OR any Sunday. From Jan 2 2026 (Fri),
        // the first Sunday (Jan 4) comes before the 15th.
        let v = explain("0 0 15 * SUN", "2026-01-02T00:00:00Z", 2).unwrap();
        assert_eq!(v["next"][0], "2026-01-04T00:00:00Z");
        assert_eq!(v["next"][1], "2026-01-11T00:00:00Z");
    }

    #[test]
    fn unix_reference_sunday_alias_and_leap_day() {
        // 1767225600 = 2026-01-01T00:00:00Z.
        let v = explain("0 0 * * 7", "1767225600", 1).unwrap();
        assert_eq!(v["next"][0], "2026-01-04T00:00:00Z");

        let v = explain("0 0 29 2 *", "2026-01-01T00:00:00Z", 1).unwrap();
        assert_eq!(v["next"][0], "2028-02-29T00:00:00Z");
    }

    #[test]
    fn impossible_and_malformed_error() {
        assert!(explain("0 0 30 2 *", "2026-01-01T00:00:00Z", 1).is_err());
        assert!(explain("not cron", "2026-01-01T00:00:00Z", 1).is_err());
        assert!(explain("61 * * * *", "2026-01-01T00:00:00Z", 1).is_err());
        assert!(explain("* * * * *", "not a time", 1).is_err());
    }
}
