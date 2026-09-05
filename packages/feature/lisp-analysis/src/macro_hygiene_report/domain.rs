//! The ways a `defmacro` template betrays its caller.
//!
//! A macro is a program that writes a program, and the program it writes is
//! spliced into a scope it cannot see. Five failures follow from that, all
//! invisible at the definition site and all reported here:
//!
//! - **Variable capture.** A template that binds a literal name — `` `(let
//!   ((result ,form)) …) `` — silently shadows the caller's `result`. Any
//!   argument form referring to the caller's `result` now sees the macro's.
//!   Common Lisp has no hygienic macros, so the only defence is a `gensym`, and
//!   the only way to check that it was used is to look.
//! - **Multiple evaluation.** A template that unquotes one parameter twice —
//!   `` `(if (> ,x 0) ,x 0) `` — evaluates the caller's argument form twice.
//!   `(m (pop stack))` then pops twice. This is why the `once-only` idiom
//!   exists. A `symbol-macrolet` binding whose expansion is not provably
//!   side-effect-free and is referenced twice is the same failure by another
//!   name, and reports as the same risk.
//! - **Parameter reordering.** A template that unquotes two parameters in an
//!   order the lambda list does not — `` `(list ,b ,a) `` — evaluates the
//!   caller's argument forms in an order the call site does not read.
//! - **Deep quasiquote nesting.** Three or more nested quasiquote levels,
//!   where getting an escape wrong stops being unlikely.
//! - **Missing editor declaration.** Emacs Lisp only: a macro with no leading
//!   `(declare (indent …))`/`(declare (debug …))`, which leaves indentation
//!   and Edebug guessing.
//!
//! Every check is syntactic and conservative in the same direction: they read
//! the template as written and report what is *visible* there. A template that
//! computes a binding name, or splices in a form built elsewhere, is beyond
//! this analysis and produces no finding rather than a guess.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::definition::is_macro_expander_definition;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::reader::atom_symbol_text;
use paredit_core_syntax::sexpr::{
    ByteSpan, Delimiter, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use paredit_core_cli::report::{FileFindings, Finding};

/// What a template does wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HygieneRisk {
    /// The template binds a literal name that is not obviously a gensym.
    VariableCapture,
    /// A form that re-evaluates on every reference is referenced more than
    /// once.
    ///
    /// Two unrelated shapes report this: a template that unquotes one
    /// parameter more than once, and a `symbol-macrolet`/`cl-symbol-macrolet`
    /// binding whose expansion form is not provably side-effect-free (not a
    /// literal atom or a quoted form — a function call, `incf`, and the
    /// like) but is referenced by name more than once in its body. Both are
    /// the same failure by another name: a name that looks like an ordinary
    /// binding actually re-runs a computation at every use, and a reader
    /// counting names cannot tell that apart from an ordinary reference
    /// without knowing which names are like this. Kept as one variant rather
    /// than two because the remedy is identical in both cases — bind the
    /// form's value once — and a caller filtering findings by risk should
    /// not have to know there are two shapes of it.
    MultipleEvaluation,
    /// The template unquotes two or more distinct parameters in an order
    /// that differs from their left-to-right position in the lambda list.
    ///
    /// `` `(list ,b ,a) `` for `(m (a b) ...)` evaluates `b`'s argument form
    /// before `a`'s, even though the call site reads `a` before `b`. When
    /// either argument form has a side effect — the overwhelmingly common
    /// reason to notice evaluation order at all — the caller's own left-to-
    /// right expectation is silently violated.
    ParameterReordering,
    /// The template nests three or more levels of quasiquote.
    ///
    /// Each backquote inside another backquote's template changes what an
    /// unquote there needs — the innermost unquote cancels one level, an
    /// unquote-unquote cancels two, and so on — which is already hard to
    /// track correctly by the second level. A third makes it easy to get an
    /// escape wrong in a way that only shows up when the macro that builds
    /// the macro is itself expanded.
    DeepQuasiquoteNesting,
    /// Emacs Lisp only: the macro's body does not open with a
    /// `(declare (indent ...))` and/or `(declare (debug ...))` form.
    ///
    /// Neither is required by the language, but their absence is a gap in
    /// the macro's editor integration rather than in the macro itself: with
    /// no `indent` declaration, `M-x indent-region` falls back to indenting
    /// every call to this macro as if it were an ordinary function call, and
    /// with no `debug` declaration, Edebug cannot step through the
    /// arguments a call to it actually evaluates.
    MissingEditorDeclaration,
}

impl HygieneRisk {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::VariableCapture => "variable-capture",
            Self::MultipleEvaluation => "multiple-evaluation",
            Self::ParameterReordering => "parameter-reordering",
            Self::DeepQuasiquoteNesting => "deep-quasiquote-nesting",
            Self::MissingEditorDeclaration => "missing-editor-declaration",
        }
    }
}

/// One hygiene risk in one macro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HygieneFinding {
    pub risk: HygieneRisk,
    /// The macro the template belongs to.
    pub macro_name: String,
    /// The captured binding name, the multiply-evaluated parameter, or —
    /// for [`HygieneRisk::ParameterReordering`] — every out-of-order
    /// parameter, comma-joined in the order the template unquotes them.
    /// Empty for [`HygieneRisk::DeepQuasiquoteNesting`]: the whole template
    /// is at fault there, not any one name in it.
    pub subject: String,
    /// How many times the parameter is unquoted, for
    /// [`HygieneRisk::MultipleEvaluation`]; how many parameters are
    /// out-of-order, for [`HygieneRisk::ParameterReordering`]; the nesting
    /// depth itself, for [`HygieneRisk::DeepQuasiquoteNesting`]. `0` for a
    /// capture.
    pub occurrences: usize,
    /// The remedy, named rather than left to the reader.
    pub remedy: &'static str,
    pub span: ByteSpan,
}

impl Finding for HygieneFinding {
    fn kind(&self) -> &'static str {
        self.risk.label()
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.macro_name.clone(),
            self.subject.clone(),
            format!("occurrences={}", self.occurrences),
            self.remedy.to_owned(),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("risk", json!(self.risk.label())),
            ("macro_name", json!(self.macro_name)),
            ("subject", json!(self.subject)),
            ("occurrences", json!(self.occurrences)),
            ("remedy", json!(self.remedy)),
        ]
    }
}

/// Compares an operator head the way the dialect's own reader would.
///
/// Only Common Lisp folds: `LET`, `let` and `cl:let` name one operator there,
/// while the other seven modelled dialects are case-sensitive and have no
/// package qualifier to strip. This is the same split
/// `DialectSemanticPolicy::identifiers_equal` draws for identifiers and
/// `paredit_core_lint_engine::engine::head_index::head_key` for heads;
/// applying Common Lisp's fold everywhere made `Result` and `result` one name
/// in seven languages where they are two.
fn head_eq(dialect: Dialect, head: &str, expected: &str) -> bool {
    if dialect == Dialect::CommonLisp {
        common_lisp_operator_head_eq(head, expected)
    } else {
        head == expected
    }
}

/// The identifier as the dialect's reader leaves it — both the key two
/// occurrences are compared under and the spelling a finding reports.
///
/// Common Lisp's reader upcases a bare symbol, so `RESULT` *is* what the
/// reader produced from `result` and reporting it is reporting the truth.
/// Everywhere else the source spelling is the name, and folding it would both
/// misreport it and conflate it with every other case variant.
///
/// Borrowed for the seven case-sensitive dialects, where the key is the source
/// text: this runs once per atom of a full subtree walk, so the owned `String`
/// it used to return was an allocation per atom for a pure copy. Same `Cow`
/// shape as `paredit_core_lint_engine::engine::head_index::head_key`.
fn identity_key(dialect: Dialect, name: &str) -> Cow<'_, str> {
    if dialect == Dialect::CommonLisp {
        Cow::Owned(name.to_ascii_uppercase())
    } else {
        Cow::Borrowed(name)
    }
}

/// The characters a dialect's reader spells quote, quasiquote and unquote
/// with, as `paredit_core_syntax::sexpr::reader_policy` models them.
///
/// Only the text fallbacks below consult this. A reader prefix the parser
/// folded into an atom's own text never reaches `reader_prefixes`, so the text
/// has to be read directly — and reading it against Common Lisp's characters
/// everywhere was wrong in two dialects at once. Janet quasiquotes with `~`
/// and spells a *long string* with backquotes, so `` `a long string` ``
/// counted as a quasiquote level and fired `macro-deep-quasiquote-nesting` on a
/// two-level template; Clojure unquotes with `~`, where `,` is whitespace.
#[derive(Clone, Copy)]
struct ReaderChars {
    quote: Option<char>,
    quasiquote: Option<char>,
    /// Unquote and unquote-splicing. Janet splices with a separate `;` rather
    /// than a suffix on its unquote, so this is a set and not one character.
    unquote: &'static [char],
}

const fn reader_chars(dialect: Dialect) -> ReaderChars {
    match dialect {
        // `~` quasiquotes, `,` unquotes, `;` splices. Janet has no quote
        // reader macro in `reader_policy` at all, so `'x` there parses as an
        // ordinary atom rather than a quoted form and `quote` stays `None`.
        Dialect::Janet => ReaderChars {
            quote: None,
            quasiquote: Some('~'),
            unquote: &[','],
        },
        // Clojure's syntax-quote is a backquote, but its unquote is `~`/`~@`.
        Dialect::Clojure => ReaderChars {
            quote: Some('\''),
            quasiquote: Some('`'),
            unquote: &['~'],
        },
        // `reader_policy::classify_quote_prefix` — `'`, `` ` ``, `,`/`,@` —
        // covers every dialect below.
        //
        // Hy and Carp are routed through `reader_policy`'s *legacy* table, so
        // their real unquotes (`~`/`~@` and `%`/`%@`) are not recognised at
        // parse time at all, by this or any other command. That is a
        // core-syntax gap rather than one this report can close, and it is
        // why both are listed here with Common Lisp's characters.
        Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Hy
        | Dialect::Carp
        | Dialect::Fennel
        | Dialect::Unknown => ReaderChars {
            quote: Some('\''),
            quasiquote: Some('`'),
            unquote: &[','],
        },
    }
}

/// Whether an atom's own text opens with `character` — how a reader prefix the
/// parser folded into the atom, rather than into a prefix node, survives.
fn text_starts_with(view: &ExpressionView, character: Option<char>) -> bool {
    character.is_some_and(|character| {
        view.text
            .as_deref()
            .is_some_and(|text| text.starts_with(character))
    })
}

/// Binding forms whose second child is a `((name value) …)` binding list.
///
/// `flet`/`labels` bind function names rather than values, but a template
/// that binds one shadows the caller's just as a variable binding does.
///
/// Deliberately still short. Common Lisp binds names in several more shapes —
/// `do`/`do*`/`prog` take this same `((name init …) …)` list, but `dolist`,
/// `dotimes` and `with-open-file` put a *single* binding at child 1 rather
/// than a list of them, `multiple-value-bind` and `with-slots` put a list of
/// bare names there, and `destructuring-bind` a whole lambda list. Adding the
/// first group alone would be a table edit; covering the rest needs a
/// per-form binder shape, and half-doing it would read `(dolist (x list) …)`
/// as two bindings and report the *list* as a capturable name, which is the
/// same defect [`binding_entries`] exists to avoid. These forms remain outside
/// this table until their binder shapes can be modeled explicitly.
const PAREN_BINDING_FORMS: [&str; 4] = ["let", "let*", "flet", "labels"];

/// Binding forms whose second child is a flat `[name value …]` binding
/// vector, per dialect.
///
/// Mirrors, head for head, the `BinderShape::FlatPairs` arms of `scope_shape`
/// in `paredit_core_syntax::dialect::semantic`, which is the repository's one
/// model of which head binds what. The two tables must stay in step:
/// `scope_shape` is not callable on the quasiquoted forms this analysis reads
/// (see [`binding_entries`]), so this is a copy, and a head present there and
/// missing here is a capture nobody reports.
/// `bracket_binding_forms_are_flat_pair_binders_in_the_shared_scope_model`
/// pins the direction that copy can drift in.
///
/// It had drifted: this used to mirror the far narrower private
/// `bracket_let_binding_head`, giving Clojure `let` alone, so a capturing
/// `when-let`, `loop`, `with-open` or `binding` template reported clean.
///
/// Janet's `if-let`/`if-with` stay out and Clojure's `if-let`/`if-some` come
/// in, because that is what `scope_shape` says for each: Janet's are excluded
/// there (their else branch is outside the binding's scope, which that shape
/// vocabulary cannot express) and Clojure's are not.
const fn bracket_binding_forms(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::Clojure => &[
            "let",
            "loop",
            "binding",
            "with-open",
            "with-redefs",
            "with-local-vars",
            "if-let",
            "if-some",
            "when-let",
            "when-some",
            "when-first",
        ],
        Dialect::Hy => &["let", "with", "for"],
        Dialect::Carp => &["let", "let-do"],
        Dialect::Janet => &["let", "with", "with-vars", "when-let", "when-with"],
        Dialect::Fennel => &["let", "with-open"],
        Dialect::CommonLisp
        | Dialect::EmacsLisp
        | Dialect::Lfe
        | Dialect::Scheme
        | Dialect::Racket
        | Dialect::Unknown => &[],
    }
}

/// One binding introduced by a binding form.
struct BindingEntry<'a> {
    /// The node that receives the binding. Kept as a node rather than a name
    /// so the caller can still ask about its reader prefixes, which is the
    /// only place an unquote survives.
    name: &'a ExpressionView,
    /// The form whose value it receives, where the layout has one.
    value: Option<&'a ExpressionView>,
    /// What to report against: the whole `(name value)` pair where one node
    /// covers both, the name alone where the pair is flat and no node does.
    span: ByteSpan,
}

/// The bindings a binding form introduces, or `None` when the form is not a
/// binding form of this dialect or its binder node is in no layout this
/// understands.
///
/// Two layouts exist across the modelled dialects, and reading one as the
/// other is not a near miss. `(let [result 1] …)` read as a list of
/// `(name value)` sub-lists takes `result` and `1` as two separate bindings
/// and reports the integer literal `1` as a capturable variable name.
///
/// The dialect-neutral `scope_shape` in `paredit_core_syntax::dialect::semantic`
/// draws the same distinction (`BinderShape::FlatPairs` against
/// `BinderShape::BindingList`) but cannot be reused here: it resolves a head
/// only on a list carrying no reader prefixes, and the template forms this
/// analysis exists to read are quasiquoted, so every one of them would
/// resolve to nothing.
fn binding_entries<'a>(
    dialect: Dialect,
    form: &'a ExpressionView,
) -> Option<Vec<BindingEntry<'a>>> {
    let head = list_head(form)?;
    let bindings = form.children.get(1)?;

    let binds = bracket_binding_forms(dialect).contains(&head)
        || PAREN_BINDING_FORMS
            .iter()
            .any(|name| head_eq(dialect, head, name));
    if !binds {
        return None;
    }

    // Which of the two layouts to read is decided by the binder node's own
    // delimiter, not by which of the two head tables matched. Deciding it by
    // the head is what reintroduced the flat-vector-as-pairs bug: a bracket
    // dialect's binding head missing from `bracket_binding_forms` — Clojure's
    // `let*`, a real special form — fell through to the paren branch, and its
    // `[name value]` vector was read as a list of `(name value)` sub-lists,
    // reporting the integer literal `1` as a capturable variable name. No
    // modelled dialect writes binding *pairs* inside `[…]`, so reading the
    // delimiter cannot make that mistake however the head tables drift.
    match bindings.delimiter {
        // `[name value name value …]`. A trailing odd child is a binding
        // whose initializer has not been written yet; `chunks_exact` drops
        // it rather than pairing it with nothing.
        Some(Delimiter::Bracket) => Some(
            bindings
                .children
                .chunks_exact(2)
                .map(|pair| BindingEntry {
                    name: &pair[0],
                    value: Some(&pair[1]),
                    span: pair[0].span,
                })
                .collect(),
        ),
        Some(Delimiter::Paren) => Some(
            bindings
                .children
                .iter()
                .filter_map(|binding| match binding.kind {
                    // `(let (a) …)`: a bare name binds with no initializer.
                    ExpressionKind::Atom => Some(BindingEntry {
                        name: binding,
                        value: None,
                        span: binding.span,
                    }),
                    ExpressionKind::List => Some(BindingEntry {
                        name: binding.children.first()?,
                        value: binding.children.get(1),
                        span: binding.span,
                    }),
                    ExpressionKind::Root => None,
                })
                .collect(),
        ),
        // A `{…}` binder is not a layout any modelled dialect has, and an
        // atom in the binder position is a malformed form. Neither is worth
        // a guess.
        Some(Delimiter::Brace) | None => None,
    }
}

/// The dialects whose macro system is unhygienic by default and
/// template/quasiquote-based — the precondition for variable-capture and
/// multiple-evaluation analysis to mean anything.
///
/// Scheme and Racket are the deliberate exclusions. `is_macro_expander_definition`
/// recognizes their `define-syntax` as a macro-expander form too (useful for
/// rename and navigation), but `syntax-rules` hygiene is a language guarantee
/// there, not a discipline the programmer must enforce by hand: counting
/// unquotes or capture-prone bindings in a `syntax-rules` template would be
/// analyzing a mechanism these checks were never written to model, and would
/// misfire on hygienic-by-construction code.
///
/// Exposed for `rule/`, whose rules declare the same set as their
/// [`paredit_core_lint_engine::policy::RuleDialectScope`].
pub(crate) const HYGIENE_MODELLED_DIALECTS: [Dialect; 8] = [
    Dialect::CommonLisp,
    Dialect::EmacsLisp,
    Dialect::Clojure,
    Dialect::Janet,
    Dialect::Hy,
    Dialect::Carp,
    Dialect::Fennel,
    Dialect::Lfe,
];

/// Derived from [`HYGIENE_MODELLED_DIALECTS`] rather than restating it.
///
/// These were an independent array and `matches!` listing the same eight
/// dialects, with nothing binding them: a ninth added to one and not the other
/// would have left the lint rules and this standalone report silently
/// disagreeing about which dialects they cover.
fn dialect_is_hygiene_modelled(dialect: Dialect) -> bool {
    HYGIENE_MODELLED_DIALECTS.contains(&dialect)
}

#[must_use]
pub fn build_macro_hygiene_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<HygieneFinding> {
    let modelled = dialect_is_hygiene_modelled(dialect);
    let mut findings = Vec::new();
    let mut macro_count = 0;

    if modelled {
        for form in &tree.root_view().children {
            let Some(head) = list_head(form) else {
                continue;
            };
            if !is_macro_expander_definition(dialect, head) {
                continue;
            }
            let Some(name) = form.children.get(1).and_then(atom_symbol_text) else {
                continue;
            };
            macro_count += 1;
            analyze(dialect, form, name, &mut findings);
        }
    }

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        findings,
        vec![("macro_count", json!(macro_count))],
    )
}

/// Every hygiene risk in one already-identified macro-expander form.
///
/// Exposed (rather than kept as the private `analyze`) for the lint rules in
/// `rule/`, which need the same analysis this file's own
/// `build_macro_hygiene_report` uses but each report only one risk out of it,
/// through a different surface.
#[must_use]
pub(crate) fn hygiene_findings_in(
    dialect: Dialect,
    form: &ExpressionView,
    name: &str,
) -> Vec<HygieneFinding> {
    let mut findings = Vec::new();
    analyze(dialect, form, name, &mut findings);
    findings
}

fn analyze(
    dialect: Dialect,
    form: &ExpressionView,
    name: &str,
    findings: &mut Vec<HygieneFinding>,
) {
    let macro_name = identity_key(dialect, name).into_owned();
    let parameters = form
        .children
        .get(2)
        .map(|lambda_list| Parameters::of(dialect, lambda_list))
        .unwrap_or_default();

    // Names bound by the macro's own body outside the template — the usual
    // `(let ((var (gensym))) ...)` prelude — are safe by construction, because
    // whatever they hold at expansion time is a fresh symbol.
    let gensym_bound = gensym_bindings(dialect, form);

    for body in form.children.iter().skip(3) {
        find_captures(dialect, body, &macro_name, &gensym_bound, findings);
        find_multiple_evaluation(dialect, body, &macro_name, &parameters, findings);
        find_parameter_reordering(dialect, body, &macro_name, &parameters, findings);
        find_symbol_macrolet_multiple_evaluation(dialect, body, &macro_name, findings);
        find_deep_quasiquote_nesting(dialect, body, &macro_name, findings);
    }

    find_missing_editor_declaration(dialect, form, &macro_name, findings);
}

/// The required and `&body`/`&rest` parameter names of a macro lambda list,
/// kept twice on purpose.
///
/// [`find_parameter_reordering`] compares the template's unquote order against
/// the lambda list's and genuinely needs the order. Every other use is a
/// membership test run once per atom of a full subtree walk, and answering
/// those by scanning the `Vec` was quadratic in the parameter count: a 416 KB
/// template took 0.5 s and an 865 KB one 2.3 s, a clean 4.4x per doubling.
#[derive(Default)]
struct Parameters {
    ordered: Vec<String>,
    present: BTreeSet<String>,
}

impl Parameters {
    fn of(dialect: Dialect, lambda_list: &ExpressionView) -> Self {
        let ordered: Vec<String> = lambda_list
            .children
            .iter()
            .filter_map(atom_symbol_text)
            .filter(|name| !name.starts_with('&'))
            .map(|name| identity_key(dialect, name).into_owned())
            .collect();
        let present = ordered.iter().cloned().collect();
        Self { ordered, present }
    }

    fn contains(&self, key: &str) -> bool {
        self.present.contains(key)
    }
}

/// The forms whose value is a symbol generated fresh at every expansion.
///
/// One shared list used to stand in for all eight dialects, documented as
/// "recognised in every modelled dialect". None of it was: `gentemp` and
/// `copy-symbol` are Common Lisp's alone, and half the other dialects spell
/// their own idiom differently or reach it through a module.
const fn gensym_generators(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::CommonLisp => &["gensym", "gentemp", "make-symbol", "copy-symbol"],
        // `gensym` has been a builtin since Emacs 26; `cl-gensym` is the older
        // `cl-lib` spelling, and `make-symbol` is what a macro reaches for
        // with no `cl-lib` at all.
        Dialect::EmacsLisp => &["gensym", "cl-gensym", "make-symbol"],
        // Clojure, Janet and Fennel each expose a plain `gensym` to macro
        // code. Clojure's and Fennel's `name#` auto-gensym is the far more
        // idiomatic route and is handled by [`is_auto_gensym`] instead.
        Dialect::Clojure | Dialect::Janet | Dialect::Fennel => &["gensym"],
        // Hy moved `gensym` onto the `hy` module; the bare name is what
        // macros written against older Hy still call.
        Dialect::Hy => &["hy.gensym", "gensym"],
        Dialect::Carp => &["gensym", "gensym-with"],
        // LFE: no gensym idiom distinct from `gensym` itself could be
        // confirmed, and this analysis will not invent one. `gensym` is kept
        // because a binding to it can only ever mean "a fresh symbol"; the
        // Common Lisp names above are deliberately not, since LFE's runtime
        // is Erlang's and has none of them.
        Dialect::Lfe => &["gensym"],
        Dialect::Scheme | Dialect::Racket | Dialect::Unknown => &[],
    }
}

/// Whether a template symbol is auto-gensymmed by the dialect's own reader.
///
/// Clojure's syntax-quote and Fennel's backtick both replace a symbol ending
/// in `#` with one generated fresh per expansion, so capture is impossible by
/// construction — this is *the* hygienic idiom in both languages, and
/// reporting it was reporting the fix as the bug. A bare `#` is not one:
/// there is no name in front of it to generate from.
fn is_auto_gensym(dialect: Dialect, name: &str) -> bool {
    matches!(dialect, Dialect::Clojure | Dialect::Fennel) && name.len() > 1 && name.ends_with('#')
}

/// Janet's `(with-syms [a b] …)`, which binds every name in its bracket to a
/// fresh symbol at once. `paredit_core_syntax::dialect::semantic` already
/// models it as a bare-name binder scope for the same reason: there is no
/// per-name initializer to inspect, the head is the whole proof.
const JANET_GENSYM_SCOPE_FORM: &str = "with-syms";

/// Names the macro body binds to a freshly generated symbol.
///
/// Recognised by the initial form's head rather than by the name's spelling:
/// `g!foo` is a convention and `(gensym)` is a proof, and only one of those is
/// something to build a report on.
fn gensym_bindings(dialect: Dialect, form: &ExpressionView) -> BTreeSet<String> {
    let mut bound = BTreeSet::new();
    walk(form, &mut |view| {
        if dialect == Dialect::Janet
            && list_head(view).is_some_and(|head| head == JANET_GENSYM_SCOPE_FORM)
        {
            for name in view
                .children
                .get(1)
                .map(|names| names.children.as_slice())
                .unwrap_or_default()
                .iter()
                .filter_map(atom_symbol_text)
            {
                bound.insert(identity_key(dialect, name).into_owned());
            }
            return;
        }

        let Some(entries) = binding_entries(dialect, view) else {
            return;
        };
        for entry in entries {
            let Some(name) = atom_symbol_text(entry.name) else {
                continue;
            };
            let generated = entry.value.and_then(list_head).is_some_and(|head| {
                gensym_generators(dialect)
                    .iter()
                    .any(|generator| head_eq(dialect, head, generator))
            });
            if generated {
                bound.insert(identity_key(dialect, name).into_owned());
            }
        }
    });
    bound
}

/// Reports every literal binding inside a quasiquoted template.
fn find_captures(
    dialect: Dialect,
    view: &ExpressionView,
    macro_name: &str,
    gensym_bound: &BTreeSet<String>,
    findings: &mut Vec<HygieneFinding>,
) {
    if !is_quasiquoted(dialect, view) {
        for child in &view.children {
            find_captures(dialect, child, macro_name, gensym_bound, findings);
        }
        return;
    }

    walk_emitted_code(dialect, view, &mut |inner| {
        let Some(entries) = binding_entries(dialect, inner) else {
            return;
        };
        for entry in entries {
            // An unquoted binding name — `(let ((,var ...)) …)` — is whatever
            // the macro computed, which is the `gensym` idiom spelled at the
            // use site rather than the binding site. The unquote is read off
            // the node's reader prefixes, not off its text: `atom_symbol_text`
            // deliberately steps past the prefix, so `,var` and `var` are the
            // same string by the time a name is in hand.
            if is_unquoted(dialect, entry.name) {
                continue;
            }
            let Some(name) = atom_symbol_text(entry.name) else {
                continue;
            };
            if is_auto_gensym(dialect, name) {
                continue;
            }
            let key = identity_key(dialect, name);
            if gensym_bound.contains(key.as_ref()) {
                continue;
            }
            findings.push(HygieneFinding {
                risk: HygieneRisk::VariableCapture,
                macro_name: macro_name.to_owned(),
                subject: key.into_owned(),
                occurrences: 0,
                remedy: "bind the name to (gensym) outside the template and unquote it",
                span: entry.span,
            });
        }
    });
}

/// Reports every parameter a template unquotes more than once.
fn find_multiple_evaluation(
    dialect: Dialect,
    view: &ExpressionView,
    macro_name: &str,
    parameters: &Parameters,
    findings: &mut Vec<HygieneFinding>,
) {
    if !is_quasiquoted(dialect, view) {
        for child in &view.children {
            find_multiple_evaluation(dialect, child, macro_name, parameters, findings);
        }
        return;
    }

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    // Quoted subtrees are walked here, unlike in [`find_captures`]: `quote`
    // does not stop the backquote reader, so `` `(list '(,x) ,x) `` really
    // does evaluate the caller's argument form twice.
    walk(view, &mut |inner| {
        if inner.kind != ExpressionKind::Atom {
            return;
        }
        // `,@x` splices a list and evaluates it once, however many forms come
        // out, so only the plain `,x` unquote is counted. Both are read off the
        // reader prefixes rather than the text, which no longer carries them.
        if !inner.reader_prefixes.contains(&ReaderPrefix::Unquote) {
            return;
        }
        let Some(name) = atom_symbol_text(inner) else {
            return;
        };
        let key = identity_key(dialect, name);
        if !parameters.contains(key.as_ref()) {
            return;
        }
        match counts.get_mut(key.as_ref()) {
            Some(count) => *count += 1,
            None => {
                counts.insert(key.into_owned(), 1);
            }
        }
    });

    for (name, occurrences) in counts {
        if occurrences < 2 {
            continue;
        }
        findings.push(HygieneFinding {
            risk: HygieneRisk::MultipleEvaluation,
            macro_name: macro_name.to_owned(),
            subject: name,
            occurrences,
            remedy: "bind the argument once in the expansion (the once-only idiom)",
            span: view.span,
        });
    }
}

/// Reports a template that unquotes two or more distinct parameters in an
/// order that differs from their left-to-right position in the lambda list.
///
/// Only the first unquote of each distinct parameter fixes that parameter's
/// place in the template's order — a parameter [`find_multiple_evaluation`]
/// already flags as unquoted more than once does not need flagging twice
/// here for the same reason. A single parameter, however many times it
/// repeats, is trivially "in order" with itself, so at least two distinct
/// parameters must appear before an order even exists to compare.
fn find_parameter_reordering(
    dialect: Dialect,
    view: &ExpressionView,
    macro_name: &str,
    parameters: &Parameters,
    findings: &mut Vec<HygieneFinding>,
) {
    if !is_quasiquoted(dialect, view) {
        for child in &view.children {
            find_parameter_reordering(dialect, child, macro_name, parameters, findings);
        }
        return;
    }

    // `template_order` carries the order and `unquoted` answers "have I seen
    // this one already?", which is the same split [`Parameters`] makes and for
    // the same reason: both questions used to be answered by scanning the
    // `Vec`, once per atom of the walk below and again once per parameter.
    let mut template_order: Vec<String> = Vec::new();
    let mut unquoted: BTreeSet<String> = BTreeSet::new();
    walk(view, &mut |inner| {
        if inner.kind != ExpressionKind::Atom {
            return;
        }
        if !inner.reader_prefixes.contains(&ReaderPrefix::Unquote) {
            return;
        }
        let Some(name) = atom_symbol_text(inner) else {
            return;
        };
        let key = identity_key(dialect, name);
        if !parameters.contains(key.as_ref()) || unquoted.contains(key.as_ref()) {
            return;
        }
        let key = key.into_owned();
        unquoted.insert(key.clone());
        template_order.push(key);
    });

    if template_order.len() < 2 {
        return;
    }

    // The lambda list's own order, restricted to the parameters the
    // template actually unquotes: this is what "in order" means here, since
    // a parameter the template never mentions has nothing to be out of
    // order with.
    let lambda_list_order: Vec<&String> = parameters
        .ordered
        .iter()
        .filter(|parameter| unquoted.contains(parameter.as_str()))
        .collect();

    if template_order.iter().collect::<Vec<_>>() == lambda_list_order {
        return;
    }

    findings.push(HygieneFinding {
        risk: HygieneRisk::ParameterReordering,
        macro_name: macro_name.to_owned(),
        subject: template_order.join(", "),
        occurrences: template_order.len(),
        remedy: "the template evaluates these argument forms in a different order than the \
                 call site writes them; reorder the template or bind each argument in the \
                 call's own order before using it",
        span: view.span,
    });
}

/// `symbol-macrolet`/`cl-symbol-macrolet`'s special-form name.
///
/// These are not in [`PAREN_BINDING_FORMS`]: a `let` binding evaluates its value
/// once and binds a name to the result, while a `symbol-macrolet` binding
/// substitutes a fresh copy of the expansion form at every reference — a
/// `let`'s capture risk does not apply to it, and a different risk does.
const SYMBOL_MACROLET_FORMS: [&str; 2] = ["symbol-macrolet", "cl-symbol-macrolet"];

/// Reports a `symbol-macrolet` expansion form that is not provably
/// side-effect-free and is referenced more than once in the form's body.
///
/// Every reference substitutes another copy of the expansion form, so
/// `(symbol-macrolet ((x (incf y))) (+ x x))` runs `(incf y)` twice — the
/// same failure [`find_multiple_evaluation`] reports for a template that
/// unquotes one macro parameter twice, just reached through a different
/// binding form. Not gated on being inside a quasiquoted template: a
/// `symbol-macrolet` in the macro's own generating code is exactly as live a
/// risk as one written into the template it returns.
fn find_symbol_macrolet_multiple_evaluation(
    dialect: Dialect,
    view: &ExpressionView,
    macro_name: &str,
    findings: &mut Vec<HygieneFinding>,
) {
    walk(view, &mut |inner| {
        let Some(head) = list_head(inner) else { return };
        if !SYMBOL_MACROLET_FORMS
            .iter()
            .any(|form_name| head_eq(dialect, head, form_name))
        {
            return;
        }

        // Every binding asks the same question of the same body, so the body
        // is counted once for the whole form; counting per binding made this
        // O(bindings x body). Deferred rather than eager so a form whose
        // expansions are all side-effect-free still walks nothing.
        let mut body_symbols: Option<BTreeMap<String, usize>> = None;

        for binding in inner
            .children
            .get(1)
            .map(|list| list.children.as_slice())
            .unwrap_or_default()
        {
            let Some(name) = binding.children.first().and_then(atom_symbol_text) else {
                continue;
            };
            let Some(expansion) = binding.children.get(1) else {
                continue;
            };
            if is_side_effect_free(dialect, expansion) {
                continue;
            }
            let key = identity_key(dialect, name);

            let counts = body_symbols.get_or_insert_with(|| scoped_symbol_counts(dialect, inner));
            let occurrences = counts.get(key.as_ref()).copied().unwrap_or(0);
            if occurrences < 2 {
                continue;
            }

            findings.push(HygieneFinding {
                risk: HygieneRisk::MultipleEvaluation,
                macro_name: macro_name.to_owned(),
                subject: key.into_owned(),
                occurrences,
                remedy: "this symbol-macrolet expansion is not side-effect-free; bind its value \
                         once instead of letting each reference re-evaluate it",
                span: binding.span,
            });
        }
    });
}

/// How many times each symbol appears in `form`'s body — everything after
/// the head and the binding list — folded the way the reader folds it.
fn scoped_symbol_counts(dialect: Dialect, form: &ExpressionView) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for scoped_body in form.children.iter().skip(2) {
        walk(scoped_body, &mut |node| {
            if node.kind != ExpressionKind::Atom {
                return;
            }
            let Some(text) = atom_symbol_text(node) else {
                return;
            };
            let key = identity_key(dialect, text);
            match counts.get_mut(key.as_ref()) {
                Some(count) => *count += 1,
                None => {
                    counts.insert(key.into_owned(), 1);
                }
            }
        });
    }
    counts
}

/// Whether a form's evaluation is guaranteed to have no side effect: a
/// literal atom (a self-evaluating value or a variable reference — neither
/// calls anything), or a quoted form (`quote` never evaluates its argument).
/// Anything else — a function call, `incf`, and the like — is not, since
/// this analysis has no way to know what the call does.
fn is_side_effect_free(dialect: Dialect, view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom || is_quoted(dialect, view)
}

/// Whether a form is quoted, by reader prefix, by folded-in spelling, or by
/// an explicit `quote` head.
fn is_quoted(dialect: Dialect, view: &ExpressionView) -> bool {
    view.reader_prefixes.contains(&ReaderPrefix::Quote)
        || text_starts_with(view, reader_chars(dialect).quote)
        || list_head(view).is_some_and(|head| head_eq(dialect, head, "quote"))
}

/// Walks the code a template emits, skipping quoted subtrees that hold no
/// unquote.
///
/// A quoted form with nothing unquoted beneath it is constant data the macro
/// hands to the expansion, not code spliced into the caller's scope: the `let`
/// in `` `(process '(let ((result 1)) result) ,x) `` binds nothing at the call
/// site and so captures nothing there.
///
/// A quoted form that *does* contain an unquote is the opposite case, and
/// skipping it was a regression. `quote` does not stop the backquote reader —
/// the same fact [`find_multiple_evaluation`] states for its own walk — so
/// `` `(eval '(let ((result ,f)) (list result))) `` is a template being
/// assembled at expansion time, and its `result` shadows the caller's exactly
/// as an unquoted one would.
fn walk_emitted_code(
    dialect: Dialect,
    view: &ExpressionView,
    visit: &mut impl FnMut(&ExpressionView),
) {
    if is_quoted(dialect, view) && !contains_unquote(dialect, view) {
        return;
    }
    visit(view);
    for child in &view.children {
        walk_emitted_code(dialect, child, visit);
    }
}

/// Whether an unquote or unquote-splicing appears at or anywhere beneath
/// `view` — the test that separates quoted *data* from a quoted subtree that
/// is really code under assembly.
fn contains_unquote(dialect: Dialect, view: &ExpressionView) -> bool {
    is_unquoted(dialect, view)
        || view
            .children
            .iter()
            .any(|child| contains_unquote(dialect, child))
}

/// Reports a template that nests three or more levels of quasiquote.
///
/// Unlike the other checks in this file, this one does not stop recursing
/// once it finds the outermost quasiquote of a template: [`quasiquote_depth`]
/// already measures the deepest nesting anywhere under a node in one pass, so
/// there is nothing further inside that same template worth a second,
/// separate report.
fn find_deep_quasiquote_nesting(
    dialect: Dialect,
    view: &ExpressionView,
    macro_name: &str,
    findings: &mut Vec<HygieneFinding>,
) {
    if !is_quasiquoted(dialect, view) {
        for child in &view.children {
            find_deep_quasiquote_nesting(dialect, child, macro_name, findings);
        }
        return;
    }

    let depth = quasiquote_depth(dialect, view);
    if depth < 3 {
        return;
    }

    findings.push(HygieneFinding {
        risk: HygieneRisk::DeepQuasiquoteNesting,
        macro_name: macro_name.to_owned(),
        subject: String::new(),
        occurrences: depth,
        remedy: "three or more nested backquote levels are hard to reason about and easy to get \
                 an escape wrong in; consider splitting this into helper macros or functions to \
                 reduce the nesting",
        span: view.span,
    });
}

/// The deepest quasiquote nesting reachable from `view`, counting `view`'s
/// own backquote(s) (a node can carry more than one, when several backquotes
/// appear contiguously with nothing between them) plus whatever is nested
/// inside it.
fn quasiquote_depth(dialect: Dialect, view: &ExpressionView) -> usize {
    // One pass over `reader_prefixes`: a zero count is exactly the case the
    // text fallback exists for, so the separate `contains` guard the count
    // already subsumes is gone.
    let prefixed = view
        .reader_prefixes
        .iter()
        .filter(|prefix| **prefix == ReaderPrefix::Quasiquote)
        .count();
    let own = if prefixed == 0 {
        usize::from(text_starts_with(view, reader_chars(dialect).quasiquote))
    } else {
        prefixed
    };
    let deepest_child = view
        .children
        .iter()
        .map(|child| quasiquote_depth(dialect, child))
        .max()
        .unwrap_or(0);
    own + deepest_child
}

/// Emacs Lisp only: reports a macro whose body does not open with a
/// `(declare (indent ...))` and/or `(declare (debug ...))` form.
///
/// A leading docstring is skipped, since `(declare ...)` conventionally
/// follows it rather than the arglist directly — `(defmacro m (x) "doc"
/// (declare (indent 1)) ...)` is the ordinary shape a documented macro
/// takes, not a missing declaration.
fn find_missing_editor_declaration(
    dialect: Dialect,
    form: &ExpressionView,
    macro_name: &str,
    findings: &mut Vec<HygieneFinding>,
) {
    if dialect != Dialect::EmacsLisp {
        return;
    }

    let first_body_index = match form.children.get(3) {
        Some(child) if is_string_literal(child) => 4,
        _ => 3,
    };

    let has_declaration = form
        .children
        .get(first_body_index)
        .and_then(|first_body| {
            let head = list_head(first_body)?;
            head_eq(dialect, head, "declare").then_some(first_body)
        })
        .is_some_and(|declare_form| {
            declare_form.children.iter().skip(1).any(|clause| {
                list_head(clause).is_some_and(|clause_head| {
                    head_eq(dialect, clause_head, "indent")
                        || head_eq(dialect, clause_head, "debug")
                })
            })
        });

    if has_declaration {
        return;
    }

    findings.push(HygieneFinding {
        risk: HygieneRisk::MissingEditorDeclaration,
        macro_name: macro_name.to_owned(),
        subject: String::new(),
        occurrences: 0,
        remedy: "add a leading (declare (indent ...)) and/or (declare (debug ...)) form so \
                 Emacs indents and Edebug instruments calls to this macro correctly",
        span: form.span,
    });
}

/// Whether a node is a string literal atom.
fn is_string_literal(view: &ExpressionView) -> bool {
    view.kind == ExpressionKind::Atom
        && view
            .text
            .as_deref()
            .is_some_and(|text| text.starts_with('"'))
}

fn is_unquoted(dialect: Dialect, view: &ExpressionView) -> bool {
    view.reader_prefixes.contains(&ReaderPrefix::Unquote)
        || view
            .reader_prefixes
            .contains(&ReaderPrefix::UnquoteSplicing)
        || view.text.as_deref().is_some_and(|text| {
            reader_chars(dialect)
                .unquote
                .iter()
                .any(|character| text.starts_with(*character))
        })
}

/// Whether a node is quasiquoted, by prefix node or by folded-in spelling.
///
/// Both spellings occur: a quasiquote before a list becomes a reader prefix,
/// while one consumed into an atom leaves the character in the text. Which
/// character that is comes from [`reader_chars`] and not from Common Lisp:
/// Janet's is `~`, and a Janet backquote opens a long string.
fn is_quasiquoted(dialect: Dialect, view: &ExpressionView) -> bool {
    view.reader_prefixes.contains(&ReaderPrefix::Quasiquote)
        || text_starts_with(view, reader_chars(dialect).quasiquote)
}

fn walk(view: &ExpressionView, visit: &mut impl FnMut(&ExpressionView)) {
    visit(view);
    for child in &view.children {
        walk(child, visit);
    }
}

#[cfg(test)]
mod tests {
    use paredit_core_syntax::dialect::{BinderShape, ScopeShape};

    use super::*;

    fn report(source: &str) -> FileFindings<HygieneFinding> {
        report_in(source, Dialect::CommonLisp)
    }

    fn report_in(source: &str, dialect: Dialect) -> FileFindings<HygieneFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        build_macro_hygiene_report(Path::new("t.lisp"), dialect, &tree)
    }

    #[test]
    fn a_literal_binding_in_a_template_is_a_capture_risk() {
        let report = report("(defmacro m (form) `(let ((result ,form)) (list result)))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].risk, HygieneRisk::VariableCapture);
        assert_eq!(report.findings[0].subject, "RESULT");
    }

    #[test]
    fn a_gensym_bound_name_is_not_a_capture_risk() {
        let report = report(
            "(defmacro m (form) (let ((result (gensym))) `(let ((,result ,form)) (list ,result))))",
        );
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_unquoted_binding_name_is_not_a_capture_risk() {
        let report = report("(defmacro m (var form) `(let ((,var ,form)) ,var))");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::VariableCapture),
            "{report:?}"
        );
    }

    #[test]
    fn a_parameter_unquoted_twice_is_a_multiple_evaluation_risk() {
        let report = report("(defmacro m (x) `(if (> ,x 0) ,x 0))");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.risk == HygieneRisk::MultipleEvaluation)
            .expect("multiple evaluation is reported");
        assert_eq!(finding.subject, "X");
        assert_eq!(finding.occurrences, 2);
    }

    #[test]
    fn a_parameter_unquoted_once_is_not_reported() {
        let report = report("(defmacro m (x) `(list ,x))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_splicing_unquote_evaluates_once_however_many_forms_it_produces() {
        let report = report("(defmacro m (&body body) `(progn ,@body ,@body))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_name_that_is_not_a_parameter_is_not_counted_for_multiple_evaluation() {
        let report = report("(defmacro m (x) `(list ,x ,*global* ,*global*))");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::MultipleEvaluation),
            "{report:?}"
        );
    }

    #[test]
    fn two_parameters_unquoted_out_of_lambda_list_order_is_a_reordering_risk() {
        let report = report("(defmacro m (a b) `(list ,b ,a))");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.risk == HygieneRisk::ParameterReordering)
            .expect("parameter reordering is reported");
        assert_eq!(finding.subject, "B, A");
        assert_eq!(finding.occurrences, 2);
    }

    #[test]
    fn two_parameters_unquoted_in_lambda_list_order_is_not_reported() {
        let report = report("(defmacro m (a b) `(list ,a ,b))");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::ParameterReordering),
            "{report:?}"
        );
    }

    #[test]
    fn three_parameters_partially_reordered_still_report() {
        // `a` and `c` stay in order; `b` jumps ahead of both.
        let report = report("(defmacro m (a b c) `(list ,b ,a ,c))");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.risk == HygieneRisk::ParameterReordering)
            .expect("parameter reordering is reported");
        assert_eq!(finding.subject, "B, A, C");
    }

    #[test]
    fn a_single_parameter_repeated_is_not_a_reordering_risk() {
        // Only one distinct parameter, however many times it repeats, has no
        // order to be out of; this stays a multiple-evaluation finding only.
        let report = report("(defmacro m (x) `(if (> ,x 0) ,x 0))");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::ParameterReordering),
            "{report:?}"
        );
    }

    #[test]
    fn a_parameter_the_template_never_mentions_does_not_block_reordering() {
        // `a` and `b` are reordered; `c` is never unquoted at all and so has
        // no bearing on whether `a`/`b`'s order is a violation.
        let report = report("(defmacro m (a b c) `(list ,b ,a))");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.risk == HygieneRisk::ParameterReordering)
            .expect("parameter reordering is reported");
        assert_eq!(finding.subject, "B, A");
    }

    #[test]
    fn a_symbol_macrolet_expansion_with_a_side_effect_referenced_twice_is_reported() {
        let report = report("(defmacro m () (symbol-macrolet ((x (incf y))) (+ x x)))");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.risk == HygieneRisk::MultipleEvaluation)
            .expect("multiple evaluation is reported");
        assert_eq!(finding.subject, "X");
        assert_eq!(finding.occurrences, 2);
    }

    #[test]
    fn cl_symbol_macrolet_is_recognised_in_emacs_lisp() {
        let report = report_in(
            "(defmacro m () (cl-symbol-macrolet ((x (incf y))) (+ x x)))",
            Dialect::EmacsLisp,
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.risk == HygieneRisk::MultipleEvaluation
                    && finding.subject == "x"),
            "{report:?}"
        );
    }

    #[test]
    fn a_symbol_macrolet_expansion_referenced_once_is_not_reported() {
        let report = report("(defmacro m () (symbol-macrolet ((x (incf y))) x))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_symbol_macrolet_expansion_that_is_a_literal_atom_is_not_reported() {
        let report = report("(defmacro m () (symbol-macrolet ((x 5)) (+ x x)))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_symbol_macrolet_expansion_that_is_quoted_is_not_reported() {
        let report = report("(defmacro m () (symbol-macrolet ((x '(1 2 3))) (+ (car x) (car x))))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn three_nested_backquote_levels_is_reported() {
        let report = report("(defmacro m () ```(a))");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.risk == HygieneRisk::DeepQuasiquoteNesting)
            .expect("deep quasiquote nesting is reported");
        assert_eq!(finding.occurrences, 3);
    }

    #[test]
    fn separately_nested_backquotes_across_a_list_boundary_still_count() {
        let report = report("(defmacro m () `(a `(b `(c))))");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.risk == HygieneRisk::DeepQuasiquoteNesting)
            .expect("deep quasiquote nesting is reported");
        assert_eq!(finding.occurrences, 3);
    }

    #[test]
    fn two_nested_backquote_levels_is_not_reported() {
        let report = report("(defmacro m () ``(a))");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::DeepQuasiquoteNesting),
            "{report:?}"
        );
    }

    #[test]
    fn a_single_quasiquote_is_not_reported() {
        let report = report("(defmacro m (x) `(list ,x))");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::DeepQuasiquoteNesting),
            "{report:?}"
        );
    }

    #[test]
    fn a_binding_outside_a_template_is_the_macros_own_and_is_not_reported() {
        let report = report("(defmacro m (form) (let ((result form)) `(list ,result)))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn the_macro_count_is_reported_even_when_nothing_is_wrong() {
        let report = report("(defmacro m (x) `(list ,x))");
        assert_eq!(report.summary, vec![("macro_count", json!(1))]);
    }

    #[test]
    fn a_defun_is_not_a_macro() {
        let report = report("(defun f (form) (let ((result form)) result))");
        assert_eq!(report.summary, vec![("macro_count", json!(0))]);
    }

    #[test]
    fn clojure_is_now_a_modelled_dialect() {
        // Clojure's `defmacro` is unhygienic by default and template-based,
        // just like Common Lisp's, so it is modelled rather than reported as
        // unknown territory.
        let report = report_in("(defmacro m [x] x)", Dialect::Clojure);
        assert!(report.dialect_modelled);
        assert_eq!(report.summary, vec![("macro_count", json!(1))]);
    }

    #[test]
    fn scheme_and_racket_say_so_rather_than_reporting_nothing() {
        // `define-syntax`/`syntax-rules` hygiene is a language guarantee in
        // both dialects, so unquote-counting and capture-checking do not
        // apply here even though `is_macro_expander_definition` recognizes
        // the form for other purposes (rename, navigation).
        for dialect in [Dialect::Scheme, Dialect::Racket] {
            let source = "(define-syntax m (syntax-rules () ((_ x) (let ((result x)) result))))";
            let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
            let report = build_macro_hygiene_report(Path::new("t.scm"), dialect, &tree);
            assert!(!report.dialect_modelled, "{dialect:?}");
            assert!(report.findings.is_empty(), "{dialect:?}");
        }
    }

    /// Every modelled dialect, its own `let` written the way that language
    /// really writes it: a paired `((name value))` list for Common Lisp,
    /// Emacs Lisp and LFE, a flat `[name value]` vector for the five bracket
    /// dialects. These fixtures used to spell every dialect's binding list in
    /// Common Lisp's paired shape, which is not valid syntax in any of the
    /// five and passed only because paredit parses s-expressions rather than
    /// the target language.
    ///
    /// The quasiquote and unquote characters are each dialect's own as far as
    /// `paredit_core_syntax::sexpr::reader_policy` models them: Janet's
    /// quasiquote is `~` and its unquote `,`, Clojure's are `` ` `` and `~`.
    /// Hy and Carp read through the legacy prefix table, which gives them
    /// `` ` ``/`,` — real Hy unquotes with `~` and real Carp with `%`, but
    /// that gap is the reader's, not this report's, and a fixture written
    /// against the real characters would not parse as an unquote here.
    const CAPTURE_FIXTURES: [(Dialect, &str, &str); 8] = [
        (
            Dialect::CommonLisp,
            "(defmacro m (form) `(let ((result ,form)) (list result)))",
            "RESULT",
        ),
        (
            Dialect::EmacsLisp,
            "(defmacro m (form) `(let ((result ,form)) (list result)))",
            "result",
        ),
        (
            Dialect::Lfe,
            "(defmacro m (form) `(let ((result ,form)) (list result)))",
            "result",
        ),
        (
            Dialect::Clojure,
            "(defmacro m [form] `(let [result ~form] (list result)))",
            "result",
        ),
        (
            Dialect::Janet,
            "(defmacro m [form] ~(let [result ,form] (list result)))",
            "result",
        ),
        (
            Dialect::Hy,
            "(defmacro m [form] `(let [result ,form] (list result)))",
            "result",
        ),
        (
            Dialect::Carp,
            "(defmacro m [form] `(let [result ,form] (list result)))",
            "result",
        ),
        (
            Dialect::Fennel,
            "(macro m [form] `(let [result ,form] (list result)))",
            "result",
        ),
    ];

    #[test]
    fn a_literal_binding_in_a_template_is_a_capture_risk_in_every_modelled_dialect() {
        for (dialect, source, subject) in CAPTURE_FIXTURES {
            let report = report_in(source, dialect);
            // Filtered to `VariableCapture` rather than asserting the whole
            // findings list is exactly one: Emacs Lisp also reports
            // `MissingEditorDeclaration` for a macro with no leading
            // `declare`, which every fixture here has, and that is a
            // separate concern from this test.
            let captures: Vec<_> = report
                .findings
                .iter()
                .filter(|finding| finding.risk == HygieneRisk::VariableCapture)
                .collect();
            assert_eq!(captures.len(), 1, "{dialect:?}: {report:?}");
            assert_eq!(captures[0].subject, subject, "{dialect:?}");
        }
    }

    /// The same fixtures, with the binding name unquoted (the gensym-at-the-
    /// use-site idiom), report nothing in every modelled dialect.
    #[test]
    fn an_unquoted_binding_name_is_not_a_capture_risk_in_every_modelled_dialect() {
        for (dialect, source) in [
            (
                Dialect::CommonLisp,
                "(defmacro m (var form) `(let ((,var ,form)) ,var))",
            ),
            (
                Dialect::EmacsLisp,
                "(defmacro m (var form) `(let ((,var ,form)) ,var))",
            ),
            (
                Dialect::Lfe,
                "(defmacro m (var form) `(let ((,var ,form)) ,var))",
            ),
            (
                Dialect::Clojure,
                "(defmacro m [var form] `(let [~var ~form] ~var))",
            ),
            (
                Dialect::Janet,
                "(defmacro m [var form] ~(let [,var ,form] ,var))",
            ),
            (
                Dialect::Hy,
                "(defmacro m [var form] `(let [,var ,form] ,var))",
            ),
            // Carp has no row here. It used to, spelling its unquote `,`, and
            // it passed only because Carp then shared the permissive legacy
            // reader, which read `,` as unquote in every dialect it served.
            // Carp's unquote is `%` and its unquote-splicing is `%@`
            // (`docs/Quasiquotation.md`; `src/Parsing.hs` dispatches `'%' ->
            // try unquoteSplicing <|> unquote`), and `,` is plain whitespace
            // there -- `emptyCharacters` lists it beside space and tab. So the
            // row asserted a property of a dialect that does not exist.
            //
            // `%` is not yet a reader prefix here, so there is currently no
            // spelling of "an unquoted binding name" in Carp for this test to
            // use: a `%var` written today reads as the single atom `%var`,
            // which would satisfy the assertion without exercising it. The row
            // returns when `%`/`%@` are modelled.
            (
                Dialect::Fennel,
                "(macro m [var form] `(let [,var ,form] ,var))",
            ),
        ] {
            let report = report_in(source, dialect);
            assert!(
                report
                    .findings
                    .iter()
                    .all(|finding| finding.risk != HygieneRisk::VariableCapture),
                "{dialect:?}: {report:?}"
            );
        }
    }

    #[test]
    fn cl_gensym_bound_name_is_not_a_capture_risk_in_emacs_lisp() {
        let report = report_in(
            "(defmacro m (form) (declare (indent 1)) (let ((result (cl-gensym))) `(let ((,result ,form)) (list ,result))))",
            Dialect::EmacsLisp,
        );
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn make_symbol_bound_name_is_not_a_capture_risk_in_emacs_lisp() {
        let report = report_in(
            "(defmacro m (form) (declare (indent 1)) (let ((result (make-symbol \"result\"))) `(let ((,result ,form)) (list ,result))))",
            Dialect::EmacsLisp,
        );
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn cl_gensym_is_not_recognised_outside_emacs_lisp() {
        // `cl-gensym` is an Emacs Lisp `cl-lib` idiom. A binding to it in any
        // other dialect is not a proof of freshness there, so it must still
        // be reported as a capture risk.
        let report = report(
            "(defmacro m (form) (let ((result (cl-gensym))) `(let ((result ,form)) (list result))))",
        );
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].risk, HygieneRisk::VariableCapture);
    }

    #[test]
    fn an_emacs_lisp_macro_with_no_leading_declare_is_reported() {
        let report = report_in("(defmacro m (x) (list 'progn x))", Dialect::EmacsLisp);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.risk == HygieneRisk::MissingEditorDeclaration),
            "{report:?}"
        );
    }

    #[test]
    fn an_emacs_lisp_macro_with_a_leading_indent_declare_is_not_reported() {
        let report = report_in(
            "(defmacro m (x) (declare (indent 1)) (list 'progn x))",
            Dialect::EmacsLisp,
        );
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::MissingEditorDeclaration),
            "{report:?}"
        );
    }

    #[test]
    fn an_emacs_lisp_macro_with_a_leading_debug_declare_is_not_reported() {
        let report = report_in(
            "(defmacro m (x) (declare (debug t)) (list 'progn x))",
            Dialect::EmacsLisp,
        );
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::MissingEditorDeclaration),
            "{report:?}"
        );
    }

    #[test]
    fn an_emacs_lisp_macros_declare_may_follow_a_docstring() {
        let report = report_in(
            "(defmacro m (x) \"Docstring.\" (declare (indent 1)) (list 'progn x))",
            Dialect::EmacsLisp,
        );
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::MissingEditorDeclaration),
            "{report:?}"
        );
    }

    /// Only Common Lisp's reader folds case, so only there may a finding
    /// report a spelling the source does not contain.
    #[test]
    fn a_case_sensitive_dialect_reports_the_source_spelling() {
        let report = report_in(
            "(defmacro m (form) `(let ((Result ,form)) (foo result Result)))",
            Dialect::Lfe,
        );
        let captures: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.risk == HygieneRisk::VariableCapture)
            .collect();
        assert_eq!(captures.len(), 1, "{report:?}");
        assert_eq!(captures[0].subject, "Result");
        assert_eq!(captures[0].macro_name, "m");
    }

    #[test]
    fn a_case_sensitive_dialect_does_not_let_one_spelling_vouch_for_another() {
        // The prelude proves `result` is fresh. It says nothing about
        // `Result`, which is a different name in LFE.
        let report = report_in(
            "(defmacro m (form) (let ((result (gensym))) `(let ((Result ,form)) (list Result))))",
            Dialect::Lfe,
        );
        let captures: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.risk == HygieneRisk::VariableCapture)
            .collect();
        assert_eq!(captures.len(), 1, "{report:?}");
        assert_eq!(captures[0].subject, "Result");
    }

    /// A flat `[name value]` vector read as a list of `(name value)` pairs
    /// reports the *value* as a second captured name: the integer literal `1`
    /// used to arrive as a capturable variable in all five bracket dialects.
    #[test]
    fn a_bracket_binding_vector_reports_the_name_and_never_the_value() {
        for (dialect, source) in [
            (
                Dialect::Clojure,
                "(defmacro m [form] `(let [result 1] (list result result)))",
            ),
            (
                Dialect::Janet,
                "(defmacro m [form] ~(let [result 1] [result result]))",
            ),
            (
                Dialect::Hy,
                "(defmacro m [form] `(let [result 1] [result result]))",
            ),
            (
                Dialect::Carp,
                "(defmacro m [form] `(let [result 1] [result result]))",
            ),
            (
                Dialect::Fennel,
                "(macro m [form] `(let [result 1] [result result]))",
            ),
        ] {
            let report = report_in(source, dialect);
            let captures: Vec<_> = report
                .findings
                .iter()
                .filter(|finding| finding.risk == HygieneRisk::VariableCapture)
                .collect();
            assert_eq!(captures.len(), 1, "{dialect:?}: {report:?}");
            assert_eq!(captures[0].subject, "result", "{dialect:?}");
        }
    }

    #[test]
    fn a_bracket_dialect_gensym_prelude_suppresses_the_capture() {
        let report = report_in(
            "(defmacro m [form] (let [g (gensym)] `(let [g ~form] (list g))))",
            Dialect::Clojure,
        );
        assert!(report.findings.is_empty(), "{report:?}");
    }

    /// Clojure's and Fennel's `name#` is replaced by a symbol generated fresh
    /// per expansion, which makes it the hygienic construction rather than a
    /// capture risk.
    #[test]
    fn an_auto_gensym_suffix_is_not_a_capture_risk() {
        for (dialect, source) in [
            (
                Dialect::Clojure,
                "(defmacro hygienic-auto [form] `(let [result# ~form] [result# result#]))",
            ),
            (
                Dialect::Fennel,
                "(macro hygienic-auto [form] `(let [result# ,form] [result# result#]))",
            ),
        ] {
            let report = report_in(source, dialect);
            assert!(report.findings.is_empty(), "{dialect:?}: {report:?}");
        }
    }

    #[test]
    fn an_auto_gensym_suffix_means_nothing_in_a_dialect_without_the_idiom() {
        // Hy has no `name#` reader rule, so the suffix is part of an ordinary
        // name and binding it captures exactly as any other name would.
        let report = report_in(
            "(defmacro m [form] `(let [result# ,form] (list result#)))",
            Dialect::Hy,
        );
        let captures: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.risk == HygieneRisk::VariableCapture)
            .collect();
        assert_eq!(captures.len(), 1, "{report:?}");
        assert_eq!(captures[0].subject, "result#");
    }

    #[test]
    fn janets_with_syms_is_a_gensym_prelude() {
        let report = report_in(
            "(defmacro m [form] (with-syms [g] ~(let [g ,form] (list g))))",
            Dialect::Janet,
        );
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn hys_module_qualified_gensym_is_recognised() {
        let report = report_in(
            "(defmacro m [form] (let [g (hy.gensym)] `(let [g ,form] (list g))))",
            Dialect::Hy,
        );
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_common_lisp_only_gensym_idiom_is_not_recognised_in_a_dialect_without_it() {
        // `gentemp` is Common Lisp's alone. A binding to it in Clojure proves
        // nothing about freshness there, exactly as `cl-gensym` proves
        // nothing outside Emacs Lisp.
        let report = report_in(
            "(defmacro m [form] (let [g (gentemp)] `(let [g ~form] (list g))))",
            Dialect::Clojure,
        );
        let captures: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.risk == HygieneRisk::VariableCapture)
            .collect();
        assert_eq!(captures.len(), 1, "{report:?}");
        assert_eq!(captures[0].subject, "g");
    }

    /// A quoted form inside a template is data the macro emits. It binds
    /// nothing in the caller's scope, so nothing under it can capture.
    #[test]
    fn a_quoted_literal_in_a_template_is_not_a_capture() {
        for source in [
            "(defmacro emits-literal (x) `(process (quote (let ((result 1)) result)) ,x))",
            "(defmacro emits-literal (x) `(process '(let ((result 1)) result) ,x))",
        ] {
            let report = report(source);
            assert!(report.findings.is_empty(), "{source}: {report:?}");
        }
    }

    #[test]
    fn a_binding_beside_quoted_data_is_still_a_capture() {
        // The quote-skipping walk must skip the quoted subtree and nothing
        // else: the live `let` here is still reported.
        let report = report("(defmacro m (x) `(let ((result ,x)) (process '(result) result)))");
        let captures: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.risk == HygieneRisk::VariableCapture)
            .collect();
        assert_eq!(captures.len(), 1, "{report:?}");
        assert_eq!(captures[0].subject, "RESULT");
    }

    #[test]
    fn an_unquote_inside_quoted_data_still_counts_as_an_evaluation() {
        // `quote` does not stop the backquote reader: `,x` under a quote is
        // evaluated at expansion time like any other unquote, so the capture
        // check's quote-skipping must not spread to this one.
        let report = report("(defmacro m (x) `(list '(,x) ,x))");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.risk == HygieneRisk::MultipleEvaluation)
            .expect("multiple evaluation is reported");
        assert_eq!(finding.subject, "X");
        assert_eq!(finding.occurrences, 2);
    }

    /// `quote` does not stop the backquote reader, so a quoted subtree with an
    /// unquote under it is a template being assembled at expansion time, not
    /// constant data — and the literal names it binds shadow the caller's
    /// exactly as an unquoted template's do. Skipping every quoted subtree
    /// wholesale lost this finding.
    #[test]
    fn a_quoted_subtree_containing_an_unquote_is_still_emitted_code() {
        for source in [
            "(defmacro m (f) `(eval '(let ((result ,f)) (list result))))",
            "(defmacro m (f) `(eval (quote (let ((result ,f)) (list result)))))",
        ] {
            let report = report(source);
            let captures: Vec<_> = report
                .findings
                .iter()
                .filter(|finding| finding.risk == HygieneRisk::VariableCapture)
                .collect();
            assert_eq!(captures.len(), 1, "{source}: {report:?}");
            assert_eq!(captures[0].subject, "RESULT", "{source}");
        }
    }

    /// The control for the test above, and for
    /// [`a_quoted_literal_in_a_template_is_not_a_capture`]: with no unquote
    /// anywhere beneath it, however deep, the same quoted `let` is constant
    /// data the macro emits and is still skipped.
    #[test]
    fn a_quoted_subtree_with_no_unquote_beneath_it_is_still_skipped() {
        for source in [
            "(defmacro m (f) `(eval '(let ((result 1)) (list result)) ,f))",
            "(defmacro m (f) `(eval '(a (b (c (let ((result 1)) result)))) ,f))",
            "(defmacro m (f) `(eval (quote (a (let ((result 1)) result))) ,f))",
        ] {
            let report = report(source);
            assert!(report.findings.is_empty(), "{source}: {report:?}");
        }
    }

    /// A flat `[name value]` vector is never a list of `(name value)` pairs,
    /// whatever head sits in front of it. `let*` is the case that proved the
    /// head tables alone cannot decide this: it is a real Clojure special
    /// form, it is in [`PAREN_BINDING_FORMS`], and reading its vector as pairs
    /// reported the integer literal `1` as a capturable variable name — the
    /// exact defect the bracket/paren split exists to prevent.
    #[test]
    fn a_paren_binding_head_over_a_bracket_vector_reads_it_as_flat_pairs() {
        for (dialect, source) in [
            (
                Dialect::Clojure,
                "(defmacro m [form] `(let* [result 1] result))",
            ),
            (
                Dialect::Janet,
                "(defmacro m [form] ~(let* [result 1] result))",
            ),
            (Dialect::Hy, "(defmacro m [form] `(let* [result 1] result))"),
            (
                Dialect::Carp,
                "(defmacro m [form] `(let* [result 1] result))",
            ),
            (
                Dialect::Fennel,
                "(macro m [form] `(let* [result 1] result))",
            ),
        ] {
            let report = report_in(source, dialect);
            let captures: Vec<_> = report
                .findings
                .iter()
                .filter(|finding| finding.risk == HygieneRisk::VariableCapture)
                .collect();
            assert_eq!(captures.len(), 1, "{dialect:?}: {report:?}");
            assert_eq!(captures[0].subject, "result", "{dialect:?}");
            assert!(
                captures.iter().all(|capture| capture.subject != "1"),
                "{dialect:?}: {report:?}"
            );
        }
    }

    /// The reviewer's own reproduction: a two-binding `let*` vector used to
    /// report three names, one of them an integer literal.
    #[test]
    fn a_flat_vector_reports_every_name_and_no_initializer() {
        let report = report_in(
            "(defmacro m [form] `(let* [result ~form other 1] result))",
            Dialect::Clojure,
        );
        let subjects: Vec<&str> = report
            .findings
            .iter()
            .filter(|finding| finding.risk == HygieneRisk::VariableCapture)
            .map(|finding| finding.subject.as_str())
            .collect();
        assert_eq!(subjects, ["result", "other"], "{report:?}");
    }

    /// Clojure's flat-pair binders beyond `let`. All of these reported clean
    /// while [`bracket_binding_forms`] mirrored the narrower private
    /// `bracket_let_binding_head` table instead of `scope_shape`.
    #[test]
    fn clojures_other_flat_pair_binders_report_their_captures() {
        for head in [
            "let",
            "loop",
            "binding",
            "with-open",
            "with-redefs",
            "with-local-vars",
            "if-let",
            "if-some",
            "when-let",
            "when-some",
            "when-first",
        ] {
            let source = format!("(defmacro m [form] `({head} [result ~form] result))");
            let report = report_in(&source, Dialect::Clojure);
            let captures: Vec<_> = report
                .findings
                .iter()
                .filter(|finding| finding.risk == HygieneRisk::VariableCapture)
                .collect();
            assert_eq!(captures.len(), 1, "{head}: {report:?}");
            assert_eq!(captures[0].subject, "result", "{head}");
        }
    }

    /// [`bracket_binding_forms`] is a copy of the `BinderShape::FlatPairs`
    /// arms of `scope_shape`, kept because `scope_shape` resolves a head only
    /// on a list carrying no reader prefixes and every form this analysis
    /// reads is quasiquoted. This pins the copy in the direction it can go
    /// silently wrong: a head listed here that the shared model does not call
    /// a flat-pair binder at all.
    #[test]
    fn bracket_binding_forms_are_flat_pair_binders_in_the_shared_scope_model() {
        for dialect in [
            Dialect::Clojure,
            Dialect::Hy,
            Dialect::Carp,
            Dialect::Janet,
            Dialect::Fennel,
        ] {
            let policy = dialect
                .verify_rename_binding()
                .expect("every bracket dialect has verified semantics");
            for head in bracket_binding_forms(dialect) {
                let source = format!("({head} [x 1] x)");
                let tree = SyntaxTree::parse_with_dialect(&source, dialect).expect("parse");
                let root = tree.root_view();
                let form = root.children.first().expect("one top-level form");
                assert!(
                    matches!(
                        policy.scope_shape(form).map(ScopeShape::binders),
                        Some(BinderShape::FlatPairs { .. })
                    ),
                    "{dialect:?} {head}: not a flat-pair binder in scope_shape",
                );
            }
        }
    }

    /// Janet quasiquotes with `~`, and a Janet backquote opens a *long
    /// string*. Reading the backquote as Common Lisp's quasiquote counted the
    /// string as a third level and fired `macro-deep-quasiquote-nesting` on a
    /// two-level template.
    #[test]
    fn a_janet_long_string_is_not_a_quasiquote_level() {
        let report = report_in(
            "(defmacro m [x]\n  ~(a ~(b `a long string`)))\n",
            Dialect::Janet,
        );
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::DeepQuasiquoteNesting),
            "{report:?}"
        );
    }

    #[test]
    fn three_janet_quasiquote_levels_are_still_reported() {
        let report = report_in("(defmacro m [x] ~(a ~(b ~(c x))))", Dialect::Janet);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.risk == HygieneRisk::DeepQuasiquoteNesting)
            .expect("deep quasiquote nesting is reported");
        assert_eq!(finding.occurrences, 3, "{report:?}");
    }

    /// The standalone report's dialect gate and the array the lint rules hand
    /// to `RuleDialectScope` are one list now. This pins that they answer the
    /// same for every dialect, including any added later.
    #[test]
    fn the_report_gate_agrees_with_the_rule_dialect_array_for_every_dialect() {
        for dialect in Dialect::ALL {
            let tree =
                SyntaxTree::parse_with_dialect("(defmacro m (x) x)", dialect).expect("parse");
            let report = build_macro_hygiene_report(Path::new("t.lisp"), dialect, &tree);
            assert_eq!(
                report.dialect_modelled,
                HYGIENE_MODELLED_DIALECTS.contains(&dialect),
                "{dialect:?}"
            );
        }
    }

    #[test]
    fn the_missing_editor_declaration_check_does_not_apply_outside_emacs_lisp() {
        // Common Lisp `defmacro` has no `(declare (indent ...))`/`(declare
        // (debug ...))` convention at all, so a Common Lisp macro missing
        // one is not a gap to report.
        let report = report("(defmacro m (x) (list 'progn x))");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| finding.risk != HygieneRisk::MissingEditorDeclaration),
            "{report:?}"
        );
    }
}
