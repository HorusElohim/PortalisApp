//! Rust boundary for the iOS PhotoKit adapter.
//!
//! The Objective-C implementation owns PhotoKit and AVFoundation. Nexus keeps
//! the narrow, fallible Rust calls here so its content-location model does not
//! need to know about C strings or foreign symbols.

use std::ffi::CString;

pub(crate) fn asset_available(identifier: &str) -> bool {
    let Ok(identifier) = CString::new(identifier) else {
        return false;
    };
    unsafe { portalis_photo_asset_available(identifier.as_ptr()) }
}

pub(crate) fn asset_length(identifier: &str) -> anyhow::Result<u64> {
    let identifier = CString::new(identifier)?;
    let length = unsafe { portalis_photo_asset_length(identifier.as_ptr()) };
    anyhow::ensure!(
        length > 0,
        "PhotoKit could not determine the selected asset length ({length})"
    );
    Ok(length as u64)
}

pub(crate) fn read_asset(identifier: &str, offset: u64, buffer: &mut [u8]) -> anyhow::Result<()> {
    let identifier = CString::new(identifier)?;
    let result = unsafe {
        portalis_photo_asset_read(
            identifier.as_ptr(),
            offset,
            buffer.as_mut_ptr(),
            buffer.len(),
        )
    };
    anyhow::ensure!(
        result == 0,
        "PhotoKit could not read the requested asset range ({result})"
    );
    Ok(())
}

unsafe extern "C" {
    fn portalis_photo_asset_available(identifier: *const std::ffi::c_char) -> bool;
    fn portalis_photo_asset_length(identifier: *const std::ffi::c_char) -> i64;
    fn portalis_photo_asset_read(
        identifier: *const std::ffi::c_char,
        offset: u64,
        buffer: *mut u8,
        length: usize,
    ) -> i32;
}
