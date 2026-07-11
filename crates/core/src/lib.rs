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
pub mod exercise;
mod manifest;
mod options;
pub mod stream;
mod tool;

pub use chain::{
    Chain, ChainError, ChainParam, ChainResult, Edge, Node, OnSink, ParamTarget, StreamOutcome,
};
pub use data::{DataType, DataValue, ValueMeta};
pub use manifest::{InputSpec, Manifest, OptionKind, OptionSpec};
pub use options::{validate_against_specs, validate_options, OptGet, Options};
pub use stream::{buffered_run, open_stream_validated, StreamSession};
pub use tool::{run_single, run_tool, Inputs, InputsExt, Registry, Tool, ToolError};

/// Bytes delivered to an entropy port by drivers. Generous enough for any
/// generator (a max-length password consumes well under half of this even
/// with rejection sampling).
pub const ENTROPY_LEN: usize = 1024;
