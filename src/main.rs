use anyhow::{Context, Result};

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if let Some(index) = args.iter().position(|arg| arg == "--fix") {
        let fix_id = args
            .get(index + 1)
            .context("--fix requires an allowlisted fix id")?;
        omniscient::fix::run(fix_id)
    } else if args.iter().any(|arg| arg == "--hud") {
        omniscient::headless::run()
    } else {
        omniscient::tui::run()
    }
}
