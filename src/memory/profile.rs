//! `profile.toml`: facts about the user, hand-edited for now.

use anyhow::{Context, Result};
use std::path::Path;

const TEMPLATE: &str = "# Facts about you, added to every system prompt. Any keys work.\n\
# name = \"Tony\"\n# location = \"Mumbai, India\"\n# country = \"India\"\n# units = \"metric\"\n";

pub fn create_if_missing(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::write(path, TEMPLATE)?;
    }
    Ok(())
}

pub fn load(path: &Path) -> Result<toml::Table> {
    toml::from_str(&std::fs::read_to_string(path)?).with_context(|| format!("parsing {}", path.display()))
}

/// Profile facts flattened to `key: value` lines (nested tables become `a.b`).
pub fn flatten(profile: &toml::Table) -> String {
    let mut lines = Vec::new();
    flatten_into("", &toml::Value::Table(profile.clone()), &mut lines);
    lines.join("\n")
}

fn flatten_into(key: &str, value: &toml::Value, out: &mut Vec<String>) {
    match value {
        toml::Value::Table(t) => {
            for (k, v) in t {
                flatten_into(&if key.is_empty() { k.clone() } else { format!("{key}.{k}") }, v, out);
            }
        }
        toml::Value::String(s) => out.push(format!("{key}: {s}")),
        toml::Value::Array(a) => {
            let items: Vec<String> =
                a.iter().map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string)).collect();
            out.push(format!("{key}: {}", items.join(", ")));
        }
        other => out.push(format!("{key}: {other}")),
    }
}
