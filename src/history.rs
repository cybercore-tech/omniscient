//! Small, bounded history files for signals that only mean something as a
//! trend (battery capacity, desktop-shell memory).
//!
//! Each history is a tab-separated text file under
//! `<report root>/history/`, one sample per line, oldest first. Files are
//! capped at [`MAX_SAMPLES`] lines and are never trusted: unparsable lines
//! are skipped, so a damaged file degrades to "no trend yet".

use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Most samples kept per history file.
pub const MAX_SAMPLES: usize = 500;

/// One recorded sample: seconds since the Unix epoch, a key (such as a PID
/// or a battery name), and a value.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub epoch: i64,
    pub key: String,
    pub value: f64,
}

/// Where the named history lives.
#[must_use]
pub fn path(name: &str) -> PathBuf {
    crate::paths::report_root()
        .join("history")
        .join(format!("{name}.tsv"))
}

/// Parses a history file's text, skipping anything malformed.
#[must_use]
pub fn parse(text: &str) -> Vec<Sample> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let epoch = fields.next()?.trim().parse::<i64>().ok()?;
            let key = fields.next()?.trim().to_owned();
            let value = fields.next()?.trim().parse::<f64>().ok()?;
            (fields.next().is_none() && value.is_finite()).then_some(Sample { epoch, key, value })
        })
        .collect()
}

/// Reads a history; a missing or unreadable file is an empty history.
#[must_use]
pub fn read(file: &Path) -> Vec<Sample> {
    fs::read_to_string(file)
        .map(|text| parse(&text))
        .unwrap_or_default()
}

/// Appends `sample`, replacing an earlier sample with the same key taken
/// less than `min_spacing` seconds ago (so repeated audits in one sitting
/// do not flood the trend), and keeps the newest [`MAX_SAMPLES`].
///
/// # Errors
///
/// Returns an error when the history cannot be written.
pub fn record(file: &Path, sample: &Sample, min_spacing: i64) -> Result<Vec<Sample>> {
    let mut samples = read(file);
    samples.retain(|old| !(old.key == sample.key && sample.epoch - old.epoch < min_spacing));
    samples.push(sample.clone());
    samples.sort_by_key(|entry| entry.epoch);
    if samples.len() > MAX_SAMPLES {
        samples.drain(..samples.len() - MAX_SAMPLES);
    }
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let text = samples.iter().fold(String::new(), |mut text, entry| {
        let _ = writeln!(text, "{}\t{}\t{}", entry.epoch, entry.key, entry.value);
        text
    });
    let temporary = file.with_extension("tsv.tmp");
    fs::write(&temporary, text).with_context(|| format!("writing {}", temporary.display()))?;
    fs::rename(&temporary, file).with_context(|| format!("replacing {}", file.display()))?;
    Ok(samples)
}

/// Change in value per day between the oldest and newest samples for `key`
/// within `window` seconds of the newest one. `None` until the samples span
/// at least `min_span` seconds, so a trend is never read from noise.
#[must_use]
pub fn rate_per_day(samples: &[Sample], key: &str, window: i64, min_span: i64) -> Option<f64> {
    let own = samples
        .iter()
        .filter(|sample| sample.key == key)
        .collect::<Vec<_>>();
    let newest = own.iter().max_by_key(|sample| sample.epoch)?;
    let oldest = own
        .iter()
        .filter(|sample| newest.epoch - sample.epoch <= window)
        .min_by_key(|sample| sample.epoch)?;
    let span = newest.epoch - oldest.epoch;
    if span < min_span {
        return None;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "spans are far below 2^52 seconds"
    )]
    let days = span as f64 / 86_400.0;
    Some((newest.value - oldest.value) / days)
}

#[cfg(test)]
mod tests {
    use super::{parse, rate_per_day, record, Sample, MAX_SAMPLES};

    fn sample(epoch: i64, key: &str, value: f64) -> Sample {
        Sample {
            epoch,
            key: key.to_owned(),
            value,
        }
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let text =
            "1\tBAT0\t74.5\nnot a line\n2\tBAT0\tNaN\n3\tBAT0\n4\tBAT0\t70\textra\n5\tBAT0\t73\n";
        assert_eq!(
            parse(text),
            vec![sample(1, "BAT0", 74.5), sample(5, "BAT0", 73.0)]
        );
    }

    #[test]
    fn record_replaces_close_samples_and_caps_the_file() {
        let dir = std::env::temp_dir().join(format!("omniscient-history-{}", std::process::id()));
        let file = dir.join("t.tsv");
        let _ = std::fs::remove_dir_all(&dir);
        record(&file, &sample(1_000, "a", 1.0), 3_600).expect("record");
        let kept = record(&file, &sample(1_500, "a", 2.0), 3_600).expect("record");
        assert_eq!(
            kept,
            vec![sample(1_500, "a", 2.0)],
            "same key within the spacing is replaced"
        );
        let kept = record(&file, &sample(1_600, "b", 3.0), 3_600).expect("record");
        assert_eq!(kept.len(), 2, "other keys are independent");
        for n in 0..(i64::try_from(MAX_SAMPLES).expect("small") + 50) {
            record(&file, &sample(10_000 + n * 10_000, "c", 1.0), 3_600).expect("record");
        }
        assert_eq!(super::read(&file).len(), MAX_SAMPLES);
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn rate_needs_enough_span_and_uses_the_window() {
        let day = 86_400;
        let samples = vec![
            sample(0, "BAT0", 90.0),
            sample(100 * day, "BAT0", 80.0),
            sample(130 * day, "BAT0", 77.0),
            sample(130 * day, "other", 1.0),
        ];
        let rate = rate_per_day(&samples, "BAT0", 40 * day, 7 * day).expect("rate");
        assert!((rate - (-0.1)).abs() < 1e-9, "{rate}");
        assert!(
            rate_per_day(&samples, "BAT0", 10 * day, 7 * day).is_none(),
            "span too short"
        );
        assert!(rate_per_day(&samples, "missing", 400 * day, 0).is_none());
    }
}
