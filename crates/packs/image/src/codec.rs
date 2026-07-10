//! Shared decode/encode between the image tools.

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::{DynamicImage, ImageFormat};
use std::io::Cursor;
use toolkit_core::{DataValue, ToolError};

/// Decode an Image value. The format is always sniffed from the bytes —
/// the tag on the value is advisory (it may be empty after a Bytes
/// coercion, or wrong if the user mislabeled a file).
pub fn decode(value: DataValue) -> Result<(DynamicImage, ImageFormat), ToolError> {
    let DataValue::Image { bytes, .. } = value else {
        unreachable!()
    };
    let format = image::guess_format(&bytes).map_err(|_| {
        ToolError::new("could not detect image format (supported: png, jpeg, gif, bmp, webp)")
    })?;
    let img = image::load_from_memory_with_format(&bytes, format).map_err(|e| {
        ToolError::new(format!(
            "failed to decode {} image: {e}",
            format_name(format)
        ))
    })?;
    Ok((img, format))
}

/// Encode to `format`. `quality` (1-100) applies to jpeg; png always uses
/// best compression; other formats use their encoder defaults.
pub fn encode(
    img: &DynamicImage,
    format: ImageFormat,
    quality: Option<u8>,
) -> Result<DataValue, ToolError> {
    let mut bytes = Vec::new();
    let fail = |e: image::ImageError| {
        ToolError::new(format!(
            "failed to encode {} image: {e}",
            format_name(format)
        ))
    };
    match format {
        ImageFormat::Jpeg => {
            let encoder =
                JpegEncoder::new_with_quality(Cursor::new(&mut bytes), quality.unwrap_or(85));
            // JPEG has no alpha channel; flatten before encoding.
            img.clone()
                .into_rgb8()
                .write_with_encoder(encoder)
                .map_err(fail)?;
        }
        ImageFormat::Png => {
            let encoder = PngEncoder::new_with_quality(
                Cursor::new(&mut bytes),
                CompressionType::Best,
                PngFilter::Adaptive,
            );
            img.write_with_encoder(encoder).map_err(fail)?;
        }
        _ => {
            img.write_to(&mut Cursor::new(&mut bytes), format)
                .map_err(fail)?;
        }
    }
    Ok(DataValue::Image {
        bytes,
        format: format_name(format).to_string(),
    })
}

pub fn format_name(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Gif => "gif",
        ImageFormat::Bmp => "bmp",
        ImageFormat::WebP => "webp",
        _ => "unknown",
    }
}

pub fn format_from_name(name: &str) -> Result<ImageFormat, ToolError> {
    match name {
        "png" => Ok(ImageFormat::Png),
        "jpeg" | "jpg" => Ok(ImageFormat::Jpeg),
        "gif" => Ok(ImageFormat::Gif),
        "bmp" => Ok(ImageFormat::Bmp),
        other => Err(ToolError::new(format!(
            "unsupported output format \"{other}\""
        ))),
    }
}

#[cfg(test)]
pub mod fixtures {
    use super::*;

    /// A tiny in-memory test image: `w`x`h` with a red/blue checker pattern.
    pub fn png(w: u32, h: u32) -> DataValue {
        let img = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(w, h, |x, y| {
            if (x + y) % 2 == 0 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            }
        }));
        encode(&img, ImageFormat::Png, None).unwrap()
    }

    pub fn dimensions(value: &DataValue) -> (u32, u32) {
        let DataValue::Image { bytes, .. } = value else {
            panic!("not an image")
        };
        let img = image::load_from_memory(bytes).unwrap();
        (img.width(), img.height())
    }
}
