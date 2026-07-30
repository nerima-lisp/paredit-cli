//! One implementation diagnostic, placed in the tree and compared to a baseline.

use paredit_core_cli::report::Finding;
use paredit_core_safety::external::sbcl::{Diagnostic, Severity};
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, ExpressionPath, SyntaxTree};
use serde_json::{Value, json};

/// Which implementations this tool knows how to invoke.
///
/// An enum rather than a free-form program name so `--implementation` can be
/// completed, documented, and validated. The *path* to the binary stays
/// separate, because a caller with several SBCL versions needs to say which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Implementation {
    Sbcl,
}

impl Implementation {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sbcl => "sbcl",
        }
    }

    /// The binary invoked when the caller does not name a path.
    #[must_use]
    pub const fn default_program(self) -> &'static str {
        match self {
            Self::Sbcl => "sbcl",
        }
    }
}

/// A diagnostic, with the span of the definition it was attributed to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedDiagnostic {
    pub diagnostic: Diagnostic,
    /// The definition the implementation named, when it could be found in the
    /// tree. Diagnostics about the file as a whole have none.
    pub span: ByteSpan,
    /// Whether this diagnostic is absent from the baseline.
    ///
    /// `false` when no baseline was supplied: without one, nothing is *new*,
    /// and reporting everything as introduced would make the first run look
    /// like a regression.
    pub introduced: bool,
}

impl Finding for PlacedDiagnostic {
    fn kind(&self) -> &'static str {
        match self.diagnostic.severity {
            Severity::Style => "style-warning",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.diagnostic
                .context
                .clone()
                .unwrap_or_else(|| "<file>".to_owned()),
            self.diagnostic.message.clone(),
            format!("introduced={}", self.introduced),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("severity", json!(self.diagnostic.severity.label())),
            ("context", json!(self.diagnostic.context)),
            ("message", json!(self.diagnostic.message)),
            ("introduced", json!(self.introduced)),
            ("identity", json!(self.diagnostic.identity())),
        ]
    }
}

/// Finds the definition an implementation's `in:` context refers to.
///
/// SBCL writes `in: DEFUN BAR` — the operator and the name, upper-cased by the
/// reader. Matching on the *name* alone and case-insensitively is deliberate:
/// the operator can be `DEFUN`, `DEFMETHOD`, `DEFMACRO` or a `(SETF FOO)`
/// form, and the goal is a usable line number rather than a proof.
///
/// A context that matches nothing yields the whole document's start, which is
/// what "this file, somewhere" honestly amounts to.
#[must_use]
pub fn locate_context(tree: &SyntaxTree, context: Option<&str>) -> ByteSpan {
    let whole_file = ByteSpan::new(ByteOffset::new(0), ByteOffset::new(0));
    let Some(context) = context else {
        return whole_file;
    };

    // The last token of `DEFUN BAR` or `DEFMETHOD TRANSFER :BEFORE` is not
    // always the name, but the *second* is, for every definition form SBCL
    // reports this way.
    let Some(name) = context.split_whitespace().nth(1) else {
        return whole_file;
    };
    let name = name.trim_matches(|character| character == '(' || character == ')');

    for index in 0..tree.root_children().len() {
        let Ok(selection) = tree.select_path(&ExpressionPath::from_indexes(vec![index])) else {
            continue;
        };
        let view = selection.view();
        let defined_name = view
            .children
            .first()
            .and_then(|head| head.text.as_deref())
            .filter(|head| head.len() > 3 && head[..3].eq_ignore_ascii_case("def"))
            .and(view.children.get(1))
            .and_then(|form| form.text.as_deref());

        if defined_name.is_some_and(|defined| defined.eq_ignore_ascii_case(name)) {
            return view.span;
        }
    }
    whole_file
}

#[cfg(test)]
mod tests {
    use super::{Implementation, locate_context};
    use paredit_core_syntax::sexpr::SyntaxTree;

    const SOURCE: &str =
        "(in-package :demo)\n\n(defun bar (x)\n  (+ x 1))\n\n(defmethod transfer ((a t))\n  a)\n";

    /// The line the located span starts on.
    ///
    /// `locate_context` answers with a span, and the report envelope turns
    /// that into a line. Resolving it the same way here keeps these cases
    /// stated as the line a human reads off a diagnostic, which is what they
    /// are about.
    fn line_at(tree: &SyntaxTree, context: Option<&str>) -> usize {
        paredit_core_cli::report::line_of(
            tree.source(),
            locate_context(tree, context).start().get(),
        )
    }

    #[test]
    fn an_implementation_names_itself_and_its_binary() {
        assert_eq!(Implementation::Sbcl.label(), "sbcl");
        assert_eq!(Implementation::Sbcl.default_program(), "sbcl");
    }

    #[test]
    fn a_defun_context_resolves_to_the_definition_line() {
        let tree = SyntaxTree::parse(SOURCE).expect("parse");
        assert_eq!(line_at(&tree, Some("DEFUN BAR")), 3);
    }

    /// SBCL upper-cases; the source does not. Matching has to ignore case or
    /// every diagnostic would land on line 1.
    #[test]
    fn context_matching_ignores_case() {
        let tree = SyntaxTree::parse(SOURCE).expect("parse");
        assert_eq!(line_at(&tree, Some("DEFUN bar")), 3);
        assert_eq!(line_at(&tree, Some("defmethod TRANSFER")), 6);
    }

    /// A qualifier after the name must not stop the match.
    #[test]
    fn a_method_qualifier_does_not_prevent_a_match() {
        let tree = SyntaxTree::parse(SOURCE).expect("parse");
        assert_eq!(line_at(&tree, Some("DEFMETHOD TRANSFER :BEFORE")), 6);
    }

    #[test]
    fn an_unknown_or_absent_context_falls_back_to_the_file() {
        let tree = SyntaxTree::parse(SOURCE).expect("parse");
        assert_eq!(line_at(&tree, Some("DEFUN NOWHERE")), 1);
        assert_eq!(line_at(&tree, None), 1);
        assert_eq!(line_at(&tree, Some("DEFUN")), 1);
    }
}
