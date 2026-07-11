//! Adversarial-input harness shared by the packs' property tests and the
//! cargo-fuzz targets (fuzz/). Contains no tool logic — it drives any
//! [`Tool`] through both execution paths with arbitrary bytes and checks
//! the invariants every tool must uphold:
//!
//! 1. **No panics.** Malformed input is an `Err`, never a crash — in the
//!    browser a panic aborts the whole wasm instance.
//! 2. **Chunking is invisible.** A streaming tool fed the same bytes as
//!    one chunk or split at arbitrary boundaries must produce the same
//!    outcome (identical output bytes, or an error either way).
//!
//! Every pack runs this from `cargo test` over a seeded corpus (so CI
//! exercises it on every PR, deterministically); the fuzz targets run the
//! same function under libFuzzer with coverage guidance.

use crate::stream::StreamSession;
use crate::{
    run_tool, DataValue, InputSpec, Inputs, OptionKind, Options, Tool, ToolError, ENTROPY_LEN,
};

/// Option sets to sweep: the defaults, then — one at a time — every value
/// of every enum option and both settings of every bool. Varying one knob
/// at a time keeps the sweep linear in the number of options while still
/// reaching every branch that dispatches on an option value.
pub fn option_variants(tool: &dyn Tool) -> Vec<Options> {
    let manifest = tool.manifest();
    let mut variants = vec![Options::new()];
    for spec in &manifest.options {
        match &spec.kind {
            OptionKind::Enum { values } => {
                for value in values {
                    let mut opts = Options::new();
                    opts.insert(spec.name.clone(), value.clone().into());
                    variants.push(opts);
                }
            }
            OptionKind::Bool => {
                for value in [true, false] {
                    let mut opts = Options::new();
                    opts.insert(spec.name.clone(), value.into());
                    variants.push(opts);
                }
            }
            _ => {}
        }
    }
    variants
}

fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state | 1;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

/// Deterministic corpus entry: arbitrary bytes, but biased across rounds
/// toward the alphabets tools actually parse (UTF-8 text, base64, hex,
/// JSON-ish punctuation) so the property tests reach past the first
/// validation error.
pub fn corpus_entry(seed: u64, round: u32) -> Vec<u8> {
    const BASE64ISH: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef0123456789+/=_-";
    const HEXISH: &[u8] = b"0123456789abcdefABCDEF \n";
    const JSONISH: &[u8] = b"{}[]\",:0123456789.eE+-truefalsnu \n";
    let mut state = seed.wrapping_add(u64::from(round).wrapping_mul(0x9E3779B97F4A7C15));
    let len = (xorshift(&mut state) % 2048) as usize;
    let mut data = Vec::with_capacity(len);
    for _ in 0..len {
        let byte = (xorshift(&mut state) >> 32) as u8;
        data.push(match round % 5 {
            0 => byte,        // raw bytes
            1 => byte % 0x80, // ASCII
            2 => BASE64ISH[byte as usize % BASE64ISH.len()],
            3 => HEXISH[byte as usize % HEXISH.len()],
            _ => JSONISH[byte as usize % JSONISH.len()],
        });
    }
    data
}

/// Drive one tool with `data` on every input port, across the option
/// sweep, checking the two invariants. Panics (failing the test / fuzz
/// run) if a streamed run disagrees with the whole-input run; tool errors
/// are expected and ignored.
pub fn exercise(tool: &dyn Tool, data: &[u8], seed: u64) {
    let manifest = tool.manifest();
    for options in option_variants(tool) {
        // Invariant 1: the buffered path never panics.
        let _ = run_tool(tool, build_inputs(&manifest.inputs, data), &options);

        // Invariant 2: chunk boundaries never change a streaming outcome.
        let validated = match crate::validate_options(&manifest, &options) {
            Ok(v) => v,
            Err(_) => continue, // needs options with no defaults; nothing to stream
        };
        let whole = match tool.open_stream(&validated) {
            Ok(Some(session)) => feed(session, &manifest.inputs, data, None),
            Ok(None) => continue,
            Err(_) => continue,
        };
        let session = tool
            .open_stream(&validated)
            .expect("open_stream succeeded a moment ago")
            .expect("tool reported a session a moment ago");
        let split = feed(session, &manifest.inputs, data, Some(seed));
        match (&whole, &split) {
            (Ok(a), Ok(b)) => assert_eq!(
                a, b,
                "tool \"{}\": streamed output depends on chunk boundaries (seed {seed})",
                manifest.name
            ),
            (Ok(_), Err(e)) | (Err(e), Ok(_)) => panic!(
                "tool \"{}\": one chunking succeeded, the other failed: {} (seed {seed})",
                manifest.name, e.message
            ),
            (Err(_), Err(_)) => {}
        }
    }
}

/// One value of `data` per port (two halves for multi ports, fixed bytes
/// for entropy ports) — mirrors what a driver would deliver.
fn build_inputs(specs: &[InputSpec], data: &[u8]) -> Inputs {
    let mut inputs = Inputs::new();
    for spec in specs {
        let values = if spec.entropy {
            vec![DataValue::Bytes(vec![7; ENTROPY_LEN])]
        } else if spec.multi {
            let mid = data.len() / 2;
            vec![
                DataValue::Bytes(data[..mid].to_vec()),
                DataValue::Bytes(data[mid..].to_vec()),
            ]
        } else {
            vec![DataValue::Bytes(data.to_vec())]
        };
        inputs.insert(spec.name.clone(), values);
    }
    inputs
}

/// Feed every port in manifest order — whole (`splits: None`) or split at
/// seed-derived boundaries — and collect the emitted bytes.
fn feed(
    mut session: Box<dyn StreamSession>,
    specs: &[InputSpec],
    data: &[u8],
    splits: Option<u64>,
) -> Result<Vec<u8>, ToolError> {
    let mut out = Vec::new();
    let mut state = splits.unwrap_or(0);
    for spec in specs {
        let values: Vec<&[u8]> = if spec.entropy {
            vec![&[7; ENTROPY_LEN]]
        } else if spec.multi {
            let mid = data.len() / 2;
            vec![&data[..mid], &data[mid..]]
        } else {
            vec![data]
        };
        for (index, value) in values.into_iter().enumerate() {
            match splits {
                None => out.extend(session.update(&spec.name, index, value)?),
                Some(_) => {
                    let mut rest = value;
                    while !rest.is_empty() {
                        let take = 1 + (xorshift(&mut state) as usize) % rest.len().min(97);
                        let (chunk, tail) = rest.split_at(take);
                        out.extend(session.update(&spec.name, index, chunk)?);
                        rest = tail;
                    }
                }
            }
            out.extend(session.end_input(&spec.name, index)?);
        }
    }
    out.extend(session.finish()?);
    Ok(out)
}
