use serde::{Deserialize, Serialize};

/// The type of a value flowing between tools. Manifests declare what a tool
/// accepts and produces; chain edges are checked against these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    Text,
    Bytes,
    Json,
    Image,
}

impl DataType {
    pub const ALL: [DataType; 4] = [
        DataType::Text,
        DataType::Bytes,
        DataType::Json,
        DataType::Image,
    ];

    /// Whether a value of type `from` may be coerced into `to`.
    ///
    /// Coercions may still fail at runtime on the actual value (e.g. Bytes ->
    /// Text requires valid UTF-8), but anything outside this matrix is
    /// rejected statically when validating a chain.
    pub fn can_coerce(from: DataType, to: DataType) -> bool {
        use DataType::*;
        match (from, to) {
            _ if from == to => true,
            (Text, Bytes) | (Bytes, Text) => true,
            (Text, Json) | (Json, Text) => true,
            (Bytes, Json) | (Json, Bytes) => true,
            (Bytes, Image) | (Image, Bytes) => true,
            _ => false,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            DataType::Text => "text",
            DataType::Bytes => "bytes",
            DataType::Json => "json",
            DataType::Image => "image",
        }
    }
}

/// A typed value flowing through a chain.
///
/// `Image` holds *encoded* bytes (png/jpeg/...) plus a format tag; image
/// tools decode on entry and re-encode on exit. Keeping pixels out of the
/// contract keeps the ABI and non-image tools simple.
#[derive(Debug, Clone, PartialEq)]
pub enum DataValue {
    Text(String),
    Bytes(Vec<u8>),
    Json(serde_json::Value),
    Image { bytes: Vec<u8>, format: String },
}

/// Metadata describing a payload's type, serialized alongside the raw bytes
/// wherever a `DataValue` crosses a byte boundary (wasm ABI, files).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueMeta {
    #[serde(rename = "type")]
    pub data_type: DataType,
    /// Encoded image format tag ("png", "jpeg", ...); empty when unknown.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub format: String,
}

impl DataValue {
    pub fn data_type(&self) -> DataType {
        match self {
            DataValue::Text(_) => DataType::Text,
            DataValue::Bytes(_) => DataType::Bytes,
            DataValue::Json(_) => DataType::Json,
            DataValue::Image { .. } => DataType::Image,
        }
    }

    /// Coerce this value to `to`, per the `DataType::can_coerce` matrix.
    pub fn coerce(self, to: DataType) -> Result<DataValue, String> {
        use DataType::*;
        if self.data_type() == to {
            return Ok(self);
        }
        match (self, to) {
            (DataValue::Text(s), Bytes) => Ok(DataValue::Bytes(s.into_bytes())),
            (DataValue::Text(s), Json) => serde_json::from_str(&s)
                .map(DataValue::Json)
                .map_err(|e| format!("text is not valid JSON: {e}")),
            (DataValue::Bytes(b), Text) => String::from_utf8(b)
                .map(DataValue::Text)
                .map_err(|_| "bytes are not valid UTF-8 text".to_string()),
            (DataValue::Bytes(b), Json) => serde_json::from_slice(&b)
                .map(DataValue::Json)
                .map_err(|e| format!("bytes are not valid JSON: {e}")),
            (DataValue::Bytes(b), Image) => Ok(DataValue::Image {
                bytes: b,
                format: String::new(), // image tools sniff the format from bytes
            }),
            (DataValue::Json(v), Text) => Ok(DataValue::Text(
                serde_json::to_string(&v).expect("JSON value serializes"),
            )),
            (DataValue::Json(v), Bytes) => Ok(DataValue::Bytes(
                serde_json::to_vec(&v).expect("JSON value serializes"),
            )),
            (DataValue::Image { bytes, .. }, Bytes) => Ok(DataValue::Bytes(bytes)),
            (v, to) => Err(format!(
                "cannot convert {} to {}",
                v.data_type().name(),
                to.name()
            )),
        }
    }

    /// Split into (meta, raw bytes) for transport across a byte boundary.
    pub fn into_payload(self) -> (ValueMeta, Vec<u8>) {
        let data_type = self.data_type();
        match self {
            DataValue::Text(s) => (
                ValueMeta {
                    data_type,
                    format: String::new(),
                },
                s.into_bytes(),
            ),
            DataValue::Bytes(b) => (
                ValueMeta {
                    data_type,
                    format: String::new(),
                },
                b,
            ),
            DataValue::Json(v) => (
                ValueMeta {
                    data_type,
                    format: String::new(),
                },
                serde_json::to_vec(&v).expect("JSON value serializes"),
            ),
            DataValue::Image { bytes, format } => (ValueMeta { data_type, format }, bytes),
        }
    }

    /// Reassemble from (meta, raw bytes). Inverse of `into_payload`.
    pub fn from_payload(meta: &ValueMeta, bytes: Vec<u8>) -> Result<DataValue, String> {
        match meta.data_type {
            DataType::Text => String::from_utf8(bytes)
                .map(DataValue::Text)
                .map_err(|_| "payload is not valid UTF-8 text".to_string()),
            DataType::Bytes => Ok(DataValue::Bytes(bytes)),
            DataType::Json => serde_json::from_slice(&bytes)
                .map(DataValue::Json)
                .map_err(|e| format!("payload is not valid JSON: {e}")),
            DataType::Image => Ok(DataValue::Image {
                bytes,
                format: meta.format.clone(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coercion_matrix_matches_runtime_coercions() {
        // Every statically-allowed coercion must be attempted by coerce();
        // every disallowed one must be rejected.
        let samples = |t: DataType| -> DataValue {
            match t {
                DataType::Text => DataValue::Text("{\"a\":1}".into()),
                DataType::Bytes => DataValue::Bytes(b"{\"a\":1}".to_vec()),
                DataType::Json => DataValue::Json(serde_json::json!({"a": 1})),
                DataType::Image => DataValue::Image {
                    bytes: vec![1, 2],
                    format: "png".into(),
                },
            }
        };
        for from in DataType::ALL {
            for to in DataType::ALL {
                let result = samples(from).coerce(to);
                if DataType::can_coerce(from, to) {
                    assert!(result.is_ok(), "{from:?} -> {to:?} should coerce");
                } else {
                    assert!(result.is_err(), "{from:?} -> {to:?} should be rejected");
                }
            }
        }
    }

    #[test]
    fn bytes_to_text_requires_utf8() {
        assert!(DataValue::Bytes(vec![0xff, 0xfe])
            .coerce(DataType::Text)
            .is_err());
    }

    #[test]
    fn payload_round_trip() {
        let values = [
            DataValue::Text("hello".into()),
            DataValue::Bytes(vec![0, 1, 2, 255]),
            DataValue::Json(serde_json::json!({"k": [1, 2]})),
            DataValue::Image {
                bytes: vec![9, 9],
                format: "jpeg".into(),
            },
        ];
        for v in values {
            let (meta, bytes) = v.clone().into_payload();
            assert_eq!(DataValue::from_payload(&meta, bytes).unwrap(), v);
        }
    }
}
