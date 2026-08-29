//! A local-only diagnostics log: crash reports and error traces the person
//! running Portalis can read and choose to share, and that never leaves the
//! device on its own.
//!
//! This is deliberately not a telemetry pipeline. There is no server this
//! writes to, no account it is tied to, and no automatic upload path —
//! sending a report anywhere at all is an action the person takes themselves
//! (the OS share sheet, from the Diagnostics screen), the same freedom the
//! rest of Portalis is built around. The file lives beside the other state
//! this app already keeps locally (`identity.json`, `settings.json`) in
//! [`crate::nexus::paths::state_dir`].
//!
//! Every [`crate::nexus::log::clog!`] call already writes to stderr; this
//! additionally appends the same line to a bounded file so it survives
//! after the console is gone (a release build launched from a home screen
//! has no attached console at all). The rotation keeps one bounded file
//! rather than growing forever — a diagnostics log a person is meant to
//! read must stay small enough to actually read.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

/// Above this many bytes, the file rotates: the oldest half is dropped and
/// the newest half becomes the new file. Bounded so an idle background
/// worker retried for days cannot fill the disk with its own diagnostics.
const MAX_BYTES: u64 = 1_000_000;

fn log_path() -> PathBuf {
    crate::nexus::paths::state_dir().join("diagnostics.log")
}

/// Appends one line, rotating first if the file has grown past
/// [`MAX_BYTES`]. Never fails outward — a diagnostics write must not be able
/// to break the thing it is diagnosing, matching [`crate::nexus::log::log`].
///
/// Opens the file fresh on every call rather than caching a handle: this is
/// not a hot path (one call per already-infrequent `clog!`), and a cached
/// handle would point at the wrong file the moment
/// [`crate::nexus::paths::state_dir`] changes underneath it — which the test
/// harness does per test via `redirect_to_temp`.
pub(crate) fn append(line: &str) {
    let path = log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > MAX_BYTES {
        rotate(&path);
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(file, "{line}");
}

/// Keeps the newest half of the file's lines, dropping the oldest half —
/// rather than truncating to zero, which would silently discard the report
/// of whatever just went wrong immediately before the file happened to be
/// full.
fn rotate(path: &std::path::Path) {
    let Ok(mut existing) = File::open(path) else {
        return;
    };
    let mut content = String::new();
    if existing.read_to_string(&mut content).is_err() {
        return;
    }
    let lines: Vec<&str> = content.lines().collect();
    let keep_from = lines.len() / 2;
    let kept = lines[keep_from..].join("\n");
    let _ = std::fs::write(path, kept);
}

/// The complete current diagnostics log, oldest line first.
///
/// # Errors
/// Returns a description when the file cannot be read (most commonly:
/// nothing has been logged yet, which is a normal empty-history case the
/// caller should render as "no diagnostics yet" rather than an error page).
pub fn read() -> Result<String, String> {
    std::fs::read_to_string(log_path())
        .map_err(|error| format!("could not read the diagnostics log: {error}"))
}

/// Removes everything logged so far. A person's own choice, from the
/// Diagnostics screen — nothing in Portalis clears this on its own.
///
/// # Errors
/// Returns a description when the file exists but cannot be removed.
pub fn clear() -> Result<(), String> {
    match std::fs::remove_file(log_path()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("could not clear the diagnostics log: {error}")),
    }
}

/// Where the log lives on disk, for a person who wants to find it directly
/// rather than through the app's share action.
pub fn path() -> String {
    log_path().display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_line_is_readable_back() {
        let _state = crate::nexus::paths::redirect_to_temp();
        clear().expect("starts clean");

        append("[1] [test] first line");
        append("[2] [test] second line");

        let content = read().expect("reads");
        assert!(content.contains("first line"));
        assert!(content.contains("second line"));
    }

    #[test]
    fn clearing_an_absent_log_is_not_an_error() {
        let _state = crate::nexus::paths::redirect_to_temp();
        assert!(clear().is_ok());
        assert!(clear().is_ok(), "clearing twice is still fine");
    }

    #[test]
    fn a_log_past_the_bound_rotates_to_its_newest_half_rather_than_growing_forever() {
        let _state = crate::nexus::paths::redirect_to_temp();
        clear().expect("starts clean");

        // Comfortably past MAX_BYTES: ~80 bytes/line * 20_000 lines is well
        // over the 1,000,000-byte bound, forcing at least one rotation.
        for i in 0..20_000 {
            append(&format!(
                "[{i}] [flood] padding padding padding padding padding"
            ));
        }

        let content = read().expect("reads");
        assert!(
            (content.len() as u64) < MAX_BYTES,
            "rotation must keep the file bounded, got {} bytes",
            content.len()
        );
        assert!(
            content.contains("[19999]"),
            "the newest line must survive rotation"
        );
        assert!(
            !content.contains("[0] [flood]"),
            "the oldest line must be the one rotation drops"
        );
    }

    #[test]
    fn the_reported_path_points_at_the_state_directory() {
        let _state = crate::nexus::paths::redirect_to_temp();
        assert!(path().ends_with("diagnostics.log"));
    }
}
