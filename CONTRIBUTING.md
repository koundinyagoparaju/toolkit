# Contributing

Contributions land as pull requests — that's not just process, it's the
security model: every tool and chain on the site got there through public
review, and the site never loads code from anywhere but this repo.

## Adding a tool

A tool is a struct implementing the `Tool` trait, in the pack crate that fits
it (`crates/packs/text`, `crates/packs/image`, or propose a new pack).

1. Create `crates/packs/<pack>/src/my_tool.rs`:

```rust
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct Reverse;

impl Tool for Reverse {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "reverse".into(),            // stable slug, kebab-case
            label: "Reverse".into(),
            description: "Reverse the characters of the input text.".into(),
            keywords: ["reverse", "flip"].map(String::from).to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Text,
            options: vec![],                    // see OptionSpec builders
        }
    }

    fn run(&self, inputs: Inputs, _options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(s) = inputs.sole() else { unreachable!() }; // pre-coerced
        Ok(DataValue::Text(s.chars().rev().collect()))
    }
}
```

A tool that genuinely needs several inputs declares named ports instead —
each port has a distinct role and type, and shows up as a labeled input dot
in the chain builder (see `crates/packs/image/src/merge.rs`):

```rust
inputs: vec![
    InputSpec::named("first", DataType::Image),
    InputSpec::named("second", DataType::Image),
],
// in run(): inputs.take("first"), inputs.take("second")
```

A port that accepts a *variable* number of values (doc-merge's documents)
is marked `multi` — cardinality lives on the port, so the type system stays
list-free (see `crates/packs/text/src/doc_merge.rs`):

```rust
inputs: vec![InputSpec::named("documents", DataType::Text).multi()],
// in run(): inputs.take_many("documents") -> Vec<DataValue>, in edge order
```

2. Register it in the pack's `src/lib.rs` (`mod my_tool;` + one line in
   `registry()`).
3. Add `#[cfg(test)]` tests in the same file — including at least one
   error-path test.

That's the whole integration. The CLI (`toolkit list/run`), the web catalog,
the auto-generated options form, and the chain builder all pick the tool up
from its manifest. Declare every option in the manifest (`OptionSpec::integer`,
`::enumeration`, …) — validation, defaults, CLI flags, and the web form come
for free.

### Rules

- **Pure**: no filesystem, network, clock, or randomness. A tool is a
  function of its input and options, nothing else.
- **Baseline path**: must run on plain single-threaded CPU (wasm has no
  threads/GPU here). Acceleration only ever as an additive fast path.
- **Dependencies**: pure-Rust only, well-maintained, and genuinely needed —
  hand-roll trivial things (we did hex and JWT by hand). New dependencies get
  extra scrutiny in review; `default-features = false` wherever possible.
- **Input discipline**: be liberal in what you accept (strip whitespace,
  accept common variants), precise in your error messages.
- **Types**: pick the most specific `DataType`. The coercion matrix
  (`crates/core/src/data.rs`) feeds your tool bytes/text/json as declared.

## Adding a chain

Chains are pure data — a JSON DAG of existing tools (see `chains/*.json`).
Build one visually in the web builder, click **Share**, and translate the
result into a file in `chains/` with a good `name` and `description`
(or write the JSON directly). One chain per file, filename = slug.

Declare the knobs a user of your chain should be able to turn as `params`
(see `chains/image-web-ready.json`): each param reuses the option-spec
format (`kind`, `min`/`max`/`values`, `default`) plus `maps` — the node
options it writes into. Params are what make a chain feel like a real tool:
the CLI takes them as `--set name=value` and the web builder renders them
as a settings form, while everything else about the nodes stays internal.

## Development

```sh
cargo test                        # everything Rust
cargo run -p toolkit-cli -- list  # CLI against native packs
./scripts/build-web-assets.sh     # wasm packs + manifests for the web app
cd web && npm install && npm run dev
```

Before opening a PR: `cargo test`, `cargo fmt`, `cargo clippy`, and if you
touched the web app, `npm run build`.
