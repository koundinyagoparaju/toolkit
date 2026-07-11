use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

const WORDS: &[&str] = &[
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "sed",
    "do",
    "eiusmod",
    "tempor",
    "incididunt",
    "ut",
    "labore",
    "et",
    "dolore",
    "magna",
    "aliqua",
    "enim",
    "ad",
    "minim",
    "veniam",
    "quis",
    "nostrud",
    "exercitation",
    "ullamco",
    "laboris",
    "nisi",
    "aliquip",
    "ex",
    "ea",
    "commodo",
    "consequat",
    "duis",
    "aute",
    "irure",
    "in",
    "reprehenderit",
    "voluptate",
    "velit",
    "esse",
    "cillum",
    "eu",
    "fugiat",
    "nulla",
    "pariatur",
    "excepteur",
    "sint",
    "occaecat",
    "cupidatat",
    "non",
    "proident",
    "sunt",
    "culpa",
    "qui",
    "officia",
    "deserunt",
    "mollit",
    "anim",
    "id",
    "est",
    "laborum",
];

const LOREM_OPENER: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit";

/// Placeholder text. Entropy-seeded so "generate again" varies, yet
/// reproducible if you wire fixed bytes into the entropy port.
pub struct LoremIpsum;

impl Tool for LoremIpsum {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "lorem-ipsum".into(),
            label: "Lorem Ipsum".into(),
            description: "Generate placeholder text — words, sentences, or paragraphs.".into(),
            keywords: ["lorem", "ipsum", "placeholder", "filler", "dummy", "text"]
                .map(String::from)
                .to_vec(),
            inputs: vec![InputSpec::entropy()],
            output: DataType::Text,
            streaming: false,
            options: vec![
                OptionSpec::enumeration(
                    "unit",
                    "Unit",
                    "What `count` counts.",
                    &["words", "sentences", "paragraphs"],
                )
                .default_value("paragraphs".into()),
                OptionSpec::integer(
                    "count",
                    "Count",
                    "How many to generate.",
                    Some(1),
                    Some(500),
                )
                .default_value(3.into()),
                OptionSpec::bool(
                    "start_with_lorem",
                    "Start with \"Lorem ipsum…\"",
                    "Begin the output with the classic opening line.",
                )
                .default_value(true.into()),
            ],
        }
    }

    fn run(&self, mut inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(entropy) = inputs.take("entropy") else {
            unreachable!()
        };
        let seed = entropy
            .get(..8)
            .ok_or_else(|| ToolError::new("need at least 8 bytes of entropy"))?;
        let mut rng = Rng::new(u64::from_le_bytes(seed.try_into().expect("sliced to 8")));

        let unit = options.str_opt("unit").unwrap_or("paragraphs");
        let count = options.u32_opt("count").unwrap_or(3) as usize;
        let lorem = options.bool_opt("start_with_lorem").unwrap_or(true);

        let out = match unit {
            "words" => {
                let mut words: Vec<&str> = (0..count).map(|_| rng.pick(WORDS)).collect();
                if lorem && count >= 5 {
                    for (i, w) in ["lorem", "ipsum", "dolor", "sit", "amet"]
                        .iter()
                        .enumerate()
                    {
                        words[i] = w;
                    }
                }
                words.join(" ")
            }
            "sentences" => (0..count)
                .map(|i| rng.sentence(lorem && i == 0))
                .collect::<Vec<_>>()
                .join(" "),
            _ => (0..count)
                .map(|i| {
                    let sentences = 3 + rng.below(4);
                    (0..sentences)
                        .map(|j| rng.sentence(lorem && i == 0 && j == 0))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
        };
        Ok(DataValue::Text(out))
    }
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
    fn sentence(&mut self, opener: bool) -> String {
        let mut s = if opener {
            LOREM_OPENER.to_string()
        } else {
            let len = 5 + self.below(10);
            let words: Vec<&str> = (0..len).map(|_| self.pick(WORDS)).collect();
            let mut joined = words.join(" ");
            joined.replace_range(0..1, &joined[0..1].to_uppercase());
            joined
        };
        // Occasional comma clause for texture.
        if !opener && self.below(3) == 0 {
            let clause: Vec<&str> = (0..3 + self.below(4)).map(|_| self.pick(WORDS)).collect();
            s.push_str(&format!(", {}", clause.join(" ")));
        }
        s.push('.');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_tool;

    fn entropy(byte: u8) -> Inputs {
        let mut inputs = Inputs::new();
        inputs.insert("entropy".into(), vec![DataValue::Bytes(vec![byte; 64])]);
        inputs
    }

    fn generate(unit: &str, count: i64, lorem: bool, seed: u8) -> String {
        let mut opts = Options::new();
        opts.insert("unit".into(), unit.into());
        opts.insert("count".into(), count.into());
        opts.insert("start_with_lorem".into(), lorem.into());
        let DataValue::Text(out) = run_tool(&LoremIpsum, entropy(seed), &opts).unwrap() else {
            unreachable!()
        };
        out
    }

    #[test]
    fn shapes_and_opener() {
        assert!(generate("words", 4, false, 1).split_whitespace().count() == 4);
        assert_eq!(
            generate("paragraphs", 2, true, 9).matches("\n\n").count(),
            1
        );
        assert!(generate("sentences", 3, true, 5).starts_with("Lorem ipsum dolor sit amet"));
    }

    #[test]
    fn sentences_are_capitalized_and_terminated() {
        let s = generate("sentences", 2, false, 3);
        assert!(s.ends_with('.'));
        assert!(s.chars().next().unwrap().is_uppercase());
    }

    #[test]
    fn deterministic_for_fixed_entropy() {
        assert_eq!(
            generate("paragraphs", 3, false, 7),
            generate("paragraphs", 3, false, 7)
        );
    }
}
