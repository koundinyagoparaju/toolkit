//! Image tools. All operate on encoded images (png/jpeg/gif/bmp; webp
//! decode-only) and re-encode on output.

#![deny(unsafe_code)]
mod clean;
mod codec;
mod compress;
mod convert;
mod crop;
mod exif_view;
mod merge;
mod qr;
mod resize;
mod rotate;

use toolkit_core::Registry;

pub fn registry() -> Registry {
    Registry::new(vec![
        Box::new(resize::Resize),
        Box::new(crop::Crop),
        Box::new(convert::Convert),
        Box::new(compress::Compress),
        Box::new(merge::Merge),
        Box::new(clean::ImageClean),
        Box::new(exif_view::ExifView),
        Box::new(rotate::ImageRotate),
        Box::new(qr::QrGenerate),
        Box::new(qr::QrDecode),
    ])
}

toolkit_core::export_pack_abi!(crate::registry);

#[cfg(test)]
mod pack_tests {
    use toolkit_core::{validate_against_specs, Options};

    #[test]
    fn streaming_flag_matches_sessions() {
        let registry = super::registry();
        for m in registry.manifests() {
            let tool = registry.find(&m.name).unwrap();
            match validate_against_specs(&m.options, &Options::new(), &m.name) {
                Ok(opts) => {
                    let streams = tool.open_stream(&opts).unwrap().is_some();
                    assert_eq!(
                        streams, m.streaming,
                        "tool {} flag/session mismatch",
                        m.name
                    );
                }
                Err(_) => assert!(!m.streaming, "streaming tool {} needs defaults", m.name),
            }
        }
    }
}
