//! Tools the model can call. A skill is a folder implementing `Skill` (MCP's tool
//! shape) plus one line in `registry::create`; its `[skills.<name>]` config section
//! enables it and passes its settings.
//!
//! The registry wraps every skill in `execution::Guarded`, which applies the keys all
//! sections share (`timeout_secs`, `cache_ttl_secs`, `fallback`, `description`), so
//! skills only implement the lookup itself.

mod context;
mod execution;
mod registry;
mod support;

mod currency;
mod knowledge;
mod news;
mod stocks;
mod today;
mod weather;
mod web_search;

pub use crate::core::contracts::tools::{Policy, Skill, ToolOutcome};
pub use context::SkillContext;
pub use registry::Registry;
