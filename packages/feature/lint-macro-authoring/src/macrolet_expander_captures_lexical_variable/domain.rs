//! `macrolet-expander-captures-lexical-variable` detection: a `macrolet`
//! expander that *evaluates* a name bound by an enclosing lexical binding.
//!
//! CLHS Special Operator `flet, labels, macrolet` states it directly:
//!
//! > The macro-expansion functions defined by `macrolet` are defined in the
//! > lexical environment in which the `macrolet` form appears. Declarations and
//! > `macrolet` and `symbol-macrolet` definitions affect the local macro
//! > definitions in a `macrolet`, but **the consequences are undefined if the
//! > local macro definitions reference any local variable or function bindings
//! > that are visible in that lexical environment**.
//!
//! The expander runs at macroexpansion time, when the enclosing `let` has not
//! run and its variable does not exist.
//!
//! # The runs
//!
//! SBCL 2.6.0 diagnoses the plain case — but only at the call, and only after
//! producing a loadable fasl:
//!
//! ```text
//! compile returned: warnings-p=T failure-p=T
//! call signalled [COMPILED-PROGRAM-ERROR]: Execution of a form compiled with errors.
//! Form:
//!   (REP)
//! Compile-time error:
//!   during macroexpansion of (REP). Use *BREAK-ON-SIGNALS* to intercept.
//!  The variable N is unbound.
//!  It is a local variable not available at compile-time.
//! ```
//!
//! and through `compile-file`:
//!
//! ```text
//! compile-file: fasl=T warnings-p=T failure-p=T
//! fasl loaded fine; the defect is deferred to the call
//! calling cf-victim signalled [COMPILED-PROGRAM-ERROR]
//! ```
//!
//! So the code **ships**: a fasl is written, it loads, and the failure waits
//! for whichever code path reaches the call.
//!
//! Worse, when the captured name is also a special variable there is no
//! diagnostic at all — the expander silently reads the *global* value:
//!
//! ```text
//! === P8b-silent: outer name is ALSO a special variable ===
//! lexically the let binds 3; the expander returned: 10
//! ```
//!
//! `(let ((*depth* 3)) (macrolet ((rep () (list 'quote *depth*))) (rep)))`
//! answers `10`. That is CLHS's undefined consequences biting silently, and it
//! is why this is an `Error` rather than a style note.
//!
//! # The discriminator, and why the quote model carries this rule
//!
//! The commonest `macrolet` idiom there is mentions an enclosing name:
//!
//! ```lisp
//! (let ((code '()))
//!   (macrolet ((emit (op) `(push ,op code)))   ; correct
//!     …))
//! ```
//!
//! `code` here is written *plainly in the template*. It is part of the
//! expansion, and it is bound wherever the expansion lands — which is inside
//! the `let`, where it exists. Nothing is wrong with it.
//!
//! ```lisp
//! (let ((n 3))
//!   (macrolet ((rep (f) `(progn ,@(loop repeat n collect f))))   ; undefined
//!     …))
//! ```
//!
//! `n` here is under a comma. The expander reads it at expansion time, out of
//! an environment that does not exist yet.
//!
//! The two differ **only** in whether the reference is escaped, so the entire
//! rule rests on separating a hard quote from a quasiquote and counting unquote
//! depth. A single depth counter reports the first example and misses the
//! second. See [`crate::support`].
//!
//! # Where this stops
//!
//! Only *variable* references, and only names bound by the binder shapes
//! [`crate::support::enclosing_lexical_names`] models. CLHS's sentence also
//! covers local **function** bindings (`flet`/`labels`), which are not reported
//! here: doing so needs the function namespace kept apart from the variable
//! one, and reading a head position as a variable reference would report every
//! ordinary call the expander makes.
//!
//! # Scope
//!
//! Common Lisp only. Scheme's `let-syntax` and Clojure's macros have different
//! phase rules.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{
    enclosing_lexical_names, for_each_evaluated_subview, names_bound_within, variable_name,
};

#[derive(Debug, Clone)]
pub struct MacroletExpanderCapturesLexicalVariableItem {
    /// The span of the reference itself — the comma'd name — because that is
    /// the one character's difference between this and correct code.
    pub span: ByteSpan,
    /// The local macro whose expander reads it.
    pub local_macro: String,
    /// The captured name.
    pub name: String,
}

impl Finding for MacroletExpanderCapturesLexicalVariableItem {
    fn kind(&self) -> &'static str {
        "macrolet-expander-captures-lexical-variable"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("macro={}", self.local_macro),
            format!("name={}", self.name),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("local_macro", json!(self.local_macro)),
            ("name", json!(self.name)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "the macrolet expander {} evaluates {}, which an enclosing form binds lexically: per \
             CLHS (flet, labels, macrolet) the consequences are undefined if a local macro \
             definition references a local binding visible in its lexical environment, because \
             the expander runs before that binding exists. SBCL 2.6.0 writes a loadable fasl and \
             then fails at the call with \"The variable {} is unbound. It is a local variable not \
             available at compile-time\" — and answers silently with the *global* value when {} \
             is also a special variable. Move the reference into the template, so the expansion \
             reads it where it is bound, or pass it in as a macro argument",
            self.local_macro, self.name, self.name, self.name
        )
    }
}

/// One local macro definition of a `macrolet`.
struct LocalMacro<'a> {
    name: String,
    /// The lambda list plus the expander body.
    definition: &'a ExpressionView,
}

/// Reads `(macrolet ((name lambda-list body…) …) …)`'s definition list.
///
/// A definition too short to have a lambda list is skipped rather than guessed
/// at: `(macrolet ((m)) …)` defines a macro with no expander body, which has
/// nothing to capture.
fn local_macros(view: &ExpressionView) -> Vec<LocalMacro<'_>> {
    let Some(definitions) = view.children.get(1) else {
        return Vec::new();
    };
    if !is_paren_list(definitions) {
        return Vec::new();
    }
    definitions
        .children
        .iter()
        .filter(|definition| is_paren_list(definition) && definition.children.len() > 2)
        .filter_map(|definition| {
            definition
                .children
                .first()
                .and_then(variable_name)
                .map(|name| LocalMacro { name, definition })
        })
        .collect()
}

/// Every span that is the *head* of a `(…)` list anywhere under `root`.
///
/// A head names a function, not a variable, and this rule reports variable
/// references only. Reading heads as references would report every ordinary
/// call an expander makes.
fn head_spans(root: &ExpressionView, out: &mut Vec<ByteSpan>) {
    let mut stack = vec![root];
    while let Some(view) = stack.pop() {
        if is_paren_list(view) {
            if let Some(head) = view.children.first() {
                out.push(head.span);
            }
        }
        stack.extend(view.children.iter());
    }
}

/// The names one expander *evaluates*, with the span of each reference.
///
/// Evaluated is the whole point: a name written plainly in the template is part
/// of the expansion, and only a name the expander itself reads — at
/// macroexpansion time — can capture.
fn evaluated_references(definition: &ExpressionView) -> Vec<(String, ByteSpan)> {
    let mut heads = Vec::new();
    head_spans(definition, &mut heads);

    let mut references = Vec::new();
    // Child 0 is the local macro's name and child 1 its lambda list; the
    // expander body is everything after.
    for body in definition.children.iter().skip(2) {
        for_each_evaluated_subview(body, |view| {
            if is_paren_list(view) || heads.contains(&view.span) {
                return;
            }
            if let Some(name) = variable_name(view) {
                references.push((name, view.span));
            }
        });
    }
    references
}

/// The names an expander binds itself: its lambda list, and anything its body
/// rebinds.
fn expander_own_names(definition: &ExpressionView) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(lambda_list) = definition
        .children
        .get(1)
        .filter(|view| is_paren_list(view))
    {
        // A macrolet lambda list is a macro lambda list; every name in it,
        // markers aside, is the expander's own.
        let mut stack = vec![lambda_list];
        while let Some(view) = stack.pop() {
            for child in &view.children {
                if is_paren_list(child) {
                    stack.push(child);
                } else if let Some(name) = variable_name(child) {
                    names.push(name);
                }
            }
        }
    }
    for body in definition.children.iter().skip(2) {
        names.extend(names_bound_within(body));
    }
    names
}

/// Examines one `macrolet`. Shared with the lint suite's rule.
///
/// The ordering is load-bearing: the reference scan is local to this form and
/// almost always yields nothing, so [`enclosing_lexical_names`] — which
/// descends from the root — is reached only for a `macrolet` whose expander
/// really does evaluate a free name.
pub fn examine_macrolet_expander_captures_lexical_variable(
    tree: &SyntaxTree,
    view: &ExpressionView,
    macrolet_form_count: &mut usize,
    violations: &mut Vec<MacroletExpanderCapturesLexicalVariableItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "macrolet")) {
        return;
    }
    let macros = local_macros(view);
    if macros.is_empty() {
        return;
    }
    *macrolet_form_count += 1;

    // Local, and usually empty: a pure-template expander evaluates nothing.
    let mut candidates: Vec<(String, String, ByteSpan)> = Vec::new();
    for local in &macros {
        let own = expander_own_names(local.definition);
        for (name, span) in evaluated_references(local.definition) {
            if own.contains(&name) {
                continue;
            }
            candidates.push((local.name.clone(), name, span));
        }
    }
    if candidates.is_empty() {
        return;
    }

    // Only now the root descent, which also settles whether this `macrolet` is
    // code at all.
    let Some(enclosing) = enclosing_lexical_names(tree, view.span) else {
        return;
    };
    if enclosing.is_empty() {
        return;
    }

    for (local_macro, name, span) in candidates {
        if enclosing.contains(&name) {
            violations.push(MacroletExpanderCapturesLexicalVariableItem {
                span,
                local_macro,
                name,
            });
        }
    }
}

/// Collects every capturing `macrolet` in one file, with the number of
/// `macrolet` forms scanned as the denominator beside them.
pub fn build_macrolet_expander_captures_lexical_variable_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MacroletExpanderCapturesLexicalVariableItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("macrolet_form_count", json!(0))],
        ));
    }

    let mut macrolet_form_count = 0;
    let mut violations = Vec::new();
    let root = tree.root_view();
    let mut stack = vec![&root];
    while let Some(node) = stack.pop() {
        examine_macrolet_expander_captures_lexical_variable(
            tree,
            node,
            &mut macrolet_form_count,
            &mut violations,
        );
        stack.extend(node.children.iter());
    }
    violations.sort_by_key(|item| item.span.start().get());

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("macrolet_form_count", json!(macrolet_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MacroletExpanderCapturesLexicalVariableItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_macrolet_expander_captures_lexical_variable_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<MacroletExpanderCapturesLexicalVariableItem> {
        report(input).findings
    }

    // ---- the shapes the SBCL runs demonstrated -----------------------------

    #[test]
    fn flags_an_expander_reading_an_enclosing_let_binding() {
        let found = violations("(let ((n 3)) (macrolet ((rep () n)) (rep)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "n");
        assert_eq!(found[0].local_macro, "rep");
    }

    #[test]
    fn flags_an_unquoted_reference_inside_a_template() {
        let found = violations(
            "(let ((n 3))\n\
             \x20 (macrolet ((rep (f) `(progn ,@(loop repeat n collect f))))\n\
             \x20   (rep (step))))",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "n");
    }

    /// The commonest spelling of a capture there is, and the one a name read
    /// off the atom's *full* text misses: `,n`'s `atom_text` is `",n"`, which
    /// looks like no variable at all. Every finding this rule exists for has
    /// this shape.
    #[test]
    fn flags_a_bare_comma_reference_in_a_template() {
        let found = violations(
            "(defun f (values) (let ((n 2)) (macrolet ((twice (x) `(* ,x ,n))) (twice values))))",
        );
        assert_eq!(found.len(), 1, "a `,n` reference is a reference to n");
        assert_eq!(found[0].name, "n");
    }

    #[test]
    fn flags_a_comma_splice_reference_in_a_template() {
        let found = violations("(let ((args '(1))) (macrolet ((m () `(f ,@args))) (m)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "args");
    }

    #[test]
    fn flags_an_enclosing_defun_parameter() {
        let found = violations("(defun f (a) (macrolet ((m () (list 'quote a))) (m)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "a");
    }

    #[test]
    fn flags_a_binding_from_any_enclosing_binder() {
        assert_eq!(
            violations("(dolist (item items) (macrolet ((m () item)) (m)))").len(),
            1
        );
        assert_eq!(
            violations("(multiple-value-bind (q r) (floor 7 2) (macrolet ((m () q)) (m r)))").len(),
            1
        );
    }

    // ---- the correct idiom this rule must never touch ----------------------

    /// The commonest `macrolet` there is: the name appears in the **template**,
    /// so it is part of the expansion and is bound where the expansion lands.
    #[test]
    fn does_not_flag_a_name_written_plainly_in_the_template() {
        assert!(
            violations(
                "(let ((code '()))\n\
                 \x20 (macrolet ((emit (op) `(push ,op code)))\n\
                 \x20   (emit 1) (emit 2)))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_an_expander_using_only_its_own_parameters() {
        assert!(
            violations("(let ((n 3)) (macrolet ((twice (x) `(* ,x 2))) (twice n)))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_name_the_expander_binds_itself() {
        assert!(
            violations(
                "(let ((n 3))\n\
                 \x20 (macrolet ((m () (let ((n 5)) (list 'quote n))))\n\
                 \x20   (m)))"
            )
            .is_empty()
        );
    }

    /// A global — a special variable, a constant, a function — is not an
    /// enclosing *lexical* binding, so it is not this rule's subject.
    #[test]
    fn does_not_flag_a_free_name_that_no_enclosing_form_binds() {
        assert!(violations("(let ((n 3)) (macrolet ((m () *feature-level*)) (m)))").is_empty());
        assert!(violations("(macrolet ((m () *depth*)) (m))").is_empty());
    }

    /// A `macrolet` at top level has no enclosing lexical environment at all.
    #[test]
    fn does_not_flag_a_top_level_macrolet() {
        assert!(violations("(macrolet ((m () (compute))) (m))").is_empty());
    }

    /// A head names a function; reporting it would report every call the
    /// expander makes.
    ///
    /// The enclosing binder must bind the **same name** for this to test the
    /// head guard at all — `(flet ((n () 1)) …)` binds nothing in the variable
    /// namespace, so a version of this test using `flet` passes whether the
    /// guard is there or not.
    #[test]
    fn does_not_flag_a_head_position() {
        assert!(
            violations("(let ((collect 1)) (macrolet ((m () (collect 2))) (m)))").is_empty(),
            "`collect` in operator position is a function reference, not a read of the variable"
        );
        assert!(
            violations("(defun f (list) (macrolet ((m () (list 1 2))) (m)))").is_empty(),
            "a parameter shadowing a standard function is still a call in head position"
        );
        assert!(
            violations("(let ((n 3)) (macrolet ((m () (list 'x))) (m n)))").is_empty(),
            "the reference is in the macrolet body, not in the expander"
        );
    }

    /// …but the same name in an **argument** position is a read, and is
    /// reported. This is the other side of the head guard: without it the
    /// guard could simply suppress everything.
    #[test]
    fn still_flags_the_same_name_in_an_argument_position() {
        let found = violations("(let ((collect 1)) (macrolet ((m () (list collect))) (m)))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "collect");
    }

    /// An init form runs before the binding exists, so `x` is not enclosing.
    #[test]
    fn does_not_flag_a_reference_in_the_binding_forms_own_init() {
        assert!(violations("(let ((x (macrolet ((m () x)) (m)))) x)").is_empty());
    }

    // ---- the quote model ---------------------------------------------------

    #[test]
    fn does_not_flag_a_macrolet_inside_quoted_data() {
        assert!(violations("'(let ((n 3)) (macrolet ((rep () n)) (rep)))").is_empty());
        assert!(
            violations("(defmacro outer () `(let ((n 3)) (macrolet ((rep () n)) (rep))))")
                .is_empty()
        );
    }

    /// A doubled quasiquote is still data with one comma.
    #[test]
    fn does_not_flag_a_macrolet_under_a_net_positive_quote_depth() {
        assert!(
            violations("(defun f (n) ``(let ((q 1)) ,(macrolet ((rep () n)) (rep))))").is_empty()
        );
    }

    #[test]
    fn flags_a_macrolet_reached_through_an_unquote() {
        let found = violations("(defun f (n) `(list ,(macrolet ((rep () n)) (rep))))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "n");
    }

    // ---- the two false positives the SBCL audit found ----------------------

    /// SBCL `src/code/stream.lisp:1363` and `:1430`, reduced.
    ///
    /// `(case (dsd-name dsd) ((index start) 'start) …)` — `start` is a **case
    /// clause key**, not a variable reference. Nothing marks it as data: there
    /// is no quote character anywhere in a key list. Reading it as a reference
    /// made this rule report on two of SBCL's own stream constructors, and it
    /// is the reason [`crate::support::for_each_evaluated_subview`] knows about
    /// the selector forms.
    #[test]
    fn does_not_flag_a_name_appearing_as_a_case_clause_key() {
        assert!(
            violations(
                "(defun %init-string-input-stream (stream string &optional (start 0) end)\n\
                 \x20 (macrolet ((initforms ()\n\
                 \x20              `(progn ,@(mapcar (lambda (dsd)\n\
                 \x20                  `(%instance-set stream ,(dsd-index dsd)\n\
                 \x20                     ,(case (dsd-name dsd)\n\
                 \x20                        ((index start) 'start)\n\
                 \x20                        (limit 'end)\n\
                 \x20                        (t (dsd-default dsd)))))\n\
                 \x20                 (dd-slots (find-defstruct-description 'x))))))\n\
                 \x20   (initforms)))"
            )
            .is_empty(),
            "a case clause key is not a variable reference"
        );
    }

    #[test]
    fn does_not_flag_a_name_appearing_as_a_typecase_or_handler_case_key() {
        assert!(
            violations("(let ((buffer 1)) (macrolet ((m () (typecase x (buffer 1)))) (m)))")
                .is_empty()
        );
        assert!(
            violations(
                "(let ((error 1)) (macrolet ((m () (handler-case (f) (error () nil)))) (m)))"
            )
            .is_empty()
        );
    }

    /// SBCL `src/code/type.lisp:2810`, reduced.
    ///
    /// `(loop for (class format coerce simple-coerce) in specs …)` rebinds
    /// `format`, which the enclosing `defun` also has as a parameter. `loop`'s
    /// binding positions are keyword-directed, so they are read for shadowing
    /// even though they are not trusted enough to report on.
    #[test]
    fn does_not_flag_a_name_a_loop_clause_rebinds() {
        assert!(
            violations(
                "(defun %make-union-numeric-type (class format complexp low high)\n\
                 \x20 (macrolet ((unionize (&rest specs)\n\
                 \x20              `(type-union\n\
                 \x20                ,@(loop for (class format coerce) in specs\n\
                 \x20                        collect `(make-numeric-union-type\n\
                 \x20                                  :class ',class :format ',format)))))\n\
                 \x20   (unionize (integer nil nil))))"
            )
            .is_empty(),
            "a loop destructuring variable shadows the enclosing parameter"
        );
    }

    /// The shadowing read must stay narrow: `(loop repeat n collect f)` binds
    /// nothing called `n`, so a capture of `n` there is still reported.
    #[test]
    fn still_flags_a_capture_a_loop_does_not_rebind() {
        assert_eq!(
            violations(
                "(let ((n 3))\n\
                 \x20 (macrolet ((rep (f) `(progn ,@(loop repeat n collect f))))\n\
                 \x20   (rep (step))))"
            )
            .len(),
            1,
            "`repeat n` is not a binding clause"
        );
    }

    // ---- report plumbing ---------------------------------------------------

    #[test]
    fn the_denominator_counts_every_macrolet_scanned_not_only_the_flagged_ones() {
        let scanned = report(
            "(let ((n 3))\n\
             \x20 (macrolet ((a () n)) (a))\n\
             \x20 (macrolet ((b (x) `(list ,x n))) (b 1))\n\
             \x20 (macrolet ((c (x) `(+ ,x 1))) (c 2)))",
        );
        assert_eq!(scanned.summary, vec![("macrolet_form_count", json!(3))]);
        assert_eq!(scanned.findings.len(), 1);
    }

    #[test]
    fn a_macrolet_with_no_definitions_is_not_counted() {
        let scanned = report("(macrolet () (f))");
        assert_eq!(scanned.summary, vec![("macrolet_form_count", json!(0))]);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(macrolet [] 1)", Dialect::Clojure).expect("parse");
        let built = build_macrolet_expander_captures_lexical_variable_report(
            Path::new("a.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_message_names_the_variable_and_cites_the_clhs_rule() {
        let built = report("(let ((n 3)) (macrolet ((rep () n)) (rep)))");
        let message = built.findings[0].message();
        assert!(message.contains("CLHS"), "{message}");
        assert!(message.contains('n'), "{message}");
        assert!(message.contains("template"), "{message}");
    }

    #[test]
    fn folds_case_the_way_the_reader_would() {
        assert_eq!(
            violations("(LET ((N 3)) (MACROLET ((REP () N)) (REP)))").len(),
            1
        );
    }
}
