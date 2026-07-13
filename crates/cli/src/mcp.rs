//! A Model Context Protocol server over stdio, exposing every tool in the
//! registry to an LLM agent. JSON-RPC 2.0, one message per line, logs to
//! stderr — no sockets, so the "no network code" property of the binary
//! holds. The tool schemas are derived from the manifests, so new tools
//! appear automatically.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use toolkit_core::{
    run_tool, Chain, DataValue, InputSpec, Inputs, Manifest, NamedSource, OptionKind, Registry,
    ValueMeta, ENTROPY_LEN,
};

/// Synthetic tool name for running a whole toolchain in one call.
const RUN_CHAIN: &str = "run-chain";

const PROTOCOL_VERSION: &str = "2024-11-05";

/// Run the stdio server until stdin closes.
pub fn serve(registry: &Registry) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("stdin read failed: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write_message(
                    &mut stdout,
                    &error_response(&Value::Null, -32700, &e.to_string()),
                )?;
                continue;
            }
        };
        // Notifications have no "id" and get no response.
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let response = match dispatch(registry, method, &params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error_response(&id, code, &message),
        };
        write_message(&mut stdout, &response)?;
    }
    Ok(())
}

fn dispatch(registry: &Registry, method: &str, params: &Value) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "toolkit", "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_list(registry) })),
        "tools/call" => call_tool(registry, params),
        other => Err((-32601, format!("method not found: {other}"))),
    }
}

fn tool_list(registry: &Registry) -> Vec<Value> {
    let mut manifests = registry.manifests();
    manifests.sort_by(|a, b| a.name.cmp(&b.name));
    let mut tools: Vec<Value> = manifests
        .iter()
        .map(|m| {
            json!({
                "name": m.name,
                "description": m.description,
                "inputSchema": input_schema(m),
            })
        })
        .collect();
    tools.push(run_chain_tool());
    tools
}

/// The synthetic run-chain tool: compose several tools into one call so
/// the agent doesn't shuttle intermediate data back and forth.
fn run_chain_tool() -> Value {
    json!({
        "name": RUN_CHAIN,
        "description": "Run a toolchain in one call. `chain` is either a pipe expression like \"base64-decode | json-format indent=2\" or a chain-definition object ({version, nodes, edges}, optionally with declared `inputs`); `input` is a string for a single-input chain, or an object of named inputs ({old: ..., new: ...}) for a chain with declared inputs. Returns the output(s) of the chain's final step(s).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "chain": {
                    "description": "a pipe expression string, or a chain-definition object",
                },
                "input": {
                    "description": "text for a single-input chain, or an object {name: text} for a chain with declared inputs",
                },
            },
            "required": ["chain", "input"],
        },
    })
}

/// The sole non-entropy input port is exposed as "input"; multiple ports
/// keep their names. (Mirrors the CLI's -i handling.)
fn port_property_name(manifest: &Manifest, port: &InputSpec) -> String {
    let visible = manifest.inputs.iter().filter(|p| !p.entropy).count();
    if visible <= 1 {
        "input".into()
    } else {
        port.name.clone()
    }
}

fn input_schema(manifest: &Manifest) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for port in manifest.inputs.iter().filter(|p| !p.entropy) {
        let name = port_property_name(manifest, port);
        let mut schema = port_value_schema(port.data_type);
        if port.multi {
            schema = json!({ "type": "array", "items": schema });
        }
        // Port description and example from the manifest, so an agent
        // sees what each input means and a known-good value shape. Long
        // examples (cert-decode's whole PEM) would bloat every
        // tools/list response; skip those.
        let example = port.example.as_ref().filter(|e| e.len() <= 120);
        let blurb = match (&port.description, &example) {
            (Some(d), Some(e)) => Some(format!("{d} e.g. {e}")),
            (Some(d), None) => Some(d.clone()),
            (None, Some(e)) => Some(format!("e.g. {e}")),
            (None, None) => None,
        };
        if let (Value::Object(map), Some(blurb)) = (&mut schema, blurb) {
            map.insert("description".into(), json!(blurb));
        }
        properties.insert(name.clone(), schema);
        required.push(json!(name));
    }
    for opt in &manifest.options {
        let mut schema = option_schema(&opt.kind);
        if let Value::Object(map) = &mut schema {
            if !opt.description.is_empty() {
                map.insert("description".into(), json!(opt.description));
            }
            if let Some(default) = &opt.default {
                map.insert("default".into(), default.clone());
            }
        }
        properties.insert(opt.name.clone(), schema);
        if opt.required {
            required.push(json!(opt.name));
        }
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

fn port_value_schema(data_type: toolkit_core::DataType) -> Value {
    use toolkit_core::DataType::*;
    match data_type {
        Text => json!({ "type": "string" }),
        Json => json!({ "description": "a JSON value, or a string containing JSON" }),
        // Bytes take plain text (UTF-8) — agents pass text to base64/hash/…
        Bytes => json!({ "type": "string", "description": "text (its UTF-8 bytes)" }),
        Image => json!({ "type": "string", "description": "base64-encoded image" }),
    }
}

fn option_schema(kind: &OptionKind) -> Value {
    match kind {
        OptionKind::String => json!({ "type": "string" }),
        OptionKind::Bool => json!({ "type": "boolean" }),
        OptionKind::Integer { min, max } => {
            let mut s = json!({ "type": "integer" });
            if let Some(m) = min {
                s["minimum"] = json!(m);
            }
            if let Some(m) = max {
                s["maximum"] = json!(m);
            }
            s
        }
        OptionKind::Float { min, max } => {
            let mut s = json!({ "type": "number" });
            if let Some(m) = min {
                s["minimum"] = json!(m);
            }
            if let Some(m) = max {
                s["maximum"] = json!(m);
            }
            s
        }
        OptionKind::Enum { values } => json!({ "type": "string", "enum": values }),
    }
}

fn call_tool(registry: &Registry, params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "missing tool name".into()))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    // A tool/chain error is a normal MCP result with isError, not a
    // protocol error — the agent should see the message and adjust.
    let outcome = if name == RUN_CHAIN {
        run_chain_call(registry, &args)
    } else {
        match registry.find(name) {
            Some(tool) => {
                run_call(registry, &tool.manifest(), &args).map(|v| vec![output_content(&v)])
            }
            None => return Err((-32602, format!("unknown tool: {name}"))),
        }
    };
    match outcome {
        Ok(content) => Ok(json!({ "content": content, "isError": false })),
        Err(message) => Ok(json!({
            "content": [ { "type": "text", "text": message } ],
            "isError": true,
        })),
    }
}

fn run_chain_call(registry: &Registry, args: &Value) -> Result<Vec<Value>, String> {
    let chain = match args.get("chain") {
        Some(Value::String(expr)) => Chain::from_pipe_syntax(expr).map_err(|e| e.to_string())?,
        Some(obj @ Value::Object(_)) => serde_json::from_value::<Chain>(obj.clone())
            .map_err(|e| format!("invalid chain: {e}"))?,
        _ => return Err("\"chain\" must be a pipe expression or a chain object".into()),
    };

    // (name, bytes) per chain input. A string feeds the sole input; an
    // object supplies a value per declared input by name.
    let payloads = chain_input_payloads(&chain, args.get("input"))?;
    let mut cursors: Vec<(String, std::io::Cursor<Vec<u8>>)> = payloads
        .into_iter()
        .map(|(name, bytes)| (name, std::io::Cursor::new(bytes)))
        .collect();
    let mut sources: Vec<NamedSource<'_>> = cursors
        .iter_mut()
        .map(|(name, reader)| NamedSource {
            name: name.clone(),
            meta: ValueMeta {
                data_type: toolkit_core::DataType::Bytes,
                format: String::new(),
            },
            reader,
        })
        .collect();

    let mut entropy = |n: usize| crate::os_entropy(n);
    let outcome = chain
        .execute_streaming_multi(registry, &mut sources, true, &mut entropy, &mut |_, _| {
            Ok(())
        })
        .map_err(|e| e.to_string())?;

    // One sink -> its value; several -> a labelled block per sink.
    if outcome.sinks.len() == 1 {
        let value = &outcome.outputs[&outcome.sinks[0]];
        Ok(vec![output_content(value)])
    } else {
        Ok(outcome
            .sinks
            .iter()
            .filter_map(|id| outcome.outputs.get(id).map(|v| (id, v)))
            .map(|(id, v)| {
                let mut c = output_content(v);
                c["_sink"] = json!(id);
                c
            })
            .collect())
    }
}

fn chain_input_payloads(
    chain: &Chain,
    input: Option<&Value>,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let declared: Vec<&str> = chain.inputs.iter().map(|i| i.name.as_str()).collect();
    match input {
        // A string feeds the chain's single input (name "" when none are
        // declared, or the one declared name).
        Some(Value::String(text)) => {
            let name = chain.sole_input_name().map_err(|e| e.to_string())?;
            Ok(vec![(name, text.as_bytes().to_vec())])
        }
        // An object supplies a value per declared input.
        Some(Value::Object(map)) => {
            if declared.is_empty() {
                return Err("this chain takes a single input; pass \"input\" as a string".into());
            }
            let mut payloads = Vec::new();
            for name in &declared {
                let value = map
                    .get(*name)
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("missing string input \"{name}\""))?;
                payloads.push((name.to_string(), value.as_bytes().to_vec()));
            }
            if let Some(extra) = map.keys().find(|k| !declared.contains(&k.as_str())) {
                return Err(format!("chain has no input named \"{extra}\""));
            }
            Ok(payloads)
        }
        _ => Err("\"input\" must be a string, or an object of named inputs".into()),
    }
}

fn run_call(registry: &Registry, manifest: &Manifest, args: &Value) -> Result<DataValue, String> {
    let tool = registry.find(&manifest.name).expect("looked up already");
    let mut inputs = Inputs::new();
    for port in &manifest.inputs {
        if port.entropy {
            inputs.insert(
                port.name.clone(),
                vec![DataValue::Bytes(crate::os_entropy(ENTROPY_LEN)?)],
            );
            continue;
        }
        let key = port_property_name(manifest, port);
        let arg = args
            .get(&key)
            .ok_or_else(|| format!("missing input \"{key}\""))?;
        let values = if port.multi {
            let array = arg
                .as_array()
                .ok_or_else(|| format!("input \"{key}\" must be an array"))?;
            array
                .iter()
                .map(|v| to_value(port.data_type, v))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![to_value(port.data_type, arg)?]
        };
        inputs.insert(port.name.clone(), values);
    }
    let options = args
        .as_object()
        .map(|m| {
            m.iter()
                .filter(|(k, _)| manifest.options.iter().any(|o| &o.name == *k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();
    run_tool(tool, inputs, &options).map_err(|e| e.message)
}

fn to_value(data_type: toolkit_core::DataType, arg: &Value) -> Result<DataValue, String> {
    use toolkit_core::DataType::*;
    match data_type {
        Text => arg
            .as_str()
            .map(|s| DataValue::Text(s.to_string()))
            .ok_or_else(|| "expected a string".into()),
        Json => match arg {
            // A JSON value passed directly, or a string containing JSON.
            Value::String(s) => serde_json::from_str(s)
                .map(DataValue::Json)
                .map_err(|e| format!("not valid JSON: {e}")),
            other => Ok(DataValue::Json(other.clone())),
        },
        // A Bytes port takes the string's UTF-8 bytes (so base64-encode
        // gets text to encode, not base64 to double-decode).
        Bytes => arg
            .as_str()
            .map(|s| DataValue::Bytes(s.as_bytes().to_vec()))
            .ok_or_else(|| "expected a string".into()),
        // Images are binary: base64.
        Image => {
            let s = arg.as_str().ok_or("expected a base64 string")?;
            B64.decode(s)
                .map(DataValue::Bytes)
                .map_err(|e| format!("not valid base64: {e}"))
        }
    }
}

fn output_content(value: &DataValue) -> Value {
    match value {
        DataValue::Text(s) => json!({ "type": "text", "text": s }),
        DataValue::Json(v) => {
            json!({ "type": "text", "text": serde_json::to_string_pretty(v).unwrap_or_default() })
        }
        // Show byte output as text when it's valid UTF-8 (base64-decode of
        // readable data), else base64.
        DataValue::Bytes(b) => match std::str::from_utf8(b) {
            Ok(text) => json!({ "type": "text", "text": text }),
            Err(_) => json!({
                "type": "text",
                "text": B64.encode(b),
                "_note": "base64-encoded bytes",
            }),
        },
        DataValue::Image { bytes, format } => json!({
            "type": "text",
            "text": B64.encode(bytes),
            "_note": format!("base64-encoded {format} image"),
        }),
    }
}

fn error_response(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn write_message(out: &mut impl Write, message: &Value) -> Result<(), String> {
    let line = serde_json::to_string(message).map_err(|e| e.to_string())?;
    writeln!(out, "{line}").map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_and_list_shape() {
        let reg = crate::registry();
        let init = dispatch(&reg, "initialize", &Value::Null).unwrap();
        assert_eq!(init["serverInfo"]["name"], "toolkit");
        assert_eq!(init["capabilities"]["tools"], json!({}));

        let list = dispatch(&reg, "tools/list", &Value::Null).unwrap();
        let tools = list["tools"].as_array().unwrap();
        assert!(tools.len() > 50);
        let hash = tools.iter().find(|t| t["name"] == "hash").unwrap();
        // Options surface as schema properties with enum values.
        let algs = &hash["inputSchema"]["properties"]["algorithm"]["enum"];
        assert!(algs.as_array().unwrap().contains(&json!("sha256")));
        assert_eq!(hash["inputSchema"]["required"], json!(["input"]));
    }

    #[test]
    fn call_text_and_bytes_ports() {
        let reg = crate::registry();
        // Bytes port takes plain text, not base64.
        let r = call_tool(
            &reg,
            &json!({ "name": "base64-encode", "arguments": { "input": "hi" } }),
        )
        .unwrap();
        assert_eq!(r["isError"], false);
        assert_eq!(r["content"][0]["text"], "aGk=");

        // Byte output that is valid UTF-8 comes back as text.
        let r = call_tool(
            &reg,
            &json!({ "name": "base64-decode", "arguments": { "input": "aGk=" } }),
        )
        .unwrap();
        assert_eq!(r["content"][0]["text"], "hi");
    }

    #[test]
    fn multi_port_and_options_and_errors() {
        let reg = crate::registry();
        let r = call_tool(
            &reg,
            &json!({ "name": "doc-merge",
                     "arguments": { "input": ["a", "b"], "separator": "-" } }),
        )
        .unwrap();
        assert_eq!(r["content"][0]["text"], "a-b");

        // A tool error is a result with isError, not a JSON-RPC error.
        let r = call_tool(
            &reg,
            &json!({ "name": "number-base", "arguments": { "input": "zz", "from": 16 } }),
        )
        .unwrap();
        assert_eq!(r["isError"], true);

        // Unknown method / tool are protocol errors.
        assert!(dispatch(&reg, "nope", &Value::Null).is_err());
        assert!(call_tool(&reg, &json!({ "name": "nope", "arguments": {} })).is_err());
    }

    #[test]
    fn run_chain_pipe_and_inline_json() {
        let reg = crate::registry();
        // run-chain is advertised.
        let list = dispatch(&reg, "tools/list", &Value::Null).unwrap();
        assert!(list["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == RUN_CHAIN));

        // Pipe expression.
        let r = call_tool(
            &reg,
            &json!({ "name": RUN_CHAIN, "arguments": {
                "chain": "base64-decode | json-format", "input": "eyJhIjoxfQ==" } }),
        )
        .unwrap();
        assert_eq!(r["isError"], false);
        assert!(r["content"][0]["text"].as_str().unwrap().contains("\"a\""));

        // Inline chain object.
        let r = call_tool(
            &reg,
            &json!({ "name": RUN_CHAIN, "arguments": {
                "chain": { "version": 1,
                           "nodes": [{"id": "h", "tool": "hex-encode"}],
                           "edges": [] },
                "input": "hi" } }),
        )
        .unwrap();
        assert_eq!(r["content"][0]["text"], "6869");

        // A bad chain is an isError result, not a protocol error.
        let r = call_tool(
            &reg,
            &json!({ "name": RUN_CHAIN, "arguments": { "chain": "no-such-tool", "input": "x" } }),
        )
        .unwrap();
        assert_eq!(r["isError"], true);
    }

    #[test]
    fn run_chain_multi_input_and_multi_sink() {
        let reg = crate::registry();

        // Multi-input: text-diff with declared old/new inputs.
        let chain = json!({
            "version": 1,
            "inputs": [
                { "name": "old", "binds": [{ "node": "d", "port": "old" }] },
                { "name": "new", "binds": [{ "node": "d", "port": "new" }] },
            ],
            "nodes": [{ "id": "d", "tool": "text-diff" }],
            "edges": [],
        });
        let r = call_tool(
            &reg,
            &json!({ "name": RUN_CHAIN, "arguments": {
                "chain": chain, "input": { "old": "a\nb\n", "new": "a\nc\n" } } }),
        )
        .unwrap();
        assert_eq!(r["isError"], false);
        assert!(r["content"][0]["text"].as_str().unwrap().contains("-b"));

        // A named input the chain doesn't declare is an error.
        let bad = call_tool(
            &reg,
            &json!({ "name": RUN_CHAIN, "arguments": {
                "chain": chain, "input": { "old": "a", "new": "b", "extra": "x" } } }),
        )
        .unwrap();
        assert_eq!(bad["isError"], true);

        // Multi-sink: two entry nodes, both sinks; one input feeds both.
        let fanout = json!({
            "version": 1,
            "nodes": [{ "id": "e", "tool": "base64-encode" },
                      { "id": "h", "tool": "hex-encode" }],
            "edges": [],
        });
        let r = call_tool(
            &reg,
            &json!({ "name": RUN_CHAIN, "arguments": { "chain": fanout, "input": "hi" } }),
        )
        .unwrap();
        let sinks: Vec<(&str, &str)> = r["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c["_sink"].as_str().unwrap(), c["text"].as_str().unwrap()))
            .collect();
        assert!(sinks.contains(&("e", "aGk=")));
        assert!(sinks.contains(&("h", "6869")));
    }

    #[test]
    fn json_port_accepts_value_or_string() {
        let reg = crate::registry();
        // json-diff takes two json ports; pass values directly.
        let r = call_tool(
            &reg,
            &json!({ "name": "json-diff",
                     "arguments": { "left": {"a": 1}, "right": {"a": 2} } }),
        )
        .unwrap();
        assert!(r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("changed"));
    }
}
