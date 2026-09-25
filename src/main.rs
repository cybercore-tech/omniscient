use anyhow::Result;

fn main() -> Result<()> {
    if std::env::args().any(|arg| arg == "--hud") {
        omniscient::headless::run()
    } else {
        omniscient::tui::run()
    }
}
