//! Embedded ANSI palette for the terminal surface.
//!
//! Keep the release binary self-contained.  The HUD and reports have their
//! own presentation layer, so the terminal animation only needs these fixed
//! Cybercore colors and should not fetch a remote Git dependency at build
//! time.

pub const RESET: &str = "\x1b[0m";

fn rgb(hex: &str) -> String {
    let red = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let green = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let blue = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    format!("\x1b[38;2;{red};{green};{blue}m")
}

#[must_use]
pub fn acid_green() -> String {
    rgb("c8e967")
}

#[must_use]
pub fn hot_pink() -> String {
    rgb("fd3e6a")
}

#[must_use]
pub fn purple() -> String {
    rgb("9147a8")
}

#[must_use]
pub fn cyan() -> String {
    rgb("14b9b5")
}
