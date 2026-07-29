//! `paredit tui` — an interactive terminal browser for one file's tree.
//!
//! Scoped to browsing and selecting, not editing in place: this tool's other
//! ~130 commands already cover the transform side (`edit`, `refactor`), each
//! with its own safety checks (reparse guards, hash-anchored writes) that a
//! from-scratch in-loop editor would have to either duplicate or bypass.
//! What every one of those commands still needs from a person, though, is a
//! `--path`, and building one by hand means counting parentheses. This
//! prints the path last selected, to stdout, once the session ends —
//! `paredit tui --file f.lisp` composes with the rest of the CLI the same
//! way `git rev-parse` composes with the rest of `git`.
//!
//! Bypasses `dispatch` in `run()`, the same as `lsp`/`mcp`/`serve`: those
//! are protocol servers with their own exit status, and this is a
//! stdin/stdout-driven interactive loop with the same property — routing it
//! through the `Result`-returning dispatch path would report an ordinary `q`
//! quit as this process failing.

mod navigate;
mod raw_mode;
mod render;

use std::io::{IsTerminal, Write as _};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args;

use paredit_core_cli::shared::read_input_and_dialect;
use paredit_core_syntax::sexpr::{ExpressionPath, SyntaxTree};

use raw_mode::Key;

#[derive(Debug, Args)]
pub(crate) struct TuiArgs {
    /// File to browse.
    file: PathBuf,
}

pub(crate) fn tui(args: TuiArgs) -> ExitCode {
    match run(&args) {
        Ok(path) => {
            println!("{path}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("Error: paredit tui: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &TuiArgs) -> Result<ExpressionPath, String> {
    // Both, not just stdout: a redirected stdin has no keys to read, and a
    // redirected stdout is a script that did not ask for a screen to clear.
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err("needs an interactive terminal on both stdin and stdout".to_owned());
    }

    let (input, dialect) =
        read_input_and_dialect(Some(args.file.clone()), None).map_err(|error| error.to_string())?;
    let tree =
        SyntaxTree::parse_with_dialect(&input.text, dialect).map_err(|error| error.to_string())?;

    let _raw_mode: raw_mode::RawMode = raw_mode::enter().map_err(|error| error.to_string())?;
    let root = ExpressionPath::from_indexes(Vec::new());
    // Starts on the first top-level form rather than the virtual root
    // itself: the root has no siblings of its own, so `j`/`k` would do
    // nothing until a person first pressed `l` — and "browse this file's
    // top-level forms" is the whole point of opening it. `h` still reaches
    // the root's own whole-document view from here.
    let mut path = navigate::first_child(&tree, &root).unwrap_or(root);
    let mut stdout = std::io::stdout();

    loop {
        let frame = render::frame(&tree, &path, paredit_core_cli::terminal::width());
        // `\x1b[2J\x1b[H` clears the screen and homes the cursor — the same
        // full-redraw-every-frame approach `less` itself falls back to
        // without a real curses backend, which is more than adequate for a
        // frame that redraws only on a keypress rather than a timer.
        write!(stdout, "\x1b[2J\x1b[H{frame}").map_err(|error| error.to_string())?;
        stdout.flush().map_err(|error| error.to_string())?;

        let Some(key) = raw_mode::read_key().map_err(|error| error.to_string())? else {
            break;
        };
        match key {
            Key::Quit => break,
            Key::Next => path = navigate::next_sibling(&tree, &path).unwrap_or(path),
            Key::Previous => path = navigate::previous_sibling(&tree, &path).unwrap_or(path),
            Key::In => path = navigate::first_child(&tree, &path).unwrap_or(path),
            Key::Out => path = navigate::parent(&path).unwrap_or(path),
            Key::Other => {}
        }
    }

    Ok(path)
}
