use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct SortLines;

impl Tool for SortLines {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "sort-lines".into(),
            label: "Sort Lines".into(),
            description: "Sort lines of text; optionally numeric, reversed, and de-duplicated."
                .into(),
            keywords: ["sort", "lines", "unique", "dedupe", "uniq", "order"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::bool("unique", "Remove duplicates", "").default_value(false.into()),
                OptionSpec::bool("numeric", "Numeric sort", "Compare by leading number.")
                    .default_value(false.into()),
                OptionSpec::bool("reverse", "Reverse order", "").default_value(false.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let mut lines: Vec<&str> = text.lines().collect();
        if options.bool_opt("numeric").unwrap_or(false) {
            lines.sort_by(|a, b| {
                let na = leading_number(a);
                let nb = leading_number(b);
                na.partial_cmp(&nb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.cmp(b))
            });
        } else {
            lines.sort_unstable();
        }
        if options.bool_opt("unique").unwrap_or(false) {
            lines.dedup();
        }
        if options.bool_opt("reverse").unwrap_or(false) {
            lines.reverse();
        }
        Ok(DataValue::Text(lines.join("\n")))
    }
}

fn leading_number(s: &str) -> f64 {
    let t = s.trim_start();
    let end = t
        .char_indices()
        .take_while(|(i, c)| c.is_ascii_digit() || *c == '.' || (*i == 0 && *c == '-'))
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    t[..end].parse().unwrap_or(f64::NEG_INFINITY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::run_single;

    fn sort(text: &str, opts: serde_json::Value) -> String {
        let out = run_single(
            &SortLines,
            DataValue::Text(text.into()),
            opts.as_object().unwrap(),
        )
        .unwrap();
        let DataValue::Text(s) = out else { panic!() };
        s
    }

    #[test]
    fn sorts_uniques_reverses() {
        assert_eq!(sort("b\na\nb\nc", json!({"unique": true})), "a\nb\nc");
        assert_eq!(sort("a\nc\nb", json!({"reverse": true})), "c\nb\na");
    }

    #[test]
    fn numeric_sort() {
        assert_eq!(
            sort("10 x\n2 y\n1 z", json!({"numeric": true})),
            "1 z\n2 y\n10 x"
        );
    }
}
