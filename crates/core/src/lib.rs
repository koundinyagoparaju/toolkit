//! toolkit-core: the shared contract between tools, the CLI, and the web app.
//!
//! Contains no tool logic. Defines:
//! - [`DataValue`]/[`DataType`]: the typed values that flow between tools,
//!   with sanctioned coercions (e.g. Bytes -> Text requires valid UTF-8).
//! - [`Manifest`]/[`OptionSpec`]: how a tool describes itself; UIs and the
//!   CLI generate forms/flags from this.
//! - [`Tool`]: the trait every tool implements, and [`Registry`] to look
//!   tools up by name.
//! - [`Chain`]: the versioned DAG schema and its executor.
//! - [`abi`]: the byte-level request/response encoding used by the wasm
//!   pack ABI (and the `export_pack_abi!` macro that generates the exports).

pub mod abi;
mod chain;
mod data;
mod manifest;
mod options;
mod tool;

pub use chain::{Chain, ChainError, ChainParam, ChainResult, Edge, Node, ParamTarget};
pub use data::{DataType, DataValue};
pub use manifest::{InputSpec, Manifest, OptionKind, OptionSpec};
pub use options::{validate_against_specs, validate_options, OptGet, Options};
pub use tool::{run_single, run_tool, Inputs, InputsExt, Registry, Tool, ToolError};
