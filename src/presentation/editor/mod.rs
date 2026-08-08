//! `paredit editor` — a small, terminal-native editing session.

//! The terminal adapter deliberately contains no buffer mutation logic. It
//! translates keypresses into the `paredit-feature-editor` application API,
//! while rendering the returned aggregate state.

mod raw_mode;
mod render;

use std::io::{IsTerminal, Write as _};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;
use paredit_feature_editor::{EditorAction, EditorSession, FileDocumentStore};

use raw_mode::Key;

#[derive(Debug, Args)]
pub(crate) struct EditorArgs {
    /// File to open, creating it on the first save when it does not exist.
    file: PathBuf,
}

pub(crate) fn editor(args: EditorArgs) -> ExitCode {
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("Error: paredit editor: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &EditorArgs) -> Result<(), String> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err("needs an interactive terminal on both stdin and stdout".to_owned());
    }

    let store = FileDocumentStore;
    let mut session = EditorSession::open(&args.file, &store).map_err(|error| error.to_string())?;
    let _raw_mode = raw_mode::enter().map_err(|error| error.to_string())?;
    let _screen = ScreenGuard;
    let mut stdout = std::io::stdout();

    loop {
        let frame = render::frame(
            &session,
            paredit_core_cli::terminal::width(),
            terminal_height(),
        );
        write!(stdout, "\x1b[?25l\x1b[2J\x1b[H{frame}").map_err(|error| error.to_string())?;
        stdout.flush().map_err(|error| error.to_string())?;

        let Some(key) = raw_mode::read_key().map_err(|error| error.to_string())? else {
            break;
        };
        if let Some(action) = action_for(key) {
            session
                .apply(action, &store)
                .map_err(|error| error.to_string())?;
        } else if matches!(key, Key::Ctrl('q') | Key::Ctrl('c') | Key::Escape) {
            break;
        }
    }

    Ok(())
}

const fn action_for(key: Key) -> Option<EditorAction> {
    match key {
        Key::Char(character) => Some(EditorAction::Insert(character)),
        Key::Enter => Some(EditorAction::Newline),
        Key::Backspace => Some(EditorAction::Backspace),
        Key::Delete => Some(EditorAction::Delete),
        Key::Left => Some(EditorAction::Left),
        Key::Right => Some(EditorAction::Right),
        Key::Up => Some(EditorAction::Up),
        Key::Down => Some(EditorAction::Down),
        Key::Home => Some(EditorAction::Home),
        Key::End => Some(EditorAction::End),
        Key::Ctrl('s') => Some(EditorAction::Save),
        Key::Ctrl('z') => Some(EditorAction::Undo),
        Key::Ctrl('y') => Some(EditorAction::Redo),
        Key::Ctrl(_) | Key::Escape | Key::Other => None,
    }
}

fn terminal_height() -> usize {
    std::env::var("LINES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|height| *height > 0)
        .unwrap_or(24)
}

struct ScreenGuard;

impl Drop for ScreenGuard {
    fn drop(&mut self) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "\x1b[0m\x1b[?25h\r\n");
        let _ = stdout.flush();
    }
}
