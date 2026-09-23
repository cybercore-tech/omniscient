use cybercore::palette;
use std::io::{self, Write};
use std::thread::sleep;
use std::time::Duration;

/// Plays the "SCANNING... COMPLETE" animation for a module label.
/// Same visual beat as the original fish version, using the real
/// theme palette instead of xterm-256 approximations.
pub fn scan(label: &str) {
    let frames = [
        (palette::purple(), "▓▒▒"),
        (palette::hot_pink(), "▓▓▒"),
        (palette::cyan(), "▓▓▓"),
    ];

    for _ in 0..2 {
        for (color, bar) in &frames {
            print!("\r{color} SCANNING {label:<18} {bar}");
            io::stdout().flush().ok();
            sleep(Duration::from_millis(50));
        }
    }

    println!(
        "\r{} COMPLETE  {:<18} ✓{}",
        palette::acid_green(),
        label,
        palette::RESET
    );
}
