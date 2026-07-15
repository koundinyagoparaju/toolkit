//! Data-format tools: converters, parsers, and formatters.

#![deny(unsafe_code)]
mod calc;
mod cidr;
mod color;
mod combinatorics;
mod contrast;
mod cron;
mod csv_stats;
mod csv_tools;
mod duration;
mod filetype;
mod grep;
mod http_status;
mod json_diff;
mod json_query;
mod json_schema_infer;
mod markdown;
mod number_factor;
mod number_stats;
mod percent;
mod regex_tool;
mod semver_check;
mod sql_format;
mod text_replace;
mod timestamp;
mod toml_tools;
mod units;
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
        Box::new(json_diff::JsonDiff),
        Box::new(json_query::JsonQuery),
        Box::new(json_schema_infer::JsonSchemaInfer),
        Box::new(http_status::HttpStatus),
        Box::new(sql_format::SqlFormat),
        Box::new(grep::TextGrep),
        Box::new(text_replace::TextReplace),
        Box::new(duration::DurationConvert),
        Box::new(cidr::CidrCalc),
        Box::new(contrast::ContrastRatio),
        Box::new(csv_stats::CsvStats),
        Box::new(cron::CronExplain),
        Box::new(semver_check::SemverCheck),
        Box::new(calc::Calc),
        Box::new(number_stats::NumberStats),
        Box::new(number_factor::NumberFactor),
        Box::new(combinatorics::Combinatorics),
        Box::new(percent::PercentCalc),
        Box::new(units::DATA_SIZE),
        Box::new(units::LENGTH),
        Box::new(units::MASS),
        Box::new(units::VOLUME),
        Box::new(units::TemperatureConvert),
        Box::new(units::PxConvert),
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
