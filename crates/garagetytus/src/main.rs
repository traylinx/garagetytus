//! `garagetytus` — entry point.
//!
//! Subcommand surface lives in `cli.rs`; per-command bodies live
//! under `commands/`. Logging is `RUST_LOG`-driven via tracing-
//! subscriber; default level is `info`.

mod cli;
mod commands;
mod context;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Cmd};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(false))
        .init();

    let args = Cli::parse();
    let ctx = context::CliContext::new()?;

    let exit_code = match args.cmd {
        Cmd::Install => commands::install::run(&ctx).await,
        Cmd::Start => commands::start::run(&ctx, false),
        Cmd::Stop => commands::start::stop(&ctx),
        Cmd::Status => commands::start::status(&ctx),
        Cmd::Restart => commands::start::run(&ctx, true),
        Cmd::Serve => commands::start::serve(&ctx),
        Cmd::Bootstrap => commands::bootstrap::run(&ctx).await,
        Cmd::About => commands::about::run(),
        Cmd::Bucket { cmd } => commands::bucket::run(&ctx, cmd).await,
    }?;

    std::process::exit(exit_code);
}
