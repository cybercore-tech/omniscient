//! Runs audit modules on a small worker pool.
//!
//! Most module time is spent waiting on commands (Bluetooth timeouts,
//! `vmstat 1 2`, journal queries), so a few workers cut a full audit's wall
//! time substantially. Events are delivered to the caller's thread in the
//! order they happen, so snapshot publishing stays single-threaded.

use crate::modules::AuditModule;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex, PoisonError};
use std::thread;

/// Default number of modules run at once.
pub const DEFAULT_WORKERS: usize = 4;
/// Upper bound for `OMNISCIENT_WORKERS`.
pub const MAX_WORKERS: usize = 8;

/// Progress of one module.
#[derive(Debug)]
pub enum Event {
    /// The module at this index started.
    Started(usize),
    /// The module finished: its report path, or why it failed.
    Finished(usize, Result<PathBuf, String>),
}

/// Worker count: `OMNISCIENT_WORKERS` (1..=8) if set, otherwise the smaller
/// of [`DEFAULT_WORKERS`] and the available parallelism.
#[must_use]
pub fn workers() -> usize {
    let requested = std::env::var("OMNISCIENT_WORKERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok());
    let available = thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    requested.map_or_else(
        || DEFAULT_WORKERS.min(available),
        |n| n.clamp(1, MAX_WORKERS),
    )
}

/// Runs the `selected` modules, each into `<root>/<slug>-<timestamp>/`, on
/// `workers` threads, calling `on_event` on this thread for every start and
/// finish. Returns when every selected module has finished.
pub fn run(
    modules: &[Box<dyn AuditModule>],
    selected: &[usize],
    root: &Path,
    timestamp: &str,
    workers: usize,
    mut on_event: impl FnMut(Event),
) {
    let queue = Mutex::new(
        selected
            .iter()
            .copied()
            .filter(|index| *index < modules.len())
            .collect::<VecDeque<_>>(),
    );
    let total = queue.lock().unwrap_or_else(PoisonError::into_inner).len();
    let (sender, receiver) = mpsc::channel();
    thread::scope(|scope| {
        for _ in 0..workers.clamp(1, MAX_WORKERS).min(total.max(1)) {
            let sender = sender.clone();
            let queue = &queue;
            scope.spawn(move || loop {
                let next = queue
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .pop_front();
                let Some(index) = next else {
                    break;
                };
                let module = &modules[index];
                let _ = sender.send(Event::Started(index));
                let dir = root.join(format!("{}-{timestamp}", module.slug()));
                let result = std::fs::create_dir_all(&dir)
                    .map_err(|error| error.to_string())
                    .and_then(|()| module.run(&dir).map_err(|error| format!("{error:#}")))
                    .map(|()| dir.join(module.report_filename()));
                let _ = sender.send(Event::Finished(index, result));
            });
        }
        drop(sender);
        for event in receiver {
            on_event(event);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{run, Event};
    use crate::modules::AuditModule;
    use anyhow::Result;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    static RUNNING: AtomicUsize = AtomicUsize::new(0);
    static PEAK: AtomicUsize = AtomicUsize::new(0);

    struct Sleepy(&'static str, bool);
    impl AuditModule for Sleepy {
        fn name(&self) -> &'static str {
            self.0
        }
        fn slug(&self) -> &'static str {
            self.0
        }
        fn menu_label(&self) -> &'static str {
            self.0
        }
        fn tools(&self) -> &'static [&'static str] {
            &[]
        }
        fn run(&self, dir: &Path) -> Result<()> {
            let now = RUNNING.fetch_add(1, Ordering::SeqCst) + 1;
            PEAK.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(200));
            RUNNING.fetch_sub(1, Ordering::SeqCst);
            anyhow::ensure!(self.1, "module {} failed", self.0);
            std::fs::write(dir.join(format!("{}.md", self.0)), "ok")?;
            Ok(())
        }
    }

    #[test]
    fn modules_run_concurrently_bounded_and_report_every_outcome() {
        let modules: Vec<Box<dyn AuditModule>> = vec![
            Box::new(Sleepy("a", true)),
            Box::new(Sleepy("b", true)),
            Box::new(Sleepy("c", false)),
            Box::new(Sleepy("d", true)),
            Box::new(Sleepy("e", true)),
            Box::new(Sleepy("skipped", true)),
        ];
        let root = std::env::temp_dir().join(format!("omniscient-runner-{}", std::process::id()));
        let started = Instant::now();
        let mut starts = 0;
        let mut outcomes = Vec::new();
        run(
            &modules,
            &[0, 1, 2, 3, 4, 99],
            &root,
            "t",
            2,
            |event| match event {
                Event::Started(_) => starts += 1,
                Event::Finished(index, result) => outcomes.push((index, result.is_ok())),
            },
        );
        let elapsed = started.elapsed();
        outcomes.sort_unstable();
        assert_eq!(starts, 5, "out-of-range indices are ignored");
        assert_eq!(
            outcomes,
            vec![(0, true), (1, true), (2, false), (3, true), (4, true)]
        );
        assert_eq!(
            PEAK.load(Ordering::SeqCst),
            2,
            "never more than the worker count at once"
        );
        assert!(
            elapsed < Duration::from_millis(900),
            "ran in parallel: {elapsed:?}"
        );
        assert!(root.join("a-t/a.md").exists());
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
