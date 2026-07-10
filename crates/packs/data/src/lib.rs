//! Data-format tools: converters, parsers, and formatters.

mod color;
mod csv_tools;
mod filetype;
mod markdown;
mod regex_tool;
mod timestamp;
mod toml_tools;
mod url_tool;
mod xml;
mod yaml;

use toolkit_core::Registry;

pub fn registry() -> Registry {
    Registry::new(vec![
        Box::new(yaml::JsonToYaml),
        Box::new(yaml::YamlToJson),
        Box::new(toml_tools::TomlToJson),
        Box::new(toml_tools::JsonToToml),
        Box::new(csv_tools::CsvToJson),
        Box::new(csv_tools::JsonToCsv),
        Box::new(xml::XmlFormat),
        Box::new(timestamp::TimestampConvert),
        Box::new(url_tool::UrlParse),
        Box::new(regex_tool::RegexExtract),
        Box::new(markdown::MarkdownToHtml),
        Box::new(color::ColorConvert),
        Box::new(filetype::FileType),
    ])
}

toolkit_core::export_pack_abi!(crate::registry);

#[cfg(test)]
mod pack_tests {
    use toolkit_core::{validate_against_specs, Options};

    #[test]
    fn streaming_flag_matches_sessions() {
        let registry = super::registry();
        for m in registry.manifests() {
            let tool = registry.find(&m.name).unwrap();
            match validate_against_specs(&m.options, &Options::new(), &m.name) {
                Ok(opts) => {
                    let streams = tool.open_stream(&opts).unwrap().is_some();
                    assert_eq!(streams, m.streaming, "tool {} flag mismatch", m.name);
                }
                Err(_) => assert!(!m.streaming),
            }
        }
    }
}
