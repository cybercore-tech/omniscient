//! True-color ANSI helpers matching the actual CYBERGRID theme palette
//! (the same hex values Ghostty's generated theme uses), rather than
//! xterm-256 approximations.

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";

fn rgb(hex: &str) -> String {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    format!("\x1b[38;2;{r};{g};{b}m")
}

pub fn acid_green() -> String {
    rgb("c8e967")
}
pub fn hot_pink() -> String {
    rgb("FD3E6A")
}
pub fn purple() -> String {
    rgb("9147a8")
}
pub fn cyan() -> String {
    rgb("14B9B5")
}
pub fn red() -> String {
    rgb("f93d3b")
}
pub fn white() -> String {
    rgb("ffffff")
}
