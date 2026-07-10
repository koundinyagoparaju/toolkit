use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct CaseConvert;

impl Tool for CaseConvert {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "case-convert".into(),
            label: "Case Convert".into(),
            description: "Convert identifier/text casing: camelCase, PascalCase, snake_case, kebab-case, CONSTANT_CASE, Title Case, and more.".into(),
            keywords: ["case", "camel", "snake", "kebab", "pascal", "convert", "identifier"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: false,
            options: vec![OptionSpec::enumeration(
                "target",
                "Target case",
                "",
                &["camel", "pascal", "snake", "kebab", "constant", "title", "lower", "upper"],
            )
            .required()],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let target = options.str_opt("target").expect("required");
        // Convert per line so lists of identifiers work naturally.
        let out: Vec<String> = text.lines().map(|line| convert(line, target)).collect();
        Ok(DataValue::Text(out.join("\n")))
    }
}

/// Split into lowercase words on separators and case boundaries:
/// "XMLHttpRequest2" -> ["xml", "http", "request2"].
fn words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        let prev = i.checked_sub(1).map(|p| chars[p]);
        let next = chars.get(i + 1);
        let boundary = match prev {
            Some(p) if p.is_alphanumeric() => {
                // aB | 1B boundary, and the AAb side of an acronym (XMLHttp).
                (c.is_uppercase() && !p.is_uppercase())
                    || (c.is_uppercase()
                        && p.is_uppercase()
                        && next.is_some_and(|n| n.is_lowercase()))
            }
            _ => false,
        };
        if boundary && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn capitalize(w: &str) -> String {
    let mut chars = w.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn convert(line: &str, target: &str) -> String {
    let words = words(line);
    match target {
        "camel" => words
            .iter()
            .enumerate()
            .map(|(i, w)| if i == 0 { w.clone() } else { capitalize(w) })
            .collect(),
        "pascal" => words.iter().map(|w| capitalize(w)).collect(),
        "snake" => words.join("_"),
        "kebab" => words.join("-"),
        "constant" => words.join("_").to_uppercase(),
        "title" => words
            .iter()
            .map(|w| capitalize(w))
            .collect::<Vec<_>>()
            .join(" "),
        "upper" => words.join(" ").to_uppercase(),
        _ => words.join(" "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::run_single;

    fn conv(text: &str, target: &str) -> String {
        let out = run_single(
            &CaseConvert,
            DataValue::Text(text.into()),
            json!({ "target": target }).as_object().unwrap(),
        )
        .unwrap();
        let DataValue::Text(s) = out else { panic!() };
        s
    }

    #[test]
    fn converts_between_conventions() {
        assert_eq!(conv("hello world_example", "camel"), "helloWorldExample");
        assert_eq!(conv("helloWorldExample", "snake"), "hello_world_example");
        assert_eq!(conv("XMLHttpRequest", "kebab"), "xml-http-request");
        assert_eq!(conv("some-flag", "constant"), "SOME_FLAG");
        assert_eq!(conv("the_end", "title"), "The End");
    }

    #[test]
    fn per_line() {
        assert_eq!(conv("one_a\ntwo_b", "pascal"), "OneA\nTwoB");
    }
}
