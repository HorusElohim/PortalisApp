//! The authoritative Portalis content-format and sealing contract.
//!
//! This crate owns the manifest/revision/device-log formats, the sealing and
//! signing primitives, the identifier derivations, and the wire limits they
//! are bounded by. It has no sockets, database drivers, or platform adapters.
//!
//! It deliberately does *not* carry a server-facing envelope protocol. The
//! protobuf `v1` envelope types, their frame codec, payload vocabulary, and
//! server-hello validation were the client half of a Nexus server that no
//! longer exists in this repository, and had no consumer in the app; they
//! were removed rather than kept compiling as an unreachable contract.

pub mod format;

mod ids;
mod limits;
mod sealing;
mod signing;

pub use format::aead::{AeadError, CONTENT_KEY_BYTES, ContentKey};
pub use format::devicelog::{
    Action, Device, DeviceLog, DeviceLogError, LOG_HASH_BYTES, LogEntry, LogHash,
    NO_PREVIOUS as NO_PREVIOUS_ENTRY,
};
pub use format::entry::{
    ENTRY_PAYLOAD_VERSION, EntryContext, EntryError, open as open_entry, seal as seal_entry,
};
pub use format::manifest::{
    ENTRY_VERSION, INFO_HASH_BYTES, MAX_ENTRIES, MAX_ENTRY_NAME_BYTES, Manifest, ManifestEntry,
    ManifestError, ManifestHash, THUMBNAIL_HASH_BYTES,
};
pub use format::revision::{
    MAX_MEMBERS, Member, NO_PREVIOUS as NO_PREVIOUS_REVISION, REVISION_HASH_BYTES, Revision,
    RevisionError, RevisionHash,
};
pub use format::sealed::{
    ManifestContext, SEALED_MANIFEST_VERSION, SealedManifestError, open as open_manifest,
    seal as seal_manifest,
};
pub use ids::{
    UUID_V7_ENTROPY_BYTES, derive_device_id, format_id, new_challenge, new_message_id, user_id_from,
};
pub use limits::{
    CHALLENGE_BYTES, CHALLENGE_LIFETIME_NS, CONNECTION_ID_BYTES, CURRENT_PROTOCOL_VERSION,
    DEVICE_ID_BYTES, DEVICE_KEY_BYTES, DISCRIMINATOR_CHARS, ENCRYPTION_KEY_BYTES,
    INFO_HASH_V1_BYTES, INFO_HASH_V2_BYTES, MAX_FRAME_BYTES, MAX_KEY_ENVELOPE_CIPHERTEXT_BYTES,
    MAX_KEY_ENVELOPES_PER_PAGE, MAX_OUTBOUND_QUEUE, MAX_PENDING_REQUESTS, MAX_SHARE_CAPSULE_BYTES,
    MAX_SHARE_HANDOFF_BYTES, MAX_SHARES_PER_RESPONSE, MAX_SWARM_CANDIDATES, MAX_USERNAME_CHARS,
    MESSAGE_ID_BYTES, MIN_USERNAME_CHARS, NANOS_PER_MILLI, SHARE_ID_BYTES, SIGNATURE_BYTES,
    SNAPSHOT_ID_BYTES, SWARM_LEASE_SECONDS, SWARM_REFRESH_SECONDS, USER_ID_BYTES,
};
pub use sealing::{
    EnvelopeContext, SealError, SealedEnvelope, is_contributory_x25519_public_key,
    open as open_envelope, seal as seal_envelope,
};
pub use signing::{
    AUTHENTICATION_CONTEXT, LINK_DEVICE_CONTEXT, REGISTRATION_CONTEXT, SessionBinding,
    SignatureError, authentication_payload, link_device_payload, registration_payload,
    verify_signature,
};
