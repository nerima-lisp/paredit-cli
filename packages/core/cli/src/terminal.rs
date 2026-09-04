//! Terminal geometry: how wide a line can be before it should wrap.
//!
//! Consulted only by renderers that already know they are drawing for a
//! human at a live terminal — nothing here changes what a piped or
//! redirected run produces, since [`width`] is never consulted unless the
//! caller has separately checked `is_terminal()`.

#![allow(unsafe_code)]

/// A conservative width for a run with no terminal, no `COLUMNS`, and no
/// ioctl to ask — the width terminals defaulted to before wider ones were
/// common, and still a safe assumption today.
const FALLBACK_WIDTH: usize = 80;

/// The terminal's column count for stdout, or a fallback in this order:
/// an ioctl reading (the terminal's actual width), then `COLUMNS` (what a
/// shell exports, and what a wrapper without a real tty can still set), then
/// [`FALLBACK_WIDTH`].
#[must_use]
pub fn width() -> usize {
    ioctl_width().or_else(columns_env).unwrap_or(FALLBACK_WIDTH)
}

fn columns_env() -> Option<usize> {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .filter(|&value| value > 0)
}

#[cfg(unix)]
fn ioctl_width() -> Option<usize> {
    use std::os::fd::AsRawFd;

    // SAFETY: `winsize` is a plain C struct of integers, so zero-initializing
    // it is always valid; `ioctl` is called with a valid fd (stdout, open for
    // the lifetime of the process) and a pointer to that same live struct,
    // matching `TIOCGWINSZ`'s documented contract.
    unsafe {
        let mut size: libc::winsize = std::mem::zeroed();
        let stdout = std::io::stdout();
        if libc::ioctl(stdout.as_raw_fd(), libc::TIOCGWINSZ, &raw mut size) != 0 {
            return None;
        }
        (size.ws_col > 0).then_some(size.ws_col as usize)
    }
}

#[cfg(not(unix))]
fn ioctl_width() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::columns_env;

    #[test]
    fn columns_env_reads_a_positive_integer() {
        // SAFETY: test-only mutation of this process's environment, and this
        // is the only test in the crate that touches `COLUMNS`.
        unsafe {
            std::env::set_var("COLUMNS", "120");
        }
        assert_eq!(columns_env(), Some(120));

        unsafe {
            std::env::set_var("COLUMNS", "0");
        }
        assert_eq!(columns_env(), None, "zero is not a usable width");

        unsafe {
            std::env::set_var("COLUMNS", "not-a-number");
        }
        assert_eq!(columns_env(), None);

        unsafe {
            std::env::remove_var("COLUMNS");
        }
        assert_eq!(columns_env(), None);
    }
}
