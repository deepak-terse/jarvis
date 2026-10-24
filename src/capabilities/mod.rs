//! Capability slots, named by function (`stt`, `tts`, later `vad`, `vision`).
//! The traits live in `core::contracts::capabilities`; each slot picks an adapter by
//! `[capabilities.<name>] provider` and holds it according to `keep_warm`.

pub mod stt;
pub mod tts;

use anyhow::Result;

/// Holds a provider according to `keep_warm`: loaded up front and kept, or loaded
/// on demand and released when idle to save memory.
pub struct Warm<T> {
    load: Box<dyn Fn() -> Result<T> + Send>,
    instance: Option<T>,
    keep: bool,
}

impl<T> Warm<T> {
    pub fn new(keep: bool, load: impl Fn() -> Result<T> + Send + 'static) -> Result<Self> {
        let instance = if keep { Some(load()?) } else { None };
        Ok(Self { load: Box::new(load), instance, keep })
    }

    pub fn get(&mut self) -> Result<&mut T> {
        if self.instance.is_none() {
            self.instance = Some((self.load)()?);
        }
        Ok(self.instance.as_mut().unwrap())
    }

    /// Called at the end of a turn; releases the instance unless kept warm.
    pub fn idle(&mut self) {
        if !self.keep {
            self.instance = None;
        }
    }
}
