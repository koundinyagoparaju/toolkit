use crate::html_entities::decode_entities;
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Strip HTML to readable plain text: tags removed, entities decoded,
/// block elements become line breaks, list items get a dash. Built for
/// feeding page content to something that wants prose (a diff, a word
/// count, an LLM's context window) without the markup tax.
pub struct HtmlToText;

/// Tags whose entire content is dropped, not just the tags themselves.
const DROP_CONTENT: &[&str] = &["script", "style", "head", "noscript", "template"];

/// Tags that imply a line break around them.
const BLOCK: &[&str] = &[
    "p",
    "div",
    "br",
    "li",
    "ul",
    "ol",
    "table",
    "tr",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "pre",
    "section",
    "article",
    "header",
    "footer",
    "main",
    "nav",
    "aside",
    "hr",
    "form",
    "fieldset",
    "figure",
    "figcaption",
    "address",
    "dt",
    "dd",
    "dl",
];

impl Tool for HtmlToText {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "html-to-text".into(),
            label: "HTML → Text".into(),
            description: "Strip HTML to readable plain text: tags removed, entities decoded, \
                          block elements become line breaks, list items become \"- \" lines. \
                          Scripts, styles, and comments are dropped entirely."
                .into(),
            keywords: [
                "html", "text", "strip", "tags", "extract", "readable", "convert", "scrape",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(
                DataType::Text,
                "<h1>Title</h1><p>Hello <b>world</b> &amp; friends.</p><ul><li>one</li><li>two</li></ul>",
            ),
            output: DataType::Text,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(html) = inputs.sole() else {
            unreachable!()
        };
        Ok(DataValue::Text(html_to_text(&html)))
    }
}

/// The tag name (lowercased) and whether it's a closing tag, from the
/// text between `<` and `>`.
fn tag_name(tag: &str) -> (String, bool) {
    let inner = tag.trim_start_matches('/');
    let closing = tag.starts_with('/');
    let name: String = inner
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    (name, closing)
}

fn html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len() / 2);
    let mut rest = html;
    let mut dropping: Option<String> = None; // inside a DROP_CONTENT element

    while let Some(lt) = rest.find('<') {
        if dropping.is_none() {
            text.push_str(&rest[..lt]);
        }
        rest = &rest[lt..];

        // Comments end at the first "-->", regardless of '>' inside.
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(end) => rest = &rest[end + 3..],
                None => return finalize(&text),
            }
            continue;
        }

        let Some(gt) = rest.find('>') else {
            // Unterminated tag: treat the rest as dropped markup.
            return finalize(&text);
        };
        let (name, closing) = tag_name(&rest[1..gt]);
        rest = &rest[gt + 1..];

        match &dropping {
            Some(active) => {
                if closing && *active == name {
                    dropping = None;
                }
            }
            None => {
                if !closing && DROP_CONTENT.contains(&name.as_str()) {
                    dropping = Some(name);
                } else if name == "li" && !closing {
                    text.push_str("\n- ");
                } else if BLOCK.contains(&name.as_str()) {
                    text.push('\n');
                }
            }
        }
    }
    if dropping.is_none() {
        text.push_str(rest);
    }
    finalize(&text)
}

/// Decode entities, collapse intra-line whitespace, and drop blank lines
/// (adjacent block tags each emit a newline; one is enough).
fn finalize(raw: &str) -> String {
    let decoded = decode_entities(raw);
    let mut out = String::with_capacity(decoded.len());
    for line in decoded.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn convert(html: &str) -> String {
        let DataValue::Text(out) =
            run_single(&HtmlToText, DataValue::Text(html.into()), &Options::new()).unwrap()
        else {
            unreachable!()
        };
        out
    }

    #[test]
    fn blocks_lists_and_entities() {
        let out = convert(
            "<h1>Title</h1><p>Hello <b>world</b> &amp; friends.</p><ul><li>one</li><li>two</li></ul>",
        );
        assert_eq!(out, "Title\nHello world & friends.\n- one\n- two");
    }

    #[test]
    fn drops_scripts_styles_comments() {
        let out = convert(
            "<style>p{color:red}</style><p>keep</p><script>var x = '<p>no</p>';</script><!-- <p>gone</p> --><p>also keep</p>",
        );
        assert_eq!(out, "keep\nalso keep");
    }

    #[test]
    fn collapses_whitespace_but_keeps_paragraph_gaps() {
        let out = convert("<p>a   b</p>\n\n\n<p>c</p>");
        assert_eq!(out, "a b\nc");
    }

    /// Property test over generated documents: every visible marker
    /// survives in order, nothing from dropped sections leaks, and no
    /// markup survives (marker text is alphanumeric, so any '<' in the
    /// output would be an unstripped tag).
    #[test]
    fn generated_documents_hold_the_invariants() {
        let mut state = 7u64;
        let mut next = move || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state >> 33
        };
        for round in 0..50 {
            let mut html = String::new();
            let mut markers = Vec::new();
            for i in 0..20 {
                match next() % 7 {
                    0 => html.push_str(&format!("<script>NEVER{i} = '<p>NEVER</p>';</script>")),
                    1 => html.push_str(&format!("<style>NEVER{i} {{}}</style>")),
                    2 => html.push_str(&format!("<!-- NEVER{i} <div> -->")),
                    3 => html.push_str("<div class=\"x\"><ul><li>"),
                    4 => html.push_str("</li></ul></div><br/>"),
                    _ => {
                        let m = format!("T{round}x{i}");
                        html.push_str(&format!("<p> {m} &amp; </p>"));
                        markers.push(m);
                    }
                }
            }
            let out = convert(&html);
            assert!(!out.contains("NEVER"), "dropped content leaked: {html}");
            assert!(!out.contains('<'), "markup survived: {out}");
            let mut rest = out.as_str();
            for m in &markers {
                let at = rest.find(m.as_str()).unwrap_or_else(|| {
                    panic!("marker {m} missing or out of order in {out:?} for {html}")
                });
                rest = &rest[at + m.len()..];
            }
        }
    }

    #[test]
    fn tolerates_malformed_markup() {
        assert_eq!(convert("plain, no tags"), "plain, no tags");
        assert_eq!(convert("<p>unclosed <b>bold"), "unclosed bold");
        assert_eq!(convert("stray < left alone"), "stray");
        assert_eq!(convert("<script>never closed"), "");
    }
}
