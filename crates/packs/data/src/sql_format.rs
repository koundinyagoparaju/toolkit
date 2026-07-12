use sqlformat::{FormatOptions, Indent, QueryParams};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// Pretty-print SQL — indentation and keyword casing, offline, so a query
/// with real table and column names never goes to a website.
pub struct SqlFormat;

impl Tool for SqlFormat {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "sql-format".into(),
            label: "SQL Format".into(),
            description: "Reformat a SQL query with consistent indentation and keyword casing."
                .into(),
            keywords: ["sql", "format", "beautify", "pretty", "query", "indent"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Text,
                "select id,name from users where active=1 order by name",
            ),
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::integer(
                    "indent",
                    "Indent width",
                    "Spaces per indent level (0 uses tabs).",
                    Some(0),
                    Some(8),
                )
                .default_value(2.into()),
                OptionSpec::enumeration(
                    "keywords",
                    "Keyword case",
                    "How to case SQL keywords.",
                    &["upper", "lower", "preserve"],
                )
                .default_value("upper".into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(sql) = inputs.sole() else {
            unreachable!()
        };
        let indent = options.u32_opt("indent").unwrap_or(2);
        let uppercase = match options.str_opt("keywords").unwrap_or("upper") {
            "upper" => Some(true),
            "lower" => Some(false),
            _ => None,
        };
        let opts = FormatOptions {
            indent: if indent == 0 {
                Indent::Tabs
            } else {
                Indent::Spaces(indent as u8)
            },
            uppercase,
            ..FormatOptions::default()
        };
        Ok(DataValue::Text(sqlformat::format(
            &sql,
            &QueryParams::None,
            &opts,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn format(sql: &str, keywords: &str) -> String {
        let mut opts = Options::new();
        opts.insert("keywords".into(), keywords.into());
        let DataValue::Text(out) =
            run_single(&SqlFormat, DataValue::Text(sql.into()), &opts).unwrap()
        else {
            unreachable!()
        };
        out
    }

    #[test]
    fn formats_and_cases_keywords() {
        let out = format("select id, name from users where id=1", "upper");
        assert!(out.contains("SELECT"), "{out}");
        assert!(out.contains("FROM"));
        assert!(out.contains('\n')); // multi-line
        assert!(format("SELECT 1", "lower").contains("select"));
    }
}
