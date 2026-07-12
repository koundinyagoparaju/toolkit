use image::{DynamicImage, ImageFormat};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

pub struct QrGenerate;

impl Tool for QrGenerate {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "qr-generate".into(),
            label: "QR Generate".into(),
            description: "Turn text into a QR code image — WiFi passwords and private links never touch a website.".into(),
            keywords: ["qr", "qrcode", "generate", "barcode", "wifi"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "https://example.com"),
            output: DataType::Image,
            streaming: false,
            options: vec![
                OptionSpec::enumeration(
                    "error_correction",
                    "Error correction",
                    "Higher levels survive more damage but make denser codes.",
                    &["low", "medium", "quartile", "high"],
                )
                .default_value("medium".into()),
                OptionSpec::integer("module_px", "Pixels per module", "", Some(1), Some(40))
                    .default_value(8.into()),
            ],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let ec = match options.str_opt("error_correction").unwrap_or("medium") {
            "low" => qrcode::EcLevel::L,
            "quartile" => qrcode::EcLevel::Q,
            "high" => qrcode::EcLevel::H,
            _ => qrcode::EcLevel::M,
        };
        let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), ec)
            .map_err(|e| ToolError::new(format!("cannot encode as QR: {e}")))?;
        let module_px = options.u32_opt("module_px").unwrap_or(8);
        let side = (code.width() as u32 + 8) * module_px; // 4-module quiet zone
        let img = code
            .render::<image::Luma<u8>>()
            .min_dimensions(side, side)
            .build();
        crate::codec::encode(&DynamicImage::ImageLuma8(img), ImageFormat::Png, None)
    }
}

pub struct QrDecode;

impl Tool for QrDecode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "qr-decode".into(),
            label: "QR Decode".into(),
            description:
                "Read the QR code in an image (e.g. a screenshot) without uploading it anywhere."
                    .into(),
            keywords: ["qr", "qrcode", "decode", "read", "scan"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Image),
            output: DataType::Text,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let (img, _) = crate::codec::decode(inputs.sole())?;
        let gray = img.to_luma8();
        let mut prepared = rqrr::PreparedImage::prepare(gray);
        let grids = prepared.detect_grids();
        let grid = grids
            .first()
            .ok_or_else(|| ToolError::new("no QR code found in the image"))?;
        let (_, content) = grid
            .decode()
            .map_err(|e| ToolError::new(format!("QR code found but unreadable: {e}")))?;
        Ok(DataValue::Text(content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    #[test]
    fn generate_then_decode_round_trip() {
        let secret = "WIFI:S:homenet;T:WPA;P:hunter2-but-long;;";
        let png = run_single(&QrGenerate, DataValue::Text(secret.into()), &Options::new()).unwrap();
        let DataValue::Image { ref format, .. } = png else {
            panic!()
        };
        assert_eq!(format, "png");
        let out = run_single(&QrDecode, png, &Options::new()).unwrap();
        assert_eq!(out, DataValue::Text(secret.into()));
    }

    #[test]
    fn no_qr_in_plain_image_errors() {
        let err = run_single(
            &QrDecode,
            crate::codec::fixtures::png(64, 64),
            &Options::new(),
        )
        .unwrap_err();
        assert!(err.message.contains("no QR code"), "{err}");
    }
}
