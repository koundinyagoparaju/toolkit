use crate::data::{DataType, DataValue, ValueMeta};
use crate::manifest::OptionSpec;
use crate::options::{validate_against_specs, Options};
use crate::tool::{run_tool, Inputs, Registry};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt;

pub const CHAIN_SCHEMA_VERSION: u32 = 1;

/// A toolchain: a DAG of tool invocations. This JSON schema is the shared
/// format between the CLI, the web builder, shareable URLs, and the
/// community `chains/` library.
///
/// Shape rules (validated by [`Chain::validate`]):
/// - node ids unique; edges reference existing nodes and input ports; no cycles
/// - fan-out is allowed (a node's output may feed several nodes)
/// - each *input port* accepts at most one incoming edge
/// - a node is either fully wired (every input port has an edge) or an
///   entry node (no incoming edges at all) — entry nodes receive the
///   chain's input on every port
///
/// Declared `params` make a chain a first-class, callable unit: named,
/// typed knobs (same spec format as tool options) that map onto node
/// options. Apply invocation values with [`Chain::with_params`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chain {
    pub version: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ChainParam>,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub tool: String,
    #[serde(default)]
    pub options: Options,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// Input port on `to`. May be omitted when the target tool has exactly
    /// one input port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_port: Option<String>,
}

/// A declared chain parameter: a typed knob (reusing [`OptionSpec`], so UIs
/// and the CLI render it exactly like a tool option) that writes its value
/// into one or more node options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainParam {
    #[serde(flatten)]
    pub spec: OptionSpec,
    pub maps: Vec<ParamTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamTarget {
    pub node: String,
    pub option: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainError {
    pub message: String,
    /// Node id the error is attached to, when known (lets UIs highlight it).
    pub node: Option<String>,
}

impl ChainError {
    fn new(message: impl Into<String>) -> Self {
        ChainError {
            message: message.into(),
            node: None,
        }
    }
    fn at(node: &str, message: impl Into<String>) -> Self {
        ChainError {
            message: message.into(),
            node: Some(node.to_string()),
        }
    }
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.node {
            Some(node) => write!(f, "node \"{node}\": {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ChainError {}

/// All node outputs, keyed by node id. Sinks (no outgoing edge) are the
/// chain's results; intermediate outputs are kept for step-by-step preview.
pub struct ChainResult {
    pub outputs: BTreeMap<String, DataValue>,
    pub sinks: Vec<String>,
}

impl Chain {
    /// Build a linear chain from shell-pipe syntax:
    /// `"base64-decode | json-format indent=4"` — steps separated by `|`,
    /// each step a tool name followed by key=value options (values parsed as
    /// JSON when possible, else strings).
    pub fn from_pipe_syntax(spec: &str) -> Result<Chain, ChainError> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for (i, step) in spec.split('|').enumerate() {
            let mut parts = step.split_whitespace();
            let tool = parts
                .next()
                .ok_or_else(|| ChainError::new("empty step in pipe expression"))?;
            let mut options = Options::new();
            for kv in parts {
                let (key, raw) = kv.split_once('=').ok_or_else(|| {
                    ChainError::new(format!(
                        "expected key=value in step \"{}\", got \"{kv}\"",
                        step.trim()
                    ))
                })?;
                let value = serde_json::from_str(raw)
                    .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
                options.insert(key.to_string(), value);
            }
            let id = format!("s{}", i + 1);
            if i > 0 {
                edges.push(Edge {
                    from: format!("s{i}"),
                    to: id.clone(),
                    to_port: None,
                });
            }
            nodes.push(Node {
                id,
                tool: tool.to_string(),
                options,
            });
        }
        if nodes.is_empty() {
            return Err(ChainError::new("empty pipe expression"));
        }
        Ok(Chain {
            version: CHAIN_SCHEMA_VERSION,
            name: String::new(),
            description: String::new(),
            params: Vec::new(),
            nodes,
            edges,
        })
    }

    /// Return a copy with declared-parameter `values` validated and written
    /// into the mapped node options.
    pub fn with_params(&self, values: &Options) -> Result<Chain, ChainError> {
        let specs: Vec<OptionSpec> = self.params.iter().map(|p| p.spec.clone()).collect();
        let context = if self.name.is_empty() {
            "chain".to_string()
        } else {
            format!("chain \"{}\"", self.name)
        };
        let resolved = validate_against_specs(&specs, values, &context)
            .map_err(|e| ChainError::new(e.message))?;

        let mut chain = self.clone();
        for param in &self.params {
            let Some(value) = resolved.get(&param.spec.name) else {
                continue; // optional param without value or default
            };
            for target in &param.maps {
                let node = chain
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == target.node)
                    .ok_or_else(|| {
                        ChainError::new(format!(
                            "param \"{}\" maps to unknown node \"{}\"",
                            param.spec.name, target.node
                        ))
                    })?;
                node.options.insert(target.option.clone(), value.clone());
            }
        }
        Ok(chain)
    }

    /// Static validation: structure, tool existence, port wiring, and edge
    /// type compatibility per the coercion matrix.
    pub fn validate(&self, registry: &Registry) -> Result<(), ChainError> {
        if self.version != CHAIN_SCHEMA_VERSION {
            return Err(ChainError::new(format!(
                "unsupported chain schema version {} (expected {CHAIN_SCHEMA_VERSION})",
                self.version
            )));
        }
        if self.nodes.is_empty() {
            return Err(ChainError::new("chain has no nodes"));
        }

        let mut manifests = HashMap::new();
        let mut seen = HashSet::new();
        for node in &self.nodes {
            if !seen.insert(node.id.as_str()) {
                return Err(ChainError::new(format!(
                    "duplicate node id \"{}\"",
                    node.id
                )));
            }
            let tool = registry.find(&node.tool).ok_or_else(|| {
                ChainError::at(&node.id, format!("unknown tool \"{}\"", node.tool))
            })?;
            manifests.insert(node.id.as_str(), tool.manifest());
        }

        // Edges: known endpoints, resolvable target port, one edge per port,
        // type-compatible.
        let mut wired: HashSet<(&str, String)> = HashSet::new();
        for edge in &self.edges {
            let from_m = manifests.get(edge.from.as_str()).ok_or_else(|| {
                ChainError::new(format!("edge references unknown node \"{}\"", edge.from))
            })?;
            let to_m = manifests.get(edge.to.as_str()).ok_or_else(|| {
                ChainError::new(format!("edge references unknown node \"{}\"", edge.to))
            })?;
            let port = self.resolve_port(edge, to_m)?;
            if !DataType::can_coerce(from_m.output, port.data_type) {
                return Err(ChainError::at(
                    &edge.to,
                    format!(
                        "type mismatch: \"{}\" outputs {} which cannot feed the {} port \"{}\"",
                        edge.from,
                        from_m.output.name(),
                        port.data_type.name(),
                        port.name
                    ),
                ));
            }
            if !wired.insert((edge.to.as_str(), port.name.clone())) && !port.multi {
                return Err(ChainError::at(
                    &edge.to,
                    format!(
                        "input port \"{}\" has more than one incoming edge",
                        port.name
                    ),
                ));
            }
        }

        // Port coverage: a node with any incoming edge must have all its
        // input ports wired; nodes with none are entry nodes.
        for node in &self.nodes {
            let m = &manifests[node.id.as_str()];
            let wired_count = m
                .inputs
                .iter()
                .filter(|p| wired.contains(&(node.id.as_str(), p.name.clone())))
                .count();
            if wired_count != 0 && wired_count != m.inputs.len() {
                let missing: Vec<&str> = m
                    .inputs
                    .iter()
                    .filter(|p| !wired.contains(&(node.id.as_str(), p.name.clone())))
                    .map(|p| p.name.as_str())
                    .collect();
                return Err(ChainError::at(
                    &node.id,
                    format!("input port(s) not connected: {}", missing.join(", ")),
                ));
            }
        }

        // Param targets must reference existing nodes and declared options.
        for param in &self.params {
            for target in &param.maps {
                let m = manifests.get(target.node.as_str()).ok_or_else(|| {
                    ChainError::new(format!(
                        "param \"{}\" maps to unknown node \"{}\"",
                        param.spec.name, target.node
                    ))
                })?;
                if !m.options.iter().any(|o| o.name == target.option) {
                    return Err(ChainError::new(format!(
                        "param \"{}\" maps to unknown option \"{}\" of tool \"{}\"",
                        param.spec.name, target.option, m.name
                    )));
                }
            }
        }

        self.topo_order()?;
        Ok(())
    }

    /// Execute the chain on a complete input value. A thin wrapper over the
    /// push engine ([`Chain::execute_streaming`]) that feeds the value as a
    /// single chunk and retains every node's output — so buffered and
    /// streamed execution share one engine and cannot diverge.
    pub fn execute(
        &self,
        registry: &Registry,
        input: DataValue,
    ) -> Result<ChainResult, ChainError> {
        let (meta, bytes) = input.into_payload();
        let mut source = std::io::Cursor::new(bytes);
        let outcome =
            self.execute_streaming(registry, &meta, &mut source, true, &mut |_, _| Ok(()))?;
        Ok(ChainResult {
            outputs: outcome.outputs,
            sinks: outcome.sinks,
        })
    }

    /// Execute the chain as a push-based dataflow: the source is read in
    /// chunks, streaming nodes transform chunk-by-chunk, non-streaming
    /// nodes buffer at their inputs ("reservoirs") and run once complete.
    /// Sink output is delivered incrementally through `on_sink`.
    ///
    /// Memory is bounded by the reservoirs' working sets — an all-streaming
    /// chain runs in O(chunk) memory regardless of input size. With
    /// `retain_all`, every node's full output is additionally kept (used by
    /// [`Chain::execute`] and previews); leave it off for large inputs.
    pub fn execute_streaming<R: std::io::Read>(
        &self,
        registry: &Registry,
        input_meta: &ValueMeta,
        source: &mut R,
        retain_all: bool,
        on_sink: &mut OnSink,
    ) -> Result<StreamOutcome, ChainError> {
        self.validate(registry)?;
        let mut engine = Engine::build(self, registry, input_meta, retain_all)?;

        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = source
                .read(&mut buf)
                .map_err(|e| ChainError::new(format!("failed to read chain input: {e}")))?;
            if n == 0 {
                break;
            }
            engine.push_input(registry, &buf[..n], on_sink)?;
        }
        engine.end_input(registry, on_sink)?;
        engine.into_outcome(registry)
    }

    fn resolve_port(
        &self,
        edge: &Edge,
        to_manifest: &crate::manifest::Manifest,
    ) -> Result<crate::manifest::InputSpec, ChainError> {
        match &edge.to_port {
            Some(name) => to_manifest.input_port(name).cloned().ok_or_else(|| {
                ChainError::at(
                    &edge.to,
                    format!("tool \"{}\" has no input port \"{name}\"", to_manifest.name),
                )
            }),
            None => to_manifest.sole_input().cloned().ok_or_else(|| {
                ChainError::at(
                    &edge.to,
                    format!(
                        "tool \"{}\" has {} input ports; the edge must name one (to_port)",
                        to_manifest.name,
                        to_manifest.inputs.len()
                    ),
                )
            }),
        }
    }

    /// Successor slots of `node_idx`'s output, in edge-declaration order.
    fn build_engine_wiring(&self, registry: &Registry) -> Result<Wiring, ChainError> {
        let index: HashMap<&str, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();

        // Slots per node: for wired nodes, one per incoming edge, grouped
        // by port in edge-declaration order (this order defines multi-port
        // value order). For entry nodes, one per port, fed by chain input.
        let mut slots: Vec<Vec<Slot>> = (0..self.nodes.len()).map(|_| Vec::new()).collect();
        let mut outgoing: Vec<Vec<(usize, usize)>> = vec![Vec::new(); self.nodes.len()];
        let mut edge_slot: HashMap<usize, (usize, usize)> = HashMap::new();

        for (n_idx, node) in self.nodes.iter().enumerate() {
            let manifest = registry.find(&node.tool).expect("validated").manifest();
            let wired = self.edges.iter().any(|e| e.to == node.id);
            for port in &manifest.inputs {
                if wired {
                    let mut value_index = 0;
                    for (e_idx, edge) in self.edges.iter().enumerate() {
                        if edge.to != node.id
                            || self.resolve_port(edge, &manifest)?.name != port.name
                        {
                            continue;
                        }
                        edge_slot.insert(e_idx, (n_idx, slots[n_idx].len()));
                        slots[n_idx].push(Slot::new(&port.name, value_index));
                        value_index += 1;
                    }
                } else {
                    slots[n_idx].push(Slot::new(&port.name, 0));
                }
            }
        }
        for (e_idx, edge) in self.edges.iter().enumerate() {
            let from = index[edge.from.as_str()];
            outgoing[from].push(edge_slot[&e_idx]);
        }
        Ok((slots, outgoing))
    }

    /// Kahn's algorithm; errors on cycles. Ties broken by node declaration
    /// order so execution and results are deterministic.
    fn topo_order(&self) -> Result<Vec<String>, ChainError> {
        let index: HashMap<&str, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let mut indegree = vec![0usize; self.nodes.len()];
        for edge in &self.edges {
            if let Some(&to) = index.get(edge.to.as_str()) {
                indegree[to] += 1;
            }
        }
        let mut queue: VecDeque<usize> = (0..self.nodes.len())
            .filter(|&i| indegree[i] == 0)
            .collect();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(i) = queue.pop_front() {
            order.push(self.nodes[i].id.clone());
            for edge in self.edges.iter().filter(|e| e.from == self.nodes[i].id) {
                if let Some(&to) = index.get(edge.to.as_str()) {
                    indegree[to] -= 1;
                    if indegree[to] == 0 {
                        queue.push_back(to);
                    }
                }
            }
        }
        if order.len() != self.nodes.len() {
            return Err(ChainError::new("chain contains a cycle"));
        }
        Ok(order)
    }
}

/// Incremental consumer of sink output: (node id, chunk).
pub type OnSink<'a> = dyn FnMut(&str, &[u8]) -> Result<(), String> + 'a;

/// Engine wiring: per-node input slots, per-node outgoing (node, slot).
type Wiring = (Vec<Vec<Slot>>, Vec<Vec<(usize, usize)>>);

/// Result of a streamed execution.
pub struct StreamOutcome {
    /// Full output values: reservoir nodes always; streaming nodes only
    /// when `retain_all` was requested.
    pub outputs: BTreeMap<String, DataValue>,
    pub sinks: Vec<String>,
    /// Bytes emitted per streaming node (streams aren't retained).
    pub streamed_bytes: BTreeMap<String, u64>,
}

/// One expected input value of a node: the port it belongs to, its value
/// index on that port, and the metadata used to reassemble buffered bytes.
struct Slot {
    port: String,
    index: usize,
    meta: ValueMeta,
    buffer: Vec<u8>,
    ended: bool,
}

impl Slot {
    fn new(port: &str, index: usize) -> Slot {
        Slot {
            port: port.to_string(),
            index,
            meta: ValueMeta {
                data_type: DataType::Bytes,
                format: String::new(),
            },
            buffer: Vec::new(),
            ended: false,
        }
    }
}

enum Kind {
    /// Chunks flow through a live session; nothing is buffered.
    Streaming(Option<Box<dyn crate::stream::StreamSession>>),
    /// Inputs accumulate in the slots; the tool runs once they complete.
    Reservoir,
}

struct EngineNode {
    kind: Kind,
    slots: Vec<Slot>,
    /// (target node, target slot) fed by this node's output.
    outgoing: Vec<(usize, usize)>,
    is_sink: bool,
    finished: bool,
    emitted: u64,
    retained: Vec<u8>,
}

enum Ev {
    Chunk(usize, usize, Vec<u8>),
    End(usize, usize),
}

struct Engine {
    ids: Vec<String>,
    tool_names: Vec<String>,
    node_options: Vec<Options>,
    nodes: Vec<EngineNode>,
    retain_all: bool,
    outputs: BTreeMap<String, DataValue>,
}

impl Engine {
    fn build(
        chain: &Chain,
        registry: &Registry,
        input_meta: &ValueMeta,
        retain_all: bool,
    ) -> Result<Engine, ChainError> {
        let (mut slots, outgoing) = chain.build_engine_wiring(registry)?;
        let has_incoming: HashSet<&str> = chain.edges.iter().map(|e| e.to.as_str()).collect();
        let has_outgoing: HashSet<&str> = chain.edges.iter().map(|e| e.from.as_str()).collect();

        // Slot metas known statically: chain input for entry slots, the
        // source tool's declared output for streaming sources. Reservoir
        // sources overwrite the meta at delivery time (e.g. image format).
        let mut nodes = Vec::with_capacity(chain.nodes.len());
        for (n_idx, node) in chain.nodes.iter().enumerate() {
            if !has_incoming.contains(node.id.as_str()) {
                for slot in &mut slots[n_idx] {
                    slot.meta = input_meta.clone();
                }
            }
            let tool = registry.find(&node.tool).expect("validated");
            let session = crate::stream::open_stream_validated(tool, &node.options)
                .map_err(|e| ChainError::at(&node.id, e.message))?;
            nodes.push(EngineNode {
                kind: match session {
                    Some(s) => Kind::Streaming(Some(s)),
                    None => Kind::Reservoir,
                },
                slots: std::mem::take(&mut slots[n_idx]),
                outgoing: outgoing[n_idx].clone(),
                is_sink: !has_outgoing.contains(node.id.as_str()),
                finished: false,
                emitted: 0,
                retained: Vec::new(),
            });
        }
        // Static metas for slots fed by streaming sources.
        for (n_idx, node) in chain.nodes.iter().enumerate() {
            if matches!(nodes[n_idx].kind, Kind::Streaming(_)) {
                let output = registry
                    .find(&node.tool)
                    .expect("validated")
                    .manifest()
                    .output;
                for (t, s) in nodes[n_idx].outgoing.clone() {
                    nodes[t].slots[s].meta = ValueMeta {
                        data_type: output,
                        format: String::new(),
                    };
                }
            }
        }
        Ok(Engine {
            ids: chain.nodes.iter().map(|n| n.id.clone()).collect(),
            tool_names: chain.nodes.iter().map(|n| n.tool.clone()).collect(),
            node_options: chain.nodes.iter().map(|n| n.options.clone()).collect(),
            nodes,
            retain_all,
            outputs: BTreeMap::new(),
        })
    }

    fn entry_slots(&self) -> Vec<(usize, usize)> {
        let fed: HashSet<(usize, usize)> = self
            .nodes
            .iter()
            .flat_map(|n| n.outgoing.iter().copied())
            .collect();
        let mut entries = Vec::new();
        for (n, node) in self.nodes.iter().enumerate() {
            for s in 0..node.slots.len() {
                if !fed.contains(&(n, s)) {
                    entries.push((n, s));
                }
            }
        }
        entries
    }

    fn push_input(
        &mut self,
        registry: &Registry,
        chunk: &[u8],
        on_sink: &mut OnSink,
    ) -> Result<(), ChainError> {
        let mut queue: VecDeque<Ev> = self
            .entry_slots()
            .into_iter()
            .map(|(n, s)| Ev::Chunk(n, s, chunk.to_vec()))
            .collect();
        self.process(registry, &mut queue, on_sink)
    }

    fn end_input(&mut self, registry: &Registry, on_sink: &mut OnSink) -> Result<(), ChainError> {
        let mut queue: VecDeque<Ev> = self
            .entry_slots()
            .into_iter()
            .map(|(n, s)| Ev::End(n, s))
            .collect();
        self.process(registry, &mut queue, on_sink)
    }

    fn process(
        &mut self,
        registry: &Registry,
        queue: &mut VecDeque<Ev>,
        on_sink: &mut OnSink,
    ) -> Result<(), ChainError> {
        while let Some(ev) = queue.pop_front() {
            match ev {
                Ev::Chunk(n, s, bytes) => {
                    if matches!(self.nodes[n].kind, Kind::Reservoir) {
                        self.nodes[n].slots[s].buffer.extend_from_slice(&bytes);
                        continue;
                    }
                    let (port, index) = {
                        let slot = &self.nodes[n].slots[s];
                        (slot.port.clone(), slot.index)
                    };
                    let result = {
                        let Kind::Streaming(session) = &mut self.nodes[n].kind else {
                            unreachable!()
                        };
                        session
                            .as_mut()
                            .expect("session live")
                            .update(&port, index, &bytes)
                    };
                    let out = result.map_err(|e| ChainError::at(&self.ids[n], e.message))?;
                    self.emit(n, out, queue, on_sink)?;
                }
                Ev::End(n, s) => {
                    self.nodes[n].slots[s].ended = true;
                    let all_ended = self.nodes[n].slots.iter().all(|sl| sl.ended);
                    if matches!(self.nodes[n].kind, Kind::Reservoir) {
                        if all_ended && !self.nodes[n].finished {
                            self.run_reservoir(n, registry, queue, on_sink)?;
                        }
                        continue;
                    }
                    let (port, index) = {
                        let slot = &self.nodes[n].slots[s];
                        (slot.port.clone(), slot.index)
                    };
                    let result = {
                        let Kind::Streaming(session) = &mut self.nodes[n].kind else {
                            unreachable!()
                        };
                        session
                            .as_mut()
                            .expect("session live")
                            .end_input(&port, index)
                    };
                    let out = result.map_err(|e| ChainError::at(&self.ids[n], e.message))?;
                    self.emit(n, out, queue, on_sink)?;
                    if all_ended && !self.nodes[n].finished {
                        let result = {
                            let Kind::Streaming(session) = &mut self.nodes[n].kind else {
                                unreachable!()
                            };
                            session.take().expect("session live").finish()
                        };
                        let final_out =
                            result.map_err(|e| ChainError::at(&self.ids[n], e.message))?;
                        self.emit(n, final_out, queue, on_sink)?;
                        self.finish_node(n, queue);
                    }
                }
            }
        }
        Ok(())
    }

    fn run_reservoir(
        &mut self,
        n: usize,
        registry: &Registry,
        queue: &mut VecDeque<Ev>,
        on_sink: &mut OnSink,
    ) -> Result<(), ChainError> {
        let tool = registry.find(&self.tool_names[n]).expect("validated");
        let mut inputs = Inputs::new();
        for slot in &mut self.nodes[n].slots {
            let bytes = std::mem::take(&mut slot.buffer);
            let value = DataValue::from_payload(&slot.meta, bytes)
                .map_err(|e| ChainError::at(&self.ids[n], e))?;
            inputs.entry(slot.port.clone()).or_default().push(value);
        }
        let output = run_tool(tool, inputs, &self.node_options[n])
            .map_err(|e| ChainError::at(&self.ids[n], e.message))?;
        self.outputs.insert(self.ids[n].clone(), output.clone());

        let (meta, bytes) = output.into_payload();
        // Downstream reservoirs need the real runtime meta (image format).
        for (t, s) in self.nodes[n].outgoing.clone() {
            self.nodes[t].slots[s].meta = meta.clone();
        }
        self.nodes[n].emitted = bytes.len() as u64;
        if self.nodes[n].is_sink {
            on_sink(&self.ids[n].clone(), &bytes).map_err(ChainError::new)?;
        }
        for (t, s) in self.nodes[n].outgoing.clone() {
            queue.push_back(Ev::Chunk(t, s, bytes.clone()));
        }
        self.finish_node(n, queue);
        Ok(())
    }

    fn finish_node(&mut self, n: usize, queue: &mut VecDeque<Ev>) {
        self.nodes[n].finished = true;
        for (t, s) in self.nodes[n].outgoing.clone() {
            queue.push_back(Ev::End(t, s));
        }
    }

    fn emit(
        &mut self,
        n: usize,
        bytes: Vec<u8>,
        queue: &mut VecDeque<Ev>,
        on_sink: &mut OnSink,
    ) -> Result<(), ChainError> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.nodes[n].emitted += bytes.len() as u64;
        if self.retain_all {
            self.nodes[n].retained.extend_from_slice(&bytes);
        }
        if self.nodes[n].is_sink {
            on_sink(&self.ids[n].clone(), &bytes).map_err(ChainError::new)?;
        }
        for (t, s) in self.nodes[n].outgoing.clone() {
            queue.push_back(Ev::Chunk(t, s, bytes.clone()));
        }
        Ok(())
    }

    fn into_outcome(mut self, registry: &Registry) -> Result<StreamOutcome, ChainError> {
        let mut streamed_bytes = BTreeMap::new();
        let mut sinks = Vec::new();
        for n in 0..self.nodes.len() {
            if self.nodes[n].is_sink {
                sinks.push(self.ids[n].clone());
            }
            if matches!(self.nodes[n].kind, Kind::Streaming(_)) {
                streamed_bytes.insert(self.ids[n].clone(), self.nodes[n].emitted);
                if self.retain_all {
                    let output = registry
                        .find(&self.tool_names[n])
                        .expect("validated")
                        .manifest()
                        .output;
                    let retained = std::mem::take(&mut self.nodes[n].retained);
                    let value = crate::stream::assemble_output(output, retained)
                        .map_err(|e| ChainError::at(&self.ids[n], e.message))?;
                    self.outputs.insert(self.ids[n].clone(), value);
                }
            }
        }
        Ok(StreamOutcome {
            outputs: self.outputs,
            sinks,
            streamed_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{InputSpec, Manifest};
    use crate::options::OptGet;
    use crate::tool::{InputsExt, Tool, ToolError};

    fn manifest(name: &str, inputs: Vec<InputSpec>, output: DataType) -> Manifest {
        Manifest {
            name: name.into(),
            label: name.into(),
            description: String::new(),
            keywords: vec![],
            inputs,
            output,
            streaming: false,
            options: vec![],
        }
    }

    /// Test tool: uppercases text.
    struct Upper;
    impl Tool for Upper {
        fn manifest(&self) -> Manifest {
            manifest("upper", InputSpec::sole(DataType::Text), DataType::Text)
        }
        fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
            let DataValue::Text(s) = inputs.sole() else {
                unreachable!()
            };
            Ok(DataValue::Text(s.to_uppercase()))
        }
    }

    /// Test tool: text -> length as JSON number.
    struct Len;
    impl Tool for Len {
        fn manifest(&self) -> Manifest {
            manifest("len", InputSpec::sole(DataType::Text), DataType::Json)
        }
        fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
            let DataValue::Text(s) = inputs.sole() else {
                unreachable!()
            };
            Ok(DataValue::Json(serde_json::json!(s.len())))
        }
    }

    /// Test tool with two ports and an option: "<left><sep><right>".
    struct Join;
    impl Tool for Join {
        fn manifest(&self) -> Manifest {
            let mut m = manifest(
                "join",
                vec![
                    InputSpec::named("left", DataType::Text),
                    InputSpec::named("right", DataType::Text),
                ],
                DataType::Text,
            );
            m.options = vec![OptionSpec::string("sep", "Separator", "").default_value("+".into())];
            m
        }
        fn run(&self, mut inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
            let DataValue::Text(l) = inputs.take("left") else {
                unreachable!()
            };
            let DataValue::Text(r) = inputs.take("right") else {
                unreachable!()
            };
            let sep = options.str_opt("sep").unwrap_or("+").to_string();
            Ok(DataValue::Text(format!("{l}{sep}{r}")))
        }
    }

    /// Test tool with one multi port: joins all values with commas.
    struct Gather;
    impl Tool for Gather {
        fn manifest(&self) -> Manifest {
            manifest(
                "gather",
                vec![InputSpec::named("items", DataType::Text).multi()],
                DataType::Text,
            )
        }
        fn run(&self, mut inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
            let parts: Vec<String> = inputs
                .take_many("items")
                .into_iter()
                .map(|v| match v {
                    DataValue::Text(s) => s,
                    _ => unreachable!(),
                })
                .collect();
            Ok(DataValue::Text(parts.join(",")))
        }
    }

    /// Streaming test tool: uppercases chunk-by-chunk.
    struct StreamUpper;
    struct UpperSession;
    impl crate::stream::StreamSession for UpperSession {
        fn update(&mut self, _: &str, _: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
            Ok(chunk.to_ascii_uppercase())
        }
        fn end_input(&mut self, _: &str, _: usize) -> Result<Vec<u8>, ToolError> {
            Ok(Vec::new())
        }
        fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
            Ok(Vec::new())
        }
    }
    impl Tool for StreamUpper {
        fn manifest(&self) -> Manifest {
            let mut m = manifest("supper", InputSpec::sole(DataType::Text), DataType::Text);
            m.streaming = true;
            m
        }
        fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
            let session = self.open_stream(options)?.expect("streams");
            crate::stream::buffered_run(session, &self.manifest(), inputs)
        }
        fn open_stream(
            &self,
            _: &Options,
        ) -> Result<Option<Box<dyn crate::stream::StreamSession>>, ToolError> {
            Ok(Some(Box::new(UpperSession)))
        }
    }

    fn registry() -> Registry {
        Registry::new(vec![
            Box::new(Upper),
            Box::new(Len),
            Box::new(Join),
            Box::new(Gather),
            Box::new(StreamUpper),
        ])
    }

    /// A reader that yields at most 3 bytes per read, to force chunking.
    struct Trickle<'a>(&'a [u8]);
    impl std::io::Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.0.len().min(3).min(buf.len());
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }

    #[test]
    fn streaming_chain_chunks_through_and_feeds_reservoirs() {
        // supper (streaming) -> len (reservoir); observed via sinks.
        let c = chain(&[("u", "supper"), ("l", "len")], &[("u", "l", None)]);
        let mut sink_chunks: Vec<(String, Vec<u8>)> = Vec::new();
        let meta = ValueMeta {
            data_type: DataType::Text,
            format: String::new(),
        };
        let outcome = c
            .execute_streaming(
                &registry(),
                &meta,
                &mut Trickle(b"hello world"),
                false,
                &mut |id, bytes| {
                    sink_chunks.push((id.to_string(), bytes.to_vec()));
                    Ok(())
                },
            )
            .unwrap();
        // Reservoir "l" saw the full uppercased text.
        assert_eq!(outcome.outputs["l"], DataValue::Json(serde_json::json!(11)));
        assert_eq!(outcome.streamed_bytes["u"], 11);
        // Streaming intermediates are not retained without retain_all.
        assert!(!outcome.outputs.contains_key("u"));
        // Only the sink ("l") reached the sink callback, with the final value.
        assert!(sink_chunks.iter().all(|(id, _)| id == "l"));
        let sink_bytes: Vec<u8> = sink_chunks.into_iter().flat_map(|(_, b)| b).collect();
        assert_eq!(sink_bytes, b"11");
    }

    #[test]
    fn streamed_and_buffered_execution_agree() {
        let c = chain(
            &[("u", "supper"), ("l", "len"), ("g", "gather")],
            &[("u", "g", Some("items")), ("l", "g", Some("items"))],
        );
        let buffered = c
            .execute(&registry(), DataValue::Text("hey".into()))
            .unwrap();
        assert_eq!(buffered.outputs["g"], DataValue::Text("HEY,3".into()));
        assert_eq!(buffered.outputs["u"], DataValue::Text("HEY".into()));
    }

    fn chain(nodes: &[(&str, &str)], edges: &[(&str, &str, Option<&str>)]) -> Chain {
        Chain {
            version: CHAIN_SCHEMA_VERSION,
            name: String::new(),
            description: String::new(),
            params: Vec::new(),
            nodes: nodes
                .iter()
                .map(|(id, tool)| Node {
                    id: id.to_string(),
                    tool: tool.to_string(),
                    options: Options::new(),
                })
                .collect(),
            edges: edges
                .iter()
                .map(|(from, to, port)| Edge {
                    from: from.to_string(),
                    to: to.to_string(),
                    to_port: port.map(String::from),
                })
                .collect(),
        }
    }

    #[test]
    fn linear_chain_executes() {
        let c = chain(&[("a", "upper"), ("b", "len")], &[("a", "b", None)]);
        let result = c
            .execute(&registry(), DataValue::Text("hey".into()))
            .unwrap();
        assert_eq!(result.sinks, vec!["b"]);
        assert_eq!(result.outputs["a"], DataValue::Text("HEY".into()));
        assert_eq!(result.outputs["b"], DataValue::Json(serde_json::json!(3)));
    }

    #[test]
    fn multi_port_node_joins_two_branches() {
        // input -> upper -> join.left; input -> len -> (json->text) join.right
        let c = chain(
            &[("u", "upper"), ("l", "len"), ("j", "join")],
            &[("u", "j", Some("left")), ("l", "j", Some("right"))],
        );
        let result = c
            .execute(&registry(), DataValue::Text("hey".into()))
            .unwrap();
        assert_eq!(result.outputs["j"], DataValue::Text("HEY+3".into()));
        assert_eq!(result.sinks, vec!["j"]);
    }

    #[test]
    fn multi_port_entry_node_gets_input_on_every_port() {
        let c = chain(&[("j", "join")], &[]);
        let result = c.execute(&registry(), DataValue::Text("x".into())).unwrap();
        assert_eq!(result.outputs["j"], DataValue::Text("x+x".into()));
    }

    #[test]
    fn multi_port_collects_values_in_edge_declaration_order() {
        // upper -> "HEY", len -> 3 (Json, coerced to Text "3").
        let c = chain(
            &[("u", "upper"), ("l", "len"), ("g", "gather")],
            &[("l", "g", Some("items")), ("u", "g", Some("items"))],
        );
        let result = c
            .execute(&registry(), DataValue::Text("hey".into()))
            .unwrap();
        assert_eq!(result.outputs["g"], DataValue::Text("3,HEY".into()));

        // Same graph, edges declared in the opposite order: order flips.
        let c = chain(
            &[("u", "upper"), ("l", "len"), ("g", "gather")],
            &[("u", "g", Some("items")), ("l", "g", Some("items"))],
        );
        let result = c
            .execute(&registry(), DataValue::Text("hey".into()))
            .unwrap();
        assert_eq!(result.outputs["g"], DataValue::Text("HEY,3".into()));
    }

    #[test]
    fn second_edge_into_single_port_still_rejected() {
        let c = chain(
            &[("a", "upper"), ("b", "upper"), ("j", "join")],
            &[
                ("a", "j", Some("left")),
                ("b", "j", Some("right")),
                ("a", "j", Some("right")),
            ],
        );
        assert!(c.validate(&registry()).is_err());
    }

    #[test]
    fn partially_wired_multi_port_node_rejected() {
        let c = chain(
            &[("u", "upper"), ("j", "join")],
            &[("u", "j", Some("left"))],
        );
        let err = c.validate(&registry()).unwrap_err();
        assert!(err.message.contains("not connected"), "{err}");
    }

    #[test]
    fn edge_to_multi_port_tool_must_name_port() {
        let c = chain(&[("u", "upper"), ("j", "join")], &[("u", "j", None)]);
        assert!(c.validate(&registry()).is_err());
    }

    #[test]
    fn duplicate_edge_on_same_port_rejected() {
        let c = chain(
            &[("a", "upper"), ("b", "upper"), ("c", "len")],
            &[("a", "c", None), ("b", "c", None)],
        );
        assert!(c.validate(&registry()).is_err());
    }

    #[test]
    fn fan_out_runs_both_branches() {
        let c = chain(
            &[("a", "upper"), ("b", "len"), ("c", "upper")],
            &[("a", "b", None), ("a", "c", None)],
        );
        let result = c
            .execute(&registry(), DataValue::Text("hey".into()))
            .unwrap();
        assert_eq!(result.sinks.len(), 2);
        assert_eq!(result.outputs["c"], DataValue::Text("HEY".into()));
    }

    #[test]
    fn cycle_rejected() {
        let c = chain(
            &[("a", "upper"), ("b", "upper")],
            &[("a", "b", None), ("b", "a", None)],
        );
        assert!(c.validate(&registry()).is_err());
    }

    #[test]
    fn unknown_tool_rejected_with_node_attribution() {
        let c = chain(&[("a", "nope")], &[]);
        let err = c.validate(&registry()).unwrap_err();
        assert_eq!(err.node.as_deref(), Some("a"));
    }

    #[test]
    fn pipe_syntax_parses_options_and_links_steps() {
        let c = Chain::from_pipe_syntax("upper | len").unwrap();
        assert_eq!(c.nodes.len(), 2);
        assert_eq!(c.edges.len(), 1);
        let result = c
            .execute(&registry(), DataValue::Text("hi".into()))
            .unwrap();
        assert_eq!(result.outputs["s2"], DataValue::Json(serde_json::json!(2)));

        let c = Chain::from_pipe_syntax("resize width=100 mode=fit").unwrap();
        assert_eq!(c.nodes[0].options["width"], serde_json::json!(100));
        assert_eq!(c.nodes[0].options["mode"], serde_json::json!("fit"));
    }

    #[test]
    fn params_apply_to_mapped_nodes() {
        let mut c = chain(&[("j", "join")], &[]);
        c.params = vec![ChainParam {
            spec: OptionSpec::string("glue", "Glue", "").default_value("-".into()),
            maps: vec![ParamTarget {
                node: "j".into(),
                option: "sep".into(),
            }],
        }];
        c.validate(&registry()).unwrap();

        // Default applies when no value given.
        let defaulted = c.with_params(&Options::new()).unwrap();
        assert_eq!(defaulted.nodes[0].options["sep"], serde_json::json!("-"));

        // Explicit value wins.
        let mut values = Options::new();
        values.insert("glue".into(), serde_json::json!("|"));
        let overridden = c.with_params(&values).unwrap();
        let result = overridden
            .execute(&registry(), DataValue::Text("x".into()))
            .unwrap();
        assert_eq!(result.outputs["j"], DataValue::Text("x|x".into()));

        // Unknown param rejected.
        let mut bad = Options::new();
        bad.insert("nope".into(), serde_json::json!(1));
        assert!(c.with_params(&bad).is_err());
    }

    #[test]
    fn params_mapping_to_unknown_node_or_option_rejected() {
        let mut c = chain(&[("j", "join")], &[]);
        c.params = vec![ChainParam {
            spec: OptionSpec::string("glue", "Glue", ""),
            maps: vec![ParamTarget {
                node: "j".into(),
                option: "nope".into(),
            }],
        }];
        assert!(c.validate(&registry()).is_err());
    }
}
