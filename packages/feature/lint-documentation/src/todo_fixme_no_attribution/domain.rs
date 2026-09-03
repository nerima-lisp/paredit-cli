//! A `TODO`/`FIXME` marker with nothing that says who owns it or where it is
//! tracked.
//!
//! `;; TODO: handle the empty case` is a note to nobody. It has no owner, no
//! ticket, and no date, so nothing outside the file can schedule it, nothing
//! can tell whether it is a week old or six years old, and the only way to find
//! out is to run `git blame`. `;; TODO(ada): handle the empty case` and
//! `;; TODO: handle the empty case (#412)` both cost one more token and answer
//! the question.
//!
//! # Data source
//!
//! Comments are trivia beside the tree rather than nodes in it, so this rule
//! reads [`SyntaxTree::comments`] — the same source `inspect todo` and
//! `commented-out-code` read, for a different question.
//!
//! # What this is not
//!
//! Not a duplicate of `inspect todo`
//! (`paredit-feature-code-metrics`'s `todo_report`). That report *inventories*
//! markers and extracts a `TODO(name):` attribution when one is present; a bare
//! `; TODO: later` is listed with `author: None` and no complaint. This rule
//! is the enforcement half: it fires on exactly the markers that report leaves
//! unattributed, and it recognises six more attribution shapes than that
//! report's single parenthetical.
//!
//! # Limits, deliberately
//!
//! The failure mode that matters is firing on a `TODO` whose reference is in a
//! format nobody anticipated, so acceptance is deliberately generous — see
//! [`ATTRIBUTION_SHAPES`] for the list. Anything that looks like an owner, a
//! ticket, a URL, or a date silences the rule.
//!
//! - **The marker must open the comment.** `;; the TODO list is in NOTES.md` is
//!   prose about a marker, not a marker, which is the rule `inspect todo`
//!   already applies and is tested against.
//! - **Datum comments are not prose.** `#;(form)` and `#_form` comment out a
//!   *form*; their contents are never read as English.
//! - **Tagged `pedantic`.** Whether every marker needs an owner is a project's
//!   decision, and on a project that has decided otherwise this fires on all of
//!   them.

use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};

use crate::support::comment_prose;

/// The markers this rule recognises, in the same order and with the same
/// spelling as `inspect todo`'s, so the two cannot disagree about what a marker
/// is.
const MARKERS: [&str; 5] = ["FIXME", "XXX", "BUG", "HACK", "TODO"];

/// The attribution shapes this rule accepts, as documentation. The predicate
/// is [`has_attribution`]; this list is what it is asserted against.
///
/// | Shape | Example |
/// | --- | --- |
/// | a parenthetical owner | `TODO(ada): …` |
/// | a bracketed owner | `TODO[ada]: …` |
/// | an `@`-handle | `TODO: @ada should look at this` |
/// | a hash issue reference | `TODO: drop this (#412)` |
/// | a tracker key | `TODO: see PROJ-412` / `gh-412` |
/// | a URL | `TODO: https://example.com/issues/412` |
/// | an ISO or slashed date | `TODO: revisit after 2026-08-01` |
pub const ATTRIBUTION_SHAPES: usize = 7;

/// One unattributed task marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnattributedMarker {
    pub span: ByteSpan,
    /// The marker word, upper-cased.
    pub marker: &'static str,
    /// The note after the marker, whitespace collapsed.
    pub note: String,
}

impl UnattributedMarker {
    /// The sentence the rule reports.
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "this {} names no owner, ticket, or date, so nothing outside the file can schedule \
             it: write `{}(name):`, reference an issue, or add a date",
            self.marker, self.marker
        )
    }
}

/// Every unattributed marker in one file.
///
/// The whole of this rule's per-file cost. A file with no comments iterates an
/// empty list and allocates nothing; a file with comments pays one prefix test
/// of at most five bytes per comment before anything else happens.
#[must_use]
pub fn collect(tree: &SyntaxTree) -> Vec<UnattributedMarker> {
    tree.comments()
        .filter_map(|comment| {
            let prose = comment_prose(comment)?;
            let (marker, rest) = split_marker(prose)?;
            if has_attribution(rest) {
                return None;
            }
            Some(UnattributedMarker {
                span: comment.span(),
                marker,
                note: rest
                    .trim_start()
                    .trim_start_matches(':')
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            })
        })
        .collect()
}

/// Splits a comment's prose into its opening marker word and the rest.
///
/// The marker must be a whole word at the start: `TODOs` in prose is not a
/// task, and a sentence *about* the todo list is not one either. Matching is
/// case-insensitive because both `TODO` and `todo` are written in practice.
/// This mirrors `inspect todo`'s own `split_marker`, so the two agree about
/// what a marker is.
fn split_marker(prose: &str) -> Option<(&'static str, &str)> {
    for marker in MARKERS {
        let Some(head) = prose.get(..marker.len()) else {
            continue;
        };
        if !head.eq_ignore_ascii_case(marker) {
            continue;
        }
        let tail = &prose[marker.len()..];
        // A word boundary: end of comment, punctuation, or whitespace.
        if tail
            .chars()
            .next()
            .is_none_or(|character| !character.is_alphanumeric() && character != '-')
        {
            return Some((marker, tail));
        }
    }
    None
}

/// Whether `rest` — everything after the marker word — carries anything that
/// says who owns this or where it is tracked.
///
/// Generous on purpose. A missed shape is a false positive on a marker that is
/// doing its job, which is exactly how a rule like this gets switched off.
fn has_attribution(rest: &str) -> bool {
    let trimmed = rest.trim_start();
    // `TODO(ada):` and `TODO[ada]:` — an owner attached to the marker itself.
    // The delimiter must close, and must have something between it and the
    // marker: `TODO()` names nobody.
    if let Some(inner) = trimmed
        .strip_prefix('(')
        .and_then(|rest| rest.split(')').next())
    {
        if !inner.trim().is_empty() {
            return true;
        }
    }
    if let Some(inner) = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.split(']').next())
    {
        if !inner.trim().is_empty() {
            return true;
        }
    }
    // A URL is a reference to wherever this is tracked.
    if rest.contains("http://") || rest.contains("https://") {
        return true;
    }
    rest.split(|character: char| character.is_whitespace() || character == '(')
        .any(|token| {
            let token = token.trim_matches(|character: char| {
                !character.is_alphanumeric()
                    && character != '#'
                    && character != '-'
                    && character != '/'
                    && character != '@'
            });
            is_handle(token) || is_issue_reference(token) || is_date(token)
        })
}

/// `@ada` — a handle, as every code host and chat tool spells one.
fn is_handle(token: &str) -> bool {
    token
        .strip_prefix('@')
        .is_some_and(|name| !name.is_empty() && name.chars().all(is_name_character))
}

fn is_name_character(character: char) -> bool {
    character.is_alphanumeric() || character == '-' || character == '_' || character == '.'
}

/// `#412`, `PROJ-412`, `gh-412` — a reference into a tracker.
///
/// The two shapes: a `#` followed by digits, or a word followed by `-` and
/// digits. The latter deliberately accepts any alphabetic prefix rather than a
/// list of known trackers, because the trackers this rule has never heard of
/// are exactly the ones it must not fire on.
fn is_issue_reference(token: &str) -> bool {
    if let Some(digits) = token.strip_prefix('#') {
        return !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit());
    }
    let Some((prefix, digits)) = token.rsplit_once('-') else {
        return false;
    };
    !prefix.is_empty()
        && prefix.chars().all(|c| c.is_ascii_alphabetic())
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
}

/// `2026-08-01`, `2026/08/01`, `01/08/2026` — a date, in the shapes people
/// write one in a comment.
///
/// A date is attribution in the sense that matters: it is what lets a reader
/// tell a marker written last week from one written six years ago.
fn is_date(token: &str) -> bool {
    let parts: Vec<&str> = if token.contains('-') {
        token.split('-').collect()
    } else if token.contains('/') {
        token.split('/').collect()
    } else {
        return false;
    };
    if parts.len() != 3 {
        return false;
    }
    parts
        .iter()
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        && parts.iter().any(|part| part.len() == 4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    fn found(source: &str) -> Vec<UnattributedMarker> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        collect(&tree)
    }

    fn markers(source: &str) -> Vec<&'static str> {
        found(source).into_iter().map(|item| item.marker).collect()
    }

    // --- positive

    #[test]
    fn flags_a_bare_todo() {
        let items = found(";; TODO: handle the empty case\n(defun f () 1)\n");
        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(items[0].marker, "TODO");
        assert_eq!(items[0].note, "handle the empty case");
    }

    #[test]
    fn flags_every_marker_word() {
        for word in ["TODO", "FIXME", "XXX", "HACK", "BUG"] {
            assert_eq!(
                markers(&format!("; {word} something\n(f)\n")),
                vec![word],
                "{word} was not flagged"
            );
        }
    }

    #[test]
    fn a_lower_case_marker_is_recognised_and_reported_upper_cased() {
        assert_eq!(markers("; todo: later\n(f)\n"), vec!["TODO"]);
    }

    #[test]
    fn flags_a_trailing_comment_as_well_as_an_own_line_one() {
        assert_eq!(markers("(defun f () 1) ; TODO: later\n"), vec!["TODO"]);
    }

    #[test]
    fn flags_a_marker_with_no_note_at_all() {
        assert_eq!(markers(";; TODO\n(f)\n"), vec!["TODO"]);
        assert_eq!(markers(";; FIXME!\n(f)\n"), vec!["FIXME"]);
    }

    #[test]
    fn flags_a_marker_inside_a_block_comment() {
        assert_eq!(markers("#| TODO: later |#\n(f)\n"), vec!["TODO"]);
    }

    // --- every accepted attribution shape

    #[test]
    fn a_parenthetical_owner_is_attribution() {
        assert!(found(";; TODO(ada): rewrite this\n(f)\n").is_empty());
        assert!(found(";; FIXME(ada-b): rewrite this\n(f)\n").is_empty());
    }

    #[test]
    fn a_bracketed_owner_is_attribution() {
        assert!(found(";; TODO[ada]: rewrite this\n(f)\n").is_empty());
    }

    #[test]
    fn an_at_handle_is_attribution() {
        assert!(found(";; TODO: @ada should look at this\n(f)\n").is_empty());
        assert!(found(";; TODO: ask @ada-b about it\n(f)\n").is_empty());
    }

    #[test]
    fn a_hash_issue_reference_is_attribution() {
        assert!(found(";; TODO: drop this once #412 lands\n(f)\n").is_empty());
        assert!(found(";; TODO: drop this (#412)\n(f)\n").is_empty());
    }

    #[test]
    fn a_tracker_key_is_attribution_whatever_the_tracker() {
        for reference in ["PROJ-412", "gh-412", "GH-412", "ABC-1", "sc-99999"] {
            assert!(
                found(&format!(";; TODO: see {reference}\n(f)\n")).is_empty(),
                "{reference} was not accepted"
            );
        }
    }

    #[test]
    fn a_url_is_attribution() {
        assert!(found(";; TODO: https://example.com/issues/412\n(f)\n").is_empty());
        assert!(found(";; TODO: see http://bugs.example.com/1\n(f)\n").is_empty());
    }

    #[test]
    fn a_date_is_attribution() {
        for date in ["2026-08-01", "2026/08/01", "01/08/2026"] {
            assert!(
                found(&format!(";; TODO: revisit after {date}\n(f)\n")).is_empty(),
                "{date} was not accepted"
            );
        }
    }

    #[test]
    fn the_documented_shape_count_matches_the_shapes_that_are_tested() {
        // Owner-parenthetical, owner-bracketed, @handle, #issue, tracker key,
        // URL, date.
        assert_eq!(ATTRIBUTION_SHAPES, 7);
    }

    // --- near-miss negatives

    /// The rule `inspect todo` already applies: a marker must open the comment.
    #[test]
    fn a_marker_inside_a_longer_word_is_not_a_task() {
        assert!(found(";; TODOs are tracked elsewhere\n(f)\n").is_empty());
        assert!(found(";; BUGGY behaviour below\n(f)\n").is_empty());
    }

    #[test]
    fn a_marker_in_the_middle_of_prose_is_not_a_task() {
        assert!(found(";; the TODO list lives in NOTES.md\n(f)\n").is_empty());
    }

    #[test]
    fn an_ordinary_comment_is_not_a_task() {
        assert!(found(";; This explains the next form.\n(f)\n").is_empty());
        assert!(found(";;; A section heading\n(f)\n").is_empty());
    }

    #[test]
    fn a_file_with_no_comments_produces_nothing() {
        assert!(found("(defun f () 1)\n(defvar *x* 2)\n").is_empty());
        assert!(found("").is_empty());
    }

    /// An empty owner names nobody, so it is not attribution.
    #[test]
    fn an_empty_parenthetical_is_not_attribution() {
        assert_eq!(markers(";; TODO(): later\n(f)\n"), vec!["TODO"]);
        assert_eq!(markers(";; TODO[]: later\n(f)\n"), vec!["TODO"]);
    }

    /// A word with a dash in it is not a tracker key unless what follows the
    /// dash is a number.
    #[test]
    fn an_ordinary_hyphenated_word_is_not_a_tracker_key() {
        assert_eq!(
            markers(";; TODO: handle the well-known case\n(f)\n"),
            vec!["TODO"]
        );
    }

    /// A bare number is not a date and not an issue.
    #[test]
    fn a_bare_number_is_not_attribution() {
        assert_eq!(markers(";; TODO: retry 3 times\n(f)\n"), vec!["TODO"]);
        assert_eq!(markers(";; TODO: fix by 2026\n(f)\n"), vec!["TODO"]);
    }

    /// A three-part number with no four-digit component is a version, not a
    /// date.
    #[test]
    fn a_version_number_is_not_a_date() {
        assert_eq!(markers(";; TODO: drop after 1/2/3\n(f)\n"), vec!["TODO"]);
    }

    // --- the string-literal trap

    /// The comment-rule analogue of a quoted node: text that looks like a
    /// comment but is inside a string literal is not a comment, and the parser
    /// already knows the difference.
    #[test]
    fn a_marker_inside_a_string_literal_is_not_a_comment() {
        assert!(found("(defun f () \"; TODO: not a comment\")").is_empty());
        assert!(found("(format nil \";; TODO: later~%\")").is_empty());
        assert!(found("(defun f (x) \"TODO: this is a docstring, not a comment.\" x)").is_empty());
    }

    /// A datum comment comments out a *form*. Reading it as prose would let
    /// `#;(todo-list x)` be judged as a marker.
    #[test]
    fn a_datum_comment_is_not_read_as_prose() {
        let tree =
            SyntaxTree::parse_with_dialect("#;(todo-list x)\n(f)", Dialect::Scheme).expect("parse");
        assert!(collect(&tree).is_empty());
    }

    #[test]
    fn findings_are_in_source_order() {
        let items = found(";; TODO: a\n;; TODO: b\n;; TODO: c\n(f)\n");
        let starts: Vec<usize> = items.iter().map(|item| item.span.start().get()).collect();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 3);
    }

    #[test]
    fn the_message_names_the_marker_and_what_would_satisfy_the_rule() {
        let message = found(";; FIXME: later\n(f)\n")[0].message();
        assert!(message.contains("FIXME(name):"), "{message}");
        assert!(message.contains("ticket"), "{message}");
    }
}
