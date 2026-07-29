//! Adding `:accessor` to a `defclass` slot that has neither `:accessor`,
//! `:reader`, nor `:writer`.
//!
//! A bare slot — `x` alone, or `(x :initform 0)` with no accessor option —
//! is readable only through `slot-value`. Naming the accessor
//! `<class>-<slot>` follows the convention `defstruct` generates
//! automatically and CLOS does not: nothing requires it, but nothing else in
//! a codebase that never overrides it needs a different one either.
//!
//! Only `:accessor`, `:reader`, and `:writer` count as "already has one" —
//! a slot with only `:initarg` or `:initform` still has no way to read it
//! back.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in};

/// One slot's edit: either widen a bare atom into a list with an accessor
/// option, or insert the option into an existing list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotEdit {
    pub slot_name: String,
    pub span: ByteSpan,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessorsOutcome {
    /// At least one slot needed an accessor; these are its edits.
    Ready {
        class_name: String,
        edits: Vec<SlotEdit>,
        /// Slots that already had `:accessor`, `:reader`, or `:writer`.
        already_had_one: usize,
    },
    /// A `defclass` with no slot lacking an accessor.
    Nothing {
        class_name: String,
    },
    Unsupported {
        reason: &'static str,
    },
}

const ACCESSOR_OPTIONS: [&str; 3] = [":accessor", ":reader", ":writer"];

fn has_accessor_option(slot: &ExpressionView) -> bool {
    slot.kind == ExpressionKind::List
        && slot.children.iter().skip(1).any(|child| {
            atom_text(child).is_some_and(|text| {
                ACCESSOR_OPTIONS
                    .iter()
                    .any(|option| text.eq_ignore_ascii_case(option))
            })
        })
}

fn slot_name(slot: &ExpressionView) -> Option<&str> {
    match slot.kind {
        ExpressionKind::Atom => atom_text(slot),
        ExpressionKind::List => slot.children.first().and_then(atom_text),
        ExpressionKind::Root => None,
    }
}

#[must_use]
pub fn plan_accessors(source: &str, view: &ExpressionView) -> AccessorsOutcome {
    let Some(head) = list_head(view) else {
        return AccessorsOutcome::Unsupported {
            reason: "not a defclass form",
        };
    };
    if !symbol_in(head, &["defclass"]) {
        return AccessorsOutcome::Unsupported {
            reason: "not a defclass form",
        };
    }
    let Some(class_name) = view.children.get(1).and_then(atom_text) else {
        return AccessorsOutcome::Unsupported {
            reason: "the defclass has no name",
        };
    };
    let class_name = class_name.to_owned();
    let Some(slots) = view.children.get(3) else {
        return AccessorsOutcome::Nothing { class_name };
    };

    let mut edits = Vec::new();
    let mut already_had_one = 0;
    for slot in &slots.children {
        if has_accessor_option(slot) {
            already_had_one += 1;
            continue;
        }
        let Some(name) = slot_name(slot) else {
            continue;
        };
        let accessor = format!("{class_name}-{name}");
        let replacement = match slot.kind {
            ExpressionKind::Atom => format!("({name} :accessor {accessor})"),
            ExpressionKind::List => {
                // The slot's own text, with the new option inserted just
                // before its closing paren, so every other option and any
                // formatting inside the slot is preserved verbatim.
                let original = slot.span.slice(source);
                let Some(before_close) = original.strip_suffix(')') else {
                    continue;
                };
                format!("{before_close} :accessor {accessor})")
            }
            ExpressionKind::Root => continue,
        };
        edits.push(SlotEdit {
            slot_name: name.to_owned(),
            span: slot.span,
            replacement,
        });
    }

    if edits.is_empty() {
        return AccessorsOutcome::Nothing { class_name };
    }
    AccessorsOutcome::Ready {
        class_name,
        edits,
        already_had_one,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn target(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    fn apply(input: &str) -> String {
        let view = target(input);
        match plan_accessors(input, &view) {
            AccessorsOutcome::Ready { edits, .. } => {
                let mut rewritten = input.to_owned();
                for edit in edits.iter().rev() {
                    rewritten.replace_range(edit.span.as_range(), &edit.replacement);
                }
                rewritten
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn a_bare_atom_slot_becomes_a_list_with_an_accessor() {
        let rewritten = apply("(defclass point () (x y))");
        assert_eq!(
            rewritten,
            "(defclass point () ((x :accessor point-x) (y :accessor point-y)))"
        );
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp)
            .expect("rewritten class parses");
    }

    #[test]
    fn a_list_slot_without_an_accessor_gets_one_appended() {
        let rewritten = apply("(defclass point () ((x :initform 0)))");
        assert_eq!(
            rewritten,
            "(defclass point () ((x :initform 0 :accessor point-x)))"
        );
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp)
            .expect("rewritten class parses");
    }

    #[test]
    fn a_slot_with_an_existing_accessor_is_left_alone() {
        let view = target("(defclass point () ((x :accessor px)))");
        match plan_accessors("(defclass point () ((x :accessor px)))", &view) {
            AccessorsOutcome::Nothing { .. } => {}
            other => panic!("expected Nothing, got {other:?}"),
        }
    }

    #[test]
    fn a_reader_or_writer_also_counts_as_already_having_one() {
        let input = "(defclass point () ((x :reader px) (y :writer (setf py))))";
        let view = target(input);
        match plan_accessors(input, &view) {
            AccessorsOutcome::Nothing { .. } => {}
            other => panic!("expected Nothing, got {other:?}"),
        }
    }

    #[test]
    fn a_class_with_no_slots_has_nothing_to_do() {
        let view = target("(defclass point () ())");
        match plan_accessors("(defclass point () ())", &view) {
            AccessorsOutcome::Nothing { .. } => {}
            other => panic!("expected Nothing, got {other:?}"),
        }
    }

    #[test]
    fn a_non_defclass_form_is_unsupported() {
        let view = target("(defstruct point x y)");
        assert!(matches!(
            plan_accessors("(defstruct point x y)", &view),
            AccessorsOutcome::Unsupported { .. }
        ));
    }

    #[test]
    fn a_mix_of_documented_and_bare_slots_only_edits_the_bare_ones() {
        let input = "(defclass point () ((x :accessor px) y))";
        let rewritten = apply(input);
        assert_eq!(
            rewritten,
            "(defclass point () ((x :accessor px) (y :accessor point-y)))"
        );
        let view = target(input);
        match plan_accessors(input, &view) {
            AccessorsOutcome::Ready {
                already_had_one, ..
            } => assert_eq!(already_had_one, 1),
            other => panic!("expected Ready, got {other:?}"),
        }
    }
}
