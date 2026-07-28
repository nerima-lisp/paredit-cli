//! Which definitions are provably pure, which observably do something, and
//! which this layer cannot decide.
//!
//! Most refactor-safety questions reduce to this one. May a form be hoisted out
//! of a loop? Only if evaluating it fewer times changes nothing. May two
//! duplicate bodies be folded into one call? Only if calling once instead of
//! twice changes nothing. May a binding be inlined into three reference sites?
//! Only if evaluating its initial form three times changes nothing. Each of
//! those is "is this pure", asked in a different accent, and each is currently
//! answered by its own conservative hand-rolled check.
//!
//! The classification is three-valued, and the third value carries the weight.
//! `Unknown` is not a failure of the analysis — it is the correct answer for a
//! body that calls an unregistered head, because that head may be a macro and a
//! macro can expand into an assignment that appears nowhere in the source. A
//! two-valued verdict would have to fold `Unknown` into one side, and folding
//! it into `Pure` would make this report unsafe to act on.
//!
//! Effects propagate along the file's own call graph to a fixpoint: a function
//! whose body is nothing but a call to an effectful sibling is itself
//! effectful, however clean it reads.

mod policy;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use paredit_core_syntax::definition::{DefinitionShape, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::list_head;

use crate::shared::{SemanticFile, line_of};

pub use policy::{HeadEffect, head_effect};

/// What a definition does, as far as this layer can prove.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Purity {
    /// Every head the body reaches is a known-pure standard function, and so
    /// is every same-file definition it calls.
    Pure,
    /// The body reaches an operator that observably does something, or calls a
    /// definition that does.
    Effectful,
    /// The body reaches a head this layer does not model. Says nothing in
    /// either direction, and must be treated as "may do anything".
    Unknown,
}

impl Purity {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::Effectful => "effectful",
            Self::Unknown => "unknown",
        }
    }

    /// Combines two observations about one body.
    ///
    /// `Effectful` absorbs, because one effect makes the whole body effectful.
    /// `Unknown` beats `Pure` for the same reason in the other direction: a
    /// body that is pure except for one call it cannot see is not pure.
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Effectful, _) | (_, Self::Effectful) => Self::Effectful,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Pure, Self::Pure) => Self::Pure,
        }
    }

    pub const ALL: [Self; 3] = [Self::Pure, Self::Effectful, Self::Unknown];
}

/// One definition and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionEffect {
    pub name: String,
    pub head: String,
    pub purity: Purity,
    pub span: ByteSpan,
    pub line: usize,
    /// The operator that made it effectful, or the unmodelled head that made
    /// it unknown. The single actionable field: it names what to look at.
    pub cause: Option<String>,
    /// Whether the verdict came from a callee rather than the body's own
    /// operators. Distinguishes "this function writes a file" from "this
    /// function calls something that does".
    pub inherited: bool,
    /// Same-file definitions this one calls, in name order.
    pub calls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectReportFile {
    pub path: PathBuf,
    pub dialect: Dialect,
    pub dialect_modelled: bool,
    pub definitions: Vec<DefinitionEffect>,
}

impl EffectReportFile {
    #[must_use]
    pub fn count_of(&self, purity: Purity) -> usize {
        self.definitions
            .iter()
            .filter(|definition| definition.purity == purity)
            .count()
    }
}

/// What the body's own operators say, before any callee is consulted.
struct LocalVerdict {
    purity: Purity,
    cause: Option<String>,
    calls: BTreeSet<String>,
}

#[must_use]
pub fn build_effect_report(file: &SemanticFile) -> EffectReportFile {
    let source = file.tree.source();
    let root = file.tree.root_view();

    // Two passes, because the first cannot be decided without the second's
    // input: an unregistered head means "unknown" only if it is *not* a
    // definition this file makes. Walking before the names are collected would
    // mark every forward reference unknown and no fixpoint could take it back —
    // `Unknown` is above `Pure` in the join order, so nothing ever lowers.
    let mut order = Vec::new();
    let mut defined = BTreeSet::new();
    for form in &root.children {
        if let Some((name, head, shape)) = definition_of(form, file.dialect) {
            defined.insert(fold(&name));
            order.push((fold(&name), name, head, shape, form));
        }
    }

    let mut locals = BTreeMap::new();
    for (key, _, _, shape, form) in &order {
        // A name defined twice keeps the first verdict rather than the last:
        // which one a call sees is not knowable, and the first is at least a
        // stable choice.
        locals
            .entry(key.clone())
            .or_insert_with(|| local_verdict(form, *shape, &defined));
    }

    let resolved = resolve_effects(&locals);

    let mut definitions = order
        .into_iter()
        .map(|(key, name, head, _, form)| {
            let local = &locals[&key];
            let (purity, cause, inherited) = resolved[&key].clone();
            DefinitionEffect {
                name,
                head,
                purity,
                span: form.span,
                line: line_of(source, form.span.start().get()),
                cause: cause.or_else(|| local.cause.clone()),
                inherited,
                calls: local.calls.iter().cloned().collect(),
            }
        })
        .collect::<Vec<_>>();
    definitions
        .sort_by_key(|definition| (definition.span.start().get(), definition.span.end().get()));

    EffectReportFile {
        path: file.path.clone(),
        dialect: file.dialect,
        dialect_modelled: file.dialect_is_modelled(),
        definitions,
    }
}

/// Propagates effects along the file's own call graph until nothing changes.
///
/// Bounded by the number of definitions: each round can only move a definition
/// up the `Pure → Unknown → Effectful` order, which it can do at most twice, so
/// the loop cannot run forever even on a mutually recursive cycle.
fn resolve_effects(
    locals: &BTreeMap<String, LocalVerdict>,
) -> BTreeMap<String, (Purity, Option<String>, bool)> {
    let mut resolved = locals
        .iter()
        .map(|(key, local)| (key.clone(), (local.purity, None, false)))
        .collect::<BTreeMap<_, _>>();

    loop {
        let mut changed = false;
        for (key, local) in locals {
            let mut purity = local.purity;
            let mut cause = None;
            for callee in &local.calls {
                let Some((callee_purity, _, _)) = resolved.get(callee) else {
                    continue;
                };
                let joined = purity.join(*callee_purity);
                if joined != purity {
                    purity = joined;
                    cause = Some(callee.clone());
                }
            }
            let entry = resolved.get_mut(key).expect("every local key is resolved");
            if entry.0 != purity {
                *entry = (purity, cause, local.purity != purity);
                changed = true;
            }
        }
        if !changed {
            return resolved;
        }
    }
}

/// Reads one definition's body, without following calls.
///
/// The body comes from the shape rather than from `children.skip(n)`: the
/// definition's own head, name, lambda list, and docstring are not forms the
/// body evaluates, and a fixed skip count gets that wrong for every form whose
/// shape differs from `defun`'s.
fn local_verdict(
    form: &ExpressionView,
    shape: DefinitionShape,
    defined: &BTreeSet<String>,
) -> LocalVerdict {
    let mut purity = Purity::Pure;
    let mut cause = None;
    let mut calls = BTreeSet::new();

    for body_form in shape.body_forms(form) {
        walk(body_form, defined, &mut purity, &mut cause, &mut calls);
    }

    LocalVerdict {
        purity,
        cause,
        calls,
    }
}

fn walk(
    view: &ExpressionView,
    defined: &BTreeSet<String>,
    purity: &mut Purity,
    cause: &mut Option<String>,
    calls: &mut BTreeSet<String>,
) {
    if let Some(head) = list_head(view) {
        match head_effect(head) {
            Some(HeadEffect::Effectful) => {
                if *purity != Purity::Effectful {
                    *cause = Some(head.to_owned());
                }
                *purity = Purity::Effectful;
            }
            Some(HeadEffect::Pure) => {}
            None => {
                let key = fold(head);
                if defined.contains(&key) {
                    // A call to a sibling. Its verdict is not known yet, so it
                    // is deferred to the fixpoint rather than guessed at.
                    calls.insert(key);
                } else if *purity == Purity::Pure {
                    // Nothing in this file defines it, so it may be a macro,
                    // and a macro can expand into anything.
                    *cause = Some(head.to_owned());
                    *purity = Purity::Unknown;
                }
            }
        }
    }

    for child in &view.children {
        walk(child, defined, purity, cause, calls);
    }
}

/// A definition's name, defining head, and shape, when the form is one.
fn definition_of(
    form: &ExpressionView,
    dialect: Dialect,
) -> Option<(String, String, DefinitionShape)> {
    let head = list_head(form)?;
    let shape = definition_shape(dialect, form, head)?;
    let name = shape.name(form)?;
    Some((name.to_owned(), head.to_owned(), shape))
}

/// The key two spellings of one Common Lisp symbol share.
fn fold(name: &str) -> String {
    name.to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::sexpr::SyntaxTree;
    use std::path::Path;

    fn report_of(source: &str, dialect: Dialect) -> EffectReportFile {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        build_effect_report(&SemanticFile::analyze(Path::new("t.lisp"), dialect, tree))
    }

    fn report(source: &str) -> EffectReportFile {
        report_of(source, Dialect::CommonLisp)
    }

    fn definition<'a>(report: &'a EffectReportFile, name: &str) -> &'a DefinitionEffect {
        report
            .definitions
            .iter()
            .find(|definition| definition.name.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| panic!("{name} is reported: {report:?}"))
    }

    #[test]
    fn arithmetic_over_parameters_is_pure() {
        let report = report("(defun add (a b) (+ a b))");
        assert_eq!(definition(&report, "add").purity, Purity::Pure);
    }

    #[test]
    fn writing_to_a_stream_is_effectful() {
        let report = report("(defun greet () (write-line \"hi\"))");
        let greet = definition(&report, "greet");
        assert_eq!(greet.purity, Purity::Effectful);
        assert_eq!(greet.cause.as_deref(), Some("write-line"));
        assert!(!greet.inherited);
    }

    #[test]
    fn an_unregistered_head_makes_the_verdict_unknown_not_pure() {
        let report = report("(defun f (x) (my-macro x))");
        let f = definition(&report, "f");
        assert_eq!(f.purity, Purity::Unknown);
        assert_eq!(f.cause.as_deref(), Some("my-macro"));
    }

    #[test]
    fn an_effect_propagates_from_a_callee_to_its_caller() {
        let report = report("(defun inner () (write-line \"hi\"))\n(defun outer () (inner))");
        let outer = definition(&report, "outer");
        assert_eq!(outer.purity, Purity::Effectful);
        assert!(outer.inherited);
        assert_eq!(outer.cause.as_deref(), Some("INNER"));
    }

    #[test]
    fn a_call_to_a_pure_sibling_keeps_the_caller_pure() {
        let report = report("(defun inner (a) (+ a 1))\n(defun outer (a) (inner a))");
        assert_eq!(definition(&report, "outer").purity, Purity::Pure);
        assert_eq!(definition(&report, "inner").purity, Purity::Pure);
    }

    #[test]
    fn a_mutually_recursive_pair_reaches_a_fixpoint_rather_than_looping() {
        let report = report(
            "(defun ping (n) (pong n))\n(defun pong (n) (if (zerop n) (write-line \"done\") (ping n)))",
        );
        assert_eq!(definition(&report, "pong").purity, Purity::Effectful);
        assert_eq!(definition(&report, "ping").purity, Purity::Effectful);
    }

    #[test]
    fn a_destructive_call_is_effectful_even_inside_otherwise_pure_arithmetic() {
        let report = report("(defun f (xs) (+ 1 (length (nreverse xs))))");
        assert_eq!(definition(&report, "f").purity, Purity::Effectful);
    }

    #[test]
    fn the_definitions_own_name_is_not_read_as_a_call_to_something_unknown() {
        let report = report("(defun f (a) (+ a 1))");
        assert!(
            !definition(&report, "f")
                .calls
                .iter()
                .any(|call| call == "F"),
            "{report:?}"
        );
    }

    #[test]
    fn findings_are_sorted_by_source_position() {
        let report = report("(defun a () 1)\n(defun b () 2)\n(defun c () 3)");
        let starts = report
            .definitions
            .iter()
            .map(|definition| definition.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 3);
    }

    #[test]
    fn effectful_absorbs_and_unknown_beats_pure() {
        assert_eq!(Purity::Pure.join(Purity::Unknown), Purity::Unknown);
        assert_eq!(Purity::Unknown.join(Purity::Effectful), Purity::Effectful);
        assert_eq!(Purity::Pure.join(Purity::Pure), Purity::Pure);
        // Commutative, so the walk order cannot change a verdict.
        for left in Purity::ALL {
            for right in Purity::ALL {
                assert_eq!(left.join(right), right.join(left));
            }
        }
    }

    #[test]
    fn every_verdict_has_a_distinct_label() {
        let mut labels = Purity::ALL.map(Purity::label).to_vec();
        labels.sort_unstable();
        let count = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), count);
    }
}
