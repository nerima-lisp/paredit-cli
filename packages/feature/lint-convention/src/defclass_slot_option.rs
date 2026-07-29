//! `defclass-slot-option`: a slot option CLOS does not have, or one whose
//! value is the wrong kind of thing.
//!
//! `(defclass point () ((x :init-form 0)))` compiles on some implementations
//! and signals on others, and on the ones where it compiles the slot simply has
//! no initform — the typo is silent. The set of standard slot options is
//! closed, which makes this one of the few CLOS mistakes a reader can settle
//! completely.
//!
//! `:initarg` gets a second check because its value must be a symbol used as a
//! keyword, and `(:initarg "x")` — a string — is accepted by the reader and
//! then matches no `make-instance` argument ever.
//!
//! Report-only: `:init-form` is *probably* `:initform`, and acting on
//! "probably" in a rewrite that changes object construction is not a trade
//! worth making. The message names the likely intent instead.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in};

pub const META: RuleMeta = RuleMeta::new(
    "defclass-slot-option",
    RuleCategory::ObjectSystem,
    Severity::Error,
    "a defclass slot option that is not one of the standard ones, or an :initarg that is not a keyword",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "The standard slot options are a closed set. An option outside it is either ignored — so \
         the slot silently lacks the initform, reader, or type it appears to declare — or \
         signalled, depending on the implementation.",
    )
    .with_example(
        "(defclass point () ((x :init-form 0)))",
        "(defclass point () ((x :initform 0)))",
    )
    .with_caveat(
        "A slot written as a bare symbol has no options and is never reported; neither is a class \
         *option* such as `(:documentation \"…\")`, which is a different list.",
    ),
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("defclass"),
    NormalizedHead::new("define-condition"),
];

/// The slot options the standard defines.
const STANDARD_OPTIONS: [&str; 8] = [
    ":reader",
    ":writer",
    ":accessor",
    ":allocation",
    ":initarg",
    ":initform",
    ":type",
    ":documentation",
];

/// What is wrong with a slot option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotOptionProblem {
    /// Not one of the standard options.
    Unknown,
    /// `:initarg` given something that is not a keyword.
    InitargNotKeyword,
}

/// One bad slot option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadSlotOption {
    pub span: ByteSpan,
    pub slot: String,
    pub option: String,
    pub problem: SlotOptionProblem,
    /// The standard option this is probably a misspelling of, when one is
    /// close enough to name.
    pub suggestion: Option<&'static str>,
}

/// The standard option `option` is probably a misspelling of.
///
/// Matched by removing the hyphens rather than by edit distance: every typo
/// that occurs in practice is a hyphen in the wrong place (`:init-form`,
/// `:init-arg`), and a distance metric would start guessing at genuinely
/// different names.
fn likely_intent(option: &str) -> Option<&'static str> {
    let squashed = option.replace('-', "").to_ascii_lowercase();
    STANDARD_OPTIONS
        .iter()
        .copied()
        .find(|standard| standard.replace('-', "") == squashed && *standard != option)
}

/// Every bad slot option in one `defclass`.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<BadSlotOption> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    if !symbol_in(head, &["defclass", "define-condition"]) {
        return Vec::new();
    }
    // `(defclass name (supers) (slot…) option…)` — the slot list is index 3.
    let Some(slots) = view.children.get(3) else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for slot in &slots.children {
        // A bare symbol is a slot with no options.
        let Some(name) = slot.children.first().and_then(atom_text) else {
            continue;
        };
        let mut index = 1;
        while index < slot.children.len() {
            let Some(option) = atom_text(&slot.children[index]) else {
                index += 1;
                continue;
            };
            if !option.starts_with(':') {
                index += 1;
                continue;
            }
            let lowered = option.to_ascii_lowercase();
            if !STANDARD_OPTIONS.contains(&lowered.as_str()) {
                found.push(BadSlotOption {
                    span: slot.children[index].span,
                    slot: name.to_owned(),
                    option: option.to_owned(),
                    problem: SlotOptionProblem::Unknown,
                    suggestion: likely_intent(&lowered),
                });
            } else if lowered == ":initarg" {
                if let Some(value) = slot.children.get(index + 1) {
                    let is_keyword = atom_text(value).is_some_and(|text| text.starts_with(':'));
                    if !is_keyword {
                        found.push(BadSlotOption {
                            span: value.span,
                            slot: name.to_owned(),
                            option: option.to_owned(),
                            problem: SlotOptionProblem::InitargNotKeyword,
                            suggestion: None,
                        });
                    }
                }
            }
            // Every standard option takes exactly one value.
            index += 2;
        }
    }
    found
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
        for bad in examine(view) {
            let message = match bad.problem {
                SlotOptionProblem::Unknown => match bad.suggestion {
                    Some(intended) => format!(
                        "slot {} has no option {}; {intended} is the standard spelling",
                        bad.slot, bad.option
                    ),
                    None => format!(
                        "slot {} is given {}, which is not a standard slot option and may be \
                         silently ignored",
                        bad.slot, bad.option
                    ),
                },
                SlotOptionProblem::InitargNotKeyword => format!(
                    "slot {}'s :initarg is not a keyword, so no make-instance argument can ever \
                     match it",
                    bad.slot
                ),
            };
            sink.report(bad.span, message);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn problems(input: &str) -> Vec<(String, SlotOptionProblem)> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view)
            .into_iter()
            .map(|item| (item.option, item.problem))
            .collect()
    }

    fn suggestion(input: &str) -> Option<&'static str> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view).into_iter().next().and_then(|i| i.suggestion)
    }

    #[test]
    fn flags_a_misspelled_option_and_names_the_intent() {
        assert_eq!(
            problems("(defclass point () ((x :init-form 0)))"),
            vec![(":init-form".to_owned(), SlotOptionProblem::Unknown)]
        );
        assert_eq!(
            suggestion("(defclass point () ((x :init-form 0)))"),
            Some(":initform")
        );
    }

    #[test]
    fn flags_an_option_that_resembles_nothing_standard() {
        assert_eq!(
            problems("(defclass point () ((x :frobnicate t)))"),
            vec![(":frobnicate".to_owned(), SlotOptionProblem::Unknown)]
        );
        assert_eq!(suggestion("(defclass point () ((x :frobnicate t)))"), None);
    }

    #[test]
    fn accepts_every_standard_option() {
        assert!(
            problems(
                "(defclass point () ((x :initarg :x :initform 0 :accessor point-x \
                 :type integer :documentation \"the x\" :allocation :instance)))"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_an_initarg_that_is_not_a_keyword() {
        assert_eq!(
            problems("(defclass point () ((x :initarg x)))"),
            vec![(":initarg".to_owned(), SlotOptionProblem::InitargNotKeyword)]
        );
        assert_eq!(
            problems(r#"(defclass point () ((x :initarg "x")))"#),
            vec![(":initarg".to_owned(), SlotOptionProblem::InitargNotKeyword)]
        );
    }

    #[test]
    fn does_not_flag_a_bare_slot() {
        assert!(problems("(defclass point () (x y))").is_empty());
    }

    #[test]
    fn does_not_read_a_class_option_as_a_slot_option() {
        assert!(
            problems(r#"(defclass point () (x) (:documentation "d") (:metaclass m))"#).is_empty()
        );
    }

    #[test]
    fn does_not_confuse_a_value_with_an_option() {
        // `:instance` is the *value* of `:allocation`, not an option itself.
        assert!(problems("(defclass point () ((x :allocation :instance)))").is_empty());
    }

    #[test]
    fn reads_define_condition_the_same_way() {
        assert_eq!(
            problems("(define-condition my-error (error) ((code :init-form 0)))"),
            vec![(":init-form".to_owned(), SlotOptionProblem::Unknown)]
        );
    }

    #[test]
    fn reads_every_slot() {
        assert_eq!(
            problems("(defclass p () ((x :init-form 0) (y :initform 0) (z :reeder z-of)))").len(),
            2
        );
    }
}
