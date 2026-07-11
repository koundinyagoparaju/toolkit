use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// Turn a title into a URL-safe slug: "Hello, World!" -> "hello-world".
pub struct Slugify;

impl Tool for Slugify {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "slugify".into(),
            label: "Slugify".into(),
            description: "Make a URL-safe slug from text: lowercased, accents folded to ASCII (ü->u), non-alphanumerics collapsed to a separator.".into(),
            keywords: ["slug", "slugify", "url", "permalink", "kebab"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::string("separator", "Separator", "Character between words.")
                    .default_value("-".into()),
                OptionSpec::bool("lowercase", "Lowercase", "Lowercase the result.")
                    .default_value(true.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let sep = options.str_opt("separator").unwrap_or("-").to_string();
        let lowercase = options.bool_opt("lowercase").unwrap_or(true);

        // Runs of alphanumerics become words; accented Latin letters fold
        // to ASCII; every other character is a word boundary.
        let mut words: Vec<String> = Vec::new();
        let mut current = String::new();
        for c in text.chars() {
            if c.is_ascii_alphanumeric() {
                current.push(if lowercase { c.to_ascii_lowercase() } else { c });
            } else if let Some(folded) = ascii_fold(c) {
                // Accented Latin letter (ü -> u, é -> e): fold to ASCII so
                // "münchen" -> "munchen", not "mnchen".
                current.push_str(folded);
            } else if c.is_alphanumeric() {
                // Other non-ASCII letter/digit with no ASCII fold: drop it
                // without splitting the word.
            } else if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            words.push(current);
        }
        Ok(DataValue::Text(words.join(&sep)))
    }
}

/// Lowercase ASCII equivalent of an accented Latin letter (Latin-1
/// Supplement + a few common Extended-A). Returns None for anything
/// without a sensible single-letter fold. Output is lowercase; the
/// caller re-cases if it wants title-case.
fn ascii_fold(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'ā' | 'ă' | 'ą' => {
            "a"
        }
        'æ' | 'Æ' => "ae",
        'ç' | 'Ç' | 'ć' | 'č' | 'ĉ' | 'ċ' => "c",
        'ð' | 'Ð' | 'ď' | 'đ' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' | 'ē' | 'ĕ' | 'ę' | 'ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'ĵ' => "j",
        'ķ' => "k",
        'ł' | 'Ł' | 'ĺ' | 'ļ' | 'ľ' => "l",
        'ñ' | 'Ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'ō' | 'ŏ' | 'ő' => {
            "o"
        }
        'œ' | 'Œ' => "oe",
        'ŕ' | 'ř' | 'ŗ' => "r",
        'ś' | 'š' | 'ş' | 'ŝ' => "s",
        'ß' => "ss",
        'ţ' | 'ť' | 'ŧ' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => {
            "u"
        }
        'ý' | 'ÿ' | 'Ý' | 'Ŷ' => "y",
        'þ' | 'Þ' => "th",
        'ź' | 'ž' | 'ż' => "z",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn slug(input: &str) -> String {
        let DataValue::Text(out) =
            run_single(&Slugify, DataValue::Text(input.into()), &Options::new()).unwrap()
        else {
            unreachable!()
        };
        out
    }

    #[test]
    fn basic_slugs() {
        assert_eq!(slug("Hello, World!"), "hello-world");
        assert_eq!(slug("  Trim  --  Me  "), "trim-me");
        assert_eq!(slug("Rust 2.0: The Sequel"), "rust-2-0-the-sequel");
        assert_eq!(slug("café münchen"), "cafe-munchen"); // accents folded
        assert_eq!(slug("Straße"), "strasse"); // ß -> ss
        assert_eq!(slug("naïve Œuvre"), "naive-oeuvre");
        assert_eq!(slug("日本語 text"), "text"); // no fold: dropped
    }

    #[test]
    fn options() {
        let mut opts = Options::new();
        opts.insert("separator".into(), "_".into());
        opts.insert("lowercase".into(), false.into());
        let DataValue::Text(out) =
            run_single(&Slugify, DataValue::Text("Hello World".into()), &opts).unwrap()
        else {
            unreachable!()
        };
        assert_eq!(out, "Hello_World");
    }
}
