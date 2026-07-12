//! The toolkit CLI. Links every pack natively — one static binary, no
//! network code, data flows stdin -> tools -> stdout.

mod manifests_cmd;
mod mcp;

include!(concat!(env!("OUT_DIR"), "/builtin_chains.rs"));

/// A built-in chain's JSON by name (user and project files take priority
/// at the call sites).
fn builtin_chain(name: &str) -> Option<&'static str> {
    BUILTIN_CHAINS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, json)| *json)
}

use clap::{Parser, Subcommand};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use toolkit_core::{Chain, DataType, DataValue, Options, Registry};

#[derive(Parser)]
#[command(
    name = "toolkit",
    version,
    about = "Everyday data tools — handy, fast, and running entirely on your machine."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List all available tools
    Tools,
    /// Show a tool's description and options
    Info { tool: String },
    /// Run a single tool (input as an argument, from stdin, or --input)
    RunTool {
        #[arg(index = 1)]
        tool: String,
        /// The input value itself, e.g. `toolkit run-tool base64-encode hello`.
        /// Only for tools with one input port (repeatable for
        /// variable-arity ports); use -i for files or named ports.
        #[arg(value_name = "VALUE", conflicts_with = "input", index = 2)]
        value: Vec<String>,
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
    RunChain {
        /// Pipe expression, e.g. "base64-decode | json-format indent=4"
        expression: Option<String>,
        /// Load the chain from a JSON file
        #[arg(short, long, conflicts_with = "expression")]
        file: Option<PathBuf>,
        /// Load a named chain from the library (--chains-dir if given,
        /// then ~/.config/toolkit/chains, then the chains built into this
        /// binary)
        #[arg(short, long, conflicts_with_all = ["expression", "file"])]
        name: Option<String>,
        /// Extra chain directory to search first (e.g. a project's)
        #[arg(long)]
        chains_dir: Option<PathBuf>,
        /// Input file. For chains with declared inputs, repeat as
        /// -i name=path (e.g. -i old=a.txt -i new=b.txt). Chains with
        /// one input default to stdin.
        #[arg(short, long, value_name = "[NAME=]PATH")]
        input: Vec<String>,
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
        /// Extra chain directory to include (e.g. a project's)
        #[arg(long)]
        chains_dir: Option<PathBuf>,
    },
    /// Generate shell completions (tool names included). The install
    /// scripts refresh these paths on every update:
    ///   zsh:  toolkit completions zsh > ~/.zsh/completions/_toolkit
    ///   fish: toolkit completions fish > ~/.config/fish/completions/toolkit.fish
    ///   bash: toolkit completions bash > ~/.local/share/bash-completion/completions/toolkit
    ///   pwsh: toolkit completions powershell > "$env:LOCALAPPDATA\toolkit\completions.ps1"
    ///         (then dot-source that file from $PROFILE)
    Completions { shell: clap_complete::Shell },
    /// Run a Model Context Protocol server over stdio, exposing every
    /// tool to an LLM agent (JSON-RPC on stdin/stdout; no network)
    Mcp,
    /// Show how to update (toolkit never touches the network, so the
    /// separate `toolkit-update` command does it)
    Update,
    /// Emit the full tool catalog as JSON (used by the web build)
    #[command(hide = true)]
    Manifests,
    /// Completion callback: candidates for dynamic positions, given the
    /// words typed so far (the completion scripts call this; not for
    /// humans). Kinds: "set" (option/param key=value) and "chain-name".
    #[command(hide = true, name = "__complete")]
    CompleteCallback {
        kind: String,
        #[arg(default_value = "")]
        current: String,
        #[arg(last = true, num_args = 0..)]
        words: Vec<String>,
    },
}

/// key=value candidates from option specs: names as "key=" while the key
/// is typed, enum/bool values once it has one.
fn candidates_from_specs(specs: &[toolkit_core::OptionSpec], current: &str) -> Vec<String> {
    match current.split_once('=') {
        None => specs.iter().map(|o| format!("{}=", o.name)).collect(),
        Some((key, _)) => {
            let Some(spec) = specs.iter().find(|o| o.name == key) else {
                return Vec::new();
            };
            match &spec.kind {
                toolkit_core::OptionKind::Enum { values } => {
                    values.iter().map(|v| format!("{key}={v}")).collect()
                }
                toolkit_core::OptionKind::Bool => {
                    vec![format!("{key}=true"), format!("{key}=false")]
                }
                _ => Vec::new(),
            }
        }
    }
}

/// The value of a flag in the typed words, e.g. flag_value(w, &["-n", "--name"]).
fn flag_value<'a>(words: &'a [String], flags: &[&str]) -> Option<&'a str> {
    words
        .windows(2)
        .rev()
        .find(|w| flags.contains(&w[0].as_str()))
        .map(|w| w[1].as_str())
}

/// All completion candidates for the given kind and context.
fn complete_candidates(
    registry: &Registry,
    kind: &str,
    current: &str,
    words: &[String],
) -> Vec<String> {
    let chains_dir = flag_value(words, &["--chains-dir"]).map(PathBuf::from);
    match kind {
        "chain-name" => {
            let mut names: Vec<String> = chain_dirs(chains_dir.as_deref())
                .into_iter()
                .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten())
                .filter_map(|entry| {
                    let path = entry.path();
                    (path.extension()? == "json")
                        .then(|| path.file_stem()?.to_str().map(String::from))?
                })
                .chain(BUILTIN_CHAINS.iter().map(|(n, _)| n.to_string()))
                .collect();
            names.sort();
            names.dedup();
            names
        }
        "set" => {
            let in_chain = words.iter().any(|w| w == "run-chain");
            if in_chain {
                let file = flag_value(words, &["-f", "--file"]).map(PathBuf::from);
                let name = flag_value(words, &["-n", "--name"]).map(String::from);
                let Ok(chain) = load_chain(None, file, name, chains_dir.as_deref()) else {
                    return Vec::new();
                };
                // --set accepts declared params AND node.option overrides;
                // offer both. Prefixing the option specs with the node id
                // lets one code path handle keys and values for either.
                let mut specs: Vec<toolkit_core::OptionSpec> =
                    chain.params.iter().map(|p| p.spec.clone()).collect();
                for node in &chain.nodes {
                    if let Some(tool) = registry.find(&node.tool) {
                        for opt in tool.manifest().options {
                            let mut prefixed = opt.clone();
                            prefixed.name = format!("{}.{}", node.id, opt.name);
                            specs.push(prefixed);
                        }
                    }
                }
                candidates_from_specs(&specs, current)
            } else {
                complete_set_candidates(registry, current, words)
            }
        }
        _ => Vec::new(),
    }
}

/// Post-process generated completion scripts so `run --set` completes
/// real option keys and enum/bool values: the scripts call back into the
/// binary (`toolkit __complete`) at completion time, which knows the
/// manifests. Static generators can't express context-dependent values.
fn patch_completions(shell: clap_complete::Shell, script: String, tool_names: &[String]) -> String {
    // clap_complete suggests hidden subcommands (hide covers --help, not
    // completions); scrub them from every shell's suggestion lists. The
    // dispatch arms stay — they're only reachable by typing the name.
    const HIDDEN: [&str; 2] = ["manifests", "__complete"];
    let script: String = match shell {
        clap_complete::Shell::Zsh | clap_complete::Shell::Fish => {
            script
                .lines()
                .filter(|line| {
                    !HIDDEN.iter().any(|h| {
                        line.trim_start().starts_with(&format!("'{h}:"))
                            || line.contains(&format!("-a \"{h}\""))
                    })
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        }
        clap_complete::Shell::Bash => {
            script
                .lines()
                .map(|line| {
                    if line.trim_start().starts_with("opts=\"") {
                        let mut l = line.to_string();
                        for h in HIDDEN {
                            l = l.replace(&format!(" {h} "), " ");
                        }
                        l
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        }
        _ => script,
    };
    match shell {
        clap_complete::Shell::Zsh => {
            // Rewire only the run subcommand's --set/-s (identified by
            // its help text; chain's --set has different text).
            let mut script = script
                .lines()
                .map(|line| {
                    if line.contains("Set a tool option")
                        || line.contains("Set a declared chain param")
                    {
                        line.replace(":KEY=VALUE:_default", ":KEY=VALUE:_toolkit_set_values")
                    } else if line.contains("Load a named chain from the library") {
                        line.replace(":NAME:_default", ":NAME:_toolkit_chain_names")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            // Inside a subcommand's _arguments, $words is re-based at the
            // subcommand ("run" ...), so the binary comes from $service.
            let func = "\n_toolkit_set_values() {\n    local -a candidates\n    candidates=(${(f)\"$(\"${service:-toolkit}\" __complete set \"${PREFIX}${SUFFIX}\" -- \"${words[@]}\" 2>/dev/null)\"})\n    (( ${#candidates} )) && compadd -S '' -- \"${candidates[@]}\"\n}\n\n_toolkit_chain_names() {\n    local -a candidates\n    candidates=(${(f)\"$(\"${service:-toolkit}\" __complete chain-name \"${PREFIX}${SUFFIX}\" -- \"${words[@]}\" 2>/dev/null)\"})\n    (( ${#candidates} )) && compadd -- \"${candidates[@]}\"\n}\n\n";
            // Define the function before the trailing dispatch block, so
            // it exists on the very first completion invocation.
            match script.find("if [ \"$funcstack[1]\" = \"_toolkit\" ]; then") {
                Some(pos) => script.insert_str(pos, func),
                None => script.push_str(func),
            }
            script
        }
        clap_complete::Shell::Bash => {
            // Patch the two --set/-s arms inside the run section only.
            let mut script = script;
            for (label, set_kind_arms, name_arms) in [
                ("toolkit__subcmd__run__subcmd__tool)", true, false),
                ("toolkit__subcmd__run__subcmd__chain)", true, true),
            ] {
                script = patch_bash_section(script, label, set_kind_arms, name_arms);
            }
            script
        }
        clap_complete::Shell::Fish => {
            // The fish generator skips positional values: append tool-name
            // completion for run, option/param completion for --set on
            // both subcommands, and chain-name completion for -n/--name.
            format!(
                "{script}complete -c toolkit -n \"__fish_toolkit_using_subcommand run-tool\" -f -a \"{}\"\ncomplete -c toolkit -n \"__fish_toolkit_using_subcommand run-tool\" -s s -l set -x -a \"(toolkit __complete set (commandline -ct) -- (commandline -opc))\"\ncomplete -c toolkit -n \"__fish_toolkit_using_subcommand run-chain\" -s s -l set -x -a \"(toolkit __complete set (commandline -ct) -- (commandline -opc))\"\ncomplete -c toolkit -n \"__fish_toolkit_using_subcommand run-chain\" -s n -l name -x -a \"(toolkit __complete chain-name (commandline -ct) -- (commandline -opc))\"\n",
                tool_names.join(" ")
            )
        }
        _ => script,
    }
}

/// Rewire one bash subcommand section: '=' wordbreak normalization plus
/// callback-driven --set/-s (and, for chain, --name/-n) arms.
fn patch_bash_section(script: String, label: &str, set_arms: bool, name_arms: bool) -> String {
    let Some(run_start) = script.find(label) else {
        return script;
    };
    // readline breaks words on '=', so `--set key=<cur>` arrives
    // as prev="=" (or cur="="). Reassemble those shapes into the
    // canonical prev="--set", cur="key=value" before the case.
    let prologue = "\n            if [[ ${COMP_WORDS[COMP_CWORD]} == = && ( ${COMP_WORDS[COMP_CWORD-2]:-} == --set || ${COMP_WORDS[COMP_CWORD-2]:-} == -s ) ]]; then\n                prev=\"--set\"; cur=\"${COMP_WORDS[COMP_CWORD-1]}=\"\n            elif [[ ${COMP_WORDS[COMP_CWORD-1]:-} == = && ( ${COMP_WORDS[COMP_CWORD-3]:-} == --set || ${COMP_WORDS[COMP_CWORD-3]:-} == -s ) ]]; then\n                prev=\"--set\"; cur=\"${COMP_WORDS[COMP_CWORD-2]}=${cur}\"\n            fi";
    let mut script = script;
    script.insert_str(run_start + label.len(), prologue);
    let run_end = script[run_start..]
        .find("\n        toolkit__")
        .map(|i| run_start + i)
        .unwrap_or(script.len());
    let old_arm = "COMPREPLY=($(compgen -f \"${cur}\"))\n                    return 0";
    // Candidates come back as full key=value; when readline split
    // at '=', it matches only the part after it, so strip the key.
    let set_arm = "local setcands\n                    setcands=\"$(\"${COMP_WORDS[0]}\" __complete set \"${cur}\" -- \"${COMP_WORDS[@]}\" 2>/dev/null)\"\n                    if [[ ${cur} == *=* ]]; then setcands=\"${setcands//${cur%%=*}=/}\"; fi\n                    COMPREPLY=($(compgen -W \"${setcands}\" -- \"${cur#*=}\"))\n                    [[ ${COMPREPLY-} == *= ]] && compopt -o nospace 2>/dev/null\n                    return 0";
    let name_arm = "COMPREPLY=($(compgen -W \"$(\"${COMP_WORDS[0]}\" __complete chain-name \"${cur}\" -- \"${COMP_WORDS[@]}\" 2>/dev/null)\" -- \"${cur}\"))\n                    return 0";
    let mut section = script[run_start..run_end].to_string();
    if set_arms {
        for marker in ["--set)", "-s)"] {
            if let Some(arm_pos) = section.find(marker) {
                if let Some(body_pos) = section[arm_pos..].find(old_arm) {
                    let at = arm_pos + body_pos;
                    section.replace_range(at..at + old_arm.len(), set_arm);
                }
            }
        }
    }
    if name_arms {
        for marker in ["--name)", "-n)"] {
            if let Some(arm_pos) = section.find(marker) {
                if let Some(body_pos) = section[arm_pos..].find(old_arm) {
                    let at = arm_pos + body_pos;
                    section.replace_range(at..at + old_arm.len(), name_arm);
                }
            }
        }
    }
    format!("{}{}{}", &script[..run_start], section, &script[run_end..])
}

/// Candidates for completing `--set` on `toolkit run`: option names as
/// "key=" while the key is being typed, enum/bool values once it has one.
/// `words` is the full command line so far; the tool is the first
/// non-flag word after `run` that isn't a value of a value-taking flag.
fn complete_set_candidates(registry: &Registry, current: &str, words: &[String]) -> Vec<String> {
    let value_flags = ["-i", "--input", "-o", "--output", "-s", "--set"];
    let mut after_run = words
        .iter()
        .skip_while(|w| w.as_str() != "run-tool")
        .skip(1);
    let mut tool_name = None;
    while let Some(word) = after_run.next() {
        if value_flags.contains(&word.as_str()) {
            after_run.next(); // skip the flag's value
        } else if !word.starts_with('-') {
            tool_name = Some(word.as_str());
            break;
        }
    }
    let Some(tool) = tool_name.and_then(|n| registry.find(n)) else {
        return Vec::new();
    };
    candidates_from_specs(&tool.manifest().options, current)
}

/// The per-user chain library: $XDG_CONFIG_HOME/toolkit/chains,
/// ~/.config/toolkit/chains, or %USERPROFILE%\.config\toolkit\chains on
/// Windows. Chains are data, not code, so dropping files here has no
/// code-trust implications.
/// Chain lookup directories, highest priority first: an explicit
/// --chains-dir, then the user library. (Built-ins come after these at
/// each call site.) Deliberately NOT the current directory: a stray
/// ./chains folder should never change what a chain name means.
fn chain_dirs(explicit: Option<&Path>) -> Vec<PathBuf> {
    explicit
        .map(Path::to_path_buf)
        .into_iter()
        .chain(user_chains_dir())
        .collect()
}

fn user_chains_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("toolkit").join("chains"))
}

fn registry() -> Registry {
    Registry::merge([
        toolkit_pack_text::registry(),
        toolkit_pack_image::registry(),
        toolkit_pack_crypto::registry(),
        toolkit_pack_data::registry(),
    ])
}

/// The stdlib's message when print!/println! hit a closed pipe — and
/// only that. A failed write for any other reason (disk full, I/O error)
/// must stay a loud panic, or data loss goes unnoticed.
fn is_broken_pipe_panic(message: &str) -> bool {
    message.starts_with("failed printing to")
        && (message.contains("Broken pipe") || message.contains("os error 232"))
}

fn main() {
    // Rust ignores SIGPIPE at startup, which turns `toolkit tools | head`
    // into a "failed printing to stdout: Broken pipe" panic once the
    // reader goes away. Restore the default so a closed pipe ends the
    // process quietly, like every other CLI. (The env override exists so
    // tests can exercise the panic-hook path below on Unix.)
    #[cfg(unix)]
    if std::env::var_os("TOOLKIT_SKIP_SIGPIPE_RESET").is_none() {
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }
    // Windows has no SIGPIPE: a closed pipe surfaces as a print panic
    // instead. Exit quietly for that one case, and only that one.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("");
        if is_broken_pipe_panic(message) {
            std::process::exit(0);
        }
        default_hook(info);
    }));
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

/// Greedy word-wrap; words longer than the width go on their own line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Three aligned columns; the last wraps to the terminal width ($COLUMNS,
/// defaulting to 100) with a hanging indent.
fn print_table(header: (&str, &str, &str), rows: &[(String, String, String)]) {
    let count = |s: &str| s.chars().count();
    let w1 = rows
        .iter()
        .map(|r| count(&r.0))
        .chain([count(header.0)])
        .max()
        .unwrap_or(0);
    let w2 = rows
        .iter()
        .map(|r| count(&r.1))
        .chain([count(header.1)])
        .max()
        .unwrap_or(0);
    let total: usize = std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(100);
    let desc_width = total.saturating_sub(w1 + w2 + 4).max(30);

    println!("{:<w1$}  {:<w2$}  {}", header.0, header.1, header.2);
    for (name, sig, desc) in rows {
        for (i, line) in wrap(desc, desc_width).iter().enumerate() {
            if i == 0 {
                println!("{name:<w1$}  {sig:<w2$}  {line}");
            } else {
                println!("{:<w1$}  {:<w2$}  {line}", "", "");
            }
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let registry = registry();
    match cli.command {
        Command::Tools => {
            let mut manifests = registry.manifests();
            manifests.sort_by(|a, b| a.name.cmp(&b.name));
            let rows: Vec<(String, String, String)> = manifests
                .iter()
                .map(|m| {
                    (
                        m.name.clone(),
                        format!("{} -> {}", describe_inputs(m), m.output.name()),
                        m.description.clone(),
                    )
                })
                .collect();
            print_table(("NAME", "INPUT -> OUTPUT", "DESCRIPTION"), &rows);
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
        Command::RunTool {
            tool,
            value,
            input,
            output,
            set,
        } => {
            let t = registry
                .find(&tool)
                .ok_or_else(|| format!("unknown tool \"{tool}\" (see `toolkit list`)"))?;
            let options = parse_set_options(&set)?;
            let manifest = t.manifest();
            if !value.is_empty() {
                // Values given on the command line are small by nature, so
                // the buffered path is fine even for streaming tools.
                let inputs = positional_inputs(&manifest, value)?;
                let result = toolkit_core::run_tool(t, inputs, &options).map_err(|e| e.message)?;
                return write_single_output(result, output.as_deref());
            }
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
        Command::RunChain {
            expression,
            file,
            name,
            chains_dir,
            input,
            output,
            output_dir,
            set,
        } => {
            let chain = load_chain(expression, file, name, chains_dir.as_deref())?;
            let chain = apply_chain_sets(&chain, &set)?;
            chain.validate(&registry).map_err(|e| e.to_string())?;
            run_chain_streaming(&chain, &registry, &input, output.as_deref(), output_dir)
        }
        Command::Chains { chains_dir } => {
            fn print_chain(slug: &str, origin: &str, chain: &Chain) {
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

            let mut dirs = Vec::new();
            if let Some(explicit) = chains_dir {
                dirs.push(("dir", explicit));
            }
            if let Some(user) = user_chains_dir() {
                dirs.push(("user", user));
            }
            let mut seen = std::collections::HashSet::new();
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
                    let slug = path.file_stem().unwrap_or_default().to_string_lossy();
                    seen.insert(slug.to_string());
                    print_chain(&slug, origin, &chain);
                }
            }
            // The library shipped inside the binary, minus anything a
            // user or project file overrides.
            for (name, json) in BUILTIN_CHAINS {
                if seen.contains(*name) {
                    continue;
                }
                let chain: Chain = serde_json::from_str(json).expect("built-in chains are valid");
                print_chain(name, "built-in", &chain);
            }
            Ok(())
        }
        Command::Completions { shell } => {
            use clap::CommandFactory;
            // The registry is static, so completions can offer real tool
            // names after `run`. Injected only here — the parser keeps its
            // friendlier unknown-tool error.
            let names: Vec<String> = registry.manifests().into_iter().map(|m| m.name).collect();
            let cmd = Cli::command().mut_subcommand("run-tool", |run| {
                run.mut_arg("tool", |a| {
                    a.value_parser(clap::builder::PossibleValuesParser::new(names.clone()))
                })
            });
            let mut buf = Vec::new();
            clap_complete::generate(shell, &mut cmd.clone(), "toolkit", &mut buf);
            let script = String::from_utf8(buf).expect("completion scripts are UTF-8");
            print!("{}", patch_completions(shell, script, &names));
            Ok(())
        }
        Command::CompleteCallback {
            kind,
            current,
            words,
        } => {
            // 0.6.0 scripts passed <current> with no kind; treat an
            // unrecognized kind as that legacy form.
            let (kind, current) = match kind.as_str() {
                "set" | "chain-name" => (kind, current),
                legacy => ("set".to_string(), legacy.to_string()),
            };
            for candidate in complete_candidates(&registry, &kind, &current, &words) {
                println!("{candidate}");
            }
            Ok(())
        }
        Command::Mcp => mcp::serve(&registry),
        Command::Update => {
            println!(
                "toolkit never touches the network or spawns processes, so it\n\
                 cannot update itself. Run the separate updater installed next\n\
                 to it:\n\
                 \n    toolkit-update\n\n\
                 Installed another way (or no toolkit-update yet)?\n\
                 \n    curl -fsSL https://raw.githubusercontent.com/koundinyagoparaju/toolkit/main/scripts/install.sh | sh\n\
                 \n    Homebrew: brew upgrade toolkit    Scoop: scoop update toolkit"
            );
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
/// Positional VALUEs -> the tool's single non-entropy port, as text.
fn positional_inputs(
    manifest: &toolkit_core::Manifest,
    values: Vec<String>,
) -> Result<toolkit_core::Inputs, String> {
    let ports: Vec<&toolkit_core::InputSpec> =
        manifest.inputs.iter().filter(|p| !p.entropy).collect();
    let port = match ports.as_slice() {
        [one] => one,
        [] => {
            return Err(format!(
                "tool \"{}\" takes no input; drop the argument",
                manifest.name
            ))
        }
        many => {
            return Err(format!(
                "tool \"{}\" has {} input ports; use -i port=path (ports: {})",
                manifest.name,
                many.len(),
                many.iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
    };
    if !port.multi && values.len() > 1 {
        return Err(format!(
            "tool \"{}\" takes a single input, got {} arguments — quote the value?",
            manifest.name,
            values.len()
        ));
    }
    let mut inputs = toolkit_core::Inputs::new();
    inputs.insert(
        port.name.clone(),
        values.into_iter().map(DataValue::Text).collect(),
    );
    for p in &manifest.inputs {
        if p.entropy {
            inputs.insert(
                p.name.clone(),
                vec![DataValue::Bytes(os_entropy(toolkit_core::ENTROPY_LEN)?)],
            );
        }
    }
    Ok(inputs)
}

fn read_tool_inputs(
    manifest: &toolkit_core::Manifest,
    specs: &[String],
) -> Result<toolkit_core::Inputs, String> {
    let mut inputs = toolkit_core::Inputs::new();
    if let Some(sole) = manifest.sole_input() {
        if sole.entropy && specs.is_empty() {
            inputs.insert(
                sole.name.clone(),
                vec![DataValue::Bytes(os_entropy(toolkit_core::ENTROPY_LEN)?)],
            );
            return Ok(inputs);
        }
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
            if port.entropy {
                inputs.insert(
                    port.name.clone(),
                    vec![DataValue::Bytes(os_entropy(toolkit_core::ENTROPY_LEN)?)],
                );
                continue;
            }
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
/// One opened chain input: its declared name and its reader.
type ChainSource = (String, Box<dyn Read>);

/// Open one reader per chain input from `-i [name=]path` specs. Chains
/// without declared inputs (or with exactly one) accept a bare path or
/// default to stdin; multiple declared inputs must each be named.
fn open_chain_sources(chain: &Chain, input: &[String]) -> Result<Vec<ChainSource>, String> {
    let declared: Vec<&str> = chain.inputs.iter().map(|i| i.name.as_str()).collect();
    let open = |path: &str| -> Result<Box<dyn Read>, String> {
        Ok(Box::new(
            std::fs::File::open(path).map_err(|e| format!("cannot read {path}: {e}"))?,
        ))
    };

    if declared.len() <= 1 {
        let name = chain
            .sole_input_name()
            .map_err(|e| e.to_string())?
            .to_string();
        return match input {
            [] => Ok(vec![(name, Box::new(std::io::stdin().lock()))]),
            [path] if !path.contains('=') || declared.is_empty() => Ok(vec![(name, open(path)?)]),
            [spec] => {
                let (key, path) = spec.split_once('=').expect("checked");
                if key == declared[0] {
                    Ok(vec![(name, open(path)?)])
                } else {
                    Err(format!(
                        "chain has no input named \"{key}\" (its input is \"{}\")",
                        declared[0]
                    ))
                }
            }
            _ => Err("this chain takes a single input; give at most one -i".into()),
        };
    }

    let mut sources: Vec<ChainSource> = Vec::new();
    for spec in input {
        let (name, path) = spec.split_once('=').ok_or_else(|| {
            format!(
                "this chain has {} inputs; use -i name=path (inputs: {})",
                declared.len(),
                declared.join(", ")
            )
        })?;
        if !declared.contains(&name) {
            return Err(format!(
                "chain has no input named \"{name}\" (inputs: {})",
                declared.join(", ")
            ));
        }
        if sources.iter().any(|(n, _)| n == name) {
            return Err(format!("input \"{name}\" given more than once"));
        }
        sources.push((name.to_string(), open(path)?));
    }
    let missing: Vec<&str> = declared
        .iter()
        .filter(|d| !sources.iter().any(|(n, _)| n == *d))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(format!("missing chain input(s): {}", missing.join(", ")));
    }
    Ok(sources)
}

fn run_chain_streaming(
    chain: &Chain,
    registry: &Registry,
    input: &[String],
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

    let mut readers = open_chain_sources(chain, input)?;
    let meta = toolkit_core::ValueMeta {
        data_type: DataType::Bytes,
        format: String::new(),
    };
    let mut sources: Vec<toolkit_core::NamedSource<'_>> = readers
        .iter_mut()
        .map(|(name, reader)| toolkit_core::NamedSource {
            name: name.clone(),
            meta: meta.clone(),
            reader: reader.as_mut(),
        })
        .collect();

    if let Some(dir) = output_dir {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut writers: std::collections::HashMap<String, (PathBuf, std::fs::File)> =
            Default::default();
        chain
            .execute_streaming_multi(
                registry,
                &mut sources,
                false,
                &mut os_entropy,
                &mut |id, bytes| {
                    if !writers.contains_key(id) {
                        let ext = sink_extension(sink_outputs[id], bytes);
                        let path = dir.join(format!("{id}.{ext}"));
                        let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
                        writers.insert(id.to_string(), (path, file));
                    }
                    let (_, file) = writers.get_mut(id).expect("created above");
                    file.write_all(bytes).map_err(|e| e.to_string())
                },
            )
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
            .execute_streaming_multi(
                registry,
                &mut sources,
                false,
                &mut os_entropy,
                &mut |_, bytes| writer.write_all(bytes).map_err(|e| e.to_string()),
            )
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
    chains_dir: Option<&Path>,
) -> Result<Chain, String> {
    let json = if let Some(path) = file {
        std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?
    } else if let Some(name) = name {
        let candidates: Vec<PathBuf> = chain_dirs(chains_dir)
            .into_iter()
            .map(|d| d.join(format!("{name}.json")))
            .collect();
        match candidates.iter().find(|p| p.exists()) {
            Some(path) => std::fs::read_to_string(path).map_err(|e| {
                format!("cannot read chain \"{name}\" from {}: {e}", path.display())
            })?,
            None => builtin_chain(&name)
                .ok_or_else(|| {
                    format!(
                        "no chain named \"{name}\" (not built in; also looked for {})",
                        candidates
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?
                .to_string(),
        }
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

/// Driver-side randomness for entropy ports, from the OS RNG.
fn os_entropy(n: usize) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0u8; n];
    getrandom::fill(&mut bytes).map_err(|e| format!("OS entropy unavailable: {e}"))?;
    Ok(bytes)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn w(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn set_completion_lists_option_keys_then_values() {
        let registry = registry();
        let keys = complete_set_candidates(&registry, "", &w(&["toolkit", "run-tool", "hash"]));
        assert!(keys.contains(&"algorithm=".to_string()), "{keys:?}");

        let values = complete_set_candidates(
            &registry,
            "algorithm=",
            &w(&["toolkit", "run-tool", "hash"]),
        );
        assert!(values.contains(&"algorithm=md5".to_string()), "{values:?}");
        assert!(values.contains(&"algorithm=sha256".to_string()));
    }

    #[test]
    fn set_completion_finds_tool_past_flags_and_their_values() {
        let registry = registry();
        // -i takes a value that must not be mistaken for the tool.
        let keys = complete_set_candidates(
            &registry,
            "",
            &w(&[
                "toolkit",
                "run-tool",
                "-i",
                "photo.png",
                "image-resize",
                "--set",
            ]),
        );
        assert!(keys.contains(&"width=".to_string()), "{keys:?}");
        assert!(keys.contains(&"mode=".to_string()));
    }

    #[test]
    fn builtin_chains_are_present_and_valid() {
        let registry = registry();
        assert!(!BUILTIN_CHAINS.is_empty());
        for (name, json) in BUILTIN_CHAINS {
            let chain: Chain = serde_json::from_str(json)
                .unwrap_or_else(|e| panic!("built-in chain {name} is not valid JSON: {e}"));
            chain
                .validate(&registry)
                .unwrap_or_else(|e| panic!("built-in chain {name} does not validate: {e}"));
        }
    }

    #[test]
    fn broken_pipe_matcher_is_narrow() {
        assert!(is_broken_pipe_panic(
            "failed printing to stdout: Broken pipe (os error 32)"
        ));
        assert!(is_broken_pipe_panic(
            "failed printing to stdout: The pipe is being closed. (os error 232)"
        ));
        // Real write failures must stay loud.
        assert!(!is_broken_pipe_panic(
            "failed printing to stdout: No space left on device (os error 28)"
        ));
        assert!(!is_broken_pipe_panic("index out of bounds"));
    }

    #[test]
    fn wrap_breaks_on_words_and_handles_long_ones() {
        assert_eq!(wrap("a bb ccc", 4), vec!["a bb", "ccc"]);
        assert_eq!(wrap("overlongword ok", 5), vec!["overlongword", "ok"]);
        assert_eq!(wrap("", 10), vec![""]);
    }

    #[test]
    fn chain_completion_names_and_params() {
        let registry = registry();
        let dir = std::env::temp_dir().join(format!("tk-comp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("mychain.json"),
            r#"{"version":1,"params":[{"name":"width","label":"W","kind":"integer","maps":[]},
                {"name":"format","label":"F","kind":"enum","values":["png","jpeg"],"maps":[]}],
               "nodes":[{"id":"a","tool":"json-format"}],"edges":[]}"#,
        )
        .unwrap();
        let dir_s = dir.to_str().unwrap().to_string();

        let names = complete_candidates(
            &registry,
            "chain-name",
            "",
            &w(&["toolkit", "run-chain", "--chains-dir", &dir_s, "-n"]),
        );
        assert!(names.contains(&"mychain".to_string()), "{names:?}");

        let keys = complete_candidates(
            &registry,
            "set",
            "",
            &w(&[
                "toolkit",
                "run-chain",
                "--chains-dir",
                &dir_s,
                "-n",
                "mychain",
                "--set",
            ]),
        );
        assert_eq!(keys[..2], ["width=".to_string(), "format=".to_string()]);

        let values = complete_candidates(
            &registry,
            "set",
            "format=",
            &w(&[
                "toolkit",
                "run-chain",
                "--chains-dir",
                &dir_s,
                "-n",
                "mychain",
                "--set",
            ]),
        );
        assert_eq!(values, vec!["format=png", "format=jpeg"]);

        // node.option overrides complete too (mychain's node "a" runs
        // json-format, so a.indent= is offered).
        assert!(keys.contains(&"a.indent=".to_string()), "{keys:?}");

        // Unknown chain: quiet, not an error.
        assert!(complete_candidates(
            &registry,
            "set",
            "",
            &w(&[
                "toolkit",
                "run-chain",
                "--chains-dir",
                &dir_s,
                "-n",
                "nope",
                "--set"
            ]),
        )
        .is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn set_completion_is_quiet_on_unknown_tool_or_key() {
        let registry = registry();
        assert!(complete_set_candidates(&registry, "", &w(&["toolkit", "run-tool"])).is_empty());
        assert!(
            complete_set_candidates(&registry, "nope=", &w(&["toolkit", "run-tool", "hash"]))
                .is_empty()
        );
        // integer options offer no value candidates
        assert!(complete_set_candidates(
            &registry,
            "width=",
            &w(&["toolkit", "run-tool", "image-resize"])
        )
        .is_empty());
    }
}
