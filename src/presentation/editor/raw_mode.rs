//! POSIX raw mode and key decoding for the editor delivery adapter.

#![allow(unsafe_code)]

use std::io::Read as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Key {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Ctrl(char),
    Escape,
    Other,
}

pub(super) fn read_key() -> std::io::Result<Option<Key>> {
    let mut byte = [0_u8; 1];
    if std::io::stdin().read(&mut byte)? == 0 {
        return Ok(None);
    }

    let key = match byte[0] {
        0x03 => Key::Ctrl('c'),
        0x08 | 0x7f => Key::Backspace,
        0x09 => Key::Char('\t'),
        b'\r' | b'\n' => Key::Enter,
        0x11 => Key::Ctrl('q'),
        0x13 => Key::Ctrl('s'),
        0x19 => Key::Ctrl('y'),
        0x1a => Key::Ctrl('z'),
        0x1b => read_escape_sequence()?,
        byte if byte.is_ascii_control() => Key::Other,
        byte if byte.is_ascii() => Key::Char(byte as char),
        first => read_utf8(first)?,
    };
    Ok(Some(key))
}

fn read_utf8(first: u8) -> std::io::Result<Key> {
    let width = match first {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return Ok(Key::Other),
    };
    let mut bytes = vec![0_u8; width];
    bytes[0] = first;
    std::io::stdin().read_exact(&mut bytes[1..])?;
    Ok(std::str::from_utf8(&bytes)
        .ok()
        .and_then(|text| text.chars().next())
        .map_or(Key::Other, Key::Char))
}

fn read_escape_sequence() -> std::io::Result<Key> {
    let Some(first) = read_byte()? else {
        return Ok(Key::Escape);
    };
    if first != b'[' {
        return Ok(Key::Escape);
    }

    let Some(second) = read_byte()? else {
        return Ok(Key::Escape);
    };
    Ok(match second {
        b'A' => Key::Up,
        b'B' => Key::Down,
        b'C' => Key::Right,
        b'D' => Key::Left,
        b'H' => Key::Home,
        b'F' => Key::End,
        b'1' => {
            let _ = read_byte()?;
            Key::Home
        }
        b'3' => {
            let _ = read_byte()?;
            Key::Delete
        }
        b'4' => {
            let _ = read_byte()?;
            Key::End
        }
        _ => Key::Other,
    })
}

fn read_byte() -> std::io::Result<Option<u8>> {
    let mut byte = [0_u8; 1];
    if std::io::stdin().read(&mut byte)? == 0 {
        Ok(None)
    } else {
        Ok(Some(byte[0]))
    }
}

#[cfg(unix)]
mod unix {
    use std::io::Error;
    use std::os::fd::AsRawFd;

    #[derive(Debug)]
    pub(in super::super) struct RawMode {
        original: libc::termios,
    }

    pub(in super::super) fn enter() -> std::io::Result<RawMode> {
        let fd = std::io::stdin().as_raw_fd();
        // SAFETY: `fd` is stdin's own descriptor, and `tcgetattr` initializes
        // the plain termios value before it is used.
        let original = unsafe {
            let mut original: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(fd, &raw mut original) != 0 {
                return Err(Error::last_os_error());
            }
            original
        };

        let mut raw = original;
        // SAFETY: `raw` was initialized by `tcgetattr` and is passed back to
        // the same terminal descriptor.
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
            // SAFETY: `original` came from `tcgetattr` for this descriptor.
            unsafe {
                libc::tcsetattr(fd, libc::TCSANOW, &raw const self.original);
            }
        }
    }
}

#[cfg(unix)]
pub(super) use unix::enter;

#[cfg(not(unix))]
#[derive(Debug)]
pub(super) struct RawMode;

#[cfg(not(unix))]
pub(super) fn enter() -> std::io::Result<RawMode> {
    Err(std::io::Error::other(
        "paredit editor needs a POSIX terminal, which this platform does not provide",
    ))
}
