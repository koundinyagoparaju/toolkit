use semver::{Version, VersionReq};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// Parse a semantic version into its parts and (optionally) test it
/// against a requirement — the "does 1.2.3 satisfy ^1.1?" question that
/// gets guessed wrong constantly, answered by the same `semver` crate
/// cargo itself uses.
pub struct SemverCheck;

impl Tool for SemverCheck {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "semver-check".into(),
            label: "Semver Check".into(),
            description: "Parse a semantic version into major/minor/patch/pre-release/build, \
                          and test it against a requirement (`^1.2`, `>=1, <2`, `~1.2.3`) using \
                          cargo's own semver rules."
                .into(),
            keywords: [
                "semver",
                "version",
                "requirement",
                "satisfies",
                "compare",
                "range",
                "cargo",
                "dependency",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "1.2.3-beta.1+build.5"),
            output: DataType::Json,
            streaming: false,
            options: vec![OptionSpec::string(
                "requirement",
                "Requirement",
                "Version requirement in cargo syntax (`^1.2`, `~1.2.3`, `>=1, <2`). \
                 Empty just parses the version.",
            )
            .default_value("".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let raw = text.trim().trim_start_matches(['v', 'V']);
        let version = Version::parse(raw)
            .map_err(|e| ToolError::new(format!("\"{raw}\" is not a semantic version: {e}")))?;
        let mut out = serde_json::json!({
            "version": version.to_string(),
            "major": version.major,
            "minor": version.minor,
            "patch": version.patch,
        });
        if !version.pre.is_empty() {
            out["pre_release"] = serde_json::json!(version.pre.as_str());
        }
        if !version.build.is_empty() {
            out["build"] = serde_json::json!(version.build.as_str());
        }
        let requirement = options.str_opt("requirement").unwrap_or("").trim();
        if !requirement.is_empty() {
            let req = VersionReq::parse(requirement).map_err(|e| {
                ToolError::new(format!(
                    "\"{requirement}\" is not a version requirement: {e}"
                ))
            })?;
            out["requirement"] = serde_json::json!(requirement);
            out["satisfies"] = serde_json::json!(req.matches(&version));
        }
        Ok(DataValue::Json(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn check(version: &str, req: &str) -> serde_json::Value {
        let mut opts = Options::new();
        opts.insert("requirement".into(), req.into());
        let DataValue::Json(v) =
            run_single(&SemverCheck, DataValue::Text(version.into()), &opts).unwrap()
        else {
            unreachable!()
        };
        v
    }

    #[test]
    fn parses_and_matches() {
        let v = check("v1.2.3-beta.1+build.5", "");
        assert_eq!(v["major"], 1);
        assert_eq!(v["pre_release"], "beta.1");
        assert_eq!(v["build"], "build.5");
        assert!(v.get("satisfies").is_none());

        assert_eq!(check("1.2.3", "^1.1")["satisfies"], true);
        assert_eq!(check("2.0.0", "^1.1")["satisfies"], false);
        assert_eq!(check("1.2.3-beta.1", "^1.0")["satisfies"], false);
        assert_eq!(check("1.4.9", ">=1.2, <1.5")["satisfies"], true);
    }

    #[test]
    fn junk_errors() {
        let mut opts = Options::new();
        opts.insert("requirement".into(), "not-a-req".into());
        assert!(run_single(
            &SemverCheck,
            DataValue::Text("x.y.z".into()),
            &Options::new()
        )
        .is_err());
        assert!(run_single(&SemverCheck, DataValue::Text("1.2.3".into()), &opts).is_err());
    }
}
