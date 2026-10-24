//! The stable surface of Jarvis: traits and types that plug-ins implement and the
//! agent consumes. Imports nothing else from this crate, and changes rarely; a change
//! here ripples to every implementation.

pub mod capabilities;
pub mod events;
pub mod memory;
pub mod model;
pub mod settings;
pub mod tools;
