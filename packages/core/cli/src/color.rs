//! Terminal color, gated by `--color`/`NO_COLOR`/`CLICOLOR_FORCE` and an
//! isatty check.
//!
//! Every text renderer in this tool already emits plain, script-friendly rows
//! (tab-separated fields, unified diffs); color is a purely cosmetic layer
//! painted on top at the last moment, never baked into a value a test or a
//! downstream `cut`/`awk` pipeline depends on. That is also why `Auto` checks
//! the destination stream's own tty-ness rather than a single global answer:
//! `stdout | less` and `2>err.log` are different pipelines, and a report
//! piped on one stream should not lose color on the other because of it.
//!
//! ANSI SGR codes are written by hand rather than through a crate: four
//! colors and a reset are a handful of constant strings, and every one of
//! them is a literal this tool emits itself — never untrusted source text or
//! a path, which stay behind [`crate::shared::terminal_safe`] regardless of
//! whether color is on.

use std::io::IsTerminal;
use std::sync::OnceLock;

use clap::ValueEnum;

/// The three states every color-capable CLI settles on: follow the terminal,
/// force it on, or force it off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ColorMode {
    /// Color if and only if the destination stream is a terminal, `NO_COLOR`
    /// is unset, and `CLICOLOR_FORCE` has not already decided the question.
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }

    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

/// Present (regardless of value) means "no color", per <https://no-color.org>.
/// Checked ahead of `CLICOLOR_FORCE` because that convention explicitly asks
/// implementers to let it win even when something else wants color.
fn no_color_requested() -> bool {
    std::env::var_os("NO_COLOR").is_some()
}

/// Non-`"0"` means "color even when not a terminal", the informal
/// `CLICOLOR_FORCE` convention several color-aware tools already honor.
fn force_requested() -> bool {
    std::env::var_os("CLICOLOR_FORCE").is_some_and(|value| value != "0")
}

fn resolve(mode: ColorMode, is_tty: bool) -> bool {
    match mode {
        ColorMode::Never => false,
        ColorMode::Always => true,
        ColorMode::Auto => {
            if no_color_requested() {
                false
            } else {
                is_tty || force_requested()
            }
        }
    }
}

/// Paints text for one destination stream, or passes it through unchanged.
///
/// A value rather than free functions so a renderer resolves the tty check
/// once per run and threads the answer through every line, instead of
/// re-querying `is_terminal()` per finding.
#[derive(Debug, Clone, Copy)]
pub struct Painter {
    enabled: bool,
}

/// Whether stdout was a terminal, cached from the first check.
///
/// [`crate::pager`] redirects fd 1 to a pipe partway through a run; once that
/// happens, a fresh `is_terminal()` call would (correctly, but unhelpfully)
/// say no, and every renderer after that point would think it was piped.
/// The pager primes this cache itself before redirecting, so a decision made
/// this way still reflects the terminal a human is watching through it.
static STDOUT_IS_TERMINAL: OnceLock<bool> = OnceLock::new();

fn stdout_is_terminal() -> bool {
    *STDOUT_IS_TERMINAL.get_or_init(|| std::io::stdout().is_terminal())
}

/// Forces [`stdout_is_terminal`]'s cache to resolve now, before something
/// else changes what fd 1 points at. A no-op once the cache already holds a
/// value, which is exactly the "first check wins" contract callers need.
pub fn prime_stdout_terminal_cache() {
    stdout_is_terminal();
}

impl Painter {
    #[must_use]
    pub fn stdout() -> Self {
        Self {
            enabled: resolve(crate::runtime::current().color, stdout_is_terminal()),
        }
    }

    #[must_use]
    pub fn stderr() -> Self {
        Self {
            enabled: resolve(
                crate::runtime::current().color,
                std::io::stderr().is_terminal(),
            ),
        }
    }

    /// Color always off, for callers that render to something that is
    /// provably not a terminal (a file, a captured buffer in a test).
    #[must_use]
    pub const fn disabled() -> Self {
        Self { enabled: false }
    }

    #[must_use]
    pub const fn is_enabled(self) -> bool {
        self.enabled
    }

    fn wrap(self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }

    #[must_use]
    pub fn red(self, text: &str) -> String {
        self.wrap("31", text)
    }

    #[must_use]
    pub fn yellow(self, text: &str) -> String {
        self.wrap("33", text)
    }

    #[must_use]
    pub fn green(self, text: &str) -> String {
        self.wrap("32", text)
    }

    #[must_use]
    pub fn cyan(self, text: &str) -> String {
        self.wrap("36", text)
    }

    #[must_use]
    pub fn bold(self, text: &str) -> String {
        self.wrap("1", text)
    }

    #[must_use]
    pub fn dim(self, text: &str) -> String {
        self.wrap("2", text)
    }
}

/// Colorizes a [`crate::diff::unified_diff`] result for a terminal: green
/// `+` lines, red `-` lines, cyan hunk headers, bold file headers.
///
/// Takes the finished diff text rather than folding into `unified_diff`
/// itself, so the byte-exact diff generator — and its budget accounting,
/// which counts every byte it writes — stays colorless and its extensive
/// exact-string tests stay unaffected. Coloring is purely a render-time
/// decoration applied after the diff is already correct.
#[must_use]
pub fn colorize_diff(painter: Painter, diff: &str) -> String {
    if !painter.is_enabled() || diff.is_empty() {
        return diff.to_owned();
    }

    let mut colored = String::with_capacity(diff.len());
    for line in diff.split_inclusive('\n') {
        if line.starts_with("+++") || line.starts_with("---") {
            colored.push_str(&painter.bold(line));
        } else if line.starts_with("@@") {
            colored.push_str(&painter.cyan(line));
        } else if line.starts_with('+') {
            colored.push_str(&painter.green(line));
        } else if line.starts_with('-') {
            colored.push_str(&painter.red(line));
        } else {
            colored.push_str(line);
        }
    }
    colored
}

#[cfg(test)]
mod tests {
    use super::{ColorMode, Painter, colorize_diff, resolve};

    #[test]
    fn never_is_off_even_on_a_terminal() {
        assert!(!resolve(ColorMode::Never, true));
    }

    #[test]
    fn always_is_on_even_off_a_terminal() {
        assert!(resolve(ColorMode::Always, false));
    }

    #[test]
    fn auto_follows_the_terminal() {
        assert!(resolve(ColorMode::Auto, true));
        assert!(!resolve(ColorMode::Auto, false));
    }

    #[test]
    fn labels_round_trip() {
        for mode in [ColorMode::Auto, ColorMode::Always, ColorMode::Never] {
            assert_eq!(ColorMode::from_label(mode.label()), Some(mode));
        }
        assert_eq!(ColorMode::from_label("rainbow"), None);
    }

    #[test]
    fn a_disabled_painter_passes_text_through_unchanged() {
        let painter = Painter::disabled();
        assert_eq!(painter.red("x"), "x");
        assert_eq!(painter.bold("x"), "x");
    }

    #[test]
    fn an_enabled_painter_wraps_in_sgr_codes_and_resets() {
        let painter = Painter { enabled: true };
        assert_eq!(painter.red("x"), "\x1b[31mx\x1b[0m");
        assert_eq!(painter.green("x"), "\x1b[32mx\x1b[0m");
    }

    #[test]
    fn colorize_diff_is_a_no_op_when_disabled() {
        let diff = "--- a\n+++ a\n@@ -1,1 +1,1 @@\n-old\n+new\n";
        assert_eq!(colorize_diff(Painter::disabled(), diff), diff);
    }

    #[test]
    fn colorize_diff_paints_each_line_kind_when_enabled() {
        let diff = "--- a\n+++ a\n@@ -1,1 +1,1 @@\n-old\n+new\n context\n";
        let painted = colorize_diff(Painter { enabled: true }, diff);

        assert!(painted.contains("\x1b[1m--- a\n\x1b[0m"));
        assert!(painted.contains("\x1b[36m@@ -1,1 +1,1 @@\n\x1b[0m"));
        assert!(painted.contains("\x1b[31m-old\n\x1b[0m"));
        assert!(painted.contains("\x1b[32m+new\n\x1b[0m"));
        assert!(painted.contains(" context\n"));
        assert!(!painted.ends_with("\x1b[0m\x1b[0m"));
    }

    #[test]
    fn colorize_diff_of_empty_input_stays_empty() {
        assert_eq!(colorize_diff(Painter { enabled: true }, ""), "");
    }
}
