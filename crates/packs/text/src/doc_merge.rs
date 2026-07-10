use toolkit_core::{
    buffered_run, DataType, DataValue, InputSpec, Inputs, Manifest, OptGet, OptionSpec, Options,
    StreamSession, Tool, ToolError,
};

/// Variable-arity tool: one `multi` port accepting any number of documents.
/// Streams with sequential consumption: the current document's chunks pass
/// straight through; chunks arriving early for later documents are buffered
/// until their turn (bounded degradation for lockstep chain branches,
/// perfect O(1) streaming for sequential sources like CLI file lists).
pub struct DocMerge;

impl Tool for DocMerge {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "doc-merge".into(),
            label: "Document Merge".into(),
            description: "Concatenate any number of text documents into one, in connection order, with a configurable separator.".into(),
            keywords: ["text", "merge", "concat", "join", "documents", "combine"]
                .map(String::from)
                .to_vec(),
            inputs: vec![InputSpec::named("documents", DataType::Text).multi()],
            output: DataType::Text,
            streaming: true,
            options: vec![OptionSpec::string(
                "separator",
                "Separator",
                "Inserted between documents. Escapes like \\n are taken literally as typed.",
            )
            .default_value("\n".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let session = self.open_stream(options)?.expect("streaming tool");
        buffered_run(session, &self.manifest(), inputs)
    }

    fn open_stream(&self, options: &Options) -> Result<Option<Box<dyn StreamSession>>, ToolError> {
        Ok(Some(Box::new(MergeSession {
            separator: options
                .str_opt("separator")
                .unwrap_or("\n")
                .as_bytes()
                .to_vec(),
            current: 0,
            sep_pending: false,
            buffers: std::collections::BTreeMap::new(),
            ended: std::collections::BTreeSet::new(),
        })))
    }
}

struct MergeSession {
    separator: Vec<u8>,
    /// The document currently allowed to pass through.
    current: usize,
    /// A separator is owed before the current document's first byte.
    sep_pending: bool,
    /// Early chunks of future documents, keyed by index.
    buffers: std::collections::BTreeMap<usize, Vec<u8>>,
    ended: std::collections::BTreeSet<usize>,
}

impl MergeSession {
    fn sep(&mut self, out: &mut Vec<u8>) {
        if self.sep_pending {
            out.extend_from_slice(&self.separator);
            self.sep_pending = false;
        }
    }

    /// The current document ended: move on, flushing whatever the next
    /// ones already delivered (and cascading past fully-ended ones).
    fn advance(&mut self, out: &mut Vec<u8>) {
        loop {
            self.sep(out); // an ended-but-empty document still owes one
            self.current += 1;
            self.sep_pending = true;
            if let Some(buf) = self.buffers.remove(&self.current) {
                if !buf.is_empty() {
                    self.sep(out);
                    out.extend_from_slice(&buf);
                }
            }
            if !self.ended.contains(&self.current) {
                break;
            }
        }
    }
}

impl StreamSession for MergeSession {
    fn update(&mut self, _: &str, index: usize, chunk: &[u8]) -> Result<Vec<u8>, ToolError> {
        let mut out = Vec::new();
        if index == self.current {
            self.sep(&mut out);
            out.extend_from_slice(chunk);
        } else if index > self.current {
            self.buffers
                .entry(index)
                .or_default()
                .extend_from_slice(chunk);
        } else {
            return Err(ToolError::new("document chunk arrived after its end"));
        }
        Ok(out)
    }

    fn end_input(&mut self, _: &str, index: usize) -> Result<Vec<u8>, ToolError> {
        self.ended.insert(index);
        let mut out = Vec::new();
        if index == self.current {
            self.advance(&mut out);
        }
        Ok(out)
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>, ToolError> {
        // The driver ends every input before finish, so advance() has
        // flushed everything; nothing can remain buffered.
        debug_assert!(self.buffers.is_empty());
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use toolkit_core::{run_single, run_tool};

    fn docs(texts: &[&str]) -> Inputs {
        Inputs::from([(
            "documents".to_string(),
            texts
                .iter()
                .map(|t| DataValue::Text(t.to_string()))
                .collect(),
        )])
    }

    #[test]
    fn merges_in_order_with_default_separator() {
        let out = run_tool(&DocMerge, docs(&["a", "b", "c"]), &Options::new()).unwrap();
        assert_eq!(out, DataValue::Text("a\nb\nc".into()));
    }

    #[test]
    fn custom_separator() {
        let out = run_tool(
            &DocMerge,
            docs(&["x", "y"]),
            json!({"separator": " --- "}).as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(out, DataValue::Text("x --- y".into()));
    }

    #[test]
    fn empty_documents_keep_their_separators() {
        let out = run_tool(&DocMerge, docs(&["a", "", "b"]), &Options::new()).unwrap();
        assert_eq!(out, DataValue::Text("a\n\nb".into()));
    }

    #[test]
    fn single_document_passes_through() {
        let out = run_single(&DocMerge, DataValue::Text("solo".into()), &Options::new()).unwrap();
        assert_eq!(out, DataValue::Text("solo".into()));
    }

    #[test]
    fn coerces_each_element() {
        let inputs = Inputs::from([(
            "documents".to_string(),
            vec![
                DataValue::Bytes(b"one".to_vec()),
                DataValue::Text("two".into()),
            ],
        )]);
        let out = run_tool(&DocMerge, inputs, &Options::new()).unwrap();
        assert_eq!(out, DataValue::Text("one\ntwo".into()));

        let bad = Inputs::from([(
            "documents".to_string(),
            vec![DataValue::Bytes(vec![0xff, 0xfe])],
        )]);
        assert!(run_tool(&DocMerge, bad, &Options::new()).is_err());
    }

    #[test]
    fn empty_port_is_an_error() {
        assert!(run_tool(&DocMerge, Inputs::new(), &Options::new()).is_err());
    }

    #[test]
    fn streaming_buffers_out_of_order_and_flushes_in_order() {
        // Interleaved arrival: doc1 data before doc0 finishes.
        let mut s = DocMerge.open_stream(&Options::new()).unwrap().unwrap();
        let mut out = Vec::new();
        out.extend(s.update("documents", 0, b"AA").unwrap());
        out.extend(s.update("documents", 1, b"BB").unwrap()); // buffered
        out.extend(s.update("documents", 0, b"aa").unwrap());
        out.extend(s.end_input("documents", 0).unwrap()); // flushes doc1's BB
        out.extend(s.update("documents", 1, b"bb").unwrap());
        out.extend(s.end_input("documents", 1).unwrap());
        out.extend(s.finish().unwrap());
        assert_eq!(String::from_utf8(out).unwrap(), "AAaa\nBBbb");
    }
}
