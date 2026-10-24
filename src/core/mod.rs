//! The brain (`agent`) and the contracts everything else plugs into (`contracts`).
//! The agent depends only on contracts; models, skills, memory, capabilities and
//! channels implement or consume them without the agent knowing which ones exist.

pub mod agent;
pub mod contracts;
