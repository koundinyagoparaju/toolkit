use crate::data::DataType;
use serde::{Deserialize, Serialize};

/// A tool's self-description. Everything a UI or CLI needs to present the
/// tool and construct a valid invocation is derived from this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Stable slug used in chains and CLI invocations, e.g. "base64-decode".
    pub name: String,
    /// Human-friendly name, e.g. "Base64 Decode".
    pub label: String,
    pub description: String,
    pub keywords: Vec<String>,
    /// Named input ports. Most tools have exactly one; a merge-like tool
    /// declares several, each with a distinct role and type. Chain edges
    /// target ports by name (defaulting to a tool's sole port).
    pub inputs: Vec<InputSpec>,
    pub output: DataType,
    /// True when the tool implements a [`crate::stream::StreamSession`]:
    /// it can process input incrementally with O(1) memory.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub streaming: bool,
    #[serde(default)]
    pub options: Vec<OptionSpec>,
}

impl Manifest {
    /// The single input port, if the tool has exactly one.
    pub fn sole_input(&self) -> Option<&InputSpec> {
        match self.inputs.as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }

    pub fn input_port(&self, name: &str) -> Option<&InputSpec> {
        self.inputs.iter().find(|p| p.name == name)
    }
}

/// A named input port of a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: DataType,
    /// A multi port accepts a variable number of values (one or more) —
    /// e.g. doc-merge's `documents`. Cardinality lives on the port, never
    /// in the type system: `DataType` stays list-free.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multi: bool,
    /// An entropy port is auto-filled by the *driver* (CLI: OS RNG; web:
    /// crypto.getRandomValues) with [`crate::ENTROPY_LEN`] random bytes.
    /// The tool stays a pure function of its inputs — randomness arrives
    /// as explicit, auditable data, and wasm packs keep zero host imports.
    /// UIs hide these ports; chains may wire them for reproducibility.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub entropy: bool,
    /// A sample input that demonstrates the tool (as UTF-8 text). Must
    /// run successfully with default options — enforced by a CLI test.
    /// UIs offer it as a one-click demo; `toolkit info` prints it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
    /// What this port expects, for multi-port tools where the name alone
    /// is thin (e.g. hmac's "key"). Shown in MCP input schemas, the web
    /// input panels, and `toolkit info`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl InputSpec {
    pub const SOLE_NAME: &'static str = "input";

    /// The conventional single port of an ordinary one-input tool.
    pub fn sole(data_type: DataType) -> Vec<InputSpec> {
        vec![InputSpec::named(Self::SOLE_NAME, data_type)]
    }

    /// [`InputSpec::sole`] with a demo input attached.
    pub fn sole_example(data_type: DataType, example: &str) -> Vec<InputSpec> {
        vec![InputSpec::named(Self::SOLE_NAME, data_type).example(example)]
    }

    pub fn named(name: &str, data_type: DataType) -> InputSpec {
        InputSpec {
            name: name.into(),
            data_type,
            multi: false,
            entropy: false,
            example: None,
            description: None,
        }
    }

    pub fn multi(mut self) -> InputSpec {
        self.multi = true;
        self
    }

    pub fn example(mut self, example: &str) -> InputSpec {
        self.example = Some(example.into());
        self
    }

    pub fn describe(mut self, description: &str) -> InputSpec {
        self.description = Some(description.into());
        self
    }

    /// The conventional driver-filled randomness port of a generator tool.
    pub fn entropy() -> InputSpec {
        InputSpec {
            name: "entropy".into(),
            data_type: DataType::Bytes,
            multi: false,
            entropy: true,
            example: None,
            description: None,
        }
    }
}

/// A single configurable option of a tool. Web forms and CLI flags are
/// generated from these specs, and values are validated against them before
/// the tool runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionSpec {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub kind: OptionKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OptionKind {
    String,
    Integer {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    Float {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    Bool,
    Enum {
        values: Vec<String>,
    },
}

impl OptionSpec {
    pub fn string(name: &str, label: &str, description: &str) -> Self {
        Self::new(name, label, description, OptionKind::String)
    }

    pub fn integer(
        name: &str,
        label: &str,
        description: &str,
        min: Option<i64>,
        max: Option<i64>,
    ) -> Self {
        Self::new(name, label, description, OptionKind::Integer { min, max })
    }

    pub fn float(
        name: &str,
        label: &str,
        description: &str,
        min: Option<f64>,
        max: Option<f64>,
    ) -> Self {
        Self::new(name, label, description, OptionKind::Float { min, max })
    }

    pub fn bool(name: &str, label: &str, description: &str) -> Self {
        Self::new(name, label, description, OptionKind::Bool)
    }

    pub fn enumeration(name: &str, label: &str, description: &str, values: &[&str]) -> Self {
        Self::new(
            name,
            label,
            description,
            OptionKind::Enum {
                values: values.iter().map(|s| s.to_string()).collect(),
            },
        )
    }

    fn new(name: &str, label: &str, description: &str, kind: OptionKind) -> Self {
        OptionSpec {
            name: name.into(),
            label: label.into(),
            description: description.into(),
            kind,
            required: false,
            default: None,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn default_value(mut self, v: serde_json::Value) -> Self {
        self.default = Some(v);
        self
    }
}
