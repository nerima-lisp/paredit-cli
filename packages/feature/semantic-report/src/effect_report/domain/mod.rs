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
//!
//! One case gets help from the value layer: a Common Lisp `if` whose test is
//! a `let`/`let*`-bound or `defconstant`-named constant the value layer has
//! already proven. `if` itself is not in `policy`'s tables — nothing
//! is — so ordinarily its every use defaults the verdict to `Unknown`, same as
//! any other unrecognized head, and both branches are walked. When the test's
//! value is provably known, the dead branch can never run, so it is skipped
//! rather than charged against the definition's purity, and `if` no longer
//! counts as an unresolved head. This is deliberately narrow: only two
//! sources are trusted (see `resolve_constant`), and a test that is not one
//! of them falls straight back to today's behavior, unchanged.

mod policy;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use paredit_core_cli::report::line_of;
use paredit_core_semantics::semantics::NodeKey;
use paredit_core_semantics::semantics::binding::BindingTable;
use paredit_core_semantics::semantics::value::service::constant_key;
use paredit_core_semantics::semantics::value::{LiteralValue, PropagatableValue, ValueTable};
use paredit_core_syntax::definition::{DefinitionShape, definition_shape};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::{atom_symbol_span, atom_symbol_text};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::list_head;

use crate::shared::SemanticFile;

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
            defined.insert(fold(file.dialect, &name));
            order.push((fold(file.dialect, &name), name, head, shape, form));
        }
    }

    let mut locals = BTreeMap::new();
    for (key, _, _, shape, form) in &order {
        // A name defined twice keeps the first verdict rather than the last:
        // which one a call sees is not knowable, and the first is at least a
        // stable choice.
        locals.entry(key.clone()).or_insert_with(|| {
            local_verdict(
                file.dialect,
                form,
                *shape,
                &defined,
                &file.bindings,
                &file.values,
            )
        });
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
        dialect_modelled: file.effect_dialect_supported(),
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
    dialect: Dialect,
    form: &ExpressionView,
    shape: DefinitionShape,
    defined: &BTreeSet<String>,
    bindings: &BindingTable,
    values: &ValueTable,
) -> LocalVerdict {
    let mut purity = Purity::Pure;
    let mut cause = None;
    let mut calls = BTreeSet::new();

    for body_form in shape.body_forms(form) {
        walk(
            dialect,
            body_form,
            defined,
            bindings,
            values,
            &mut purity,
            &mut cause,
            &mut calls,
        );
    }

    LocalVerdict {
        purity,
        cause,
        calls,
    }
}

#[allow(clippy::too_many_arguments)]
fn walk(
    dialect: Dialect,
    view: &ExpressionView,
    defined: &BTreeSet<String>,
    bindings: &BindingTable,
    values: &ValueTable,
    purity: &mut Purity,
    cause: &mut Option<String>,
    calls: &mut BTreeSet<String>,
) {
    if let Some(head) = list_head(view) {
        if dialect == Dialect::CommonLisp && head.eq_ignore_ascii_case("if") {
            if let Some(live) = resolve_if_branch(view, bindings, values) {
                // The test is provably constant: the branch that cannot run
                // is never walked, and `if` itself contributes nothing to the
                // verdict — unlike the unresolved-head fallback below, it is
                // not charged as an unmodelled operator, because which branch
                // executes is no longer in doubt.
                if let Some(live) = live {
                    walk(
                        dialect, live, defined, bindings, values, purity, cause, calls,
                    );
                }
                return;
            }
        }

        match head_effect(dialect, head) {
            Some(HeadEffect::Effectful) => {
                if *purity != Purity::Effectful {
                    *cause = Some(head.to_owned());
                }
                *purity = Purity::Effectful;
            }
            Some(HeadEffect::Pure) => {}
            None => {
                let key = fold(dialect, head);
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
        walk(
            dialect, child, defined, bindings, values, purity, cause, calls,
        );
    }
}

/// The live branch of `(if test then [else])`, when the value layer has
/// already proven `test`'s truth value.
///
/// `Some(None)` means the test is known false and there is no `else`, so
/// nothing runs. `None` means the test is not resolvable this way at all — a
/// non-symbol test, a reference to something the value layer does not track,
/// or (by construction, since the caller only reaches this for
/// `Dialect::CommonLisp`) any other dialect — and the caller must fall back
/// to treating `if` like any other unregistered head, exactly as before this
/// bridge existed.
fn resolve_if_branch<'a>(
    view: &'a ExpressionView,
    bindings: &BindingTable,
    values: &ValueTable,
) -> Option<Option<&'a ExpressionView>> {
    let test = view.children.get(1)?;
    let value = resolve_constant(test, bindings, values)?;
    let truthy = LiteralValue::from(value.clone()).is_truthy(Dialect::CommonLisp);
    Some(if truthy {
        view.children.get(2)
    } else {
        view.children.get(3)
    })
}

/// The provably constant value a bare symbol atom denotes, via whichever half
/// of the value layer names it: a lexical `let`/`let*` binding, or a
/// file-level `defconstant`.
///
/// Deliberately narrower than every binding [`BindingTable::resolve`] can
/// find. A `&optional`/`&key` lambda-list parameter gets a `BindingId` and an
/// `init_form` exactly like a `let` binding does, but the value on file is
/// only the *default* — a caller-supplied argument overrides it in a way this
/// layer never records as a reassignment. Trusting that value here would let
/// a branch that is not actually dead look proven dead, which is exactly the
/// false-`Pure` outcome this bridge must never produce. So only a binding
/// whose introducing form is `let` or `let*` is consulted; every other
/// lexical binding (a parameter, an `&aux`/`&optional`/`&key` default, a `do`
/// variable, …) is left to the existing conservative default, same as an
/// unresolved reference.
fn resolve_constant<'a>(
    view: &ExpressionView,
    bindings: &BindingTable,
    values: &'a ValueTable,
) -> Option<&'a PropagatableValue> {
    let name = atom_symbol_text(view)?;
    let span = atom_symbol_span(view)?;

    if let Some(id) = bindings.resolve(NodeKey::atom(span)) {
        let lexically_bound = bindings.binding(id).binder_head().is_some_and(|head| {
            head.eq_ignore_ascii_case("let") || head.eq_ignore_ascii_case("let*")
        });
        // A name that resolves to *any* binding is shadowed at this point,
        // trusted or not: falling through to the file-level constant below
        // would risk answering with an unrelated `defconstant` of the same
        // name, so an untrusted binding ends the search rather than widening
        // it.
        return lexically_bound.then(|| values.binding_value(id)).flatten();
    }

    // This bridge is only ever reached for `Dialect::CommonLisp` (guarded in
    // `walk`), same as the `is_truthy(Dialect::CommonLisp)` call above.
    constant_key(Dialect::CommonLisp, name).and_then(|key| values.constant_value(&key))
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

/// The key two spellings of one same-file definition's name share.
///
/// Common Lisp's reader folds a symbol's case, so `f` and `F` name the same
/// thing there and must map to one key. Emacs Lisp reads case-sensitively —
/// `f` and `F` are two different symbols — so folding its names the Common
/// Lisp way would wrongly conflate two distinct definitions (or a definition
/// with an unrelated call of the same letters, different case) into one
/// verdict, silently discarding whichever body lost the `or_insert_with`
/// race in [`build_effect_report`].
fn fold(dialect: Dialect, name: &str) -> String {
    if dialect == Dialect::CommonLisp {
        name.to_ascii_uppercase()
    } else {
        name.to_owned()
    }
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
    fn an_if_whose_test_is_not_provably_constant_still_defaults_to_unknown() {
        // Regression: a genuinely unresolvable `if` test — an ordinary
        // parameter, which the value layer never treats as constant — must
        // keep today's conservative behavior. `if` is not in `policy`'s
        // tables, so it is charged as an unregistered head exactly as before
        // this bridge existed, and both branches are still walked.
        let report = report("(defun f (x) (if x 1 2))");
        let f = definition(&report, "f");
        assert_eq!(f.purity, Purity::Unknown);
        assert_eq!(f.cause.as_deref(), Some("if"));
    }

    #[test]
    fn an_optional_parameters_default_is_not_trusted_as_a_constant_test() {
        // A `&optional` default gets a `BindingId` and an `init_form` exactly
        // like a `let` binding, but a caller may pass any value for `flag` —
        // trusting the default here would let this genuinely live effectful
        // branch look dead and overclaim `Pure`. `resolve_constant` only
        // trusts `let`/`let*` bindings, so this must stay unchanged.
        let report = report("(defun f (&optional (flag t)) (if flag (write-line \"x\") (+ 1 2)))");
        assert_eq!(definition(&report, "f").purity, Purity::Effectful);
    }

    #[test]
    fn a_defconstant_proven_false_prunes_the_dead_effectful_branch() {
        // The acceptance scenario: before this bridge, `write-line` sits in a
        // branch that can never run, but the walk had no way to know that —
        // `if` alone drove the verdict to `Unknown`, and `write-line`
        // unconditionally overwrote it to `Effectful`. The value layer has
        // already proven `+debug+` is `nil`, so the bridge now prunes the
        // dead branch and the verdict is exactly `Pure`, not merely
        // "less conservative".
        let report = report(
            "(defconstant +debug+ nil)\n(defun f (x) (if +debug+ (write-line \"debug\") (+ x 1)))",
        );
        assert_eq!(definition(&report, "f").purity, Purity::Pure);
    }

    #[test]
    fn a_defconstant_proven_true_prunes_the_dead_effectful_else_branch() {
        let report = report(
            "(defconstant +debug+ t)\n(defun f (x) (if +debug+ (+ x 1) (write-line \"debug\")))",
        );
        assert_eq!(definition(&report, "f").purity, Purity::Pure);
    }

    #[test]
    fn a_defconstant_proven_false_with_no_else_branch_is_pure() {
        let report =
            report("(defconstant +debug+ nil)\n(defun f () (if +debug+ (write-line \"debug\")))");
        assert_eq!(definition(&report, "f").purity, Purity::Pure);
    }

    #[test]
    fn a_lexically_shadowed_name_is_not_confused_with_a_same_named_constant() {
        // `+debug+` is a parameter here, not the file's `defconstant`. Lexical
        // shadowing must win: the parameter is not trusted (it is not a
        // `let`/`let*` binding), and the search must not fall through to the
        // unrelated file-level constant of the same name — so the `if` is not
        // resolved, and the effectful branch is walked like any other
        // unresolved `if`, same as before this bridge existed.
        let report = report(
            "(defconstant +debug+ nil)\n(defun f (+debug+) (if +debug+ (write-line \"x\") 1))",
        );
        assert_eq!(definition(&report, "f").purity, Purity::Effectful);
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
    fn an_emacs_lisp_file_is_modelled_and_reports_real_purity_findings() {
        let report = report_of("(defun add (a b) (+ a b))", Dialect::EmacsLisp);
        assert!(report.dialect_modelled);
        assert_eq!(definition(&report, "add").purity, Purity::Pure);
    }

    #[test]
    fn an_emacs_lisp_buffer_primitive_is_effectful() {
        let report = report_of("(defun greet () (message \"hi\"))", Dialect::EmacsLisp);
        let greet = definition(&report, "greet");
        assert_eq!(greet.purity, Purity::Effectful);
        assert_eq!(greet.cause.as_deref(), Some("message"));
    }

    #[test]
    fn an_emacs_lisp_effect_propagates_from_a_callee_to_its_caller() {
        let report = report_of(
            "(defun inner () (message \"hi\"))\n(defun outer () (inner))",
            Dialect::EmacsLisp,
        );
        assert_eq!(definition(&report, "outer").purity, Purity::Effectful);
    }

    /// The fix for the case-folding bug this step also found: without a
    /// dialect-aware `fold`, `my-func` and `MY-FUNC` would be conflated into
    /// one same-file definition key, silently discarding one of the two
    /// bodies. Emacs Lisp reads symbols case-sensitively, so they are two
    /// distinct definitions and must get independent verdicts.
    #[test]
    fn emacs_lisp_same_file_lookup_is_case_sensitive() {
        let report = report_of(
            "(defun my-func () (message \"hi\"))\n(defun MY-FUNC () 1)",
            Dialect::EmacsLisp,
        );
        // `definition()` matches case-insensitively, which is exactly wrong
        // for this test, so the two are told apart by exact name here.
        assert_eq!(report.definitions.len(), 2, "{:?}", report.definitions);
        let exact = |name: &str| {
            report
                .definitions
                .iter()
                .find(|definition| definition.name == name)
                .unwrap_or_else(|| panic!("{name} is reported: {:?}", report.definitions))
        };
        assert_eq!(exact("my-func").purity, Purity::Effectful);
        assert_eq!(exact("MY-FUNC").purity, Purity::Pure);
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
