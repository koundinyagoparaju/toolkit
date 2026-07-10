# Adding a chain

Chains are **pure data** — a JSON DAG of existing tools. They can't do
anything the audited tools can't, which is why the library can accept them
freely. Two ways to make one:

**Visually**: build it in the web builder, click *Share*, decode the URL
fragment (it's base64url of the JSON), tidy it up, save to `chains/`.

**By hand**: write the JSON directly.

## Schema

```jsonc
{
    "version": 1,
    "name": "Human-friendly name",
    "description": "What it does and when you'd reach for it.",
    "params": [ /* optional, see below */ ],
    "nodes": [
        { "id": "decode", "tool": "base64-decode", "options": {} },
        { "id": "pretty", "tool": "json-format", "options": { "indent": 2 } }
    ],
    "edges": [
        { "from": "decode", "to": "pretty" }
        // target a specific port of a multi-input tool:
        // { "from": "a", "to": "m", "to_port": "first" }
    ]
}
```

Validation rules (checked by CLI and web before anything runs):

- unique node ids; edges reference existing nodes and ports; no cycles
- fan-out fine; one edge per single port; multi ports take many edges,
  **ordered by their position in the `edges` array**
- a node is fully wired or an entry node; entry nodes all receive the
  chain input (entropy ports excepted — the driver fills those)
- every edge type-checks against the coercion matrix

## Params: make the chain a real tool

Declare the knobs a *user of the chain* should turn; everything else stays
internal. Each param is an option spec plus `maps`:

```jsonc
"params": [{
    "name": "quality",
    "label": "JPEG quality",
    "description": "Lower is smaller.",
    "kind": "integer", "min": 1, "max": 100, "default": 85,
    "maps": [{ "node": "jpeg", "option": "quality" }]   // one param may map to several
}]
```

The CLI accepts them (`toolkit chain --name x --set quality=70`), the web
builder renders a settings form, and `toolkit chains` lists them.
Precedence: invocation > chain file > tool defaults.

## Test it, then submit

```sh
cargo run -q -p toolkit-cli -- chains                      # your chain listed, params shown
echo 'real input' | cargo run -q -p toolkit-cli -- chain --name your-chain
./scripts/build-web-assets.sh                              # regenerates the web chain index
```

Run it with *real* input — validation catches structure, only execution
catches wrong options. One chain per file, filename = slug. In the PR
description, say what recurring task the chain encodes; the best library
chains capture domain knowledge (SAML uses raw deflate; TOTP secrets are
base32) rather than just saving keystrokes.
