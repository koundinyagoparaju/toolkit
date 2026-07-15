# Using toolkit as an MCP server

`toolkit mcp` runs a [Model Context Protocol](https://modelcontextprotocol.io)
server over stdio, exposing every tool to an LLM agent. It speaks JSON-RPC
2.0 on stdin/stdout, logs to stderr, and opens no sockets — so the "no
network code" property of the binary still holds, and everything runs on
your machine.

Each tool becomes an MCP tool automatically: its name and description come
from the manifest, and its input schema is generated from the input ports
and options. New tools show up with no extra work.

## Wiring it into an agent

Anything that speaks MCP over stdio can launch it. For Claude Desktop, add
to the config (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "toolkit": {
      "command": "toolkit",
      "args": ["mcp"]
    }
  }
}
```

For Claude Code:

```sh
claude mcp add toolkit -- toolkit mcp
```

## Compact mode

`toolkit mcp --compact` advertises three tools instead of one schema per
tool: `search-tools`, `run-tool`, and `run-chain` — a few hundred tokens
of context instead of ~10,000, however many tools exist. Use it with
clients that inject every schema into every request or cap the total
tool count.

`search-tools` takes keywords (`{"query": "decode jwt", "limit": 5}`) and
returns each match's description and full input schema — everything
needed to call it. `run-tool` then runs one by name:

```json
{"name": "run-tool", "arguments": {
  "name": "jwt-decode", "arguments": {"input": "eyJhbGciOi…"}}}
```

The trade-offs: an extra search call before first use of a tool, and
client-side permissions can no longer distinguish tools (everything runs
through `run-tool`). The default mode is better for clients that load
tool schemas lazily, like recent Claude Code.

## How arguments map

- A tool with one input takes `{"input": …}`; tools with named ports (e.g.
  `jwt-verify` with `token` and `key`) take one property per port.
- **Text** ports take a string. **JSON** ports take a JSON value directly
  (or a string containing JSON). **Bytes** ports take a string and use its
  UTF-8 bytes — so `base64-encode` gets text to encode, not base64 to
  double-decode. **Image** ports take a base64 string.
- A variable-arity (multi) port takes an array.
- Options are properties too, with their types, enums, and defaults in the
  schema. Randomness for generators (uuid, password-gen) is filled from
  the OS — the agent doesn't provide it.

## Running a whole chain in one call

The `run-chain` tool composes tools without round-trips. `chain` is a
pipe expression or an inline chain object; `input` is the value fed to
the chain's entry:

```json
{"name": "run-chain", "arguments": {
  "chain": "base64-decode | json-format indent=2",
  "input": "eyJoZWxsbyI6IndvcmxkIn0="}}
```

An inline definition works too:

```json
{"name": "run-chain", "arguments": {
  "chain": {"version": 1,
            "nodes": [{"id": "d", "tool": "base64-decode"},
                      {"id": "s", "tool": "text-stats"}],
            "edges": [{"from": "d", "to": "s"}]},
  "input": "aGVsbG8="}}
```

A chain with declared `inputs` (e.g. a diff's `old` and `new`) takes an
object of named values instead of a string:

```json
{"name": "run-chain", "arguments": {
  "chain": {"version": 1,
            "inputs": [{"name": "old", "binds": [{"node": "d", "port": "old"}]},
                       {"name": "new", "binds": [{"node": "d", "port": "new"}]}],
            "nodes": [{"id": "d", "tool": "text-diff"}], "edges": []},
  "input": {"old": "line one\nline two\n", "new": "line one\nline 2\n"}}}
```

The output is the final step's result. A chain with several final steps
(sinks) returns one block per sink, each tagged with its node id.

Output comes back as text: strings and JSON directly, byte output as text
when it's valid UTF-8 (else base64). A tool error is returned as a normal
result with `isError: true` and the message, so the agent can adjust.

## A quick manual check

```sh
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"number-base","arguments":{"input":"ff","from":16,"to":10}}}' \
  | toolkit mcp
```
