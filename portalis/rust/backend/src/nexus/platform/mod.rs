//! Native platform adapters used by the Nexus runtime.
//!
//! These modules own OS framework and JNI boundaries. The rest of Nexus stays
//! platform-neutral and reaches them through focused helpers.

#[cfg(target_os = "android")]
pub(crate) mod android_content;
#[cfg(target_os = "ios")]
pub(crate) mod ios_photo;
