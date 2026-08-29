//! Delegating long stdout output to `$PAGER`, opt-in via `--paginate`.
//!
//! Every command in this tool writes its report straight to stdout with
//! `println!`/`print!`, scattered across some 130-odd call sites — there is
//! no buffer or writer threaded through them a pager could wrap at the call
//! site. The classic Unix answer, and the one used here, is to leave every
//! call site alone and instead replace what file descriptor 1 *points at*:
//! spawn the pager with its stdin as a pipe, `dup2` that pipe onto fd 1, and
//! every subsequent write in this process — through however many layers of
//! `println!` — lands in the pager without knowing it exists.
//!
//! Unix-only, like the rest of this crate's raw-fd and xattr code
//! (`crate::shared::io`'s macOS ACL preservation is the other example):
//! `dup2` and `/bin/sh` are POSIX, and there is no Windows pager convention
//! to fall back to. `--paginate` is simply inert there.
//!
//! Never automatic. Unlike `--color auto`, which is purely cosmetic and safe
//! to guess at, engaging a pager changes the interaction itself — the
//! process will not exit until a human dismisses it — and this tool's other
//! flags (`--diff`, `--progress`, `--dry-run`) are all explicit for the same
//! reason: a behavior change big enough to notice is one the caller asks for.

#![cfg(unix)]
#![allow(unsafe_code)]

use std::io::Write as _;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::{Child, Command, Stdio};

/// Starts the pager if `--paginate` was given, stdout is a terminal, and the
/// environment does not say otherwise. Returns `None` in every other case,
/// in which case the caller's output goes straight to its original stdout.
#[must_use]
pub fn maybe_start() -> Option<Pager> {
    if !crate::runtime::current().paginate {
        return None;
    }
    // Piping into a pager that has no controlling terminal of its own would
    // just hang; and if stdout is already redirected, the caller — a script,
    // a test harness — wants exactly what was written, not a pager's framing.
    if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        return None;
    }
    let command = pager_command()?;
    // Resolved before the redirect below, so a color decision made once
    // stdout is a pipe still reflects the terminal a human is actually
    // looking at through the pager.
    crate::color::prime_stdout_terminal_cache();
    start(&command)
}

/// The command to run as a pager, or `None` when the environment has
/// explicitly opted out.
///
/// `PAGER` set to the empty string is the conventional way (shared with
/// `git`, `man`) to say "no, do not page", distinct from `PAGER` being unset
/// at all, which falls back to `less`.
fn pager_command() -> Option<String> {
    match std::env::var("PAGER") {
        Ok(value) if value.is_empty() => None,
        Ok(value) => Some(value),
        Err(_) => Some("less".to_owned()),
    }
}

/// A running pager and the means to hand its terminal back.
#[derive(Debug)]
pub struct Pager {
    child: Child,
    saved_stdout: OwnedFd,
}

/// Spawns `command` through a shell (so `$PAGER` may itself carry arguments,
/// as `"less -R"` does) and redirects fd 1 to its stdin.
fn start(command: &str) -> Option<Pager> {
    // `less -F` exits immediately, leaving no trace, when the content fits
    // one screen — which is what makes this safe to leave on unconditionally
    // rather than first measuring whether a report is "long". `-R` passes
    // this tool's own ANSI color through; `-X` keeps the report in scrollback
    // instead of a terminal that restores its prior screen on exit. Only set
    // when the caller has not already chosen their own `less` behavior.
    if std::env::var_os("LESS").is_none() {
        // SAFETY: mutates only this process's environment before any other
        // thread has been spawned by `run()`, and only to hand the child a
        // default it is free to ignore.
        unsafe {
            std::env::set_var("LESS", "FRX");
        }
    }

    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .spawn()
        .ok()?;
    let pipe = child.stdin.take()?;

    // SAFETY: `dup`/`dup2` operate on file descriptors this process owns —
    // the real stdout (fd 1, open because `maybe_start` already required it
    // to be a terminal) and the pager's pipe (`pipe`, just opened above).
    // Neither call closes a descriptor anyone else still expects open: `dup`
    // only allocates a new one, and `dup2`'s implicit close lands on our own
    // fd 1, which nothing after this point may use except through `Pager`.
    let saved_stdout = unsafe {
        let duplicated = libc::dup(libc::STDOUT_FILENO);
        if duplicated < 0 {
            return None;
        }
        OwnedFd::from_raw_fd(duplicated)
    };
    let redirected = unsafe { libc::dup2(pipe.as_raw_fd(), libc::STDOUT_FILENO) };
    if redirected < 0 {
        return None;
    }
    // `pipe`'s own descriptor is no longer needed: fd 1 now holds an
    // independent reference to the same pipe, courtesy of `dup2`. Dropping
    // it here, rather than keeping it alive until `finish`, means exactly one
    // reference to the pipe's write end remains — the one `finish` closes —
    // so the pager sees end-of-input the moment that happens.
    drop(pipe);

    Some(Pager {
        child,
        saved_stdout,
    })
}

impl Pager {
    /// Restores this process's own stdout and waits for the pager to exit —
    /// which, for an interactive `less`, means waiting for a human to quit
    /// it. Called once, after the command being paged has finished writing.
    pub fn finish(mut self) {
        let _ = std::io::stdout().flush();
        // SAFETY: `saved_stdout` was duplicated from a live fd 1 in `start`
        // and has not been touched since; restoring it is exactly the
        // inverse of the `dup2` that redirected fd 1 there. This also drops
        // fd 1's only remaining reference to the pager's stdin pipe, which is
        // what lets the pager see end-of-input and exit.
        unsafe {
            libc::dup2(self.saved_stdout.as_raw_fd(), libc::STDOUT_FILENO);
        }
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::pager_command;

    // One test, not three: `PAGER` is process-wide state, and `cargo test`
    // runs test functions concurrently on separate threads by default, so
    // three tests each setting and clearing it race each other. Sequencing
    // every case inside one test function is the whole fix.
    #[test]
    fn pager_command_resolution() {
        // SAFETY: test-only mutation of this process's environment, and this
        // is the only test in the crate that touches `PAGER`.
        unsafe {
            std::env::remove_var("PAGER");
        }
        assert_eq!(pager_command().as_deref(), Some("less"));

        // SAFETY: see above.
        unsafe {
            std::env::set_var("PAGER", "");
        }
        assert_eq!(pager_command(), None);

        // SAFETY: see above.
        unsafe {
            std::env::set_var("PAGER", "less -R");
        }
        assert_eq!(pager_command().as_deref(), Some("less -R"));

        // SAFETY: see above.
        unsafe {
            std::env::remove_var("PAGER");
        }
    }
}
