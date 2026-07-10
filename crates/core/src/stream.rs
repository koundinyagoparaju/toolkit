//! Streaming: the opt-in fast path for transducer-style tools.
//!
//! A tool that can process input incrementally implements
//! [`Tool::open_stream`] returning a [`StreamSession`]. Chunks are tagged
//! with the input port (and value index, for `multi` ports) they belong
//! to, so multi-input tools like doc-merge can stream too.
//!
//! The buffered baseline is *derived* from the session via
//! [`buffered_run`], so streaming tools are written once and the two modes
//! cannot drift apart. Tools that cannot stream (whole-value: images,
//! JSON) simply never implement a session — the chain engine buffers at
//! their boundary ("reservoir" nodes) and streams everywhere else.

use crate::data::{DataType, DataValue, ValueMeta};
use crate::manifest::Manifest;
use crate::options::{validate_options, Options};
use crate::tool::{Inputs, Tool, ToolError};

/// An in-flight streaming invocation of one tool.
///
/// The driver contract (upheld by the chain engine, the CLI, the wasm ABI,
/// and [`buffered_run`]):
/// - chunks for one `(port, index)` arrive in order;
/// - `end_input` is called exactly once per `(port, index)` after its
///   last chunk;
/// - `finish` is called once, after every input has ended.
///
/// Sessions may receive inputs interleaved (parallel chain branches) and
/// must buffer internally where their semantics require ordering.
pub trait StreamSession {
    /// Consume a chunk for an input, emitting zero or more output bytes.
    fn update(&mut self, port: &str, index: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError>;

    /// One input value's stream ended (its boundary, not the invocation's).
    fn end_input(&mut self, port: &str, index: usize) -> Result<Vec<u8>, ToolError>;

    /// The invocation ended: flush carries and emit the final bytes.
    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError>;
}

/// Validate options and open a session, if the tool streams.
pub fn open_stream_validated(
    tool: &dyn Tool,
    options: &Options,
) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
    let options = validate_options(&tool.manifest(), options)?;
    tool.open_stream(&options)
}

/// Derive a buffered run from a streaming session: feed every port's
/// values in manifest order, then assemble the output value. Streaming
/// tools implement `Tool::run` as a one-liner over this.
pub fn buffered_run(
    session: Box<dyn StreamSession>,
    manifest: &Manifest,
    mut inputs: Inputs,
) -> Result<DataValue, ToolError> {
    let mut session = session;
    let mut out = Vec::new();
    for port in &manifest.inputs {
        let values = inputs.remove(&port.name).unwrap_or_default();
        for (index, value) in values.into_iter().enumerate() {
            let (_, bytes) = value.into_payload();
            out.extend(session.update(&port.name, index, &bytes)?);
            out.extend(session.end_input(&port.name, index)?);
        }
    }
    out.extend(session.finish()?);
    assemble_output(manifest.output, out)
}

/// Turn a streaming tool's emitted bytes into its declared output value.
pub fn assemble_output(output: DataType, bytes: Vec<u8>) -> Result<DataValue, ToolError> {
    DataValue::from_payload(
        &ValueMeta {
            data_type: output,
            format: String::new(),
        },
        bytes,
    )
    .map_err(ToolError::new)
}
