//! `commented-repl-transcript` detection: a `;`-comment block whose lines are
//! heuristically shaped like a pasted REPL session — alternating
//! `;; PACKAGE> (form)` prompts and `;; => result` lines.
//!
//! # Why this rule has no fix
//!
//! `SyntaxTree` keeps comments in a separate list outside the
//! `ExpressionView` tree (see [`paredit_core_syntax::sexpr::SyntaxTree::comments`]):
//! a rewrite that walks only the node tree cannot see a comment, and so
//! cannot detect that it dropped one. This project has already shipped that
//! exact bug once — a prior write command silently deleted every comment in
//! a file on `--write` with exit 0, because neither its renderer nor its
//! reparse-equivalence guard looked outside the node tree. Proving a comment
//! removal safe needs deeper investigation than this rule's own detection
//! logic, so `META` declares [`paredit_core_lint_engine::model::Fixability::ReportOnly`]
//! and this module never builds a [`paredit_core_lint_engine::model::RuleFix`].
//!
//! # The heuristic, and why it stays conservative
//!
//! A block is a run of comment lines with nothing but a newline between them
//! (no blank line, no code) — see `comment_blocks`. Within a block, a line
//! is prompt-shaped when its text (marker and leading space stripped) matches
//! `[\w.*+-]+>\s...` (a package/prompt name, `>`, then the typed form) and
//! result-shaped when it starts with `=>`. A block is flagged only when it
//! has **two or more** prompt-shaped lines, or **at least one of each** —
//! never on a single matching line, which is what keeps ordinary prose
//! containing a bare `>` or `=>` (`"see => the docs"`, `"value > 100"`) from
//! being flagged: neither shape matches unless the marker character is the
//! very next non-space token, which prose almost never is.
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SourceComment, SyntaxTree};

/// Whether `body` (a comment's text with its `;` marker(s) and one leading
/// space already stripped) is shaped like a REPL prompt: an identifier-ish
/// token, then `>`, then whitespace and the typed form.
fn looks_like_prompt(body: &str) -> bool {
    let is_prompt_char = |c: char| c.is_ascii_alphanumeric() || "_.*+-".contains(c);
    let ident_len = body.chars().take_while(|&c| is_prompt_char(c)).count();
    if ident_len == 0 {
        return false;
    }
    let rest = &body[ident_len..];
    let Some(after_arrow) = rest.strip_prefix('>') else {
        return false;
    };
    after_arrow.is_empty() || after_arrow.starts_with(char::is_whitespace)
}

/// Whether `body` is shaped like a REPL result line: `=>` immediately
/// followed by whitespace or nothing else.
fn looks_like_result(body: &str) -> bool {
    body.strip_prefix("=>")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

/// `comment`'s text with its leading `;`-run and one following space
/// stripped, so `looks_like_prompt`/`looks_like_result` see the same content
/// regardless of whether the source wrote `;`, `;;`, or `;;;`.
fn stripped_body(comment: SourceComment<'_>) -> &str {
    comment.text().trim_start_matches(';').trim_start()
}

/// Groups `tree`'s comments into runs with nothing but a single newline (and
/// surrounding horizontal whitespace) between consecutive entries — a blank
/// line or a line of code between two comments ends the run.
fn comment_blocks<'a>(tree: &'a SyntaxTree, source: &str) -> Vec<Vec<SourceComment<'a>>> {
    let mut blocks: Vec<Vec<SourceComment<'a>>> = Vec::new();
    for comment in tree.comments() {
        let starts_new_block =
            blocks
                .last()
                .and_then(|block| block.last())
                .is_none_or(|previous| {
                    let between = source
                        .get(previous.span().end().get()..comment.span().start().get())
                        .unwrap_or("");
                    let newline_count = between.chars().filter(|&c| c == '\n').count();
                    newline_count != 1
                        || !between
                            .chars()
                            .all(|c| c == ' ' || c == '\t' || c == '\r' || c == '\n')
                });
        if starts_new_block {
            blocks.push(vec![comment]);
        } else if let Some(block) = blocks.last_mut() {
            block.push(comment);
        }
    }
    blocks
}

/// Whether a block of comment lines is shaped like a REPL transcript.
fn looks_like_transcript(block: &[SourceComment<'_>]) -> bool {
    if block.len() < 2 {
        return false;
    }
    let mut prompt_count = 0usize;
    let mut result_count = 0usize;
    for comment in block {
        let body = stripped_body(*comment);
        if looks_like_prompt(body) {
            prompt_count += 1;
        } else if looks_like_result(body) {
            result_count += 1;
        }
    }
    prompt_count >= 2 || (prompt_count >= 1 && result_count >= 1)
}

#[derive(Debug, Clone)]
pub struct CommentedReplTranscriptItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// How many comment lines make up the flagged block, for the message.
    pub line_count: usize,
}

#[derive(Debug)]
pub struct CommentedReplTranscriptSummary {
    /// The number of comment *blocks* scanned — this rule has no forms to
    /// count, so the scaffold's `scanned_form_count` field is repurposed.
    pub scanned_form_count: usize,
    pub violations: Vec<CommentedReplTranscriptItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct CommentedReplTranscriptPolicyOptions {
    fail_on_violation: bool,
}

impl CommentedReplTranscriptPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct CommentedReplTranscriptPolicy {
    pub fail_on_violation: bool,
    pub scanned_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn examine(
    tree: &SyntaxTree,
    source: &str,
    path: &Path,
    scanned_block_count: &mut usize,
    violations: &mut Vec<CommentedReplTranscriptItem>,
) {
    for block in comment_blocks(tree, source) {
        *scanned_block_count += 1;
        if !looks_like_transcript(&block) {
            continue;
        }
        let first = block.first().expect("a block is never empty");
        let last = block.last().expect("a block is never empty");
        violations.push(CommentedReplTranscriptItem {
            path: path.to_path_buf(),
            span: ByteSpan::new(first.span().start(), last.span().end()),
            line_count: block.len(),
        });
    }
}

pub fn collect_commented_repl_transcript(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<CommentedReplTranscriptItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }
    let mut scanned_block_count = 0;
    let mut violations = Vec::new();
    examine(
        tree,
        tree.source(),
        path,
        &mut scanned_block_count,
        &mut violations,
    );
    Ok((scanned_block_count, violations))
}

#[must_use]
pub const fn summarize_commented_repl_transcript(
    scanned_form_count: usize,
    violations: Vec<CommentedReplTranscriptItem>,
) -> CommentedReplTranscriptSummary {
    CommentedReplTranscriptSummary {
        scanned_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_commented_repl_transcript_policy(
    options: CommentedReplTranscriptPolicyOptions,
    summary: &CommentedReplTranscriptSummary,
) -> CommentedReplTranscriptPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    CommentedReplTranscriptPolicy {
        fail_on_violation: options.fail_on_violation(),
        scanned_form_count: summary.scanned_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations_in(input: &str, dialect: Dialect) -> Vec<CommentedReplTranscriptItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let (_, violations) =
            collect_commented_repl_transcript(&PathBuf::from("test.lisp"), dialect, &tree)
                .expect("collect commented_repl_transcript");
        violations
    }

    #[test]
    fn flags_a_prompt_and_result_pair() {
        let input = ";; CL-USER> (+ 1 2)\n;; => 3\n(defun f () 1)\n";
        let violations = violations_in(input, Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line_count, 2);
    }

    #[test]
    fn flags_two_consecutive_prompts_with_no_result_lines() {
        let input = ";; CL-USER> (foo)\n;; CL-USER> (bar)\n";
        assert_eq!(violations_in(input, Dialect::CommonLisp).len(), 1);
    }

    #[test]
    fn does_not_flag_a_single_prompt_shaped_line_alone() {
        let input = ";; CL-USER> (+ 1 2)\n(defun f () 1)\n";
        assert!(violations_in(input, Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn does_not_flag_ordinary_prose_containing_a_greater_than_sign() {
        let input = ";; Note: keep throughput > 100 items/sec, see the tuning guide\n;; for details on how to profile this path.\n(defun f () 1)\n";
        assert!(violations_in(input, Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn does_not_flag_prose_mentioning_an_arrow() {
        let input = ";; This maps requests => responses using the router below.\n;; See docs/routing.md for the full picture.\n(defun f () 1)\n";
        assert!(violations_in(input, Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_blank_line_between_comments_splits_the_block() {
        let input = ";; CL-USER> (+ 1 2)\n\n;; => 3\n";
        assert!(violations_in(input, Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn code_between_comments_splits_the_block() {
        let input = ";; CL-USER> (+ 1 2)\n(defun f () 1)\n;; => 3\n";
        assert!(violations_in(input, Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_string_literal_that_looks_like_a_transcript_is_not_a_comment_and_not_flagged() {
        let input = "(defun f () \"CL-USER> (+ 1 2)\\n=> 3\")\n";
        assert!(violations_in(input, Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn an_unmodelled_dialect_is_left_alone() {
        let input = ";; CL-USER> (+ 1 2)\n;; => 3\n";
        assert!(violations_in(input, Dialect::Clojure).is_empty());
    }

    #[test]
    fn a_crlf_file_is_still_grouped_and_flagged() {
        let input = ";; CL-USER> (+ 1 2)\r\n;; => 3\r\n";
        let violations = violations_in(input, Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn no_fix_is_ever_offered_by_this_rule() {
        // Structural guarantee, not just a `META` assertion: nothing in this
        // module constructs a `RuleFix`, checked in `rule.rs`'s own test.
        let input = ";; CL-USER> (+ 1 2)\n;; => 3\n";
        let violations = violations_in(input, Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].line_count, 2);
    }
}
