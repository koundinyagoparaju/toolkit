use crate::manifest::{Manifest, OptionKind, OptionSpec};
use crate::tool::ToolError;
use serde_json::Value;

/// Option values for one tool invocation, keyed by `OptionSpec::name`.
pub type Options = serde_json::Map<String, Value>;

/// Validate `opts` against the manifest's option specs and return a
/// normalized copy: defaults filled in, unknown keys and bad values rejected.
pub fn validate_options(manifest: &Manifest, opts: &Options) -> Result<Options, ToolError> {
    validate_against_specs(
        &manifest.options,
        opts,
        &format!("tool \"{}\"", manifest.name),
    )
}

/// Same validation against a bare spec list — used for tool options (above)
/// and for declared chain parameters.
pub fn validate_against_specs(
    specs: &[OptionSpec],
    opts: &Options,
    context: &str,
) -> Result<Options, ToolError> {
    for key in opts.keys() {
        if !specs.iter().any(|s| &s.name == key) {
            return Err(ToolError::new(format!(
                "unknown option \"{key}\" for {context}"
            )));
        }
    }

    let mut normalized = Options::new();
    for spec in specs {
        match opts.get(&spec.name) {
            Some(value) => {
                check_value(spec, value)?;
                normalized.insert(spec.name.clone(), value.clone());
            }
            None => match (&spec.default, spec.required) {
                (Some(default), _) => {
                    normalized.insert(spec.name.clone(), default.clone());
                }
                (None, true) => {
                    return Err(ToolError::new(format!(
                        "missing required option \"{}\"",
                        spec.name
                    )));
                }
                (None, false) => {}
            },
        }
    }
    Ok(normalized)
}

fn check_value(spec: &OptionSpec, value: &Value) -> Result<(), ToolError> {
    let fail = |expected: &str| {
        Err(ToolError::new(format!(
            "option \"{}\" expects {expected}, got {value}",
            spec.name
        )))
    };
    match &spec.kind {
        OptionKind::String => {
            if !value.is_string() {
                return fail("a string");
            }
        }
        OptionKind::Bool => {
            if !value.is_boolean() {
                return fail("a boolean");
            }
        }
        OptionKind::Integer { min, max } => {
            let Some(n) = value.as_i64() else {
                return fail("an integer");
            };
            if min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
                return Err(ToolError::new(format!(
                    "option \"{}\" must be in range {}..={}, got {n}",
                    spec.name,
                    min.map_or("-inf".into(), |m| m.to_string()),
                    max.map_or("inf".into(), |m| m.to_string()),
                )));
            }
        }
        OptionKind::Float { min, max } => {
            let Some(n) = value.as_f64() else {
                return fail("a number");
            };
            if min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
                return Err(ToolError::new(format!(
                    "option \"{}\" out of range, got {n}",
                    spec.name
                )));
            }
        }
        OptionKind::Enum { values } => {
            let ok = value
                .as_str()
                .is_some_and(|s| values.iter().any(|v| v == s));
            if !ok {
                return fail(&format!("one of {values:?}"));
            }
        }
    }
    Ok(())
}

/// Typed accessors for tools reading their (already validated) options.
pub trait OptGet {
    fn str_opt(&self, name: &str) -> Option<&str>;
    fn i64_opt(&self, name: &str) -> Option<i64>;
    fn f64_opt(&self, name: &str) -> Option<f64>;
    fn bool_opt(&self, name: &str) -> Option<bool>;
    fn u32_opt(&self, name: &str) -> Option<u32> {
        self.i64_opt(name).and_then(|n| u32::try_from(n).ok())
    }
}

impl OptGet for Options {
    fn str_opt(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(Value::as_str)
    }
    fn i64_opt(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(Value::as_i64)
    }
    fn f64_opt(&self, name: &str) -> Option<f64> {
        self.get(name).and_then(Value::as_f64)
    }
    fn bool_opt(&self, name: &str) -> Option<bool> {
        self.get(name).and_then(Value::as_bool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::DataType;
    use crate::manifest::InputSpec;
    use serde_json::json;

    fn manifest() -> Manifest {
        Manifest {
            name: "t".into(),
            label: "T".into(),
            description: String::new(),
            keywords: vec![],
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::integer("width", "Width", "", Some(1), Some(10_000)).required(),
                OptionSpec::enumeration("mode", "Mode", "", &["fit", "exact"])
                    .default_value(json!("fit")),
            ],
        }
    }

    #[test]
    fn fills_defaults_and_keeps_given_values() {
        let mut opts = Options::new();
        opts.insert("width".into(), json!(200));
        let normalized = validate_options(&manifest(), &opts).unwrap();
        assert_eq!(normalized.i64_opt("width"), Some(200));
        assert_eq!(normalized.str_opt("mode"), Some("fit"));
    }

    #[test]
    fn rejects_unknown_missing_and_out_of_range() {
        let m = manifest();
        let mut unknown = Options::new();
        unknown.insert("nope".into(), json!(1));
        assert!(validate_options(&m, &unknown).is_err());

        assert!(validate_options(&m, &Options::new()).is_err()); // missing required

        let mut range = Options::new();
        range.insert("width".into(), json!(0));
        assert!(validate_options(&m, &range).is_err());

        let mut bad_enum = Options::new();
        bad_enum.insert("width".into(), json!(5));
        bad_enum.insert("mode".into(), json!("stretch"));
        assert!(validate_options(&m, &bad_enum).is_err());
    }
}
