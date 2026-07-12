use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct HtmlEncode;

impl Tool for HtmlEncode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "html-encode".into(),
            label: "HTML Encode".into(),
            description: "Escape text for safe embedding in HTML (& < > \" ').".into(),
            keywords: ["html", "escape", "encode", "entities"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, r#"<a href="x.html">Fish & chips</a>"#),
            output: DataType::Text,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&#39;"),
                _ => out.push(c),
            }
        }
        Ok(DataValue::Text(out))
    }
}

pub struct HtmlDecode;

impl Tool for HtmlDecode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "html-decode".into(),
            label: "HTML Decode".into(),
            description: "Decode HTML entities: common named entities plus numeric &#123; and &#x1F600; forms.".into(),
            keywords: ["html", "unescape", "decode", "entities"].map(String::from).to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "&lt;a href=&quot;x.html&quot;&gt;Fish &amp; chips&lt;/a&gt;"),
            output: DataType::Text,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(amp) = rest.find('&') {
            out.push_str(&rest[..amp]);
            rest = &rest[amp..];
            let semi = rest.find(';').filter(|&i| i <= 32);
            let Some(semi) = semi else {
                out.push('&');
                rest = &rest[1..];
                continue;
            };
            let entity = &rest[1..semi];
            let decoded = decode_entity(entity);
            match decoded {
                Some(c) => out.push_str(&c),
                None => out.push_str(&rest[..=semi]), // unknown: keep literal
            }
            rest = &rest[semi + 1..];
        }
        out.push_str(rest);
        Ok(DataValue::Text(out))
    }
}

fn decode_entity(entity: &str) -> Option<String> {
    let named = [
        ("amp", "&"),
        ("lt", "<"),
        ("gt", ">"),
        ("quot", "\""),
        ("apos", "'"),
        ("nbsp", "\u{a0}"),
        ("copy", "©"),
        ("reg", "®"),
        ("trade", "™"),
        ("hellip", "…"),
        ("mdash", "—"),
        ("ndash", "–"),
        ("laquo", "«"),
        ("raquo", "»"),
        ("ldquo", "\u{201c}"),
        ("rdquo", "\u{201d}"),
        ("lsquo", "\u{2018}"),
        ("rsquo", "\u{2019}"),
        ("deg", "°"),
        ("plusmn", "±"),
        ("times", "×"),
        ("divide", "÷"),
        ("euro", "€"),
        ("pound", "£"),
        ("yen", "¥"),
        ("cent", "¢"),
        ("sect", "§"),
        ("para", "¶"),
        ("middot", "·"),
        ("bull", "•"),
        ("dagger", "†"),
        ("larr", "←"),
        ("uarr", "↑"),
        ("rarr", "→"),
        ("darr", "↓"),
    ];
    if let Some((_, replacement)) = named.iter().find(|(name, _)| *name == entity) {
        return Some(replacement.to_string());
    }
    let code = if let Some(hex) = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else if let Some(dec) = entity.strip_prefix('#') {
        dec.parse().ok()?
    } else {
        return None;
    };
    char::from_u32(code).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn encode_escapes_the_dangerous_five() {
        let out = run_single(
            &HtmlEncode,
            DataValue::Text("<a href=\"x\">&'</a>".into()),
            &Options::new(),
        )
        .unwrap();
        assert_eq!(
            out,
            DataValue::Text("&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;".into())
        );
    }

    #[test]
    fn decode_named_numeric_and_unknown() {
        let out = run_single(
            &HtmlDecode,
            DataValue::Text("&lt;b&gt; &copy; &#65;&#x1F600; &unknown; & plain".into()),
            &Options::new(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Text("<b> © A😀 &unknown; & plain".into()));
    }
}
