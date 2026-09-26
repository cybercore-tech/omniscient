//! Bounded command capture.
//!
//! Every byte an audit module collects ends up in a Markdown report that the
//! Omarchy HUD later loads into the desktop shell. Unbounded capture let a
//! single `omarchy debug` section grow to 75 MB, which froze the shell when
//! the report was opened. Everything that runs an external command for a
//! report goes through [`run`], which enforces three limits:
//!
//! * **time**: the child is killed after its timeout;
//! * **memory**: at most [`Limits::retain_bytes`] of each stream is kept
//!   while the rest is drained and counted, so the child never blocks on a
//!   full pipe;
//! * **report size**: [`bound_text`] cleans control characters and caps the
//!   result by bytes and lines, saying exactly what was left out.

use std::fmt::Write as _;
use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// Most bytes of one report section.
pub const MAX_SECTION_BYTES: usize = 256 * 1024;
/// Most lines of one report section.
pub const MAX_SECTION_LINES: usize = 4_000;
/// Default time a captured command may run.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
/// How long to wait for output readers after the child has been killed.
const READER_GRACE: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// The limits applied to one command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Time before the child is killed.
    pub timeout: Duration,
    /// Bytes of each stream kept in memory; the rest is counted and dropped.
    pub retain_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            retain_bytes: MAX_SECTION_BYTES,
        }
    }
}

/// One stream of a finished command.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Stream {
    /// The retained prefix of the stream.
    pub bytes: Vec<u8>,
    /// Every byte the command wrote, retained or not.
    pub total: u64,
    /// Every newline the command wrote, retained or not, so a caller can
    /// count lines without keeping them.
    pub lines: u64,
}

impl Stream {
    /// Whether bytes were dropped.
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.dropped() > 0
    }

    /// How many bytes were counted but not retained.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.total.saturating_sub(self.bytes.len() as u64)
    }

    /// The retained bytes as text (invalid UTF-8 replaced).
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

/// A finished (or killed) command.
#[derive(Debug)]
pub struct Output {
    /// Exit status; `None` only when waiting for the child failed.
    pub status: Option<ExitStatus>,
    /// Whether the command was killed for exceeding its timeout.
    pub timed_out: bool,
    /// Standard output.
    pub stdout: Stream,
    /// Standard error.
    pub stderr: Stream,
}

impl Output {
    /// Whether the command ran to completion and exited zero.
    #[must_use]
    pub fn success(&self) -> bool {
        !self.timed_out && self.status.is_some_and(|status| status.success())
    }
}

/// Runs `executable` with `args` under `limits`. Standard input is closed.
///
/// # Errors
///
/// Returns the spawn error when the command cannot be started.
pub fn run(executable: &Path, args: &[&str], limits: Limits) -> std::io::Result<Output> {
    let mut child = Command::new(executable)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .map(|pipe| Reader::spawn(pipe, limits.retain_bytes));
    let stderr = child
        .stderr
        .take()
        .map(|pipe| Reader::spawn(pipe, limits.retain_bytes));

    let deadline = Instant::now() + limits.timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() >= deadline => {
                timed_out = true;
                let _ = child.kill();
                break child.wait().ok();
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(_) => {
                let _ = child.kill();
                break child.wait().ok();
            }
        }
    };

    // A grandchild that inherited the pipes can keep them open after the
    // child exits or is killed. Wait at most the grace for the streams to
    // close, then take whatever was read so far.
    let grace_end = Instant::now() + READER_GRACE;
    Ok(Output {
        status,
        timed_out,
        stdout: stdout
            .map(|reader| reader.finish(grace_end))
            .unwrap_or_default(),
        stderr: stderr
            .map(|reader| reader.finish(grace_end))
            .unwrap_or_default(),
    })
}

/// A thread reading one pipe into a shared, bounded buffer.
struct Reader {
    stream: Arc<Mutex<Stream>>,
    done: mpsc::Receiver<()>,
}

impl Reader {
    fn spawn(mut source: impl Read + Send + 'static, retain: usize) -> Self {
        let stream = Arc::new(Mutex::new(Stream::default()));
        let (done_tx, done) = mpsc::channel();
        let shared = Arc::clone(&stream);
        thread::spawn(move || {
            let mut buffer = [0_u8; 16 * 1024];
            loop {
                match source.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        let mut stream = shared.lock().unwrap_or_else(PoisonError::into_inner);
                        stream.total += read as u64;
                        #[expect(
                            clippy::naive_bytecount,
                            reason = "a newline count does not justify a dependency"
                        )]
                        let newlines = buffer[..read].iter().filter(|byte| **byte == b'\n').count();
                        stream.lines += newlines as u64;
                        let room = retain.saturating_sub(stream.bytes.len());
                        stream.bytes.extend_from_slice(&buffer[..read.min(room)]);
                    }
                }
            }
            // Returning drops `done_tx`, which disconnects the channel and
            // wakes `finish` exactly as a message would.
            drop(done_tx);
        });
        Self { stream, done }
    }

    fn finish(self, until: Instant) -> Stream {
        let _ = self
            .done
            .recv_timeout(until.saturating_duration_since(Instant::now()));
        let stream = self.stream.lock().unwrap_or_else(PoisonError::into_inner);
        stream.clone()
    }
}

/// Runs `executable` with root privileges, never prompting: directly when
/// this process is already the elevated audit child, otherwise through
/// `sudo -n`, which only uses a credential the dashboard already cached.
///
/// # Errors
///
/// Returns an error when no prompt-free elevation is available or the
/// command cannot be started.
pub fn run_elevated(executable: &Path, args: &[&str], limits: Limits) -> std::io::Result<Output> {
    if crate::elevation::is_privileged() {
        return run(executable, args, limits);
    }
    let executable = executable.to_string_lossy();
    let elevated = crate::elevation::non_interactive_args(&executable, args)
        .ok_or_else(|| std::io::Error::other("needs the elevated audit"))?;
    run(Path::new(crate::elevation::program()), &elevated, limits)
}

/// Removes terminal control sequences and control characters, keeping
/// newlines and tabs. Carriage returns become line breaks only as part of
/// `\r\n`; progress-bar redraws (lone `\r`) are dropped.
#[must_use]
pub fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => skip_escape(&mut chars),
            // inxi-style IRC colour codes: ^C followed by up to two digits,
            // optionally ",NN" for the background.
            '\u{3}' => {
                skip_digits(&mut chars, 2);
                if chars.peek() == Some(&',') {
                    let mut look = chars.clone();
                    look.next();
                    if look.peek().is_some_and(char::is_ascii_digit) {
                        chars.next();
                        skip_digits(&mut chars, 2);
                    }
                }
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                    out.push('\n');
                }
            }
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn skip_digits(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, most: usize) {
    for _ in 0..most {
        if chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        } else {
            break;
        }
    }
}

fn skip_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.peek() {
        // CSI: parameters and intermediates, then one final byte @..~.
        Some('[') => {
            chars.next();
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        }
        // OSC: until BEL or ST (ESC \).
        Some(']') => {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '\u{7}' {
                    break;
                }
                if c == '\u{1b}' {
                    if chars.peek() == Some(&'\\') {
                        chars.next();
                    }
                    break;
                }
            }
        }
        // Two-character escapes (ESC c, ESC =, ...).
        Some(_) => {
            chars.next();
        }
        None => {}
    }
}

/// Cleans `text` and caps it at `max_bytes` and `max_lines`. A cut is made
/// on a line boundary where possible (never inside a UTF-8 character), and a
/// note saying how much was omitted is appended. `dropped_bytes` is how much
/// the capture already discarded before the text got here. Code fences in
/// the text are defused so a section can never break the report's Markdown.
#[must_use]
pub fn bound_text(text: &str, dropped_bytes: u64, max_bytes: usize, max_lines: usize) -> String {
    let clean = sanitize(text).replace("```", "'''");
    let total_lines = clean.lines().count();
    let mut end = clean.len();
    if let Some((index, _)) = clean.match_indices('\n').nth(max_lines.saturating_sub(1)) {
        end = end.min(index);
    }
    if end > max_bytes {
        let mut cut = max_bytes;
        while !clean.is_char_boundary(cut) {
            cut -= 1;
        }
        end = clean[..cut].rfind('\n').unwrap_or(cut);
    }
    let kept = &clean[..end];
    let omitted_bytes = (clean.len() - end) as u64 + dropped_bytes;
    if omitted_bytes == 0 {
        return clean;
    }
    let kept_lines = kept.lines().count();
    let mut out = kept.trim_end().to_owned();
    let _ = write!(out,
        "\n\n[omniscient: output truncated / showing {kept_lines} of {}{} lines, {} bytes omitted]\n",
        total_lines,
        if dropped_bytes > 0 { "+" } else { "" },
        omitted_bytes
    );
    out
}

#[cfg(test)]
mod tests {
    use super::{bound_text, run, sanitize, Limits};
    use std::fmt::Write as _;
    use std::path::Path;
    use std::time::{Duration, Instant};

    #[test]
    fn sanitize_strips_ansi_irc_and_control_characters() {
        let raw = "\u{1b}[1;31mred\u{1b}[0m \u{3}12System:\u{3} ok\u{3}04,01x\r\nnext\rover\u{7}\u{1b}]0;title\u{7}!";
        assert_eq!(sanitize(raw), "red System: okx\nnextover!");
        assert_eq!(sanitize("tab\tkept\nline"), "tab\tkept\nline");
        assert_eq!(sanitize("\u{1b}]8;;http://x\u{1b}\\link"), "link");
        assert_eq!(sanitize("trailing escape \u{1b}"), "trailing escape ");
        assert_eq!(sanitize("ünïcødé ✓"), "ünïcødé ✓");
    }

    #[test]
    fn short_text_is_returned_clean_and_whole() {
        assert_eq!(bound_text("a\nb\n", 0, 100, 10), "a\nb\n");
    }

    #[test]
    fn line_cap_keeps_exactly_the_first_lines() {
        let text = (1..=10).fold(String::new(), |mut text, n| {
            let _ = writeln!(text, "line{n}");
            text
        });
        let out = bound_text(&text, 0, 10_000, 3);
        assert!(
            out.starts_with(
                "line1\nline2\nline3\n\n[omniscient: output truncated / showing 3 of 10 lines"
            ),
            "{out}"
        );
        assert!(!out.contains("line4"));
    }

    #[test]
    fn byte_cap_cuts_on_a_line_boundary_and_never_splits_utf8() {
        let text = "ααααα\n".repeat(1_000);
        for cap in [1, 2, 3, 11, 12, 13, 100, 999] {
            let out = bound_text(&text, 0, cap, usize::MAX);
            let body = out.split("\n\n[omniscient:").next().unwrap_or_default();
            assert!(body.len() <= cap, "cap {cap}: {} bytes", body.len());
            assert!(out.contains("output truncated"), "cap {cap}");
        }
    }

    #[test]
    fn already_dropped_bytes_are_reported_even_when_the_rest_fits() {
        let out = bound_text("kept\n", 4096, 1_000, 1_000);
        assert!(
            out.contains("showing 1 of 1+ lines, 4096 bytes omitted"),
            "{out}"
        );
    }

    #[test]
    fn code_fences_cannot_escape_a_section() {
        assert_eq!(bound_text("```\nx\n```", 0, 100, 100), "'''\nx\n'''");
    }

    #[test]
    fn output_is_bounded_in_memory_and_counted() {
        let limits = Limits {
            timeout: Duration::from_secs(30),
            retain_bytes: 1024,
        };
        let out = run(
            Path::new("/usr/bin/head"),
            &["-c", "5000000", "/dev/zero"],
            limits,
        )
        .expect("head runs");
        assert!(out.success());
        assert_eq!(out.stdout.bytes.len(), 1024);
        assert_eq!(out.stdout.total, 5_000_000);
        assert_eq!(out.stdout.lines, 0);
        assert!(out.stdout.truncated());
    }

    #[test]
    fn lines_are_counted_even_when_not_retained() {
        let limits = Limits {
            timeout: Duration::from_secs(30),
            retain_bytes: 16,
        };
        let out = run(Path::new("/usr/bin/seq"), &["1", "100000"], limits).expect("seq runs");
        assert_eq!(out.stdout.lines, 100_000);
        assert_eq!(out.stdout.bytes.len(), 16);
    }

    #[test]
    fn a_hung_command_is_killed_at_its_timeout() {
        let limits = Limits {
            timeout: Duration::from_millis(300),
            retain_bytes: 1024,
        };
        let started = Instant::now();
        let out = run(Path::new("/usr/bin/sleep"), &["30"], limits).expect("sleep runs");
        assert!(out.timed_out);
        assert!(!out.success());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_grandchild_holding_the_pipe_cannot_hang_capture() {
        // The shell exits at once but leaves a background sleep holding stdout.
        let limits = Limits {
            timeout: Duration::from_secs(10),
            retain_bytes: 1024,
        };
        let started = Instant::now();
        let out = run(
            Path::new("/usr/bin/sh"),
            &["-c", "echo hi; sleep 4 & exit 0"],
            limits,
        )
        .expect("sh runs");
        assert!(out.success());
        assert_eq!(out.stdout.text(), "hi\n");
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_quick_command_returns_without_waiting_for_the_reader_grace() {
        let limits = Limits {
            timeout: Duration::from_secs(10),
            retain_bytes: 1024,
        };
        let started = Instant::now();
        let out = run(Path::new("/usr/bin/echo"), &["hi"], limits).expect("echo runs");
        assert_eq!(out.stdout.text(), "hi\n");
        assert!(
            started.elapsed() < Duration::from_millis(1500),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn the_child_reads_from_dev_null() {
        let limits = Limits {
            timeout: Duration::from_secs(10),
            retain_bytes: 1024,
        };
        let out = run(Path::new("/usr/bin/readlink"), &["/proc/self/fd/0"], limits)
            .expect("readlink runs");
        assert_eq!(out.stdout.text(), "/dev/null\n");
    }

    #[test]
    fn stdin_is_closed_so_readers_do_not_wait_forever() {
        let limits = Limits {
            timeout: Duration::from_secs(10),
            retain_bytes: 1024,
        };
        let out = run(Path::new("/usr/bin/cat"), &[], limits).expect("cat runs");
        assert!(out.success());
        assert!(out.stdout.bytes.is_empty());
    }
}
