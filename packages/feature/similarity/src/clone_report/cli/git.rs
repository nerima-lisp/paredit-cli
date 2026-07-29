//! `git blame` behind the genealogy port.
//!
//! One `blame` per clone member, restricted to that member's line range with
//! `-L`, run in the file's own directory so a report spanning several
//! repositories answers for each of them — the same shape `inspect blame` uses,
//! for the same reason.
//!
//! Where `blame` wants the *newest* line (who to ask about this code), this
//! wants the *oldest*: a form is at least as old as its oldest surviving line,
//! and dating a clone by its last reformatting would put every member in the
//! same week and destroy the ordering the report exists to produce.

use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use crate::clone_report::domain::{CloneHistoryPort, MemberHistory};

#[derive(Debug, Default)]
pub struct GitCloneHistory {
    /// One `blame` per file range, memoised: a class with several members in
    /// one file otherwise spawns a process per member.
    cache: Mutex<HashMap<(PathBuf, usize, usize), MemberHistory>>,
}

impl CloneHistoryPort for GitCloneHistory {
    fn oldest_commit(
        &self,
        path: &FsPath,
        lines: std::ops::RangeInclusive<usize>,
    ) -> MemberHistory {
        let key = (path.to_path_buf(), *lines.start(), *lines.end());
        if let Some(cached) = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&key)
        {
            return cached.clone();
        }

        let history = blame_range(path, *lines.start(), *lines.end());
        self.cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key, history.clone());
        history
    }
}

fn blame_range(path: &FsPath, start: usize, end: usize) -> MemberHistory {
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let Some(directory) = absolute.parent() else {
        return MemberHistory::unavailable("file has no parent directory");
    };

    let output = Command::new("git")
        .current_dir(directory)
        .args(["blame", "--line-porcelain", "-L"])
        .arg(format!("{start},{end}"))
        .arg("--")
        .arg(&absolute)
        .output();

    match output {
        Err(error) => MemberHistory::unavailable(format!("git blame failed to start: {error}")),
        Ok(output) if !output.status.success() => MemberHistory::unavailable(
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("git blame failed")
                .trim()
                .to_owned(),
        ),
        Ok(output) => oldest_from_porcelain(&String::from_utf8_lossy(&output.stdout)),
    }
}

/// The oldest committed line in a `--line-porcelain` stream.
///
/// Uncommitted lines carry the all-zero object name and no history worth
/// reporting, so they are skipped rather than dated to now — a working-tree
/// edit is not evidence about when the clone appeared.
fn oldest_from_porcelain(stdout: &str) -> MemberHistory {
    let mut best: Option<(i64, String, Option<String>)> = None;
    let mut commit: Option<String> = None;
    let mut author: Option<String> = None;
    let mut author_time: Option<i64> = None;

    fn flush(
        commit: &mut Option<String>,
        author: &mut Option<String>,
        author_time: &mut Option<i64>,
        best: &mut Option<(i64, String, Option<String>)>,
    ) {
        if let (Some(sha), Some(time)) = (commit.clone(), *author_time) {
            if !sha.bytes().all(|byte| byte == b'0')
                && best.as_ref().is_none_or(|(oldest, _, _)| time < *oldest)
            {
                *best = Some((time, sha, author.clone()));
            }
        }
        *commit = None;
        *author = None;
        *author_time = None;
    }

    for line in stdout.lines() {
        if let Some((head, _)) = line.split_once(' ') {
            if head.len() == 40 && head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                flush(&mut commit, &mut author, &mut author_time, &mut best);
                commit = Some(head.to_owned());
                continue;
            }
        }
        if let Some(value) = line.strip_prefix("author ") {
            author = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("author-time ") {
            author_time = value.trim().parse().ok();
        }
    }
    flush(&mut commit, &mut author, &mut author_time, &mut best);

    match best {
        None => MemberHistory::unavailable("no committed lines in range"),
        Some((timestamp, commit, author)) => MemberHistory {
            commit: Some(commit.chars().take(12).collect()),
            author,
            date: Some(iso_date_utc(timestamp)),
            timestamp: Some(timestamp),
            unavailable: None,
        },
    }
}

/// Formats an epoch second as an ISO-8601 UTC timestamp.
///
/// Written out rather than pulled in: a date crate would be a dependency for
/// one format string, and ISO-8601 in UTC is the one representation where
/// lexical order and chronological order agree, which is the only property the
/// report needs from it.
fn iso_date_utc(timestamp: i64) -> String {
    let days = timestamp.div_euclid(86_400);
    let seconds = timestamp.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

/// Howard Hinnant's `civil_from_days`, shifted to a March-based year so the
/// leap day lands at the end and the month arithmetic stays branch-free.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let march_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * march_month + 2) / 5 + 1) as u32;
    let month = if march_month < 10 {
        march_month + 3
    } else {
        march_month - 9
    } as u32;
    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_parsing_picks_the_oldest_committed_line() {
        let stdout = "\
1111111111111111111111111111111111111111 1 1 2
author Newer Author
author-time 1700000000
author-tz +0000
\t(defun alpha ()
2222222222222222222222222222222222222222 2 2
author Older Author
author-time 1600000000
author-tz +0000
\t  (helper))
";
        let history = oldest_from_porcelain(stdout);

        assert_eq!(history.timestamp, Some(1_600_000_000));
        assert_eq!(history.author.as_deref(), Some("Older Author"));
        assert_eq!(history.commit.as_deref(), Some("222222222222"));
        assert_eq!(history.date.as_deref(), Some("2020-09-13T12:26:40Z"));
        assert!(history.is_available());
    }

    #[test]
    fn uncommitted_lines_are_not_history() {
        let stdout = "\
0000000000000000000000000000000000000000 1 1 1
author Not Committed Yet
author-time 1800000000
author-tz +0000
\t(defun alpha ())
";
        let history = oldest_from_porcelain(stdout);

        assert!(!history.is_available());
        assert_eq!(
            history.unavailable.as_deref(),
            Some("no committed lines in range")
        );
    }

    #[test]
    fn iso_dates_cover_leap_years_and_the_epoch() {
        assert_eq!(iso_date_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_date_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso_date_utc(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(iso_date_utc(1_735_689_599), "2024-12-31T23:59:59Z");
    }

    #[test]
    fn iso_dates_sort_chronologically_as_strings() {
        let mut timestamps = [1_700_000_000i64, 0, 951_782_400, 1_709_164_800];
        let mut lexical = timestamps.map(iso_date_utc).to_vec();
        lexical.sort();
        timestamps.sort_unstable();

        assert_eq!(lexical, timestamps.map(iso_date_utc).to_vec());
    }
}
