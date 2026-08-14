//! The operations a person actually performs on a collection.
//!
//! Steps 1 to 6 are each one piece of machinery. This is where they become
//! something a person can do: make a collection, put media in it, publish it,
//! and decide who can read it.
//!
//! Still no network. A collection is a set of signed and sealed objects, and
//! handing them to a peer is step 8's problem — writing the workflows first
//! means the transport has nothing left to decide, and two cores in one
//! process can exchange objects by hand and both verify.
//!
//! - [`model`]: what a collection is on this device.
//! - [`publish`]: creating one, adding to it, and producing a revision.
//! - [`receive`]: deciding whether to believe someone else's.
//! - [`members`]: changing who can read it, and rotating when someone leaves.

/// The Flutter-facing commands as they were before v3, kept working while the
/// modules beside it are written. Step 9 replaces the bridge and deletes it.
///
/// Named `legacy` rather than left as `collections.rs` so that every reference
pub mod members;
pub mod model;
pub mod publish;
pub mod receive;

