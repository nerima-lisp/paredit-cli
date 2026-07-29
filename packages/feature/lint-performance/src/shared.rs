//! What a cost rule needs to know about an iteration form.
//!
//! Every rule here asks the same question — "does this expression change from
//! round to round?" — and answering it means reading the loop's bindings, which
//! each iteration form spells differently. Getting that reading wrong in the
//! permissive direction turns a precise rule into noise.
//!
//! Symbol comparison lives in [`paredit_core_syntax::view_query`] and is
//! re-exported here, because it is not about loops and three lint packages
//! need it.

use paredit_core_syntax::sexpr::ExpressionView;
use paredit_core_syntax::view_query::{atom_text, list_head};
pub use paredit_core_syntax::view_query::{calls, symbol_in, symbol_is, unqualified};

/// The iteration forms whose body a cost rule looks inside.
pub const ITERATION_HEADS: [&str; 7] =
    ["dolist", "dotimes", "loop", "do", "do*", "mapc", "maplist"];

/// The variables an iteration form binds, so a rule can tell "changes each
/// round" from "loop-invariant".
///
/// Each form spells its bindings differently and this reads all of them:
///
/// - `(dolist (var list) …)` and `(dotimes (var count) …)` bind one name;
/// - `(do ((v init step) …) …)` and `(do* …)` bind a name per spec;
/// - `(loop for v … and w …)` binds after each `for`/`as`/`with`.
///
/// Over-reporting a variable is safe here — a rule that thinks something varies
/// stays silent — so the `loop` reading deliberately errs toward listing too
/// many names rather than too few.
#[must_use]
pub fn iteration_variables(view: &ExpressionView) -> Vec<String> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };
    let mut names = Vec::new();

    if symbol_in(head, &["dolist", "dotimes"]) {
        if let Some(spec) = view.children.get(1) {
            if let Some(name) = spec.children.first().and_then(atom_text) {
                names.push(name.to_owned());
            }
        }
        return names;
    }

    if symbol_in(head, &["do", "do*"]) {
        if let Some(specs) = view.children.get(1) {
            for spec in &specs.children {
                // `(do (v …))` and `(do ((v init) …))` are both legal.
                if let Some(name) = atom_text(spec) {
                    names.push(name.to_owned());
                } else if let Some(name) = spec.children.first().and_then(atom_text) {
                    names.push(name.to_owned());
                }
            }
        }
        return names;
    }

    if symbol_is(head, "loop") {
        let mut expect_variable = false;
        for child in view.children.iter().skip(1) {
            let Some(text) = atom_text(child) else {
                // A destructuring template: `(loop for (a . b) in …)`. Every
                // symbol in it is a bound name.
                if expect_variable {
                    collect_symbols(child, &mut names);
                    expect_variable = false;
                }
                continue;
            };
            if symbol_in(text, &["for", "as", "with", "and"]) {
                expect_variable = true;
                continue;
            }
            if expect_variable {
                names.push(text.to_owned());
                expect_variable = false;
            }
        }
    }

    names
}

/// Every variable name in a destructuring template.
///
/// The reader hands back `.` as an atom of a dotted pair and `nil` is the
/// standard "discard this position" placeholder; neither names a binding, and
/// keeping them would make `mentions_any` treat a literal `nil` anywhere in the
/// body as a loop dependency.
fn collect_symbols(view: &ExpressionView, into: &mut Vec<String>) {
    if let Some(text) = atom_text(view) {
        if text != "." && !text.eq_ignore_ascii_case("nil") {
            into.push(text.to_owned());
        }
        return;
    }
    for child in &view.children {
        collect_symbols(child, into);
    }
}

/// Whether any atom anywhere in `view` names one of `names`.
///
/// This is how a rule asks "does this expression depend on the loop?". It reads
/// every position, including operator positions, which over-approximates in the
/// harmless direction: a form that *mentions* a loop variable is treated as
/// varying, and a rule that thinks something varies stays silent.
#[must_use]
pub fn mentions_any(view: &ExpressionView, names: &[String]) -> bool {
    if names.is_empty() {
        return false;
    }
    if let Some(text) = atom_text(view) {
        return names.iter().any(|name| symbol_is(text, unqualified(name)));
    }
    view.children.iter().any(|child| mentions_any(child, names))
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    pub(crate) fn form(input: &str) -> ExpressionView {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        tree.select_path(&Path::root_child(0))
            .expect("root form")
            .view()
    }

    #[test]
    fn dolist_and_dotimes_bind_one_name() {
        assert_eq!(iteration_variables(&form("(dolist (x xs) x)")), vec!["x"]);
        assert_eq!(iteration_variables(&form("(dotimes (i 10) i)")), vec!["i"]);
    }

    #[test]
    fn do_binds_every_spec_in_either_spelling() {
        assert_eq!(
            iteration_variables(&form("(do ((i 0 (1+ i)) (acc nil)) (nil))")),
            vec!["i", "acc"]
        );
        assert_eq!(iteration_variables(&form("(do (i) (nil))")), vec!["i"]);
    }

    #[test]
    fn loop_binds_after_for_as_with_and_and() {
        assert_eq!(
            iteration_variables(&form("(loop for x in xs and y in ys with z = 1 do (f x))")),
            vec!["x", "y", "z"]
        );
    }

    #[test]
    fn a_loop_destructuring_template_binds_every_symbol_in_it() {
        assert_eq!(
            iteration_variables(&form("(loop for (a . b) in pairs do (f a))")),
            vec!["a", "b"]
        );
    }

    #[test]
    fn a_form_that_names_no_loop_variable_is_invariant() {
        let names = vec!["x".to_owned()];
        assert!(mentions_any(&form("(f x)"), &names));
        assert!(mentions_any(&form("(f (g (h x)))"), &names));
        assert!(!mentions_any(&form("(f y)"), &names));
        assert!(!mentions_any(&form("(f x)"), &[]));
    }
}
