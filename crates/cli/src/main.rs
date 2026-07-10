//! The toolkit CLI. Links every pack natively — one static binary, no
//! network code, data flows stdin -> tools -> stdout.

mod manifests_cmd;

use clap::{Parser, Subcommand};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use toolkit_core::{Chain, DataType, DataValue, Options, Registry};

#[derive(Parser)]
#[command(
    name = "toolkit",
    version,
    about = "Privacy-first data tools. Everything runs locally."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List all available tools
    List,
    /// Show a tool's description and options
    Info { tool: String },
    /// Run a single tool (input from stdin or --input, output to stdout or --output)
    Run {
        tool: String,
        /// Input file. For multi-input tools, repeat as -i port=path
        /// (e.g. -i first=a.png -i second=b.png). Single-input tools
        /// default to stdin.
        #[arg(short, long, value_name = "[PORT=]PATH")]
        input: Vec<String>,
        /// Write output to a file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Set a tool option, e.g. --set width=200 (repeatable)
        #[arg(short, long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
    },
    /// Run a toolchain: pipe syntax, a chain JSON file, or a named chain
    Chain {
        /// Pipe expression, e.g. "base64-decode | json-format indent=4"
        expression: Option<String>,
        /// Load the chain from a JSON file
        #[arg(short, long, conflicts_with = "expression")]
        file: Option<PathBuf>,
        /// Load a named chain from the library (~/.config/toolkit/chains,
        /// then the --chains-dir)
        #[arg(short, long, conflicts_with_all = ["expression", "file"])]
        name: Option<String>,
        /// Project chain library directory
        #[arg(long, default_value = "chains")]
        chains_dir: PathBuf,
        /// Read input from a file instead of stdin
        #[arg(short, long)]
        input: Option<PathBuf>,
        /// Write output to a file (single sink) instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Directory for outputs when the chain has multiple sinks
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Set a declared chain param (name=value) or override a node
        /// option directly (node.option=value); repeatable
        #[arg(short, long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
    },
    /// List the chains available in the chain library
    Chains {
        /// Project chain library directory
        #[arg(long, default_value = "chains")]
        chains_dir: PathBuf,
    },
    /// Emit the full tool catalog as JSON (used by the web build)
    #[command(hide = true)]
    Manifests,
}

/// The per-user chain library: $XDG_CONFIG_HOME/toolkit/chains or
/// ~/.config/toolkit/chains. Chains are data, not code, so dropping files
/// here has no code-trust implications.
fn user_chains_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("toolkit").join("chains"))
}

fn registry() -> Registry {
    Registry::merge([
        toolkit_pack_text::registry(),
        toolkit_pack_image::registry(),
    ])
}

fn main() {
    if let Err(message) = run(Cli::parse()) {
        eprintln!("error: {message}");
        std::process::exit(1);
    }
}

/// "text" for one-port tools, "text…" for a variable-arity port,
/// "first: image, second: image" for multi-port.
fn describe_inputs(m: &toolkit_core::Manifest) -> String {
    let one = |p: &toolkit_core::InputSpec| {
        format!("{}{}", p.data_type.name(), if p.multi { "…" } else { "" })
    };
    match m.sole_input() {
        Some(port) => one(port),
        None => m
            .inputs
            .iter()
            .map(|p| format!("{}: {}", p.name, one(p)))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let registry = registry();
    match cli.command {
        Command::List => {
            let mut manifests = registry.manifests();
            manifests.sort_by(|a, b| a.name.cmp(&b.name));
            let width = manifests.iter().map(|m| m.name.len()).max().unwrap_or(0);
            for m in manifests {
                println!(
                    "{:width$}  {} -> {}\t{}",
                    m.name,
                    describe_inputs(&m),
                    m.output.name(),
                    m.description
                );
            }
            Ok(())
        }
        Command::Info { tool } => {
            let t = registry
                .find(&tool)
                .ok_or_else(|| format!("unknown tool \"{tool}\" (see `toolkit list`)"))?;
            let m = t.manifest();
            println!("{} — {}", m.name, m.label);
            println!("{}", m.description);
            println!(
                "input: {}   output: {}",
                describe_inputs(&m),
                m.output.name()
            );
            if m.options.is_empty() {
                println!("options: none");
            } else {
                println!("options:");
                for o in &m.options {
                    let kind = serde_json::to_value(&o.kind)
                        .ok()
                        .and_then(|v| v.get("kind").and_then(|k| k.as_str().map(String::from)))
                        .unwrap_or_default();
                    let mut extras = Vec::new();
                    if o.required {
                        extras.push("required".to_string());
                    }
                    if let Some(d) = &o.default {
                        extras.push(format!("default: {d}"));
                    }
                    let extras = if extras.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", extras.join(", "))
                    };
                    println!("  --set {}=<{kind}>{extras}  {}", o.name, o.description);
                }
            }
            Ok(())
        }
        Command::Run {
            tool,
            input,
            output,
            set,
        } => {
            let t = registry
                .find(&tool)
                .ok_or_else(|| format!("unknown tool \"{tool}\" (see `toolkit list`)"))?;
            let options = parse_set_options(&set)?;
            let manifest = t.manifest();
            if manifest.streaming {
                // O(1) memory: sources are read in chunks, sequentially in
                // port order, and output is written as it is emitted.
                let sources = input_sources(&manifest, &input)?;
                return stream_run(t, &manifest, sources, &options, output.as_deref());
            }
            let inputs = read_tool_inputs(&manifest, &input)?;
            let result = toolkit_core::run_tool(t, inputs, &options).map_err(|e| e.message)?;
            write_single_output(result, output.as_deref())
        }
        Command::Chain {
            expression,
            file,
            name,
            chains_dir,
            input,
            output,
            output_dir,
            set,
        } => {
            let chain = load_chain(expression, file, name, &chains_dir)?;
            let chain = apply_chain_sets(&chain, &set)?;
            chain.validate(&registry).map_err(|e| e.to_string())?;
            run_chain_streaming(
                &chain,
                &registry,
                input.as_deref(),
                output.as_deref(),
                output_dir,
            )
        }
        Command::Chains { chains_dir } => {
            let mut dirs = Vec::new();
            if let Some(user) = user_chains_dir() {
                dirs.push(("user", user));
            }
            dirs.push(("project", chains_dir));
            let mut any = false;
            for (origin, dir) in dirs {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                let mut files: Vec<PathBuf> = entries
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| {
                        p.extension().is_some_and(|x| x == "json")
                            && p.file_name().is_some_and(|f| f != "index.json")
                    })
                    .collect();
                files.sort();
                for path in files {
                    let Ok(json) = std::fs::read_to_string(&path) else {
                        continue;
                    };
                    let Ok(chain) = serde_json::from_str::<Chain>(&json) else {
                        eprintln!("warning: {} is not a valid chain", path.display());
                        continue;
                    };
                    any = true;
                    let slug = path.file_stem().unwrap_or_default().to_string_lossy();
                    println!("{slug}  [{origin}]  {}", chain.description);
                    println!(
                        "    steps: {}",
                        chain
                            .nodes
                            .iter()
                            .map(|n| n.tool.as_str())
                            .collect::<Vec<_>>()
                            .join(" | ")
                    );
                    for p in &chain.params {
                        let default = p
                            .spec
                            .default
                            .as_ref()
                            .map(|d| format!(" (default: {d})"))
                            .unwrap_or_default();
                        println!(
                            "    --set {}=…{default}  {}",
                            p.spec.name, p.spec.description
                        );
                    }
                }
            }
            if !any {
                println!("no chains found (looked in ~/.config/toolkit/chains and ./chains)");
            }
            Ok(())
        }
        Command::Manifests => {
            println!("{}", manifests_cmd::catalog_json());
            Ok(())
        }
    }
}

/// Build the input set for `run`: `-i path`/stdin for single-port tools,
/// `-i port=path` (repeated) for multi-port tools.
fn read_tool_inputs(
    manifest: &toolkit_core::Manifest,
    specs: &[String],
) -> Result<toolkit_core::Inputs, String> {
    let mut inputs = toolkit_core::Inputs::new();
    if let Some(sole) = manifest.sole_input() {
        // Values may be given bare (-i path, repeatable if the port is
        // multi) or as port=path; stdin when none are given.
        let mut values = Vec::new();
        if specs.is_empty() {
            values.push(read_input(None)?);
        } else {
            for spec in specs {
                let path = match spec.split_once('=') {
                    Some((port, path)) if port == sole.name => path,
                    _ => spec.as_str(),
                };
                values.push(read_input(Some(Path::new(path)))?);
            }
        }
        if !sole.multi && values.len() > 1 {
            return Err(format!(
                "tool \"{}\" takes a single input; give at most one -i",
                manifest.name
            ));
        }
        inputs.insert(sole.name.clone(), values);
        return Ok(inputs);
    }

    for spec in specs {
        let Some((port, path)) = spec.split_once('=') else {
            return Err(format!(
                "tool \"{}\" has {} input ports; use -i port=path (ports: {})",
                manifest.name,
                manifest.inputs.len(),
                manifest
                    .inputs
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        };
        inputs
            .entry(port.to_string())
            .or_default()
            .push(read_input(Some(Path::new(path)))?);
    }
    for port in &manifest.inputs {
        if !inputs.contains_key(&port.name) {
            return Err(format!(
                "missing -i {}=<path> for tool \"{}\"",
                port.name, manifest.name
            ));
        }
    }
    Ok(inputs)
}

/// Ordered streaming sources for a tool invocation: (port, index, path);
/// `None` path means stdin. Mirrors read_tool_inputs' argument rules.
fn input_sources(
    manifest: &toolkit_core::Manifest,
    specs: &[String],
) -> Result<Vec<(String, usize, Option<PathBuf>)>, String> {
    let mut sources = Vec::new();
    if let Some(sole) = manifest.sole_input() {
        if specs.is_empty() {
            sources.push((sole.name.clone(), 0, None));
        } else {
            if !sole.multi && specs.len() > 1 {
                return Err(format!(
                    "tool \"{}\" takes a single input; give at most one -i",
                    manifest.name
                ));
            }
            for (index, spec) in specs.iter().enumerate() {
                let path = match spec.split_once('=') {
                    Some((port, path)) if port == sole.name => path,
                    _ => spec.as_str(),
                };
                sources.push((sole.name.clone(), index, Some(PathBuf::from(path))));
            }
        }
        return Ok(sources);
    }
    // Multi-port tools: gather per port, ordered by manifest port order.
    let mut per_port: std::collections::BTreeMap<&str, Vec<PathBuf>> = Default::default();
    for spec in specs {
        let Some((port, path)) = spec.split_once('=') else {
            return Err(format!(
                "tool \"{}\" has {} input ports; use -i port=path",
                manifest.name,
                manifest.inputs.len()
            ));
        };
        per_port.entry(port).or_default().push(PathBuf::from(path));
    }
    for port in &manifest.inputs {
        let paths = per_port.remove(port.name.as_str()).unwrap_or_default();
        if paths.is_empty() {
            return Err(format!(
                "missing -i {}=<path> for tool \"{}\"",
                port.name, manifest.name
            ));
        }
        for (index, path) in paths.into_iter().enumerate() {
            sources.push((port.name.clone(), index, Some(path)));
        }
    }
    if let Some((port, _)) = per_port.into_iter().next() {
        return Err(format!(
            "tool \"{}\" has no input port \"{port}\"",
            manifest.name
        ));
    }
    Ok(sources)
}

/// Run a streaming tool: sources are consumed sequentially in port order,
/// output is written as emitted. Constant memory for any input size.
fn stream_run(
    tool: &dyn toolkit_core::Tool,
    manifest: &toolkit_core::Manifest,
    sources: Vec<(String, usize, Option<PathBuf>)>,
    options: &Options,
    output: Option<&Path>,
) -> Result<(), String> {
    let mut session = toolkit_core::open_stream_validated(tool, options)
        .map_err(|e| e.message)?
        .unwrap_or_else(|| panic!("tool \"{}\" advertises streaming", manifest.name));

    let mut writer: Box<dyn Write> = match output {
        Some(p) => Box::new(
            std::fs::File::create(p).map_err(|e| format!("cannot create {}: {e}", p.display()))?,
        ),
        None => Box::new(std::io::stdout().lock()),
    };
    let mut buf = vec![0u8; 1 << 20];
    for (port, index, path) in sources {
        let mut reader: Box<dyn Read> = match &path {
            Some(p) => Box::new(
                std::fs::File::open(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?,
            ),
            None => Box::new(std::io::stdin().lock()),
        };
        loop {
            let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            let out = session
                .update(&port, index, &buf[..n])
                .map_err(|e| e.message)?;
            writer.write_all(&out).map_err(|e| e.to_string())?;
        }
        let out = session.end_input(&port, index).map_err(|e| e.message)?;
        writer.write_all(&out).map_err(|e| e.to_string())?;
    }
    let out = session.finish().map_err(|e| e.message)?;
    writer.write_all(&out).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())
}

/// File extension for a sink, from its tool's output type; image formats
/// are sniffed from the first bytes.
fn sink_extension(output: DataType, first_bytes: &[u8]) -> &'static str {
    match output {
        DataType::Text => "txt",
        DataType::Json => "json",
        DataType::Bytes => "bin",
        DataType::Image => match first_bytes {
            [0x89, b'P', b'N', b'G', ..] => "png",
            [0xff, 0xd8, ..] => "jpeg",
            [b'G', b'I', b'F', ..] => "gif",
            [b'B', b'M', ..] => "bmp",
            [b'R', b'I', b'F', b'F', ..] => "webp",
            _ => "img",
        },
    }
}

/// Run a chain through the push engine: input read in chunks, sink output
/// written incrementally. Memory is bounded by the reservoirs (nodes whose
/// tools cannot stream); an all-streaming chain runs in constant memory.
fn run_chain_streaming(
    chain: &Chain,
    registry: &Registry,
    input: Option<&Path>,
    output: Option<&Path>,
    output_dir: Option<PathBuf>,
) -> Result<(), String> {
    let has_outgoing: std::collections::HashSet<&str> =
        chain.edges.iter().map(|e| e.from.as_str()).collect();
    let sinks: Vec<&str> = chain
        .nodes
        .iter()
        .filter(|n| !has_outgoing.contains(n.id.as_str()))
        .map(|n| n.id.as_str())
        .collect();
    let sink_outputs: std::collections::HashMap<String, DataType> = chain
        .nodes
        .iter()
        .filter(|n| sinks.contains(&n.id.as_str()))
        .map(|n| {
            let out = registry.find(&n.tool).expect("validated").manifest().output;
            (n.id.clone(), out)
        })
        .collect();

    let mut reader: Box<dyn Read> = match input {
        Some(p) => Box::new(
            std::fs::File::open(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?,
        ),
        None => Box::new(std::io::stdin().lock()),
    };
    let meta = toolkit_core::ValueMeta {
        data_type: DataType::Bytes,
        format: String::new(),
    };

    if let Some(dir) = output_dir {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut writers: std::collections::HashMap<String, (PathBuf, std::fs::File)> =
            Default::default();
        chain
            .execute_streaming(registry, &meta, &mut reader, false, &mut |id, bytes| {
                if !writers.contains_key(id) {
                    let ext = sink_extension(sink_outputs[id], bytes);
                    let path = dir.join(format!("{id}.{ext}"));
                    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
                    writers.insert(id.to_string(), (path, file));
                }
                let (_, file) = writers.get_mut(id).expect("created above");
                file.write_all(bytes).map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?;
        for (_, (path, _)) in writers {
            eprintln!("wrote {}", path.display());
        }
        Ok(())
    } else if sinks.len() == 1 {
        let mut writer: Box<dyn Write> = match output {
            Some(p) => Box::new(
                std::fs::File::create(p)
                    .map_err(|e| format!("cannot create {}: {e}", p.display()))?,
            ),
            None => Box::new(std::io::stdout().lock()),
        };
        chain
            .execute_streaming(registry, &meta, &mut reader, false, &mut |_, bytes| {
                writer.write_all(bytes).map_err(|e| e.to_string())
            })
            .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())
    } else {
        Err(format!(
            "chain has {} outputs ({}); use --output-dir to write them",
            sinks.len(),
            sinks.join(", ")
        ))
    }
}

/// Split `--set` pairs into declared-param values (plain keys) and direct
/// node-option overrides (`node.option=value`), and apply both. Precedence:
/// node overrides > param values > chain file > tool defaults.
fn apply_chain_sets(chain: &Chain, sets: &[String]) -> Result<Chain, String> {
    let mut params = Options::new();
    let mut overrides: Vec<(String, String, serde_json::Value)> = Vec::new();
    for pair in sets {
        let (key, raw) = pair
            .split_once('=')
            .ok_or_else(|| format!("--set expects key=value, got \"{pair}\""))?;
        let value = serde_json::from_str(raw)
            .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
        if chain.params.iter().any(|p| p.spec.name == key) {
            params.insert(key.to_string(), value);
        } else if let Some((node, option)) = key.split_once('.') {
            overrides.push((node.to_string(), option.to_string(), value));
        } else {
            let known: Vec<&str> = chain.params.iter().map(|p| p.spec.name.as_str()).collect();
            return Err(format!(
                "\"{key}\" is not a declared chain param{} — use node.option=value to override a node directly",
                if known.is_empty() {
                    " (this chain declares none)".to_string()
                } else {
                    format!(" (declared: {})", known.join(", "))
                }
            ));
        }
    }
    let mut chain = chain.with_params(&params).map_err(|e| e.to_string())?;
    for (node_id, option, value) in overrides {
        let node = chain
            .nodes
            .iter_mut()
            .find(|n| n.id == node_id)
            .ok_or_else(|| format!("no node \"{node_id}\" in this chain"))?;
        node.options.insert(option, value);
    }
    Ok(chain)
}

fn load_chain(
    expression: Option<String>,
    file: Option<PathBuf>,
    name: Option<String>,
    chains_dir: &Path,
) -> Result<Chain, String> {
    let json = if let Some(path) = file {
        std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?
    } else if let Some(name) = name {
        // User library first, then the project library.
        let candidates: Vec<PathBuf> = user_chains_dir()
            .into_iter()
            .chain([chains_dir.to_path_buf()])
            .map(|d| d.join(format!("{name}.json")))
            .collect();
        let path = candidates.iter().find(|p| p.exists()).ok_or_else(|| {
            format!(
                "no chain named \"{name}\" (looked for {})",
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read chain \"{name}\" from {}: {e}", path.display()))?
    } else if let Some(expr) = expression {
        return Chain::from_pipe_syntax(&expr).map_err(|e| e.to_string());
    } else {
        return Err("give a pipe expression, --file, or --name".into());
    };
    serde_json::from_str(&json).map_err(|e| format!("invalid chain JSON: {e}"))
}

fn parse_set_options(pairs: &[String]) -> Result<Options, String> {
    let mut options = Options::new();
    for pair in pairs {
        let (key, raw) = pair
            .split_once('=')
            .ok_or_else(|| format!("--set expects key=value, got \"{pair}\""))?;
        let value = serde_json::from_str(raw)
            .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
        options.insert(key.to_string(), value);
    }
    Ok(options)
}

/// Input always enters as Bytes; the coercion matrix turns it into whatever
/// the tool needs (valid UTF-8 for text, parseable JSON, decodable image).
fn read_input(path: Option<&Path>) -> Result<DataValue, String> {
    let bytes = match path {
        Some(p) => std::fs::read(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?,
        None => {
            let mut buf = Vec::new();
            std::io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| format!("cannot read stdin: {e}"))?;
            buf
        }
    };
    Ok(DataValue::Bytes(bytes))
}

fn write_single_output(value: DataValue, path: Option<&Path>) -> Result<(), String> {
    let is_text = value.data_type() == DataType::Text;
    let (_, bytes) = value.into_payload();
    match path {
        Some(p) => {
            std::fs::write(p, bytes).map_err(|e| format!("cannot write {}: {e}", p.display()))
        }
        None => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&bytes).map_err(|e| e.to_string())?;
            // Trailing newline for text on a terminal, so the shell prompt
            // doesn't glue onto the output. Never added when piping.
            if is_text && !bytes.ends_with(b"\n") && is_terminal(&stdout) {
                let _ = stdout.write_all(b"\n");
            }
            Ok(())
        }
    }
}

fn is_terminal(stdout: &std::io::StdoutLock<'_>) -> bool {
    use std::io::IsTerminal;
    stdout.is_terminal()
}
