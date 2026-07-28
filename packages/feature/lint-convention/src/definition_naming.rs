//! `definition-naming`: a name that contradicts its own defining form.
//!
//! Common Lisp has no syntax for "this is a constant" or "this is special" —
//! the reader cannot tell `+limit+` from `limit`, and neither can a human
//! reading a use site. The `+earmuffs+` and `*earmuffs*` conventions carry that
//! information, and they carry it *only* if they are applied consistently: one
//! unmarked special is enough to make every unmarked name suspect.
//!
//! Which is exactly why this is one rule and not three. `defconstant`,
//! `defvar`, and `defparameter` each imply a spelling, the implication is the
//! same shape in all three cases, and a project either has the convention or
//! does not. Splitting them would mean three rules that are always enabled
//! together and always disabled together.
//!
//! Tagged `pedantic`: a project that has not adopted the convention is not
//! making a mistake, and this rule would fire on every definition it has.
//!
//! Report-only. Renaming a global is a refactor with call sites, which is
//! `refactor rename` territory, not a lint fix.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleTag,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in, unqualified};

pub const META: RuleMeta = RuleMeta::new(
    "definition-naming",
    RuleCategory::Naming,
    Severity::Warning,
    "a defconstant/defvar/defparameter whose name lacks the +constant+ or *special* marker",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Pedantic, RuleTag::Style])
.with_explanation(
    RuleExplanation::new(
        "Nothing in the syntax of a *use* site distinguishes a special variable from a lexical \
         one, or a constant from either. The earmuff conventions are what carry that, and they \
         carry it only while they are applied without exception.",
    )
    .with_example("(defparameter timeout 30)", "(defparameter *timeout* 30)")
    .with_caveat(
        "Only the definition is read. A `let` binding named `*x*` is a different mistake, and a \
         constant named `+x+` used before it is defined is nobody's.",
    ),
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("defconstant"),
    NormalizedHead::new("defvar"),
    NormalizedHead::new("defparameter"),
    NormalizedHead::new("define-symbol-macro"),
];

/// The marker a defining form implies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameMarker {
    /// `+name+`, for a constant.
    Plus,
    /// `*name*`, for a special variable.
    Earmuffs,
}

impl NameMarker {
    const fn character(self) -> char {
        match self {
            Self::Plus => '+',
            Self::Earmuffs => '*',
        }
    }

    #[must_use]
    pub const fn as_pattern(self) -> &'static str {
        match self {
            Self::Plus => "+name+",
            Self::Earmuffs => "*name*",
        }
    }

    /// Whether `name` carries this marker on both ends with something between.
    #[must_use]
    pub fn matches(self, name: &str) -> bool {
        let stripped = unqualified(name);
        let marker = self.character();
        stripped.len() > 2 && stripped.starts_with(marker) && stripped.ends_with(marker)
    }
}

/// One misnamed definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MisnamedDefinition {
    pub span: ByteSpan,
    pub name: String,
    pub form: String,
    pub expected: NameMarker,
}

/// Reads one definition and reports the marker its name is missing.
#[must_use]
pub fn examine(view: &ExpressionView) -> Option<MisnamedDefinition> {
    let head = list_head(view)?;
    let expected = if symbol_in(head, &["defconstant"]) {
        NameMarker::Plus
    } else if symbol_in(head, &["defvar", "defparameter", "define-symbol-macro"]) {
        NameMarker::Earmuffs
    } else {
        return None;
    };

    let name_view = view.children.get(1)?;
    let name = atom_text(name_view)?;
    // A name already carrying the *other* convention's marker is a deliberate
    // choice somebody made — `+*weird*+` is not a mistake this rule can name —
    // so only a name with neither marker is reported.
    if NameMarker::Plus.matches(name) || NameMarker::Earmuffs.matches(name) {
        return None;
    }

    Some(MisnamedDefinition {
        span: name_view.span,
        name: name.to_owned(),
        form: unqualified(head).to_ascii_lowercase(),
        expected,
    })
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        if let Some(found) = examine(view) {
            sink.report(
                found.span,
                format!(
                    "{} is defined by {} but is not spelled {}, so a use site cannot tell it from \
                     a lexical binding",
                    found.name,
                    found.form,
                    found.expected.as_pattern()
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn expected(input: &str) -> Option<NameMarker> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).map(|found| found.expected)
    }

    #[test]
    fn reads_each_marker() {
        assert!(NameMarker::Plus.matches("+limit+"));
        assert!(NameMarker::Earmuffs.matches("*timeout*"));
        assert!(NameMarker::Earmuffs.matches("cl:*print-base*"));
        assert!(!NameMarker::Earmuffs.matches("*"));
        assert!(!NameMarker::Earmuffs.matches("**"));
        assert!(!NameMarker::Earmuffs.matches("*timeout"));
    }

    #[test]
    fn flags_a_constant_without_plus_signs() {
        assert_eq!(expected("(defconstant limit 100)"), Some(NameMarker::Plus));
    }

    #[test]
    fn flags_a_special_without_earmuffs() {
        assert_eq!(
            expected("(defparameter timeout 30)"),
            Some(NameMarker::Earmuffs)
        );
        assert_eq!(
            expected("(defvar registry nil)"),
            Some(NameMarker::Earmuffs)
        );
        assert_eq!(
            expected("(define-symbol-macro here (get-here))"),
            Some(NameMarker::Earmuffs)
        );
    }

    #[test]
    fn does_not_flag_a_correctly_marked_name() {
        assert_eq!(expected("(defconstant +limit+ 100)"), None);
        assert_eq!(expected("(defparameter *timeout* 30)"), None);
    }

    #[test]
    fn does_not_flag_a_name_carrying_the_other_marker() {
        // Deliberate, if unusual; not a mistake this rule can name.
        assert_eq!(expected("(defconstant *limit* 100)"), None);
    }

    #[test]
    fn does_not_flag_another_defining_form() {
        assert_eq!(expected("(defun timeout () 30)"), None);
    }

    #[test]
    fn does_not_flag_a_form_with_no_name() {
        assert_eq!(expected("(defvar)"), None);
    }

    #[test]
    fn reads_the_head_case_insensitively() {
        assert_eq!(
            expected("(DEFPARAMETER timeout 30)"),
            Some(NameMarker::Earmuffs)
        );
    }
}
