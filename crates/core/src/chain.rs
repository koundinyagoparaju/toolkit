use crate::data::{DataType, DataValue};
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

    /// Execute the chain: every entry node receives `input` on each of its
    /// ports; values flow along edges (with runtime coercion per port).
    pub fn execute(
        &self,
        registry: &Registry,
        input: DataValue,
    ) -> Result<ChainResult, ChainError> {
        self.validate(registry)?;
        let order = self.topo_order()?;

        let mut outputs: BTreeMap<String, DataValue> = BTreeMap::new();
        let has_incoming: HashSet<&str> = self.edges.iter().map(|e| e.to.as_str()).collect();
        let has_outgoing: HashSet<&str> = self.edges.iter().map(|e| e.from.as_str()).collect();

        for id in &order {
            let node = self
                .nodes
                .iter()
                .find(|n| &n.id == id)
                .expect("node exists");
            let tool = registry.find(&node.tool).expect("validated");
            let manifest = tool.manifest();

            // Pull inputs from predecessor outputs, scanning edges in
            // declaration order — which is what defines the value order on
            // multi ports.
            let mut inputs = Inputs::new();
            for port in &manifest.inputs {
                let values = if has_incoming.contains(id.as_str()) {
                    let mut values = Vec::new();
                    for edge in self.edges.iter().filter(|e| &e.to == id) {
                        if self.resolve_port(edge, &manifest)?.name == port.name {
                            values.push(
                                outputs
                                    .get(&edge.from)
                                    .expect("validated: predecessor ran first")
                                    .clone(),
                            );
                        }
                    }
                    values
                } else {
                    vec![input.clone()]
                };
                inputs.insert(port.name.clone(), values);
            }

            let output =
                run_tool(tool, inputs, &node.options).map_err(|e| ChainError::at(id, e.message))?;
            outputs.insert(id.clone(), output);
        }

        let sinks = order
            .iter()
            .filter(|id| !has_outgoing.contains(id.as_str()))
            .cloned()
            .collect();
        Ok(ChainResult { outputs, sinks })
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

    fn registry() -> Registry {
        Registry::new(vec![
            Box::new(Upper),
            Box::new(Len),
            Box::new(Join),
            Box::new(Gather),
        ])
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
