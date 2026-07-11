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
