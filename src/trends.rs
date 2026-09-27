//! Trends for the HUD's TRENDS tab (`omniscient --trends`).
//!
//! Series come from the bounded history files in `<report root>/history/`
//! (see [`crate::history`]): the health score recorded after each audit,
//! CPU temperature and alert counts recorded by each watch, battery capacity
//! and omarchy-shell memory recorded by Deep Signals. Read-only; at most
//! [`MAX_POINTS`] points per series from the last [`WINDOW_DAYS`] days.

use crate::history::{self, Sample};
use serde::Serialize;

pub const WINDOW_DAYS: i64 = 30;
pub const MAX_POINTS: usize = 500;

/// One series for the HUD.
#[derive(Debug, PartialEq, Serialize)]
pub struct Series {
    pub id: String,
    pub label: String,
    pub unit: String,
    /// Whether higher values are better (colours the change).
    pub higher_is_better: bool,
    /// `(epoch seconds, value)`, oldest first.
    pub points: Vec<(i64, f64)>,
}

#[derive(Debug, Serialize)]
pub struct Output {
    pub version: u32,
    pub series: Vec<Series>,
}

/// Records the health score of a finished audit.
pub fn record_health(score: i32) {
    let _ = history::record(
        &history::path("health"),
        &Sample {
            epoch: chrono::Local::now().timestamp(),
            key: "score".to_owned(),
            value: f64::from(score),
        },
        600,
    );
}

/// Records what a watch saw: alert count and CPU package temperature.
pub fn record_watch(alerts: usize, cpu_celsius: Option<f64>) {
    let now = chrono::Local::now().timestamp();
    #[expect(clippy::cast_precision_loss, reason = "alert counts are tiny")]
    let alerts = alerts as f64;
    let _ = history::record(
        &history::path("watch"),
        &Sample {
            epoch: now,
            key: "alerts".to_owned(),
            value: alerts,
        },
        1800,
    );
    if let Some(celsius) = cpu_celsius {
        let _ = history::record(
            &history::path("watch"),
            &Sample {
                epoch: now,
                key: "cpu".to_owned(),
                value: celsius,
            },
            1800,
        );
    }
}

/// Points of `key` within the window, oldest first, at most [`MAX_POINTS`].
#[must_use]
pub fn points(samples: &[Sample], key: &str, now: i64) -> Vec<(i64, f64)> {
    let mut points = samples
        .iter()
        .filter(|s| s.key == key && now - s.epoch <= WINDOW_DAYS * 86_400)
        .map(|s| (s.epoch, s.value))
        .collect::<Vec<_>>();
    points.sort_by_key(|(epoch, _)| *epoch);
    if points.len() > MAX_POINTS {
        points.drain(..points.len() - MAX_POINTS);
    }
    points
}

/// The desktop shell's memory in MiB across restarts: samples are keyed by
/// PID, so the series follows whichever shell was sampled at each moment.
#[must_use]
pub fn shell_points(samples: &[Sample], now: i64) -> Vec<(i64, f64)> {
    let mut points = samples
        .iter()
        .filter(|s| now - s.epoch <= WINDOW_DAYS * 86_400)
        .map(|s| (s.epoch, (s.value / 1024.0).round()))
        .collect::<Vec<_>>();
    points.sort_by_key(|(epoch, _)| *epoch);
    if points.len() > MAX_POINTS {
        points.drain(..points.len() - MAX_POINTS);
    }
    points
}

/// Builds every series from the history directory.
#[must_use]
pub fn collect(now: i64) -> Vec<Series> {
    let health = history::read(&history::path("health"));
    let watch = history::read(&history::path("watch"));
    let battery = history::read(&history::path("battery"));
    let shell = history::read(&history::path("shell-memory"));
    let series =
        |id: &str, label: &str, unit: &str, higher_is_better: bool, points: Vec<(i64, f64)>| {
            Series {
                id: id.to_owned(),
                label: label.to_owned(),
                unit: unit.to_owned(),
                higher_is_better,
                points,
            }
        };
    let mut all = vec![
        series(
            "health",
            "HEALTH SCORE / PER AUDIT",
            "/100",
            true,
            points(&health, "score", now),
        ),
        series(
            "alerts",
            "URGENT + WARNING FINDINGS / HOURLY WATCH",
            "",
            false,
            points(&watch, "alerts", now),
        ),
        series(
            "cpu",
            "CPU TEMPERATURE / HOURLY WATCH",
            "°C",
            false,
            points(&watch, "cpu", now),
        ),
        series(
            "shell",
            "OMARCHY-SHELL MEMORY",
            " MiB",
            false,
            shell_points(&shell, now),
        ),
    ];
    let mut batteries = battery.iter().map(|s| s.key.clone()).collect::<Vec<_>>();
    batteries.sort();
    batteries.dedup();
    for name in batteries {
        all.push(series(
            &format!("battery:{name}"),
            &format!("{name} CAPACITY VS DESIGN"),
            "%",
            true,
            points(&battery, &name, now),
        ));
    }
    all
}

/// `omniscient --trends`.
///
/// # Errors
///
/// Returns an error when the output cannot be encoded or written.
pub fn run() -> anyhow::Result<()> {
    let output = Output {
        version: 1,
        series: collect(chrono::Local::now().timestamp()),
    };
    crate::emit(&serde_json::to_string(&output)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{points, shell_points, MAX_POINTS, WINDOW_DAYS};
    use crate::history::Sample;

    fn sample(epoch: i64, key: &str, value: f64) -> Sample {
        Sample {
            epoch,
            key: key.into(),
            value,
        }
    }

    #[test]
    fn points_are_windowed_sorted_and_capped() {
        let now = 100 * 86_400;
        let samples = vec![
            sample(now - 10, "score", 90.0),
            sample(now - (WINDOW_DAYS + 1) * 86_400, "score", 50.0),
            sample(now - 20, "score", 80.0),
            sample(now - 5, "other", 1.0),
        ];
        assert_eq!(
            points(&samples, "score", now),
            vec![(now - 20, 80.0), (now - 10, 90.0)]
        );
        let many = (0..(i64::try_from(MAX_POINTS).expect("small") + 40))
            .map(|n| sample(now - 1000 + n, "k", 1.0))
            .collect::<Vec<_>>();
        let capped = points(&many, "k", now);
        assert_eq!(capped.len(), MAX_POINTS);
        assert_eq!(
            capped.last().map(|p| p.0),
            Some(now - 1000 + i64::try_from(MAX_POINTS).expect("small") + 39),
            "the newest points are kept"
        );
    }

    #[test]
    fn shell_memory_follows_restarts_in_mib() {
        let samples = vec![sample(10, "111", 450_000.0), sample(20, "222", 380_000.0)];
        assert_eq!(shell_points(&samples, 30), vec![(10, 439.0), (20, 371.0)]);
    }
}
