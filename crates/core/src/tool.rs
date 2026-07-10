use crate::data::DataValue;
use crate::manifest::Manifest;
use crate::options::{validate_options, Options};
use std::collections::BTreeMap;
use std::fmt;

/// Error produced by a tool (or by validation/coercion around it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    pub message: String,
}

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        ToolError {
            message: message.into(),
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ToolError {}

impl From<String> for ToolError {
    fn from(message: String) -> Self {
        ToolError { message }
    }
}

/// A tool invocation's input values: port name -> ordered values. Ordinary
/// ports carry exactly one value; `multi` ports carry one or more.
pub type Inputs = BTreeMap<String, Vec<DataValue>>;

/// Build the input set for a one-port invocation.
pub fn single_input(port: &str, value: DataValue) -> Inputs {
    Inputs::from([(port.to_string(), vec![value])])
}

/// Accessors for tools consuming their (already validated) inputs. The
/// panics below cannot fire for values delivered through [`run_tool`],
/// which enforces per-port cardinality first.
pub trait InputsExt {
    /// Take the value of a one-port tool.
    fn sole(self) -> DataValue;
    /// Take a named single port's value.
    fn take(&mut self, port: &str) -> DataValue;
    /// Take a named multi port's values, in edge/invocation order.
    fn take_many(&mut self, port: &str) -> Vec<DataValue>;
}

impl InputsExt for Inputs {
    fn sole(mut self) -> DataValue {
        assert_eq!(self.len(), 1, "sole() on a multi-port input set");
        let (_, values) = self.pop_first().expect("one entry");
        assert_eq!(values.len(), 1, "sole() on a multi-valued port");
        values.into_iter().next().expect("one value")
    }
    fn take(&mut self, port: &str) -> DataValue {
        let values = self.take_many(port);
        assert_eq!(values.len(), 1, "take() on multi-valued port \"{port}\"");
        values.into_iter().next().expect("one value")
    }
    fn take_many(&mut self, port: &str) -> Vec<DataValue> {
        self.remove(port)
            .unwrap_or_else(|| panic!("input port \"{port}\" not provided"))
    }
}

/// The contract every tool implements.
///
/// Rules for implementations:
/// - Pure: no filesystem, network, clock, or randomness (hash salts etc.
///   would break reproducibility and auditability).
/// - Baseline path: must run on plain single-threaded CPU; hardware
///   acceleration may only ever be an additive fast path.
/// - `run` receives one value per declared input port, already coerced to
///   the port's type, and options already validated/defaulted, when invoked
///   via [`run_tool`].
pub trait Tool: Sync + Send {
    fn manifest(&self) -> Manifest;
    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError>;

    /// Open a streaming session, for tools that process input
    /// incrementally. `options` must already be validated (use
    /// [`crate::stream::open_stream_validated`] from outside).
    ///
    /// Returning `Some` must agree with `manifest().streaming`; a
    /// pack-level test enforces this. Streaming tools implement `run` by
    /// delegating to [`crate::stream::buffered_run`].
    fn open_stream(
        &self,
        options: &Options,
    ) -> Result<Option<Box<dyn crate::stream::StreamSession>>, ToolError> {
        let _ = options;
        Ok(None)
    }
}

/// Coerce every port's value, validate options, and run the tool. The single
/// entry point used by the CLI, the wasm ABI, and the chain executor — so
/// every caller gets identical semantics.
pub fn run_tool(
    tool: &dyn Tool,
    inputs: Inputs,
    options: &Options,
) -> Result<DataValue, ToolError> {
    let manifest = tool.manifest();
    for port in inputs.keys() {
        if manifest.input_port(port).is_none() {
            return Err(ToolError::new(format!(
                "tool \"{}\" has no input port \"{port}\"",
                manifest.name
            )));
        }
    }
    let mut coerced = Inputs::new();
    for spec in &manifest.inputs {
        let values = inputs.get(&spec.name).cloned().unwrap_or_default();
        if values.is_empty() {
            return Err(ToolError::new(format!(
                "missing input \"{}\" for tool \"{}\"",
                spec.name, manifest.name
            )));
        }
        if !spec.multi && values.len() > 1 {
            return Err(ToolError::new(format!(
                "input \"{}\" of tool \"{}\" takes one value, got {}",
                spec.name,
                manifest.name,
                values.len()
            )));
        }
        let values = values
            .into_iter()
            .map(|v| v.coerce(spec.data_type))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ToolError::new(format!("input \"{}\": {e}", spec.name)))?;
        coerced.insert(spec.name.clone(), values);
    }
    let options = validate_options(&manifest, options)?;
    let output = tool.run(coerced, &options)?;
    debug_assert_eq!(
        output.data_type(),
        manifest.output,
        "tool \"{}\" produced a value of the wrong type",
        manifest.name
    );
    Ok(output)
}

/// Convenience for the overwhelmingly common one-input case: wraps the value
/// in the tool's sole port and calls [`run_tool`].
pub fn run_single(
    tool: &dyn Tool,
    input: DataValue,
    options: &Options,
) -> Result<DataValue, ToolError> {
    let manifest = tool.manifest();
    let port = manifest.sole_input().ok_or_else(|| {
        ToolError::new(format!(
            "tool \"{}\" takes {} inputs; provide them by port name",
            manifest.name,
            manifest.inputs.len()
        ))
    })?;
    run_tool(tool, single_input(&port.name, input), options)
}

/// A named collection of tools. Each pack exposes one; the CLI merges all
/// packs into a single registry.
pub struct Registry {
    tools: Vec<Box<dyn Tool>>,
}

impl Registry {
    pub fn new(tools: Vec<Box<dyn Tool>>) -> Self {
        Registry { tools }
    }

    pub fn merge(registries: impl IntoIterator<Item = Registry>) -> Registry {
        Registry {
            tools: registries.into_iter().flat_map(|r| r.tools).collect(),
        }
    }

    pub fn find(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|t| t.manifest().name == name)
            .map(|t| t.as_ref())
    }

    pub fn manifests(&self) -> Vec<Manifest> {
        self.tools.iter().map(|t| t.manifest()).collect()
    }
}
