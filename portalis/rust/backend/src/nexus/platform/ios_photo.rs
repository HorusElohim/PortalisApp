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

/// Imports one verified received file into Photos and returns the durable
/// identifier. The caller persists and rebinds that identifier before it
/// removes the app-owned download, so an interruption never loses content.
pub(crate) fn import_completed_media(path: &str, video: bool) -> anyhow::Result<String> {
    let path = CString::new(path)?;
    let mut identifier = vec![0_i8; 512];
    let result = unsafe {
        portalis_photo_asset_import(
            path.as_ptr(),
            video,
            identifier.as_mut_ptr(),
            identifier.len(),
        )
    };
    anyhow::ensure!(
        result == 0,
        "PhotoKit could not import the completed media ({result})"
    );
    let identifier = unsafe { std::ffi::CStr::from_ptr(identifier.as_ptr()) }
        .to_str()?
        .to_owned();
    anyhow::ensure!(
        !identifier.is_empty(),
        "PhotoKit created an asset without an identifier"
    );
    Ok(identifier)
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
    fn portalis_photo_asset_import(
        path: *const std::ffi::c_char,
        video: bool,
        identifier: *mut std::ffi::c_char,
        identifier_capacity: usize,
    ) -> i32;
}
