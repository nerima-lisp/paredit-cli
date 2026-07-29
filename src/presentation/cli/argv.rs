//! Reading the raw argument vector against the `clap` tree.
//!
//! Two things need to know what a command line means *before* `clap` has
//! parsed it, or *after* it has failed to: the configuration bridge, which
//! decides which flags a command would accept, and the error renderer, which
//! decides whether to answer in JSON. Both walk the same subcommand tree, so
//! the walk lives here rather than twice.

use clap::builder::Command as ClapCommand;

/// Walks the subcommand tree with `argv` to find the command being run.
///
/// Returns the leaf and its space-separated path, or `None` when no subcommand
/// was named. Stops at the first token that is not a known subcommand name,
/// which is exactly where the flags and positionals start.
///
/// `argv` includes the program name, as `std::env::args` produces it.
#[must_use]
pub fn resolve_leaf<'a>(
    argv: &[String],
    root: &'a ClapCommand,
) -> Option<(&'a ClapCommand, String)> {
    let mut current = root;
    let mut path = Vec::new();

    for token in argv.iter().skip(1) {
        if token == "--" || token.starts_with('-') {
            break;
        }
        let Some(next) = current
            .get_subcommands()
            .find(|candidate| candidate.get_name() == token)
        else {
            break;
        };
        current = next;
        path.push(token.clone());
    }

    (!path.is_empty()).then(|| (current, path.join(" ")))
}

/// The long flags already on the command line, `--flag` and `--flag=value`
/// alike. Anything after `--` is a positional and does not count.
#[must_use]
pub fn long_flags_present(argv: &[String]) -> Vec<String> {
    let mut present = Vec::new();
    for token in argv {
        if token == "--" {
            break;
        }
        let Some(rest) = token.strip_prefix("--") else {
            continue;
        };
        let name = rest.split('=').next().unwrap_or(rest);
        if !name.is_empty() {
            present.push(name.to_owned());
        }
    }
    present
}

/// The value given to a long flag, in either spelling.
#[must_use]
pub fn long_flag_value(argv: &[String], flag: &str) -> Option<String> {
    let equals = format!("--{flag}=");
    let bare = format!("--{flag}");

    let mut tokens = argv.iter();
    while let Some(token) = tokens.next() {
        if token == "--" {
            return None;
        }
        if let Some(value) = token.strip_prefix(&equals) {
            return Some(value.to_owned());
        }
        if *token == bare {
            return tokens.next().cloned();
        }
    }
    None
}

#[must_use]
pub fn declares_flag(command: &ClapCommand, flag: &str) -> bool {
    command
        .get_arguments()
        .any(|argument| argument.get_long() == Some(flag))
}

/// The `--output` value this invocation is running with: what was passed, or
/// failing that the command's own default.
///
/// Consulted so a failure is reported in the format the caller was already
/// going to parse. Answering a `--output json` invocation with a prose line on
/// stderr makes the one consumer who most needs the detail parse English.
#[must_use]
pub fn effective_output_format(argv: &[String], root: &ClapCommand) -> Option<String> {
    if let Some(value) = long_flag_value(argv, "output") {
        return Some(value);
    }
    let (leaf, _) = resolve_leaf(argv, root)?;
    leaf.get_arguments()
        .find(|argument| argument.get_long() == Some("output"))
        .and_then(|argument| {
            argument
                .get_default_values()
                .first()
                .map(|value| value.to_string_lossy().into_owned())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    use crate::presentation::cli::Cli;

    fn argv(tokens: &[&str]) -> Vec<String> {
        std::iter::once("paredit")
            .chain(tokens.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn a_nested_command_resolves_to_its_full_path() {
        let root = Cli::command();
        let (_, path) =
            resolve_leaf(&argv(&["inspect", "lint", "a.lisp"]), &root).expect("resolves");
        assert_eq!(path, "inspect lint");
    }

    #[test]
    fn a_namespace_alone_resolves_to_the_namespace() {
        let root = Cli::command();
        let (_, path) = resolve_leaf(&argv(&["inspect"]), &root).expect("resolves");
        assert_eq!(path, "inspect");
    }

    #[test]
    fn nothing_resolves_without_a_subcommand() {
        let root = Cli::command();
        assert!(resolve_leaf(&argv(&[]), &root).is_none());
        assert!(resolve_leaf(&argv(&["--help"]), &root).is_none());
        assert!(resolve_leaf(&argv(&["nonsense"]), &root).is_none());
    }

    #[test]
    fn a_flag_value_is_read_in_either_spelling() {
        assert_eq!(
            long_flag_value(&argv(&["edit", "format", "--indent", "4"]), "indent"),
            Some("4".to_owned())
        );
        assert_eq!(
            long_flag_value(&argv(&["edit", "format", "--indent=4"]), "indent"),
            Some("4".to_owned())
        );
        assert_eq!(long_flag_value(&argv(&["edit", "format"]), "indent"), None);
    }

    /// After `--` everything is a value, including something spelled like a
    /// flag. Reading past it would attribute a file name to a flag.
    #[test]
    fn a_flag_after_a_double_dash_is_not_a_flag() {
        assert_eq!(
            long_flag_value(&argv(&["edit", "format", "--", "--indent", "4"]), "indent"),
            None
        );
        assert!(long_flags_present(&argv(&["inspect", "lint", "--", "--output"])).is_empty());
    }

    #[test]
    fn an_explicit_output_flag_is_what_the_command_is_running_with() {
        let root = Cli::command();
        assert_eq!(
            effective_output_format(
                &argv(&["inspect", "lint", "a.lisp", "--output", "text"]),
                &root
            ),
            Some("text".to_owned())
        );
    }

    /// The interesting half: a command that defaults to JSON is running in
    /// JSON even though nothing on the command line says so.
    #[test]
    fn a_commands_own_default_is_used_when_no_flag_was_passed() {
        let root = Cli::command();
        assert_eq!(
            effective_output_format(&argv(&["inspect", "lint", "a.lisp"]), &root),
            Some("json".to_owned())
        );
        assert_eq!(
            effective_output_format(&argv(&["inspect", "check", "--file", "a.lisp"]), &root),
            Some("text".to_owned())
        );
    }

    #[test]
    fn a_command_with_no_output_flag_has_no_format() {
        let root = Cli::command();
        assert_eq!(
            effective_output_format(&argv(&["edit", "format", "--file", "a.lisp"]), &root),
            None
        );
    }
}
