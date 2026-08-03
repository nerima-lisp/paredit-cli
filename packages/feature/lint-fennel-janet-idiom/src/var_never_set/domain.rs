//! `var-never-set` detection: a mutable binding nothing ever reassigns.
//!
//! Both dialects split their binding forms the same way — Fennel has
//! `local`/`var`, Janet has `def`/`var` — and in both the mutable spelling
//! exists only so that a later assignment is legal. A `var` with no assignment
//! anywhere states a mutability the code does not use, and both languages'
//! communities read `var` as "watch this, it changes".
//!
//! This is not an invented rule. Fennel ships a linter plugin whose
//! `check-unused` asserts `(or (not meta.var) meta.set)` and reports
//! `"%s declared as var but never set"` (`src/linter.fnl:80-82`), and its test
//! suite pins the behaviour: `(var x 1) (+ x 9)` must fail to compile under the
//! plugin while `(var x 1) (set x 9)` must succeed (`test/linter.fnl`,
//! `test-var-never-set`). Janet has no such plugin, but the same `def`/`var`
//! distinction and the same `set` special (`src/boot/boot.janet`), so the
//! analysis transfers unchanged; only the mutator vocabulary differs.
//!
//! # Why the whole file, and why that is the safe direction
//!
//! The engine hands a `HeadFilter::Heads` rule one node with no parent pointer
//! and no depth, so "is this name assigned anywhere in its scope" cannot be
//! asked of the node alone — and
//! [`paredit_core_lint_engine::engine::RuleContext::binding_table`] is empty
//! for both of these dialects (`build_binding_table` returns early for
//! anything outside Common Lisp / Emacs Lisp / Scheme / Racket), so there is no
//! resolved binding to consult either.
//!
//! What is left is a name search over the whole file, and the direction of its
//! error matters: the search *suppresses* findings. It is deliberately blind to
//! scope, to shadowing, and to quoting, so every one of those blindnesses can
//! only hide a true positive, never invent one. A module-level `var` assigned
//! from a function defined three hundred lines later is found; so is one
//! assigned only inside a macro template; so is an unrelated `x` in a different
//! function's `(set x …)`.
//!
//! The scan is not cached in
//! [`paredit_core_lint_engine::engine::RuleContext::scratch_cache`] on purpose:
//! that slot holds one type per file's pass and panics on a second, and
//! `leftover-print-debug` — which is in scope for both of these dialects —
//! already claims it.
//!
//! # Measured cost, and the shape this rule wants
//!
//! One invocation costs what a whole-tree rule's single invocation costs,
//! because it does the same thing: 484 µs against `leftover-print-debug`'s
//! 531 µs over the same 63 KB Fennel document. The difference is how often.
//! `HeadFilter::Heads` dispatches per node, so a file with *n* `var` forms pays
//! *n* times — 97 ms at 200 `var`s and 381 ms at 400, a doubling ratio of 3.93
//! where the two shipped rules measured beside it are 1.85 and 1.93.
//!
//! Nearly all of that is
//! [`paredit_core_syntax::sexpr::SyntaxTree::root_view`] materializing the
//! document, and there is no cheaper door: the borrowed node accessors are
//! `pub(in crate::sexpr)`, so a feature package's only whole-tree access is the
//! materializing one. [`is_candidate`] keeps everything else out of that path,
//! which is what makes the real-world cost bearable — the 288-file Fennel
//! corpus averages 0.73 `var` forms per file and the 241-file Janet corpus
//! 1.93, against the 200 the measurement above uses.
//!
//! The shape that fixes this properly is `HeadFilter::WholeTree`: the
//! dispatcher materializes the root exactly once per file and hands it over
//! (`dispatch.rs`, before the walk), which is one materialization instead of
//! *n*. This rule uses `Heads` because that is what it was specified to use.

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView, SyntaxTree};

use crate::support::{head_symbol, symbol_text};

/// The dialects this rule is meaningful for. `rule.rs` passes this same
/// constant to `RuleDialectScope::new`, so the scope and the per-dialect
/// vocabulary below cannot drift apart.
pub const DIALECTS: [Dialect; 2] = [Dialect::Fennel, Dialect::Janet];

/// The heads that introduce a mutable binding in `dialect`, or `&[]` for a
/// dialect this rule does not model.
///
/// Janet's `var-` is `(var x :private …)` (`boot.janet:73-77`). `varfn` is
/// deliberately absent: it is redefined by writing `varfn` again rather than by
/// `set`, so it needs a different mutator vocabulary than the one below.
#[must_use]
pub const fn binder_heads_for(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::Fennel => &["var"],
        Dialect::Janet => &["var", "var-"],
        _ => &[],
    }
}

/// The heads that assign to an existing binding in `dialect`.
///
/// Fennel: `set` (`specials.fnl:424`) and `set-forcibly!` (`:434`) are the only
/// two specials that destructure with `declaration` unset, which is what makes
/// an assignment an assignment there.
///
/// Janet: `set` plus every macro in `boot.janet:138-144` and `:79-82` that
/// expands to it — `++`, `--`, `+=`, `-=`, `*=`, `/=`, `%=`, `toggle`. Omitting
/// one of those would turn `(var i 0) (while … (++ i))` into a false positive,
/// which is precisely the shape the rule is aimed at.
#[must_use]
pub const fn mutator_heads_for(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::Fennel => &["set", "set-forcibly!"],
        Dialect::Janet => &["set", "++", "--", "+=", "-=", "*=", "/=", "%=", "toggle"],
        _ => &[],
    }
}

/// One mutable binding with no assignment anywhere in its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsetVar {
    /// The whole `(var …)` form, which is what the finding points at.
    pub span: ByteSpan,
    /// The span of the name atom alone.
    pub name_span: ByteSpan,
    pub name: String,
    /// The head as written, so the message can say `var-` when that is what
    /// the author wrote.
    pub head: String,
    /// What to suggest instead: `local` in Fennel, `def` in Janet.
    pub immutable_head: &'static str,
}

/// The name a `(var …)` form binds, if it binds exactly one plain name.
///
/// Destructuring binders (`(var [a b] …)`, `(var {: x} …)`) return `None`: the
/// rule would have to decide which of several names is unassigned, and a
/// partially-assigned destructure is a different claim than this rule makes.
///
/// A Fennel multi-sym (`t.x`, `t:x`) also returns `None` — it is not a new
/// binding at all, and `var` rejects it (`nomulti`, `specials.fnl:417`).
fn bound_name(view: &ExpressionView) -> Option<&ExpressionView> {
    let name = view.children.get(1)?;
    if name.kind != ExpressionKind::Atom {
        return None;
    }
    let text = symbol_text(name)?;
    let plain = !text.is_empty()
        && !text.contains('.')
        && !text.contains(':')
        && !text.starts_with('"')
        && !text.starts_with(|byte: char| byte.is_ascii_digit());
    plain.then_some(name)
}

/// The macro names a file binds, which is what makes a call opaque.
///
/// A macro can expand to a `set` on any argument it is handed, and nothing in
/// the call site says so. Two independent third-party corpora produced the
/// same false positive from exactly that:
///
/// - `tangerine.nvim`'s `serialize.fnl:26` defines
///   `` (macro append! [name ...] … `(set-forcibly! ,name (.. ,name …))) ``
///   and calls `(append! out …)` three times, so `out` is assigned and no
///   literal `(set out …)` exists;
/// - `jpm`'s `cgen.janet:15` defines
///   `` (defmacro- setfn [name & body] ~(set ,name (fn ,name ,;body))) ``
///   and uses it for five forward-declared `var`s.
///
/// Both were reported before this existed. The suppression is per file, which
/// is the only scope available: a macro imported from elsewhere has no body
/// here to read.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MacroVocabulary {
    /// Names that head a macro call in this file.
    names: Vec<String>,
    /// Prefixes from `(import-macros m :mod)`, whose calls read `(m.name …)`.
    prefixes: Vec<String>,
    /// Set when the file pulls in macros whose names cannot be enumerated —
    /// `require-macros` imports a whole module into scope without naming what
    /// it brought. Every call in such a file is potentially a macro call, so
    /// the rule declines the file entirely.
    opaque: bool,
}

impl MacroVocabulary {
    /// Whether a call to `head` could be a macro expanding to an assignment.
    #[must_use]
    pub fn is_macro_call(&self, head: &str) -> bool {
        self.names.iter().any(|name| name == head)
            || self
                .prefixes
                .iter()
                .any(|prefix| head.starts_with(prefix) && head[prefix.len()..].starts_with('.'))
    }

    #[must_use]
    pub const fn is_opaque(&self) -> bool {
        self.opaque
    }
}

/// Every atom under `view`, with a leading `:` stripped.
///
/// Used on the binding table of `(macros {…})` and `(import-macros {…} …)`.
/// Fennel's `{: name}` shorthand reads as the two atoms `:` and `name`, and
/// the explicit `{:name local-name}` reads as `:name` and `local-name`, so
/// taking every atom rather than trying to pick out keys covers both — and
/// over-collecting a macro name only widens the suppression.
fn collect_atom_names(view: &ExpressionView, out: &mut Vec<String>) {
    let mut stack = vec![view];
    while let Some(node) = stack.pop() {
        if let Some(text) = symbol_text(node) {
            let name = text.strip_prefix(':').unwrap_or(text);
            if !name.is_empty() {
                out.push(name.to_owned());
            }
        }
        stack.extend(node.children.iter());
    }
}

/// Reads the macro vocabulary a file establishes for itself.
#[must_use]
pub fn macro_vocabulary(dialect: Dialect, root: &ExpressionView) -> MacroVocabulary {
    let mut vocabulary = MacroVocabulary::default();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if let Some(head) = head_symbol(view) {
            match (dialect, head) {
                // `(macro name [args] body)`.
                (Dialect::Fennel, "macro") | (Dialect::Janet, "defmacro" | "defmacro-") => {
                    if let Some(name) = view.children.get(1).and_then(symbol_text) {
                        vocabulary.names.push(name.to_owned());
                    }
                }
                // `(macros {: a : b})` defines several at once.
                (Dialect::Fennel, "macros") => {
                    if let Some(table) = view.children.get(1) {
                        collect_atom_names(table, &mut vocabulary.names);
                    }
                }
                // `(import-macros {: a} :mod)` names what it brought;
                // `(import-macros m :mod)` does not, and its calls read `m.a`.
                (Dialect::Fennel, "import-macros") => match view.children.get(1) {
                    Some(binding) if binding.kind == ExpressionKind::Atom => {
                        if let Some(name) = symbol_text(binding) {
                            vocabulary.prefixes.push(name.to_owned());
                        }
                    }
                    Some(binding) => collect_atom_names(binding, &mut vocabulary.names),
                    None => {}
                },
                // `(require-macros :mod)` brings every macro of a module into
                // scope under its own name, and the module is not this file.
                (Dialect::Fennel, "require-macros") => vocabulary.opaque = true,
                _ => {}
            }
        }
        stack.extend(view.children.iter());
    }
    vocabulary
}

/// Whether any assignment form anywhere in `root` names `name` as a target,
/// or any file-local macro call could have expanded into one.
///
/// Only the *target* of an assignment counts, which is what separates this
/// from a plain reference: `(var x 1) (print x)` still reports, and that is
/// the whole point of the rule.
///
/// Every atom of the target position is collected rather than just a bare
/// symbol, so `(set [a b] …)`, `(set {: a} …)` and `(set (. t k) …)` all mark
/// every name they mention. That over-marks — `(set (. t k) 1)` marks `t` —
/// and over-marking suppresses.
fn is_assigned_anywhere(
    root: &ExpressionView,
    mutators: &[&str],
    macros: &MacroVocabulary,
    name: &str,
) -> bool {
    let mut stack = vec![root];
    while let Some(view) = stack.pop() {
        // A bound `let` in an `&&` chain is a let chain, which is edition-2024
        // syntax this workspace's 1.85 MSRV does not have and which only the
        // `msrv` check catches. `is_some_and` says the same thing at 1.85.
        let assigns_here = head_symbol(view).is_some_and(|head| mutators.contains(&head))
            && view
                .children
                .get(1)
                .is_some_and(|target| mentions(target, name));
        if assigns_here {
            return true;
        }
        // A macro sees its arguments unevaluated and may expand any of them
        // into an assignment target, so every argument counts, not just the
        // first.
        let macro_could_assign = head_symbol(view).is_some_and(|head| macros.is_macro_call(head))
            && view.children[1..].iter().any(|arg| mentions(arg, name));
        if macro_could_assign {
            return true;
        }
        stack.extend(view.children.iter());
    }
    false
}

fn mentions(view: &ExpressionView, name: &str) -> bool {
    let mut stack = vec![view];
    while let Some(node) = stack.pop() {
        if symbol_text(node) == Some(name) {
            return true;
        }
        stack.extend(node.children.iter());
    }
    false
}

/// Whether `view` is a `(var name init)` this rule could report, judged from
/// the node alone.
///
/// Split out from [`examine`] because everything `examine` does past this point
/// needs the whole document, and
/// [`paredit_core_syntax::sexpr::SyntaxTree::root_view`] materializes it — a
/// `Vec` per node and a `String` per atom. Asking this first means only a real
/// `(var …)` form pays, instead of every node the head index dispatches.
#[must_use]
pub fn is_candidate(dialect: Dialect, view: &ExpressionView) -> bool {
    let binders = binder_heads_for(dialect);
    if binders.is_empty() {
        return false;
    }
    if !head_symbol(view).is_some_and(|head| binders.contains(&head)) {
        return false;
    }
    // `(var)` and `(var x)` are malformed in both dialects — Fennel asserts
    // `(= (length ast) 3)` (`specials.fnl:449`). Reporting a form that does not
    // compile would be noise on top of the compiler's own message.
    view.children.len() >= 3 && bound_name(view).is_some()
}

/// Examines one `(var …)` form against the whole file it sits in.
///
/// `macros` is the file's own macro vocabulary, read once by the caller — the
/// rule is dispatched per node and would otherwise re-read it per `var`.
#[must_use]
pub fn examine(
    dialect: Dialect,
    root: &ExpressionView,
    macros: &MacroVocabulary,
    view: &ExpressionView,
) -> Option<UnsetVar> {
    if macros.is_opaque() || !is_candidate(dialect, view) {
        return None;
    }
    let head = head_symbol(view)?;
    let name_view = bound_name(view)?;
    let name = symbol_text(name_view)?;
    if is_assigned_anywhere(root, mutator_heads_for(dialect), macros, name) {
        return None;
    }
    Some(UnsetVar {
        span: view.span,
        name_span: name_view.span,
        name: name.to_owned(),
        head: head.to_owned(),
        immutable_head: immutable_head_for(dialect),
    })
}

/// The binding form to suggest instead.
const fn immutable_head_for(dialect: Dialect) -> &'static str {
    match dialect {
        Dialect::Janet => "def",
        _ => "local",
    }
}

/// Every unassigned mutable binding in one file. The standalone entry point,
/// used by the tests and by any future report that wants the same list without
/// going through the lint engine.
#[must_use]
pub fn collect(dialect: Dialect, tree: &SyntaxTree) -> Vec<UnsetVar> {
    let root = tree.root_view();
    let macros = macro_vocabulary(dialect, &root);
    let mut found = Vec::new();
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if let Some(item) = examine(dialect, &root, &macros, view) {
            found.push(item);
        }
        stack.extend(view.children.iter());
    }
    found.sort_by_key(|item| item.span.start().get());
    found
}

/// How many binding forms this rule could have reported on: the denominator a
/// zero-finding sweep needs in order to mean anything.
#[must_use]
pub fn candidate_count(dialect: Dialect, tree: &SyntaxTree) -> usize {
    let binders = binder_heads_for(dialect);
    if binders.is_empty() {
        return 0;
    }
    let root = tree.root_view();
    let mut count = 0;
    let mut stack: Vec<&ExpressionView> = root.children.iter().collect();
    while let Some(view) = stack.pop() {
        if head_symbol(view).is_some_and(|head| binders.contains(&head)) {
            count += 1;
        }
        stack.extend(view.children.iter());
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str, dialect: Dialect) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        collect(dialect, &tree)
            .into_iter()
            .map(|item| item.name)
            .collect()
    }

    #[test]
    fn flags_a_fennel_var_that_is_never_set() {
        assert_eq!(names("(var x 1)\n(print x)", Dialect::Fennel), vec!["x"]);
    }

    #[test]
    fn leaves_a_fennel_var_that_is_set() {
        assert!(names("(var x 1)\n(set x 2)", Dialect::Fennel).is_empty());
    }

    #[test]
    fn the_two_cases_fennels_own_linter_test_pins() {
        // test/linter.fnl, `test-var-never-set`.
        assert_eq!(names("(var x 1) (+ x 9)", Dialect::Fennel), vec!["x"]);
        assert!(names("(var x 1) (set x 9)", Dialect::Fennel).is_empty());
    }

    #[test]
    fn a_set_from_a_later_function_counts() {
        assert!(
            names(
                "(var count 0)\n(fn inc []\n  (set count (+ count 1)))",
                Dialect::Fennel
            )
            .is_empty()
        );
    }

    #[test]
    fn set_forcibly_counts_as_an_assignment() {
        assert!(names("(var x 1) (set-forcibly! x 2)", Dialect::Fennel).is_empty());
    }

    #[test]
    fn a_destructuring_set_marks_every_name_it_mentions() {
        assert!(names("(var x 1)\n(set [x y] [2 3])", Dialect::Fennel).is_empty());
    }

    #[test]
    fn flags_a_janet_var_and_names_def_as_the_alternative() {
        let tree =
            SyntaxTree::parse_with_dialect("(var x 1)\n(print x)", Dialect::Janet).expect("parse");
        let found = collect(Dialect::Janet, &tree);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].immutable_head, "def");
        assert_eq!(found[0].head, "var");
    }

    #[test]
    fn every_janet_assignment_macro_counts() {
        // boot.janet:138-144 and :79-82 all expand to `set`. A rule that
        // knew only `set` would report every counter loop in the language.
        for mutator in [
            "set x 2", "++ x", "-- x", "+= x 1", "-= x 1", "*= x 2", "/= x 2", "%= x 2", "toggle x",
        ] {
            assert!(
                names(&format!("(var x 1)\n({mutator})"), Dialect::Janet).is_empty(),
                "({mutator}) was not read as an assignment"
            );
        }
    }

    #[test]
    fn janets_private_var_spelling_is_covered() {
        assert_eq!(
            names("(var- x :private 1)\n(print x)", Dialect::Janet),
            vec!["x"]
        );
    }

    #[test]
    fn a_destructuring_binder_is_not_reported() {
        // Which of `a` and `b` is unassigned is a different claim.
        assert!(names("(var [a b] [1 2])\n(print a)", Dialect::Fennel).is_empty());
    }

    /// The four sub-conditions of `bound_name`'s plain-name test, each of
    /// which is a binder position holding something that is not a new name.
    /// Removing the test made none of the other cases fail, so it gets its
    /// own.
    #[test]
    fn a_binder_that_is_not_a_plain_new_name_is_left_to_the_compiler() {
        // A multi symbol names a field of an existing table, and `var`
        // rejects it outright (`nomulti`, `specials.fnl:417`).
        assert!(names("(var t.x 1)\n(print t.x)", Dialect::Fennel).is_empty());
        assert!(names("(var t:x 1)\n(print t:x)", Dialect::Fennel).is_empty());
        // Neither a number nor a string literal is a name.
        assert!(names("(var 1 2)\n(print 1)", Dialect::Fennel).is_empty());
        assert!(names("(var \"s\" 2)\n(print 1)", Dialect::Fennel).is_empty());
        // The control: the same shape with a plain name does report, so the
        // four assertions above cannot pass because the rule stopped working.
        assert_eq!(names("(var tx 1)\n(print tx)", Dialect::Fennel), vec!["tx"]);
    }

    #[test]
    fn a_malformed_var_is_left_to_the_compiler() {
        assert!(names("(var x)", Dialect::Fennel).is_empty());
        assert!(names("(var)", Dialect::Fennel).is_empty());
    }

    #[test]
    fn a_var_nested_in_a_function_body_is_reached() {
        assert_eq!(
            names("(fn f []\n  (var acc 0)\n  acc)", Dialect::Fennel),
            vec!["acc"]
        );
    }

    #[test]
    fn an_unmodelled_dialect_reports_nothing() {
        assert!(names("(var x 1)", Dialect::Clojure).is_empty());
        assert!(names("(var x 1)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn the_candidate_count_is_the_denominator_not_the_finding_count() {
        let tree = SyntaxTree::parse_with_dialect("(var a 1) (set a 2) (var b 1)", Dialect::Fennel)
            .expect("parse");
        assert_eq!(candidate_count(Dialect::Fennel, &tree), 2);
        assert_eq!(collect(Dialect::Fennel, &tree).len(), 1);
    }

    // -- the macro-opacity suppression, and its symmetric controls ---------

    #[test]
    fn a_file_local_fennel_macro_taking_the_name_suppresses() {
        // tangerine.nvim serialize.fnl:26 — `append!` expands to
        // `(set-forcibly! ,name (.. ,name …))`, so `out` *is* assigned.
        let source = "(macro append! [name ...]\n  `(set-forcibly! ,name (.. ,name ,...)))\n\
                      (fn render [xs]\n  (var out \"\")\n  (each [_ x (ipairs xs)]\n    \
                      (append! out x))\n  out)";
        assert!(names(source, Dialect::Fennel).is_empty());
    }

    /// The control the suppression needs: the same file with the macro call
    /// replaced by a call to something that is *not* a macro must still
    /// report. Without this the assertion above passes for a rule that has
    /// stopped working.
    #[test]
    fn an_ordinary_call_taking_the_name_does_not_suppress() {
        let source = "(macro append! [name ...]\n  `(set-forcibly! ,name (.. ,name ,...)))\n\
                      (fn render [xs]\n  (var out \"\")\n  (each [_ x (ipairs xs)]\n    \
                      (print out x))\n  out)";
        assert_eq!(names(source, Dialect::Fennel), vec!["out"]);
    }

    #[test]
    fn a_file_local_janet_macro_taking_the_name_suppresses() {
        // jpm cgen.janet:15 — `setfn` expands to `(set ,name (fn ,name …))`.
        let source = "(defmacro- setfn [name & body]\n  ~(set ,name (fn ,name ,;body)))\n\
                      (var emit-type nil)\n(setfn emit-type [x] x)";
        assert!(names(source, Dialect::Janet).is_empty());
    }

    #[test]
    fn an_undeclared_janet_macro_name_does_not_suppress() {
        // Same call shape, but nothing in the file defines `setfn`, so the
        // suppression must not fire on the head alone.
        assert_eq!(
            names(
                "(var emit-type nil)\n(setfn emit-type [x] x)",
                Dialect::Janet
            ),
            vec!["emit-type"]
        );
    }

    #[test]
    fn macros_and_import_macros_both_contribute_names() {
        assert!(
            names(
                "(macros {:bump! (fn [n] `(set ,n 1))})\n(var total 0)\n(bump! total)",
                Dialect::Fennel
            )
            .is_empty()
        );
        assert!(
            names(
                "(import-macros {: bump!} :my.macros)\n(var total 0)\n(bump! total)",
                Dialect::Fennel
            )
            .is_empty()
        );
    }

    #[test]
    fn a_module_bound_import_macros_suppresses_its_dotted_calls() {
        assert!(
            names(
                "(import-macros m :my.macros)\n(var total 0)\n(m.bump! total)",
                Dialect::Fennel
            )
            .is_empty()
        );
        // …and only those. A different module's dotted call is not it.
        assert_eq!(
            names(
                "(import-macros m :my.macros)\n(var total 0)\n(other.bump! total)",
                Dialect::Fennel
            ),
            vec!["total"]
        );
    }

    #[test]
    fn require_macros_makes_the_whole_file_opaque() {
        // It imports every macro of a module under its own name and says
        // nothing about what those names are.
        assert!(
            names(
                "(require-macros :my.macros)\n(var total 0)\n(print total)",
                Dialect::Fennel
            )
            .is_empty()
        );
    }

    #[test]
    fn the_macro_vocabulary_reads_only_what_the_file_declares() {
        let tree = SyntaxTree::parse_with_dialect(
            "(macro a [] nil)\n(import-macros {: b} :m)\n(import-macros p :n)",
            Dialect::Fennel,
        )
        .expect("parse");
        let vocabulary = macro_vocabulary(Dialect::Fennel, &tree.root_view());
        assert!(vocabulary.is_macro_call("a"));
        assert!(vocabulary.is_macro_call("b"));
        assert!(vocabulary.is_macro_call("p.c"));
        assert!(!vocabulary.is_macro_call("pc"));
        assert!(!vocabulary.is_macro_call("d"));
        assert!(!vocabulary.is_opaque());
    }

    #[test]
    fn a_head_that_only_looks_like_var_is_not_one() {
        assert!(names("(variable x 1) (print x)", Dialect::Fennel).is_empty());
        assert!(names("(my.var x 1) (print x)", Dialect::Fennel).is_empty());
    }
}
