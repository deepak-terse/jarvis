//! Live-data skills: answers that depend on the current state of the world.
//! Each skill owns its provider clients; shared plumbing lives in `skills::common`.

mod currency;
mod knowledge;
mod news;
mod stocks;
mod today;
mod weather;
mod web_search;

use super::{Skill, SkillContext};
use anyhow::Result;
use std::sync::Arc;

/// This group's skills by config name; `None` if the name belongs to another group.
pub fn create(name: &str, settings: &toml::Table, ctx: &SkillContext) -> Option<Result<Arc<dyn Skill>>> {
    Some(match name {
        "web_search" => web_search::create(settings, ctx),
        "news" => news::create(settings, ctx),
        "knowledge" => knowledge::create(settings, ctx),
        "about_today" => today::create(settings, ctx),
        "stocks" => stocks::create(settings, ctx),
        "currency" => currency::create(settings, ctx),
        "weather" => weather::create(settings, ctx),
        _ => return None,
    })
}
