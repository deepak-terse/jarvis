mod adapters;
mod app;
mod capabilities;
mod channels;
mod core;
mod memory;
mod routines;
mod skills;
mod tooling;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    app::start(app::commands::Cli::parse())
}
