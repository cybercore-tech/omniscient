// Also set in Cargo.toml [lints]; repeated here so cargo-geiger sees it.
#![forbid(unsafe_code)]

pub mod capture;
pub mod changes;
pub mod elevation;
pub mod fix;
pub mod headless;
pub mod health;
pub mod helper;
pub mod history;
pub mod hud;
pub mod journal;
pub mod modules;
pub mod palette;
pub mod pathcheck;
pub mod paths;
#[cfg(kani)]
mod proofs;
pub mod report;
pub mod runner;
#[cfg(test)]
mod scratch;
pub mod sensors;
pub mod signals;
pub mod snapshot;
pub mod suggestions;
pub mod trends;
pub mod tui;
pub mod watch;

/// Writes one line to stdout for a reader such as the HUD. A reader that
/// went away (closed pipe: the panel closed, the tab changed) is not an
/// error; `println!` would panic there, which `panic = "abort"` turns into a
/// crash with a core dump.
///
/// # Errors
///
/// Returns any other write error.
pub fn emit(text: &str) -> std::io::Result<()> {
    emit_to(&mut std::io::stdout().lock(), text)
}

/// [`emit`] to any writer.
///
/// # Errors
///
/// Returns any write error other than a broken pipe.
pub fn emit_to(out: &mut impl std::io::Write, text: &str) -> std::io::Result<()> {
    match out
        .write_all(text.as_bytes())
        .and_then(|()| out.write_all(b"\n"))
        .and_then(|()| out.flush())
    {
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod emit_tests {
    struct Closed;
    impl std::io::Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct Full;
    impl std::io::Write for Full {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::StorageFull.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_reader_that_went_away_is_not_an_error() {
        assert!(super::emit_to(&mut Closed, "{}").is_ok());
        assert!(
            super::emit_to(&mut Full, "{}").is_err(),
            "other errors still surface"
        );
        let mut buffer = Vec::new();
        super::emit_to(&mut buffer, "x").expect("write");
        assert_eq!(buffer, b"x\n");
    }
}
