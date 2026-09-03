//! One stream down, commands up.
//!
//! The interface stops asking: it subscribes once and is told, and every
//! question it used to poll for is answered here instead —
//! including the ones it answered for itself by deriving state from a list.
//!
//! - [`state`]: what the interface is told, and what it may ask for.
//! - [`emit`]: deciding what crosses the bridge, and when.

pub mod emit;
pub mod state;
pub mod wire;
