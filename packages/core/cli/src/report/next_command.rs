//! What to run next, and why.
//!
//! A report answers one question and usually raises another. `refactor status`
//! already ends with the next safe action, and it is the most-used thing it
//! does; this is that idea generalised so any report can carry it.
//!
//! The rule that keeps it useful rather than noisy: a suggestion is only
//! emitted when the report's *own contents* justify it. "You could also run
//! lint" on every report is an advertisement. "Nine findings — here is the
//! command that shows the first one in context" is an answer.

use serde_json::{Value as Json, json};

/// One thing worth running after this report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextCommand {
    /// A command line that runs exactly as written.
    pub command: String,
    /// One line saying what it would tell you that this report did not.
    pub why: String,
}

impl NextCommand {
    #[must_use]
    pub fn new(command: impl Into<String>, why: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            why: why.into(),
        }
    }

    /// Escaped for a terminal: `command` embeds paths this tool did not choose.
    #[must_use]
    pub fn to_json(&self) -> Json {
        json!({
            "command": crate::shared::terminal_safe(&self.command).to_string(),
            "why": crate::shared::terminal_safe(&self.why).to_string(),
        })
    }
}

/// Renders a list for a report's JSON, or `None` when there is nothing to say.
///
/// `None` rather than an empty array so a report with no suggestion does not
/// carry a field that reads as "we looked and there is nothing", which is a
/// different claim from "this report does not suggest follow-ups".
#[must_use]
pub fn to_json(commands: &[NextCommand]) -> Option<Json> {
    (!commands.is_empty()).then(|| Json::Array(commands.iter().map(NextCommand::to_json).collect()))
}

/// Quotes a path for a suggested command line when it needs it.
///
/// The suggestion is meant to be runnable, and a path with a space in it is
/// not runnable unquoted.
#[must_use]
pub fn shell_path(path: &str) -> String {
    if path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._-/~+=@:".contains(c))
    {
        return path.to_owned();
    }
    format!("'{}'", path.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_to_suggest_is_absent_rather_than_empty() {
        assert!(to_json(&[]).is_none());
    }

    #[test]
    fn a_suggestion_carries_a_command_and_a_reason() {
        let json = to_json(&[NextCommand::new(
            "paredit inspect outline --file a.lisp",
            "lists the paths this document has",
        )])
        .expect("some");
        assert_eq!(json[0]["command"], "paredit inspect outline --file a.lisp");
        assert_eq!(json[0]["why"], "lists the paths this document has");
    }

    /// The command line embeds a path, and a path is not something this tool
    /// chose. A terminal escape reaching stdout through a suggestion would be
    /// the same hole as one reaching it through a finding.
    #[test]
    fn control_characters_in_a_suggestion_are_escaped() {
        let json = to_json(&[NextCommand::new(
            "paredit inspect outline --file a\u{1b}[31m.lisp",
            "why",
        )])
        .expect("some");
        let command = json[0]["command"].as_str().expect("command");
        assert!(!command.contains('\u{1b}'), "{command}");
        assert!(command.contains("\\u{1b}"), "{command}");
    }

    #[test]
    fn an_ordinary_path_is_not_quoted() {
        assert_eq!(shell_path("src/core.lisp"), "src/core.lisp");
        assert_eq!(shell_path("./a-b_c.lisp"), "./a-b_c.lisp");
    }

    /// A suggestion is meant to be run. A path with a space in it that is not
    /// quoted would be run as two arguments.
    #[test]
    fn a_path_needing_quoting_gets_it() {
        assert_eq!(shell_path("my files/a.lisp"), "'my files/a.lisp'");
        assert_eq!(shell_path("it's.lisp"), r"'it'\''s.lisp'");
    }
}
