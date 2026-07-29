//! Raw terminal mode and single-key reads, for [`super`]'s interactive loop.
//!
//! Unix only, like `paredit`'s pager (`paredit_core_cli::pager`): there is no
//! POSIX termios on Windows, and no established convention to fall back to
//! for a from-scratch TUI. `enter` returns an error there instead of leaving
//! the command missing, so `--help` and `inspect capabilities` still describe
//! it uniformly on every platform; only running it differs.

#![allow(unsafe_code)]

use std::io::Read as _;

/// One decoded keypress `paredit tui`'s loop acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Key {
    Next,
    Previous,
    In,
    Out,
    Quit,
    /// Anything this loop has no binding for. Not an error: an unmapped key
    /// should redraw the same screen, not end the session.
    Other,
}

/// Reads one keypress from stdin, decoding the `ESC [ letter` shape a
/// terminal sends for an arrow key.
///
/// Returns `None` at end-of-input, which the caller treats as "quit" without
/// treating it as a failure — a pty closing out from under this process is
/// not this process's error.
///
/// `q`/`Q` is the one quit key this reads without ambiguity. A bare `Esc`
/// (pressed alone, with no `[ letter` following) is deliberately *not* also
/// bound to quit: telling it apart from the first byte of an arrow sequence
/// needs either a read timeout or non-blocking I/O, and this reads with
/// neither. The visible cost is that a bare `Esc` waits, harmlessly, for one
/// more keypress before redrawing — never a hang past that, since the very
/// next byte always completes the read one way or the other.
pub(super) fn read_key() -> std::io::Result<Option<Key>> {
    let mut byte = [0u8; 1];
    if std::io::stdin().read(&mut byte)? == 0 {
        return Ok(None);
    }
    Ok(Some(match byte[0] {
        b'q' | b'Q' => Key::Quit,
        b'j' | b'J' => Key::Next,
        b'k' | b'K' => Key::Previous,
        b'l' | b'L' | b'\r' | b'\n' => Key::In,
        b'h' | b'H' => Key::Out,
        0x1b => match read_arrow_direction()? {
            Some(ArrowDirection::Down) => Key::Next,
            Some(ArrowDirection::Up) => Key::Previous,
            Some(ArrowDirection::Right) => Key::In,
            Some(ArrowDirection::Left) => Key::Out,
            None => Key::Other,
        },
        _ => Key::Other,
    }))
}

enum ArrowDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Reads the `[ letter` that follows an already-consumed `ESC`, returning the
/// arrow it names or `None` for any other (or incomplete) sequence.
fn read_arrow_direction() -> std::io::Result<Option<ArrowDirection>> {
    let mut rest = [0u8; 2];
    let stdin = std::io::stdin();
    let mut handle = stdin.lock();
    if handle.read(&mut rest[..1])? == 0 || rest[0] != b'[' {
        return Ok(None);
    }
    if handle.read(&mut rest[1..])? == 0 {
        return Ok(None);
    }
    Ok(match rest[1] {
        b'A' => Some(ArrowDirection::Up),
        b'B' => Some(ArrowDirection::Down),
        b'C' => Some(ArrowDirection::Right),
        b'D' => Some(ArrowDirection::Left),
        _ => None,
    })
}

#[cfg(unix)]
mod unix {
    use std::io::Error;
    use std::os::fd::AsRawFd;

    /// Restores the terminal's original mode when dropped, however the
    /// session ends — a normal quit, an error return, or a panic unwinding
    /// through it. A raw-mode terminal left behind after a crash is exactly
    /// the "why is my shell not echoing" report this exists to prevent.
    #[derive(Debug)]
    pub(in super::super) struct RawMode {
        original: libc::termios,
    }

    pub(in super::super) fn enter() -> std::io::Result<RawMode> {
        let fd = std::io::stdin().as_raw_fd();
        // SAFETY: `fd` is stdin's own descriptor, open for the process
        // lifetime; `termios` is a plain struct of integers, so
        // zero-initializing it before `tcgetattr` overwrites it is valid.
        let original = unsafe {
            let mut original: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(fd, &raw mut original) != 0 {
                return Err(Error::last_os_error());
            }
            original
        };

        let mut raw = original;
        // SAFETY: `raw` is a valid, initialized `termios` on the stack;
        // `cfmakeraw` only mutates its flag fields, and `tcsetattr` is given
        // the same valid `fd` as above.
        unsafe {
            libc::cfmakeraw(&raw mut raw);
            if libc::tcsetattr(fd, libc::TCSANOW, &raw const raw) != 0 {
                return Err(Error::last_os_error());
            }
        }

        Ok(RawMode { original })
    }

    impl Drop for RawMode {
        fn drop(&mut self) {
            let fd = std::io::stdin().as_raw_fd();
            // SAFETY: `self.original` was filled in by a prior successful
            // `tcgetattr` on this same fd in `enter`, and restoring it is the
            // exact inverse of `cfmakeraw`'s mutation there. Best-effort: a
            // `Drop` has no `Result` to report a failure through, and a
            // shell's own reset on the next prompt is the fallback here.
            unsafe {
                libc::tcsetattr(fd, libc::TCSANOW, &raw const self.original);
            }
        }
    }
}

#[cfg(unix)]
pub(super) use unix::{RawMode, enter};

#[cfg(not(unix))]
pub(super) struct RawMode;

#[cfg(not(unix))]
pub(super) fn enter() -> std::io::Result<RawMode> {
    Err(std::io::Error::other(
        "paredit tui needs a POSIX terminal, which this platform does not provide",
    ))
}
