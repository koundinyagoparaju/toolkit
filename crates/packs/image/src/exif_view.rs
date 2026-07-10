use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

pub struct ExifView;

impl Tool for ExifView {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "exif-view".into(),
            label: "EXIF View".into(),
            description: "Show the metadata a photo carries — GPS position, timestamps, camera model — without the photo leaving your device.".into(),
            keywords: ["exif", "metadata", "gps", "camera", "inspect", "image", "privacy"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Image),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Image { bytes, .. } = inputs.sole() else {
            unreachable!()
        };
        let mut cursor = std::io::Cursor::new(&bytes);
        let exif = match exif::Reader::new().read_from_container(&mut cursor) {
            Ok(exif) => exif,
            // No metadata is a result, not a failure.
            Err(exif::Error::NotFound(_) | exif::Error::BlankValue(_)) => {
                return Ok(DataValue::Json(serde_json::json!({
                    "fields": {},
                    "note": "no EXIF metadata found",
                })));
            }
            Err(e) => return Err(ToolError::new(format!("cannot read metadata: {e}"))),
        };

        let mut fields = serde_json::Map::new();
        for field in exif.fields() {
            let key = if field.ifd_num == exif::In::THUMBNAIL {
                format!("thumbnail.{}", field.tag)
            } else {
                field.tag.to_string()
            };
            fields.insert(
                key,
                serde_json::Value::String(field.display_value().with_unit(&exif).to_string()),
            );
        }
        Ok(DataValue::Json(serde_json::json!({ "fields": fields })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::fixtures;
    use toolkit_core::run_single;

    #[test]
    fn clean_image_reports_no_metadata() {
        let out = run_single(&ExifView, fixtures::png(4, 4), &Options::new()).unwrap();
        let DataValue::Json(v) = out else { panic!() };
        assert_eq!(v["fields"], serde_json::json!({}));
    }
}
