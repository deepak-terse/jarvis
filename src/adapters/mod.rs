//! Concrete implementations of the contracts: models, speech engines, audio devices.
//! Each adapter depends only on `core::contracts`; config picks which ones run.

pub mod audio;
pub mod models;
pub mod stt;
pub mod tts;
