//! Image tools. All operate on encoded images (png/jpeg/gif/bmp; webp
//! decode-only) and re-encode on output.

mod codec;
mod compress;
mod convert;
mod crop;
mod merge;
mod resize;

use toolkit_core::Registry;

pub fn registry() -> Registry {
    Registry::new(vec![
        Box::new(resize::Resize),
        Box::new(crop::Crop),
        Box::new(convert::Convert),
        Box::new(compress::Compress),
        Box::new(merge::Merge),
    ])
}

toolkit_core::export_pack_abi!(crate::registry);
