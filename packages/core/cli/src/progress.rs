//! Progress for operations long enough that silence is indistinguishable from
//! a hang.
//!
//! Two renderings share one set of events, chosen by whether stderr is a
//! terminal: JSON Lines for a consumer that is a program (a redirected file,
//! a wrapping process), one line of plain text per event for a human
//! watching the run live. Both go to stderr rather than stdout because the
//! report is the stdout contract, and a progress line landing in the middle
//! of a JSON document would break every consumer of it.
//!
//! The human rendering prints one complete line per event rather than
//! overwriting a single line in place. A single updating line would need a
//! phase-end hook to clear it before a command's own report starts printing
//! to stdout, and none exists — 130-odd commands each `println!` their
//! report directly with no shared "analysis is done" signal. A scrolling log
//! costs nothing to get right by comparison: every line is complete the
//! moment it is written, so it composes safely with whatever the command
//! prints afterward, and it never has to guess the terminal width.
//!
//! The events are emitted from the two places every multi-file command
//! already goes through — workspace discovery, and the per-file read — so a
//! command gains progress without knowing progress exists.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;

/// Files read so far in this process.
///
/// A sequence number rather than a percentage: the total is not known at the
/// point a file is read, and a percentage computed from a guess is worse than
/// an honest count.
static FILES_READ: AtomicUsize = AtomicUsize::new(0);

/// The most recent [`discovered`] count, or 0 when none has run yet (a
/// command given explicit file arguments never discovers a directory).
/// Read only by the human renderer, to show "N of M" instead of a bare N.
static LAST_DISCOVERED: AtomicUsize = AtomicUsize::new(0);

fn enabled() -> bool {
    crate::runtime::current().progress
}

/// Announces how many files an operation is about to work through.
pub fn discovered(count: usize, root: &std::path::Path) {
    if !enabled() {
        return;
    }
    LAST_DISCOVERED.store(count, Ordering::Relaxed);
    if human_rendering(std::io::stderr().is_terminal()) {
        eprintln!(
            "discovering: {count} file(s) under {}",
            crate::shared::terminal_safe(root.display())
        );
        return;
    }
    emit(&json!({
        "event": "discovered",
        "files": count,
        "root": crate::shared::terminal_safe(root.display()).to_string(),
    }));
}

/// Announces one file being read.
///
/// Called from the read path rather than from each command's loop, so a
/// command written before this existed reports progress anyway.
pub fn file_read(path: &std::path::Path) {
    if !enabled() {
        return;
    }
    let sequence = FILES_READ.fetch_add(1, Ordering::Relaxed) + 1;
    if human_rendering(std::io::stderr().is_terminal()) {
        let total = LAST_DISCOVERED.load(Ordering::Relaxed);
        let safe_path = crate::shared::terminal_safe(path.display());
        if total > 0 {
            eprintln!("[{sequence}/{total}] {safe_path}");
        } else {
            eprintln!("[{sequence}] {safe_path}");
        }
        return;
    }
    emit(&json!({
        "event": "file",
        "sequence": sequence,
        "path": crate::shared::terminal_safe(path.display()).to_string(),
    }));
}

/// Announces a phase of a multi-step operation.
pub fn phase(name: &str, detail: Option<&str>) {
    if !enabled() {
        return;
    }
    if human_rendering(std::io::stderr().is_terminal()) {
        match detail {
            Some(detail) => eprintln!(
                "== {} — {} ==",
                crate::shared::terminal_safe(name),
                crate::shared::terminal_safe(detail)
            ),
            None => eprintln!("== {} ==", crate::shared::terminal_safe(name)),
        }
        return;
    }
    emit(&json!({
        "event": "phase",
        "phase": crate::shared::terminal_safe(name).to_string(),
        "detail": detail.map(|text| crate::shared::terminal_safe(text).to_string()),
    }));
}

/// Whether progress should render as one plain-text line per event rather
/// than JSON Lines.
///
/// A pure function of "is the destination a terminal" so the branch is
/// testable without a real tty; every call site supplies its own
/// `is_terminal()` reading.
const fn human_rendering(is_tty: bool) -> bool {
    is_tty
}

fn emit(event: &serde_json::Value) {
    // One line, and never pretty-printed: a JSON Lines reader splits on
    // newlines, so a multi-line object is not one record.
    eprintln!("{event}");
}

/// The count so far, for tests and for a final summary line.
#[must_use]
pub fn files_read() -> usize {
    FILES_READ.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// With progress off — the default, and what every test in this workspace
    /// runs under — nothing is emitted and the counter does not move.
    #[test]
    fn nothing_is_emitted_or_counted_when_progress_is_off() {
        let before = files_read();
        file_read(Path::new("a.lisp"));
        discovered(3, Path::new("."));
        phase("planning", None);
        assert_eq!(files_read(), before);
    }

    /// Each event has to be one line, or a JSON Lines reader sees fragments.
    #[test]
    fn an_event_renders_as_exactly_one_line() {
        let event = json!({ "event": "file", "sequence": 1, "path": "a.lisp" });
        let rendered = event.to_string();
        assert!(!rendered.contains('\n'), "{rendered}");
        assert!(serde_json::from_str::<serde_json::Value>(&rendered).is_ok());
    }

    /// A path is not something this tool chose, and a progress line goes to a
    /// terminal.
    #[test]
    fn a_hostile_path_is_escaped_before_it_reaches_a_line() {
        let escaped =
            crate::shared::terminal_safe(Path::new("a\u{1b}[31m.lisp").display()).to_string();
        assert!(!escaped.contains('\u{1b}'));
        assert!(escaped.contains("\\u{1b}"));
    }

    #[test]
    fn human_rendering_follows_the_destination_tty() {
        assert!(human_rendering(true));
        assert!(!human_rendering(false));
    }
}
