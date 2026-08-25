//! The one authoritative place where collection bytes live.
//!
//! A location is deliberately not a Flutter path string. Today the concrete
//! filesystem implementation serves desktop and the Portalis Files library on
//! iOS. Android MediaStore URIs will add a native random-access implementation
//! here; they must never be converted to a cache path merely to fit a path API.

use std::path::PathBuf;

/// A canonical location whose bytes may be hashed, transferred, previewed,
/// and seeded. Each collection item has one such location.
#[derive(Debug, Clone)]
pub(crate) enum ContentLocation {
    Filesystem(PathBuf),
    #[cfg(target_os = "android")]
    AndroidContent(String),
    #[cfg(target_os = "ios")]
    PhotoAsset(String),
}

impl ContentLocation {
    /// Converts the Flutter bridge representation to a native location.
    pub(crate) fn from_source_path(source: &str) -> anyhow::Result<Self> {
        #[cfg(target_os = "android")]
        if source.starts_with("content://") {
            return Ok(Self::AndroidContent(source.into()));
        }
        #[cfg(target_os = "ios")]
        if let Some(identifier) = source.strip_prefix("phasset://") {
            anyhow::ensure!(
                !identifier.is_empty(),
                "a Photos identifier cannot be empty"
            );
            return Ok(Self::PhotoAsset(identifier.into()));
        }
        #[cfg(not(target_os = "android"))]
        anyhow::ensure!(
            !source.starts_with("content://"),
            "Android media URIs need Portalis' native no-copy storage adapter"
        );
        anyhow::ensure!(
            !source.trim().is_empty(),
            "a source location cannot be empty"
        );
        Ok(Self::Filesystem(PathBuf::from(source)))
    }

    pub(crate) fn length(&self, known_length: Option<u64>) -> anyhow::Result<u64> {
        #[cfg(not(target_os = "ios"))]
        let _ = known_length;
        match self {
            Self::Filesystem(path) => std::fs::metadata(path)
                .map(|metadata| metadata.len())
                .map_err(|error| anyhow::anyhow!("cannot read source {path:?}: {error}")),
            #[cfg(target_os = "android")]
            Self::AndroidContent(uri) => crate::nexus::platform::android_content::open(uri)
                .and_then(|file| file.metadata().map_err(anyhow::Error::from))
                .map(|metadata| metadata.len())
                .map_err(|error| anyhow::anyhow!("cannot read Android source {uri:?}: {error}")),
            #[cfg(target_os = "ios")]
            Self::PhotoAsset(identifier) => {
                anyhow::ensure!(
                    crate::nexus::platform::ios_photo::asset_available(identifier),
                    "Photos asset is no longer available"
                );
                let measured_length = crate::nexus::platform::ios_photo::asset_length(identifier)?;
                if known_length.is_some_and(|length| length != measured_length) {
                    crate::nexus::log::clog!(
                        "torrent",
                        "Photos asset length changed from picker metadata; using native resource length"
                    );
                }
                Ok(measured_length)
            }
        }
    }

    pub(crate) fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> anyhow::Result<()> {
        match self {
            Self::Filesystem(path) => {
                #[cfg(target_family = "unix")]
                {
                    use std::os::unix::fs::FileExt;
                    std::fs::File::open(path)?.read_exact_at(buffer, offset)?;
                    Ok(())
                }
                #[cfg(not(target_family = "unix"))]
                {
                    use std::io::{Read, Seek, SeekFrom};
                    let mut file = std::fs::File::open(path)?;
                    file.seek(SeekFrom::Start(offset))?;
                    file.read_exact(buffer)?;
                    Ok(())
                }
            }
            #[cfg(target_os = "android")]
            Self::AndroidContent(uri) => {
                use std::os::unix::fs::FileExt;
                crate::nexus::platform::android_content::open(uri)?
                    .read_exact_at(buffer, offset)?;
                Ok(())
            }
            #[cfg(target_os = "ios")]
            Self::PhotoAsset(identifier) => {
                crate::nexus::platform::ios_photo::read_asset(identifier, offset, buffer)
            }
        }
    }

    /// Writes newly acquired torrent bytes only where Portalis still owns a
    /// filesystem destination. Gallery-backed entries remain read-only: the
    /// native gallery is the finished asset, not an output folder.
    pub(crate) fn write_all_at(&self, offset: u64, buffer: &[u8]) -> anyhow::Result<()> {
        match self {
            Self::Filesystem(path) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let file = std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .open(path)?;
                #[cfg(target_family = "unix")]
                {
                    use std::os::unix::fs::FileExt;
                    file.write_all_at(buffer, offset)?;
                    Ok(())
                }
                #[cfg(not(target_family = "unix"))]
                {
                    use std::io::{Seek, SeekFrom, Write};
                    let mut file = file;
                    file.seek(SeekFrom::Start(offset))?;
                    file.write_all(buffer)?;
                    Ok(())
                }
            }
            #[cfg(target_os = "android")]
            Self::AndroidContent(_) => anyhow::bail!(
                "Android gallery-backed received media needs Portalis' native MediaStore writer"
            ),
            #[cfg(target_os = "ios")]
            Self::PhotoAsset(_) => anyhow::bail!("Photos-backed received media is read-only"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ContentLocation;

    #[cfg(not(target_os = "android"))]
    #[test]
    fn rejects_an_android_uri_instead_of_turning_it_into_a_cache_path() {
        assert!(ContentLocation::from_source_path("content://media/external/images/1").is_err());
    }

    #[test]
    fn keeps_a_filesystem_location_as_its_original_path() {
        let location = ContentLocation::from_source_path("C:/Media/photo.jpg").unwrap();
        assert!(matches!(
            location,
            ContentLocation::Filesystem(path) if path == std::path::Path::new("C:/Media/photo.jpg")
        ));
    }

    #[test]
    fn a_filesystem_location_can_receive_missing_torrent_bytes() {
        let directory = std::env::temp_dir().join(format!(
            "portalis-content-location-test-{}",
            std::process::id()
        ));
        let target = directory.join("nested").join("clip.mp4");
        let location = ContentLocation::Filesystem(target.clone());

        location
            .write_all_at(2, b"media")
            .expect("writes an incomplete received file");

        assert_eq!(std::fs::read(target).expect("reads"), b"\0\0media");
        std::fs::remove_dir_all(directory).expect("cleans up");
    }
}
