# Adding a tool

A tool is one Rust file plus one registry line. This walkthrough builds a
complete example; skim [concepts.md](concepts.md) first if terms like
"port" or "coercion" are new.

## 1. Pick the pack

| Pack | For | Heavy deps |
|---|---|---|
| `crates/packs/text` | encodings, text transforms, compression | none |
| `crates/packs/data` | format converters, parsers | serde_yaml, csv, regex, ... |
| `crates/packs/crypto` | digests, signatures, generators | RustCrypto |
| `crates/packs/image` | anything decoding pixels | image codecs, qr |

Pack choice affects web download size only — tool names are global, so a
tool can move packs later without breaking chains.

## 2. Write the tool

`crates/packs/text/src/rot13.rs`:

```rust
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct Rot13;

impl Tool for Rot13 {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "rot13".into(),                 // slug: kebab-case, stable forever
            label: "ROT13".into(),
            description: "Rotate letters by 13 places (its own inverse).".into(),
            keywords: ["rot13", "caesar", "cipher"].map(String::from).to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _options: &Options) -> Result<DataValue, ToolError> {
        // Input is already coerced to the port's declared type, so this
        // pattern cannot fail:
        let DataValue::Text(s) = inputs.sole() else { unreachable!() };
        let out: String = s
            .chars()
            .map(|c| match c {
                'a'..='z' => (((c as u8 - b'a') + 13) % 26 + b'a') as char,
                'A'..='Z' => (((c as u8 - b'A') + 13) % 26 + b'A') as char,
                other => other,
            })
            .collect();
        Ok(DataValue::Text(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn rotates_and_round_trips() {
        let once = run_single(&Rot13, DataValue::Text("Hello!".into()), &Options::new()).unwrap();
        assert_eq!(once, DataValue::Text("Uryyb!".into()));
        let twice = run_single(&Rot13, once, &Options::new()).unwrap();
        assert_eq!(twice, DataValue::Text("Hello!".into()));
    }
}
```

## 3. Register it

In the pack's `src/lib.rs`: add `mod rot13;` and one
`Box::new(rot13::Rot13),` line in `registry()`. **That is the entire
integration** — the CLI, the web catalog, the auto-generated form, chain
type-checking, and the wasm build all pick the tool up from its manifest.

## 4. Verify

```sh
cargo test -p toolkit-pack-text
echo -n 'Hello!' | cargo run -q -p toolkit-cli -- run-tool rot13    # Uryyb!
./scripts/build-web-assets.sh && cd web && npm run dev          # see it in the catalog
```

## Variations

**Options** — declare them; validation, defaults, web form, and CLI flags
come free. Read validated values with the `OptGet` helpers:

```rust
options: vec![
    OptionSpec::integer("shift", "Shift", "", Some(1), Some(25)).default_value(13.into()),
    OptionSpec::enumeration("mode", "Mode", "", &["encode", "decode"]).required(),
],
// in run():  let shift = options.i64_opt("shift").unwrap_or(13);
```

**An example input** — declare one whenever a text sample can demo the
tool (most can): the web page gets a one-click "Try an example", and
`toolkit info` prints it as a runnable command. A CLI test runs every
declared example with **default options** and fails the build if it
errors — so make it succeed, or don't declare one (image inputs and
generators don't):

```rust
inputs: InputSpec::sole_example(DataType::Text, "Uryyb, jbeyq!"),
// multi-port tools: InputSpec::named("old", DataType::Text).example("alpha\n")
```

**Multiple inputs** — named ports with distinct roles
(see `image/src/merge.rs`):

```rust
inputs: vec![InputSpec::named("old", DataType::Text), InputSpec::named("new", DataType::Text)],
// in run():  let a = inputs.take("old");  let b = inputs.take("new");
```

**Variable arity** — one `multi` port (see `text/src/doc_merge.rs`):

```rust
inputs: vec![InputSpec::named("documents", DataType::Text).multi()],
// in run():  for doc in inputs.take_many("documents") { ... }   // edge order
```

**Randomness** — an entropy port; never reach for ambient randomness
(see `crypto/src/generators.rs`, and [concepts.md](concepts.md#input-ports)
for why):

```rust
inputs: vec![InputSpec::entropy()],
// in run():  let DataValue::Bytes(entropy) = inputs.take("entropy") else { ... };
```

**Streaming** — if (and only if) the tool can transform bytes
incrementally, implement a session and derive `run` from it, so there is
one implementation (see `text/src/gzip.rs` for carries,
`text/src/doc_merge.rs` for sequential multi-input consumption):

```rust
fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
    let session = self.open_stream(options)?.expect("streaming tool");
    buffered_run(session, &self.manifest(), inputs)
}

fn open_stream(&self, options: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
    Ok(Some(Box::new(MySession::new(options))))
}
```

Driver contract your session can rely on: chunks for one `(port, index)`
arrive in order; `end_input` fires once per value; `finish` once at the
end. Set `streaming: true` in the manifest — a pack test fails if the flag
and the session disagree.

## Checklist before the PR

- [ ] Pure: no fs / network / clock / ambient randomness
- [ ] Works single-threaded (wasm has no threads); acceleration only additive
- [ ] Dependencies pure-Rust, justified, `default-features = false` where possible
- [ ] Liberal input handling (whitespace, common variants), precise errors
- [ ] Tests include at least one error path (and chunk boundaries, if streaming)
- [ ] `cargo test && cargo fmt && cargo clippy` clean; web builds if touched
