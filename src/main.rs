// Also set in Cargo.toml [lints]; repeated here so cargo-geiger sees it.
#![forbid(unsafe_code)]

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if let Some(index) = args.iter().position(|arg| arg == "--fix") {
        let fix_id = args
            .get(index + 1)
            .context("--fix requires an allowlisted fix id")?;
        omniscient::fix::run(fix_id)
    } else if args.get(1).map(String::as_str) == Some("--privileged-helper") {
        omniscient::helper::serve()
    } else if args.iter().any(|arg| arg == "--trends") {
        omniscient::trends::run()
    } else if args.iter().any(|arg| arg == "--watch") {
        omniscient::watch::run(&args)
    } else if args.iter().any(|arg| arg == "--journal") {
        omniscient::journal::run(&args)
    } else if args.iter().any(|arg| arg == "--sensors") {
        omniscient::sensors::run()
    } else if args.iter().any(|arg| arg == "--hud") {
        omniscient::headless::run()
    } else if args.iter().any(|arg| arg == "--packages") {
        omniscient::headless::run_packages()
    } else {
        omniscient::tui::run()
    }
}
